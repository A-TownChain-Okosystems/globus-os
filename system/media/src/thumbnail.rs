//! Bounded deterministic thumbnail cache.

use std::collections::VecDeque;

#[derive(Debug, Clone, PartialEq, Eq)]
/// Ein Vorschaubild.
pub struct Thumbnail {
    /// Kennung des Vorschaubilds.
    pub id: u64,
    /// Breite in Pixel.
    pub width: u32,
    /// Hoehe in Pixel.
    pub height: u32,
    /// Bilddaten.
    pub data: Vec<u8>,
}

#[derive(Debug)]
/// Zwischenspeicher fuer Vorschaubilder mit fester Kapazitaet.
pub struct ThumbnailCache {
    capacity: usize,
    entries: VecDeque<Thumbnail>,
}

impl ThumbnailCache {
    /// Legt den Zwischenspeicher mit dieser Kapazitaet an.
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            entries: VecDeque::new(),
        }
    }
    /// Anzahl gespeicherter Vorschaubilder.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Counterpart to `len`.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    /// Vorschaubild zu einer Kennung.
    pub fn get(&mut self, id: u64) -> Option<&Thumbnail> {
        let pos = self.entries.iter().position(|x| x.id == id)?;
        let item = self.entries.remove(pos)?;
        self.entries.push_front(item);
        self.entries.front()
    }
    /// Legt ein Vorschaubild ab und verdraengt bei Bedarf das aelteste.
    pub fn insert(&mut self, thumbnail: Thumbnail) {
        if self.capacity == 0 {
            return;
        }
        self.entries.retain(|x| x.id != thumbnail.id);
        self.entries.push_front(thumbnail);
        while self.entries.len() > self.capacity {
            self.entries.pop_back();
        }
    }
    /// Leert den Zwischenspeicher.
    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn t(id: u64) -> Thumbnail {
        Thumbnail {
            id,
            width: 1,
            height: 1,
            data: vec![0; 4],
        }
    }
    #[test]
    fn lru_eviction() {
        let mut c = ThumbnailCache::new(2);
        c.insert(t(1));
        c.insert(t(2));
        assert_eq!(c.get(1).unwrap().id, 1);
        c.insert(t(3));
        assert!(c.get(2).is_none());
        assert!(c.get(1).is_some());
    }
}
