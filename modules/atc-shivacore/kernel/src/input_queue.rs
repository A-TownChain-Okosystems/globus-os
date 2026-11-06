//! Begrenzte Eingabewarteschlange zwischen Interrupt-Handler und Konsument.
//!
//! Der Handler hat bisher fuer jede Taste Formatierung und Serienausgabe
//! gemacht — Arbeit, die in einen Interrupt nicht gehoert. Jetzt legt er den
//! Scancode nur noch ab; das Leeren und Ausgeben macht der Tick.
//!
//! Der Puffer ist bewusst fest und ohne Allokation: im Interrupt darf nichts
//! allokieren. Ist er voll, wird das neue Ereignis verworfen und gezaehlt — ein
//! Interrupt darf nicht warten (dieselbe Regel wie beim IPC-Kanal).

/// Fester Puffer; 64 Ereignisse sind mehr als eine Sekunde Tippen bei 100 Hz.
pub const INPUT_CAPACITY: usize = 64;

/// Ein Tastenereignis, wie der Handler es ablegt.
///
/// Zeichen wird beim Druecken uebersetzt, nicht beim Leeren: sonst koennte eine
/// innerhalb eines Ticks geloeste Umschalttaste das Zeichen nachtraeglich
/// verfaelschen.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct InputEvent {
    pub scancode: u8,
    pub character: Option<char>,
}

impl InputEvent {
    pub const fn new(scancode: u8, character: Option<char>) -> Self {
        Self {
            scancode,
            character,
        }
    }

    /// Fasst das Ereignis in ein 64-Bit-Wort: Scancode in den unteren acht
    /// Bits, das Zeichen (falls druckbar) in Bits 16..24.
    ///
    /// Die eingefrorene Syscall-ABI hat keinen Pufferzeiger, aber ein
    /// Rueckgabewert von 64 Bit. Ein Tastenereignis passt genau hinein, ohne
    /// dass ein Feld erfunden werden muss. 0 heisst "nichts anliegend", weil
    /// Scancode 0 in Set 1 nicht vorkommt.
    pub fn packed(&self) -> u64 {
        let character = match self.character {
            Some(c) => c as u32 as u64,
            None => 0,
        };
        u64::from(self.scancode) | (character << 16)
    }

    /// Gegenrichtung zu `packed`, fuer Tests und den Userspace.
    pub fn unpacked(word: u64) -> Option<Self> {
        let scancode = (word & 0xFF) as u8;
        if scancode == 0 {
            return None;
        }
        let character = ((word >> 16) & 0xFF) as u8;
        Some(Self {
            scancode,
            character: if character == 0 {
                None
            } else {
                Some(character as char)
            },
        })
    }
}

pub struct InputQueue {
    buffer: [InputEvent; INPUT_CAPACITY],
    /// Position des aeltesten Eintrags.
    head: usize,
    len: usize,
    dropped: u64,
}

impl Default for InputQueue {
    fn default() -> Self {
        Self::new()
    }
}

impl InputQueue {
    pub const fn new() -> Self {
        Self {
            buffer: [InputEvent {
                scancode: 0,
                character: None,
            }; INPUT_CAPACITY],
            head: 0,
            len: 0,
            dropped: 0,
        }
    }

    /// Legt ein Ereignis ab. `false` heisst: Puffer voll, Ereignis verworfen.
    pub fn push(&mut self, event: InputEvent) -> bool {
        if self.len == INPUT_CAPACITY {
            self.dropped = self.dropped.saturating_add(1);
            return false;
        }
        let tail = (self.head + self.len) % INPUT_CAPACITY;
        self.buffer[tail] = event;
        self.len += 1;
        true
    }

