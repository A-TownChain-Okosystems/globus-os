//! Capability-neutral VFS file and inode primitives.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct InodeId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Art eines Inode.
pub enum FileType {
    /// Regulaere Datei.
    Regular,
    /// Verzeichnis.
    Directory,
    /// Symbolischer Verweis.
    Symlink,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Zugriffsrechte eines Inode.
pub struct FileMode {
    /// Lesen erlaubt.
    pub read: bool,
    /// Schreiben erlaubt.
    pub write: bool,
    /// Ausfuehren erlaubt.
    pub execute: bool,
}

impl FileMode {
    /// Nur Lesen.
    pub const READ_ONLY: Self = Self {
        read: true,
        write: false,
        execute: false,
    };
    /// Lesen und Schreiben.
    pub const READ_WRITE: Self = Self {
        read: true,
        write: true,
        execute: false,
    };
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Metadaten einer Datei.
pub struct Inode {
    /// Inode-Nummer.
    pub id: InodeId,
    /// Art des Inode.
    pub file_type: FileType,
    /// Zugriffsrechte.
    pub mode: FileMode,
    /// Groesse in Byte.
    pub size: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Flags beim Oeffnen.
pub enum OpenFlags {
    /// Nur lesen.
    ReadOnly,
    /// Nur schreiben.
    WriteOnly,
    /// Lesen und schreiben.
    ReadWrite,
    /// Anlegen, falls nicht vorhanden.
    Create,
}

impl OpenFlags {
    /// True, wenn das Flag Lesen erlaubt.
    pub const fn readable(self) -> bool {
        matches!(self, Self::ReadOnly | Self::ReadWrite | Self::Create)
    }
    /// True, wenn das Flag Schreiben erlaubt.
    pub const fn writable(self) -> bool {
        matches!(self, Self::WriteOnly | Self::ReadWrite | Self::Create)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Offene Datei mit Position.
pub struct FileHandle {
    /// Zugehoeriger Inode.
    pub inode: InodeId,
    /// Aktuelle Position in Byte.
    pub offset: u64,
    /// Flags, mit denen geoeffnet wurde.
    pub flags: OpenFlags,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Fehler der Dateioperationen.
pub enum FileError {
    /// Die Rechte decken die Operation nicht.
    PermissionDenied,
    /// Die Position liegt ausserhalb der Datei.
    InvalidOffset,
    /// Die Positionsrechnung laeuft ueber.
    Overflow,
    /// Der Inode ist kein Verzeichnis.
    NotDirectory,
    /// Der Inode ist keine regulaere Datei.
    NotFile,
    /// Der Inode existiert nicht.
    NotFound,
}

impl FileHandle {
    /// Verschiebt die Position und meldet Ueberlauf.
    pub fn advance(&mut self, amount: usize) -> Result<u64, FileError> {
        self.offset = self
            .offset
            .checked_add(amount as u64)
            .ok_or(FileError::Overflow)?;
        Ok(self.offset)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn handle_advances_deterministically() {
        let mut h = FileHandle {
            inode: InodeId(1),
            offset: 0,
            flags: OpenFlags::ReadOnly,
        };
        assert_eq!(h.advance(8), Ok(8));
    }
    #[test]
    fn flags_expose_access() {
        assert!(OpenFlags::ReadWrite.readable());
        assert!(OpenFlags::ReadWrite.writable());
        assert!(!OpenFlags::ReadOnly.writable());
    }
}
