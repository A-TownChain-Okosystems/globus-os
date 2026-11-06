//! Scancode-zu-Keycode fuer die PS/2-Tastatur (Set 1).
//!
//! Der Treiber las bisher nur den Rohcode und verwarf ihn. Damit liess sich
//! pruefen, dass ein Interrupt ankommt, aber nicht, welche Taste er bedeutet.
//! Diese Schicht macht aus dem Code ein Zeichen.
//!
//! Aufbau von Set 1: Bit 7 gesetzt heisst "Taste losgelassen". `0xE0` ist ein
//! Praefix und gehoert zur naechsten Taste (Pfeile, rechte Strg/Alt). Uebrige
//! Codes stehen fuer sich. Zwei Tasten sind Zustand statt Zeichen: Shift und
//! Caps Lock.
//!
//! Reine Funktionen, kein Zugriff auf Ports: `decode` bekommt den Scancode und
//! den Zustand, den es fortschreibt. Damit ist das Verhalten ohne Hardware
//! pruefbar.

/// Taste, unabhaengig von der Tastaturbelegung.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    /// Bedruckbare Taste mit ihrer Lage auf einer US-Tastatur.
    Character(u8),
    Enter,
    Escape,
    Backspace,
    Tab,
    Space,
    CapsLock,
    ShiftLeft,
    ShiftRight,
    ControlLeft,
    ControlRight,
    AltLeft,
    AltRight,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    /// Code ohne Bedeutung in dieser Belegung.
    Unknown(u8),
}

/// Zustand, den die Tastatur zwischen zwei Codes haelt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ModifierState {
    pub shift: bool,
    pub caps_lock: bool,
    /// Das vorige Byte war das Praefix 0xE0.
    pub extended: bool,
}

/// Ergebnis eines Scancodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Decoded {
    pub key: Key,
    /// false, wenn die Taste losgelassen wurde.
    pub pressed: bool,
    /// Zeichen, das diese Taste in diesem Zustand erzeugt (nur beim Druecken).
    pub character: Option<u8>,
    /// true, wenn dieser Code den Modifikatorzustand veraendert hat.
    pub state_changed: bool,
}

/// Praefix fuer erweiterte Codes.
pub const EXTENDED_PREFIX: u8 = 0xE0;
/// Bit, das ein Loslassen markiert.
pub const RELEASE_BIT: u8 = 0x80;

/// Set-1-Codes der Zeichentasten: (Scancode, Zeichen ohne Shift, Zeichen mit Shift).
const CHARACTERS: [(u8, u8, u8); 48] = [
    (0x02, b'1', b'!'),
    (0x03, b'2', b'@'),
    (0x04, b'3', b'#'),
    (0x05, b'4', b'$'),
    (0x06, b'5', b'%'),
    (0x07, b'6', b'^'),
    (0x08, b'7', b'&'),
    (0x09, b'8', b'*'),
    (0x0A, b'9', b'('),
    (0x0B, b'0', b')'),
    (0x0C, b'-', b'_'),
    (0x0D, b'=', b'+'),
    (0x10, b'q', b'Q'),
    (0x11, b'w', b'W'),
    (0x12, b'e', b'E'),
    (0x13, b'r', b'R'),
    (0x14, b't', b'T'),
    (0x15, b'y', b'Y'),
    (0x16, b'u', b'U'),
    (0x17, b'i', b'I'),
    (0x18, b'o', b'O'),
    (0x19, b'p', b'P'),
    (0x1A, b'[', b'{'),
    (0x1B, b']', b'}'),
    (0x1E, b'a', b'A'),
    (0x1F, b's', b'S'),
    (0x20, b'd', b'D'),
    (0x21, b'f', b'F'),
    (0x22, b'g', b'G'),
    (0x23, b'h', b'H'),
    (0x24, b'j', b'J'),
    (0x25, b'k', b'K'),
    (0x26, b'l', b'L'),
    (0x27, b';', b':'),
    (0x28, b'\'', b'"'),
    (0x29, b'`', b'~'),
    (0x2B, b'\\', b'|'),
    (0x2C, b'z', b'Z'),
    (0x2D, b'x', b'X'),
    (0x2E, b'c', b'C'),
    (0x2F, b'v', b'V'),
    (0x30, b'b', b'B'),
    (0x31, b'n', b'N'),
    (0x32, b'm', b'M'),
    (0x33, b',', b'<'),
    (0x34, b'.', b'>'),
    (0x35, b'/', b'?'),
    (0x39, b' ', b' '),
];

