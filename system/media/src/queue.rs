//! Bounded video frame queue with late-frame accounting.

use crate::VideoFrame;
use std::collections::VecDeque;

#[derive(Debug, Default)]
/// Warteschlange dekodierter Bilder mit Zeitstempeln.
pub struct FrameQueue {
    capacity: usize,
    frames: VecDeque<(u64, VideoFrame)>,
}

impl FrameQueue {
    /// Legt eine Warteschlange mit dieser Kapazitaet an.
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            frames: VecDeque::new(),
        }
    }
    /// Anzahl gespeicherter Bilder.
    pub fn len(&self) -> usize {
        self.frames.len()
    }
    /// True, wenn kein Bild wartet.
    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }
    /// Haengt ein Bild an; false, wenn die Warteschlange voll ist.
    pub fn push(&mut self, pts_us: u64, frame: VideoFrame) -> bool {
        if self.capacity == 0 || self.frames.len() >= self.capacity {
            return false;
        }
        self.frames.push_back((pts_us, frame));
        true
    }
    /// Zeitstempel des vordersten Bildes.
    pub fn peek_pts(&self) -> Option<u64> {
        self.frames.front().map(|x| x.0)
    }
    /// Nimmt das vorderste Bild heraus.
    pub fn pop(&mut self) -> Option<(u64, VideoFrame)> {
        self.frames.pop_front()
    }
    /// Verwirft alle Bilder, die weiter als die Schwelle zurueckliegen, und liefert deren Anzahl.
    pub fn drop_late(&mut self, master_us: u64, threshold_us: u64) -> usize {
        let mut n = 0;
        while let Some((pts, _)) = self.frames.front() {
            if *pts + threshold_us >= master_us {
                break;
            }
            self.frames.pop_front();
            n += 1;
        }
        n
    }
    /// Leert die Warteschlange.
    pub fn clear(&mut self) {
        self.frames.clear();
    }
}
