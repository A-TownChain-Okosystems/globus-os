//! Deterministic physical-frame allocator over firmware-discovered usable ranges.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Ein zusammenhaengender Bereich nutzbarer Frames.
pub struct FrameRange {
    /// Erste Adresse des Bereichs.
    pub start: u64,
    /// Anzahl Frames im Bereich.
    pub frames: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Warum eine Frame-Allokation nicht moeglich ist.
pub enum FrameError {
    /// Seitengroesse ist 0 oder keine Zweierpotenz.
    InvalidPageSize,
    /// Start oder Ende liegt nicht auf einer Seitengrenze.
    Unaligned,
    /// Der Bereich enthaelt keinen Frame.
    Empty,
    /// Die Rechnung laeuft ueber den Adressraum hinaus.
    Overflow,
    /// Es sind nicht mehr genug freie Frames vorhanden.
    Exhausted,
}

#[derive(Debug, Clone, Copy)]
/// Vergibt physische Frames fortlaufend aus einem nutzbaren Bereich.
pub struct FrameAllocator {
    next: u64,
    end: u64,
    page_size: u64,
}

impl FrameAllocator {
    /// Prueft Seitengroesse, Ausrichtung und Groesse des Bereichs.
    pub fn new(start: u64, end: u64, page_size: u64) -> Result<Self, FrameError> {
        if page_size == 0 || !page_size.is_power_of_two() {
            return Err(FrameError::InvalidPageSize);
        }
        if !start.is_multiple_of(page_size) || !end.is_multiple_of(page_size) {
            return Err(FrameError::Unaligned);
        }
        if start >= end {
            return Err(FrameError::Empty);
        }
        Ok(Self {
            next: start,
            end,
            page_size,
        })
    }

    /// Vergibt `frames` zusammenhaengende Frames.
    pub fn allocate(&mut self, frames: u64) -> Result<FrameRange, FrameError> {
        if frames == 0 {
            return Err(FrameError::Empty);
        }
        let bytes = frames
            .checked_mul(self.page_size)
            .ok_or(FrameError::Overflow)?;
        let next = self.next.checked_add(bytes).ok_or(FrameError::Overflow)?;
        if next > self.end {
            return Err(FrameError::Exhausted);
        }
        let range = FrameRange {
            start: self.next,
            frames,
        };
        self.next = next;
        Ok(range)
    }

    /// Verbleibende freie Frames.
    pub fn remaining(&self) -> u64 {
        (self.end - self.next) / self.page_size
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn allocates_contiguous_frames() {
        let mut a = FrameAllocator::new(0x1000, 0x5000, 0x1000).unwrap();
        assert_eq!(
            a.allocate(2).unwrap(),
            FrameRange {
                start: 0x1000,
                frames: 2
            }
        );
        assert_eq!(a.remaining(), 2);
    }
    #[test]
    fn fails_closed_at_end() {
        let mut a = FrameAllocator::new(0x1000, 0x2000, 0x1000).unwrap();
        assert_eq!(a.allocate(2), Err(FrameError::Exhausted));
    }
}
