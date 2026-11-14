//! Bounded LRU thumbnail cache with file-metadata identity.

use std::collections::{HashMap, VecDeque};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
/// Groesse eines Vorschaubilds.
pub enum ThumbnailSize {
    /// Klein.
    Small,
    /// Mittel.
    Medium,
    /// Gross.
    Large,
}

impl ThumbnailSize {
    /// Kantenlaengen dieser Groesse in Pixel.
    pub const fn dimensions(self) -> (u32, u32) {
        match self {
            Self::Small => (128, 128),
            Self::Medium => (256, 256),
            Self::Large => (512, 512),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
/// Schluessel, unter dem ein Vorschaubild abgelegt wird.
pub struct ThumbnailKey {
    /// Pfad oder URI der Quelldatei.
    pub uri: String,
    /// Aenderungszeitpunkt der Quelle in Nanosekunden.
    pub modified_ns: u64,
    /// Groesse der Quelle in Byte.
    pub size_bytes: u64,
    /// Optionaler Fingerabdruck des Inhalts.
    pub content_fingerprint: Option<[u8; 32]>,
    /// Groesse des Vorschaubilds.
    pub size: ThumbnailSize,
}

#[derive(Debug)]
/// Zwischenspeicher nach Schluessel, begrenzt in Anzahl und Byte.
pub struct ThumbnailCache {
    capacity: usize,
    byte_limit: usize,
    bytes: usize,
    map: HashMap<ThumbnailKey, Vec<u8>>,
    order: VecDeque<ThumbnailKey>,
}

impl ThumbnailCache {
    /// Legt den Zwischenspeicher mit dieser Kapazitaet an.
    pub fn new(capacity: usize) -> Self {
        Self::with_limits(capacity, usize::MAX)
    }
    /// Legt den Zwischenspeicher mit eigener Byte-Obergrenze an.
    pub fn with_limits(capacity: usize, byte_limit: usize) -> Self {
        Self {
            capacity,
            byte_limit,
            bytes: 0,
            map: HashMap::new(),
            order: VecDeque::new(),
        }
    }
    /// Anzahl gespeicherter Eintraege.
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// Counterpart to `len`.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    /// Belegter Speicher in Byte.
    pub fn bytes(&self) -> usize {
        self.bytes
    }
    /// Legt ein Vorschaubild ab und verdraengt bei Bedarf die aeltesten Eintraege.
    pub fn insert(&mut self, key: ThumbnailKey, data: Vec<u8>) {
        if self.capacity == 0 || data.len() > self.byte_limit {
            return;
        }
        if let Some(old) = self.map.remove(&key) {
            self.bytes = self.bytes.saturating_sub(old.len());
            self.order.retain(|k| k != &key);
        }
        self.bytes = self.bytes.saturating_add(data.len());
        self.map.insert(key.clone(), data);
        self.order.push_back(key);
        while self.order.len() > self.capacity || self.bytes > self.byte_limit {
            if let Some(old) = self.order.pop_front() {
                if let Some(data) = self.map.remove(&old) {
                    self.bytes = self.bytes.saturating_sub(data.len());
                }
            } else {
                break;
            }
        }
    }
    /// Vorschaubild zu einem Schluessel.
    pub fn get(&mut self, key: &ThumbnailKey) -> Option<&[u8]> {
        if self.map.contains_key(key) {
            self.order.retain(|k| k != key);
            self.order.push_back(key.clone());
        }
        self.map.get(key).map(Vec::as_slice)
    }
    /// Verwirft alle Eintraege einer Quelldatei.
    pub fn invalidate_uri(&mut self, uri: &str) {
        let keys: Vec<_> = self
            .order
            .iter()
            .filter(|k| k.uri == uri)
            .cloned()
            .collect();
        for k in keys {
            if let Some(data) = self.map.remove(&k) {
                self.bytes = self.bytes.saturating_sub(data.len());
            }
        }
        self.order.retain(|k| k.uri != uri);
    }
    /// Leert den Zwischenspeicher.
    pub fn clear(&mut self) {
        self.map.clear();
        self.order.clear();
        self.bytes = 0;
    }
}