const SHIFT_LEFT: u8 = 0x2A;
const SHIFT_RIGHT: u8 = 0x36;
const CAPS_LOCK: u8 = 0x3A;
const CONTROL_LEFT: u8 = 0x1D;
const ALT_LEFT: u8 = 0x38;

/// Uebersetzt einen Scancode und schreibt den Modifikatorzustand fort.
pub fn decode(scancode: u8, state: &mut ModifierState) -> Decoded {
    // Praefix: nur merken, die naechste Taste entscheidet.
    if scancode == EXTENDED_PREFIX {
        state.extended = true;
        return Decoded {
            key: Key::Unknown(scancode),
            pressed: false,
            character: None,
            state_changed: false,
        };
    }

    let extended = state.extended;
    state.extended = false;
    let pressed = scancode & RELEASE_BIT == 0;
    let code = scancode & !RELEASE_BIT;

    // Modifikatoren zuerst: sie erzeugen kein Zeichen, sondern Zustand.
    match (code, extended) {
        (SHIFT_LEFT, false) | (SHIFT_RIGHT, false) => {
            state.shift = pressed;
            return Decoded {
                key: if code == SHIFT_LEFT {
                    Key::ShiftLeft
                } else {
                    Key::ShiftRight
                },
                pressed,
                character: None,
                state_changed: true,
            };
        }
        (CAPS_LOCK, false) => {
            // Caps Lock schaltet beim Druecken um, beim Loslassen nicht.
            if pressed {
                state.caps_lock = !state.caps_lock;
            }
            return Decoded {
                key: Key::CapsLock,
                pressed,
                character: None,
                state_changed: pressed,
            };
        }
        (CONTROL_LEFT, false) => {
            return Decoded {
                key: Key::ControlLeft,
                pressed,
                character: None,
                state_changed: false,
            }
        }
        (CONTROL_LEFT, true) => {
            return Decoded {
                key: Key::ControlRight,
                pressed,
                character: None,
                state_changed: false,
            }
        }
        (ALT_LEFT, false) => {
            return Decoded {
                key: Key::AltLeft,
                pressed,
                character: None,
                state_changed: false,
            }
        }
        (ALT_LEFT, true) => {
            return Decoded {
                key: Key::AltRight,
                pressed,
                character: None,
                state_changed: false,
            }
        }
        _ => {}
    }

    if extended {
        let key = match code {
            0x48 => Key::ArrowUp,
            0x50 => Key::ArrowDown,
            0x4B => Key::ArrowLeft,
            0x4D => Key::ArrowRight,
            other => Key::Unknown(other),
        };
        return Decoded {
            key,
            pressed,
            character: None,
            state_changed: false,
        };
    }

    let key = match code {
        0x01 => Key::Escape,
        0x0E => Key::Backspace,
        0x0F => Key::Tab,
        0x1C => Key::Enter,
        0x39 => Key::Space,
        other => match CHARACTERS
            .iter()
            .find(|(scancode, _, _)| *scancode == other)
        {
            Some((_, plain, shifted)) => {
                let character = if state.shift { *shifted } else { *plain };
                return Decoded {
                    key: Key::Character(character),
                    pressed,
                    character: if pressed {
                        Some(resolve_case(character, state))
                    } else {
                        None
                    },
                    state_changed: false,
                };
            }
            None => Key::Unknown(other),
        },
    };

    Decoded {
        key,
        pressed,
        character: if pressed && matches!(key, Key::Space) {
            Some(b' ')
        } else {
            None
        },
        state_changed: false,
    }
}