    /// Nimmt das aelteste Ereignis heraus.
    pub fn pop(&mut self) -> Option<InputEvent> {
        if self.len == 0 {
            return None;
        }
        let event = self.buffer[self.head];
        self.head = (self.head + 1) % INPUT_CAPACITY;
        self.len -= 1;
        Some(event)
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Wie viele Ereignisse verworfen wurden, weil der Puffer voll war.
    pub fn dropped(&self) -> u64 {
        self.dropped
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn packed_event_round_trips_through_a_single_word() {
        let event = super::InputEvent::new(0x1E, Some('a'));
        let word = event.packed();
        assert_eq!(word & 0xFF, 0x1E);
        assert_eq!((word >> 16) & 0xFF, u64::from(b'a'));
        assert_eq!(super::InputEvent::unpacked(word), Some(event));
    }

    #[test]
    fn zero_word_means_nothing_pending() {
        assert_eq!(super::InputEvent::unpacked(0), None);
    }

    #[test]
    fn named_keys_pack_without_a_character() {
        let event = super::InputEvent::new(0x1C, None);
        assert_eq!(event.packed(), 0x1C);
        assert_eq!(super::InputEvent::unpacked(0x1C), Some(event));
    }

    use super::*;

    #[test]
    fn events_come_out_in_the_order_they_went_in() {
        let mut queue = InputQueue::new();
        for event in [
            InputEvent::new(0x1E, Some('a')),
            InputEvent::new(0x9E, None),
            InputEvent::new(0x30, Some('b')),
        ] {
            assert!(queue.push(event));
        }
        assert_eq!(queue.pop(), Some(InputEvent::new(0x1E, Some('a'))));
        assert_eq!(queue.pop(), Some(InputEvent::new(0x9E, None)));
        assert_eq!(queue.pop(), Some(InputEvent::new(0x30, Some('b'))));
    }

    #[test]
    fn an_empty_queue_hands_out_nothing() {
        let mut queue = InputQueue::new();
        assert_eq!(queue.pop(), None);
        assert!(queue.is_empty());
    }

    #[test]
    fn the_capacity_is_the_limit_and_the_rest_is_counted() {
        let mut queue = InputQueue::new();
        for i in 0..INPUT_CAPACITY {
            let event = InputEvent::new(i as u8, None);
            assert!(queue.push(event), "Platz {} muss reichen", i);
        }
        assert_eq!(queue.len(), INPUT_CAPACITY);
        assert!(
            !queue.push(InputEvent::new(0xAA, None)),
            "voller Puffer lehnt ab"
        );
        assert!(!queue.push(InputEvent::new(0xAB, None)));
        assert_eq!(queue.dropped(), 2);
        assert_eq!(queue.len(), INPUT_CAPACITY, "Ablehnung aendert nichts");
    }

    #[test]
    fn a_dropped_event_does_not_shift_the_older_ones() {
        let mut queue = InputQueue::new();
        assert!(queue.push(InputEvent::new(0x01, Some('q'))));
        for _ in 0..INPUT_CAPACITY {
            queue.push(InputEvent::new(0x02, Some('w')));
        }
        assert_eq!(
            queue.pop(),
            Some(InputEvent::new(0x01, Some('q'))),
            "das aelteste bleibt das aelteste"
        );
    }

    #[test]
    fn the_ring_wraps_around_without_losing_order() {
        let mut queue = InputQueue::new();
        // Dreiviertel fuellen, halb leeren, wieder fuellen: der Umlauf muss
        // die Reihenfolge halten.
        for i in 0..48u8 {
            queue.push(InputEvent::new(i, None));
        }
        for i in 0..24u8 {
            assert_eq!(queue.pop(), Some(InputEvent::new(i, None)));
        }
        for i in 48..72u8 {
            queue.push(InputEvent::new(i, None));
        }
        for i in 24..72u8 {
            assert_eq!(
                queue.pop(),
                Some(InputEvent::new(i, None)),
                "Umlauf bei {}",
                i
            );
        }
        assert!(queue.is_empty());
        assert_eq!(queue.dropped(), 0);
    }

    #[test]
    fn a_consumer_drains_everything_that_was_pushed() {
        let mut queue = InputQueue::new();
        let events = [
            InputEvent::new(0x1E, Some('a')),
            InputEvent::new(0x9E, None),
            InputEvent::new(0x1F, Some('s')),
            InputEvent::new(0x9F, None),
            InputEvent::new(0x20, Some('d')),
        ];
        for event in events {
            queue.push(event);
        }
        let mut seen = 0;
        while queue.pop().is_some() {
            seen += 1;
        }
        assert_eq!(seen, events.len());
        assert_eq!(queue.len(), 0);
    }
}
