//! Persistence boundary for thumbnails. Storage is intentionally injected so the media crate
//! does not assume a filesystem format or backend.

use crate::thumbnail_cache::{ThumbnailKey, ThumbnailSize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Kantenlaengen eines abgelegten Vorschaubilds.
pub struct ThumbnailDimensions {
    /// Breite in Pixel.
    pub width: u32,
    /// Hoehe in Pixel.
    pub height: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Ein abgelegtes Vorschaubild.
pub struct StoredThumbnail {
    /// Schluessel des Eintrags.
    pub key: ThumbnailKey,
    /// Kantenlaengen.
    pub dimensions: ThumbnailDimensions,
    /// Bilddaten.
    pub data: Vec<u8>,
}

/// Ablage fuer Vorschaubilder.
pub trait ThumbnailStore {
    /// Laedt einen Eintrag.
    fn load(&self, key: &ThumbnailKey) -> Option<StoredThumbnail>;
    /// Legt einen Eintrag ab.
    fn save(&mut self, thumbnail: StoredThumbnail) -> Result<(), String>;
    /// Entfernt einen Eintrag.
    fn remove(&mut self, key: &ThumbnailKey) -> Result<(), String>;
    /// Entfernt alle Eintraege einer Quelldatei.
    fn clear_uri(&mut self, uri: &str) -> Result<(), String>;
}

#[derive(Debug, Default)]
/// Ablage im Arbeitsspeicher, fuer Tests und Vorstufen.
pub struct MemoryThumbnailStore {
    entries: Vec<StoredThumbnail>,
}

impl MemoryThumbnailStore {
    /// Leere Ablage.
    pub fn new() -> Self {
        Self::default()
    }
    /// Anzahl abgelegter Eintraege.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Counterpart to `len`.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    /// True, wenn fuer diese Quelle ein Vorschaubild dieser Groesse vorliegt.
    pub fn contains(&self, uri: &str, size: ThumbnailSize) -> bool {
        self.entries
            .iter()
            .any(|x| x.key.uri == uri && x.key.size == size)
    }
}

impl ThumbnailStore for MemoryThumbnailStore {
    fn load(&self, key: &ThumbnailKey) -> Option<StoredThumbnail> {
        self.entries.iter().find(|x| &x.key == key).cloned()
    }
    fn save(&mut self, thumbnail: StoredThumbnail) -> Result<(), String> {
        self.entries.retain(|x| x.key != thumbnail.key);
        self.entries.push(thumbnail);
        Ok(())
    }
    fn remove(&mut self, key: &ThumbnailKey) -> Result<(), String> {
        self.entries.retain(|x| &x.key != key);
        Ok(())
    }
    fn clear_uri(&mut self, uri: &str) -> Result<(), String> {
        self.entries.retain(|x| x.key.uri != uri);
        Ok(())
    }
}
