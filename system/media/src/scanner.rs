//! Filesystem-neutral media scanning and change-watch contracts.

use crate::library::{MediaMetadata, MimeType, mime_from_extension};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Art einer Aenderung, die der Scanner meldet.
pub enum ScanChange {
    /// Die Datei ist neu.
    Added,
    /// Die Datei hat sich geaendert.
    Modified,
    /// Die Datei ist verschwunden.
    Removed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Was der Scanner ueber eine Datei weiss.
pub struct FileRecord {
    /// Pfad oder URI der Datei.
    pub uri: String,
    /// Groesse in Byte.
    pub size_bytes: u64,
    /// Aenderungszeitpunkt in Millisekunden seit Epoche.
    pub modified_unix_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Eine gemeldete Aenderung.
pub struct ScanEvent {
    /// Art der Aenderung.
    pub change: ScanChange,
    /// Betroffene Datei.
    pub record: FileRecord,
}

/// Quelle, die Dateien auflisten kann.
pub trait MediaFileSource {
    /// Listet die Dateien unterhalb einer Wurzel.
    fn list_files(&mut self, root: &str) -> Result<Vec<FileRecord>, String>;
}
/// Quelle, die Aenderungen melden kann.
pub trait MediaWatchSource {
    /// Fragt die aufgelaufenen Aenderungen ab.
    fn poll(&mut self) -> Result<Vec<ScanEvent>, String>;
}

#[derive(Debug, Default)]
/// Vergleicht Dateilisten und leitet Aenderungen daraus ab.
pub struct MediaScanner;

impl MediaScanner {
    /// Leerer Scanner ohne bekannten Stand.
    pub fn new() -> Self {
        Self
    }

    fn stable_id(uri: &str) -> u64 {
        let mut hash = 0xcbf29ce484222325u64;
        for byte in uri.as_bytes() {
            hash ^= *byte as u64;
            hash = hash.wrapping_mul(0x100000001b3);
        }
        hash
    }

    /// Liest Metadaten zu einer Datei, falls die Quelle sie liefert.
    pub fn metadata_for(&mut self, file: &FileRecord) -> Option<MediaMetadata> {
        let mime = mime_from_extension(&file.uri);
        if mime == MimeType::Unknown {
            return None;
        }
        let title = file.uri.rsplit('/').next().unwrap_or(&file.uri).to_owned();
        Some(MediaMetadata {
            id: Self::stable_id(&file.uri),
            uri: file.uri.clone(),
            title,
            mime,
            size_bytes: file.size_bytes,
            modified_unix_ms: file.modified_unix_ms,
            content_hash: None,
            duration_ms: None,
            width: None,
            height: None,
        })
    }

    /// Liest die Quelle und meldet die Aenderungen gegenueber dem letzten Stand.
    pub fn scan<S: MediaFileSource>(
        &mut self,
        source: &mut S,
        root: &str,
    ) -> Result<Vec<MediaMetadata>, String> {
        Ok(source
            .list_files(root)?
            .iter()
            .filter_map(|f| self.metadata_for(f))
            .collect())
    }
}
