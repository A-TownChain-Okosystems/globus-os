//! Deterministic persistent directory-entry encoding and validation.

#[derive(Debug, Clone, PartialEq, Eq)]
/// Fehler beim Kodieren oder Pruefen eines Verzeichniseintrags.
pub enum DirectoryError {
    /// Der Zielpuffer ist zu klein.
    Buffer,
    /// Der Name ist leer, zu lang oder enthaelt NUL beziehungsweise Schraegstrich.
    InvalidName,
    /// Die Inode ist 0.
    InvalidInode,
    /// Typbyte oder reservierte Bytes sind unbekannt beziehungsweise nicht 0.
    InvalidType,
    /// Die Laengenrechnung laeuft ueber.
    Overflow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Art eines Verzeichniseintrags.
pub enum DirectoryType {
    /// Regulaere Datei.
    File = 1,
    /// Verzeichnis.
    Directory = 2,
    /// Symbolischer Verweis.
    Symlink = 3,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Ein Verzeichniseintrag.
pub struct DirectoryRecord {
    /// Inode des Ziels.
    pub inode: u64,
    /// Art des Eintrags.
    pub kind: DirectoryType,
    /// Name im Verzeichnis, ohne NUL und ohne Schraegstrich.
    pub name: String,
}

impl DirectoryRecord {
    /// Laenge der kodierten Form, nachdem Name, Typ und Inode geprueft wurden.
    pub fn encoded_len(&self) -> Result<usize, DirectoryError> {
        if self.inode == 0
            || self.name.is_empty()
            || self.name.len() > u16::MAX as usize
            || self.name.as_bytes().iter().any(|b| *b == 0 || *b == b'/')
        {
            return Err(DirectoryError::InvalidName);
        }
        16usize
            .checked_add(self.name.len())
            .ok_or(DirectoryError::Overflow)
    }

    /// Schreibt den Eintrag kanonisch in den Puffer.
    pub fn encode(&self, out: &mut [u8]) -> Result<usize, DirectoryError> {
        let len = self.encoded_len()?;
        if out.len() < len {
            return Err(DirectoryError::Buffer);
        }
        out[..len].fill(0);
        out[..8].copy_from_slice(&self.inode.to_le_bytes());
        out[8] = self.kind as u8;
        out[10..12].copy_from_slice(&(self.name.len() as u16).to_le_bytes());
        out[16..len].copy_from_slice(self.name.as_bytes());
        Ok(len)
    }

    /// Liest einen Eintrag und dessen Laenge.
    pub fn decode(input: &[u8]) -> Result<(Self, usize), DirectoryError> {
        if input.len() < 16 {
            return Err(DirectoryError::Buffer);
        }
        if input[9] != 0 || input[12..16].iter().any(|b| *b != 0) {
            return Err(DirectoryError::InvalidType);
        }
        let inode = u64::from_le_bytes(input[..8].try_into().map_err(|_| DirectoryError::Buffer)?);
        if inode == 0 {
            return Err(DirectoryError::InvalidInode);
        }
        let kind = match input[8] {
            1 => DirectoryType::File,
            2 => DirectoryType::Directory,
            3 => DirectoryType::Symlink,
            _ => return Err(DirectoryError::InvalidType),
        };
        let name_len = u16::from_le_bytes(
            input[10..12]
                .try_into()
                .map_err(|_| DirectoryError::Buffer)?,
        ) as usize;
        let end = 16usize
            .checked_add(name_len)
            .ok_or(DirectoryError::Overflow)?;
        if input.len() < end {
            return Err(DirectoryError::Buffer);
        }
        let name = std::str::from_utf8(&input[16..end]).map_err(|_| DirectoryError::InvalidName)?;
        if name.is_empty() || name.as_bytes().iter().any(|b| *b == 0 || *b == b'/') {
            return Err(DirectoryError::InvalidName);
        }
        Ok((
            Self {
                inode,
                kind,
                name: name.to_owned(),
            },
            end,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let record = DirectoryRecord {
            inode: 42,
            kind: DirectoryType::File,
            name: "kernel.bin".into(),
        };
        let mut buf = [0u8; 64];
        let used = record.encode(&mut buf).unwrap();
        let (decoded, size) = DirectoryRecord::decode(&buf[..used]).unwrap();
        assert_eq!(decoded, record);
        assert_eq!(size, used);
    }

    #[test]
    fn invalid_name_is_rejected() {
        let record = DirectoryRecord {
            inode: 1,
            kind: DirectoryType::File,
            name: "bad/name".into(),
        };
        assert_eq!(record.encoded_len(), Err(DirectoryError::InvalidName));
    }
}