/// Wendet Caps Lock auf Buchstaben an. Shift und Caps heben sich auf, wie auf
/// jeder Tastatur: mit beiden wird klein geschrieben.
fn resolve_case(character: u8, state: &ModifierState) -> u8 {
    if !character.is_ascii_alphabetic() {
        return character;
    }
    let wants_upper = state.shift ^ state.caps_lock;
    if wants_upper {
        character.to_ascii_uppercase()
    } else {
        character.to_ascii_lowercase()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(scancode: u8) -> (Decoded, ModifierState) {
        let mut state = ModifierState::default();
        let decoded = decode(scancode, &mut state);
        (decoded, state)
    }

    #[test]
    fn a_letter_presses_and_releases() {
        let (down, _) = press(0x1E);
        assert_eq!(down.key, Key::Character(b'a'));
        assert_eq!(down.character, Some(b'a'));
        assert!(down.pressed);

        let mut state = ModifierState::default();
        let up = decode(0x9E, &mut state);
        assert_eq!(up.key, Key::Character(b'a'), "gleiche Taste");
        assert!(!up.pressed);
        assert_eq!(up.character, None, "Loslassen erzeugt kein Zeichen");
    }

    #[test]
    fn shift_makes_it_upper_and_survives_the_next_key() {
        let mut state = ModifierState::default();
        let shift = decode(0x2A, &mut state);
        assert_eq!(shift.key, Key::ShiftLeft);
        assert!(shift.state_changed);
        assert!(state.shift);

        let letter = decode(0x1E, &mut state);
        assert_eq!(letter.character, Some(b'A'));

        decode(0xAA, &mut state);
        assert!(!state.shift, "Loslassen von Shift hebt den Zustand auf");
        assert_eq!(decode(0x1E, &mut state).character, Some(b'a'));
    }

    #[test]
    fn digits_carry_their_symbols() {
        let mut state = ModifierState::default();
        assert_eq!(decode(0x02, &mut state).character, Some(b'1'));
        assert_eq!(decode(0x0B, &mut state).character, Some(b'0'));
        state.shift = true;
        assert_eq!(decode(0x02, &mut state).character, Some(b'!'));
        assert_eq!(decode(0x0B, &mut state).character, Some(b')'));
    }

    #[test]
    fn caps_lock_toggles_on_press_only() {
        let mut state = ModifierState::default();
        assert!(decode(0x3A, &mut state).state_changed);
        assert!(state.caps_lock);
        decode(0xBA, &mut state);
        assert!(state.caps_lock, "Loslassen schaltet nicht zurueck");
        decode(0x3A, &mut state);
        assert!(!state.caps_lock);
    }

    #[test]
    fn caps_lock_and_shift_cancel_each_other() {
        let mut state = ModifierState::default();
        decode(0x3A, &mut state); // Caps an
        assert_eq!(decode(0x1E, &mut state).character, Some(b'A'));
        decode(0x2A, &mut state); // zusaetzlich Shift
        assert_eq!(
            decode(0x1E, &mut state).character,
            Some(b'a'),
            "Caps und Shift heben sich auf"
        );
    }

    #[test]
    fn caps_lock_does_not_touch_symbols() {
        let mut state = ModifierState::default();
        decode(0x3A, &mut state);
        assert_eq!(decode(0x02, &mut state).character, Some(b'1'));
    }

    #[test]
    fn the_extended_prefix_belongs_to_the_next_key() {
        let mut state = ModifierState::default();
        let prefix = decode(EXTENDED_PREFIX, &mut state);
        assert!(!prefix.pressed, "das Praefix ist keine Taste");
        assert!(state.extended);

        let up = decode(0x48, &mut state);
        assert_eq!(up.key, Key::ArrowUp);
        assert!(!state.extended, "Praefix verbraucht");

        decode(EXTENDED_PREFIX, &mut state);
        let right_ctrl = decode(0x1D, &mut state);
        assert_eq!(right_ctrl.key, Key::ControlRight);
    }

    #[test]
    fn named_keys_are_recognised() {
        let mut state = ModifierState::default();
        assert_eq!(decode(0x1C, &mut state).key, Key::Enter);
        assert_eq!(decode(0x0E, &mut state).key, Key::Backspace);
        assert_eq!(decode(0x0F, &mut state).key, Key::Tab);
        assert_eq!(decode(0x01, &mut state).key, Key::Escape);
        let space = decode(0x39, &mut state);
        assert_eq!(space.key, Key::Space);
        assert_eq!(space.character, Some(b' '));
    }

    #[test]
    fn every_code_decodes_without_panic() {
        // Ein unbekannter Code darf den IRQ-Pfad nicht abstuerzen lassen.
        for scancode in 0..=u8::MAX {
            let mut state = ModifierState::default();
            let decoded = decode(scancode, &mut state);
            if scancode & RELEASE_BIT == 0 && scancode != EXTENDED_PREFIX {
                assert!(decoded.pressed);
            }
        }
    }

    #[test]
    fn an_unknown_code_is_reported_not_swallowed() {
        let (decoded, _) = press(0x7F);
        assert_eq!(decoded.key, Key::Unknown(0x7F));
        assert_eq!(decoded.character, None);
    }
}
