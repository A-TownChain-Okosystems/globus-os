// Copyright (c) 2026 Michael Wroblewski / ShivaCore / A-TownChain-Okosystems. All Rights Reserved.
// ─────────────────────────────────────────────────────────────────────────
// SRAM-001 — Inhaltsverzeichnis der Ramdisk
//
// Bisher trug die Ramdisk genau ein ELF: der Kernel las die Bytes und hatte
// keine Moeglichkeit zu sagen, was darin liegt. Sobald mehr als ein Programm
// mitreisen soll, braucht die Ramdisk ein Verzeichnis — auch wenn es noch kein
// Dateisystem ist.
//
// Format (little endian):
//
// ```text
//   0  magic   4 Byte  "SRAM"
//   4  version u16     muss 1 sein
//   6  count   u16     Anzahl der Eintraege
//   8  Eintraege, je 24 Byte:
//        name   16 Byte  ASCII, mit Nullen aufgefuellt, nicht leer
//        offset u32      ab Beginn der Ramdisk
//        length u32      Laenge der Nutzdaten
// ```
//
// Drei Regeln, gleiche Haltung wie bei den Syscall-Bloecken:
//
//   * **Erst pruefen, dann zeigen.** Jeder Eintrag muss vollstaendig in der
//     Ramdisk liegen, sonst ist die ganze Tabelle ungueltig. Ein halb lesbares
//     Verzeichnis waere schlimmer als keins.
//   * **Keine zwei Wahrheiten.** Doppelte Namen werden abgelehnt, nicht
//     ueberschrieben. Welches Programm gilt, darf nicht von der Reihenfolge
//     abhaengen.
//   * **Nutzdaten bleiben undurchsichtig.** Diese Datei sagt, wo etwas liegt
//     und wie es heisst. Was darin steht, entscheidet der Leser (der ELF-Parser).
// ─────────────────────────────────────────────────────────────────────────

use alloc::string::String;
use alloc::vec::Vec;

/// Kennung der Ramdisk mit Verzeichnis.
pub const MAGIC: [u8; 4] = *b"SRAM";

/// Fassung des Formats.
pub const VERSION: u16 = 1;

/// Kopf: magic, version, count.
pub const HEADER_LEN: usize = 8;

/// Groesse eines Eintrags: Name plus Offset und Laenge.
pub const ENTRY_LEN: usize = 24;

/// Groesster Name, der in einen Eintrag passt.
pub const NAME_CAPACITY: usize = 16;

/// Ein Eintrag im Verzeichnis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// Name ohne Fuellbytes.
    pub name: String,
    /// Beginn der Nutzdaten, absolut in der Ramdisk.
    pub offset: usize,
    /// Laenge der Nutzdaten.
    pub length: usize,
}

impl Entry {
    /// Die Nutzdaten aus dem Abbild schneiden.
    ///
    /// Gibt `None` zurueck, wenn der Eintrag nicht in die uebergebenen Bytes
    /// passt. Nach `parse` kann das nicht passieren, nach dem Bau von Hand
    /// schon, deshalb wird hier erneut geprueft statt geschnitten.
    pub fn data<'a>(&self, image: &'a [u8]) -> Option<&'a [u8]> {
        let end = self.offset.checked_add(self.length)?;
        if end > image.len() {
            return None;
        }
        Some(&image[self.offset..end])
    }
}

/// Fehler beim Lesen des Verzeichnisses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RamdiskError {
    /// Zu kurz fuer Kopf oder ein vollstaendiges Verzeichnis.
    Truncated,
    /// Kennung passt nicht.
    BadMagic,
    /// Unbekannte Fassung.
    UnsupportedVersion(u16),
    /// Kein einziger Eintrag.
    Empty,
    /// Ein Name ist leer oder nicht druckbar.
    InvalidName,
    /// Zwei Eintraege heissen gleich.
    DuplicateName,
    /// Nutzdaten liegen ausserhalb der Ramdisk.
    OutOfBounds,
}

/// Ist das ein Verzeichnis oder ein nacktes Programm?
///
/// Ein Abbild ohne Kennung wird weiterhin als einzelnes Programm gelesen, damit
/// ein Image ohne Verzeichnis bootet.
pub fn is_container(image: &[u8]) -> bool {
    image.len() >= 4 && image[0..4] == MAGIC
}

/// Liest das Verzeichnis.
pub fn parse(image: &[u8]) -> Result<Vec<Entry>, RamdiskError> {
    if image.len() < 4 {
        return Err(RamdiskError::Truncated);
    }
    if image[0..4] != MAGIC {
        return Err(RamdiskError::BadMagic);
    }
    if image.len() < HEADER_LEN {
        return Err(RamdiskError::Truncated);
    }
    let version = u16::from_le_bytes([image[4], image[5]]);
    if version != VERSION {
        return Err(RamdiskError::UnsupportedVersion(version));
    }
    let count = u16::from_le_bytes([image[6], image[7]]) as usize;
    if count == 0 {
        return Err(RamdiskError::Empty);
    }
    let table_end = count
        .checked_mul(ENTRY_LEN)
        .and_then(|size| size.checked_add(HEADER_LEN))
        .ok_or(RamdiskError::OutOfBounds)?;
    if table_end > image.len() {
        return Err(RamdiskError::Truncated);
    }

    let mut entries: Vec<Entry> = Vec::with_capacity(count);
    for index in 0..count {
        let base = HEADER_LEN + index * ENTRY_LEN;
        let name = decode_name(&image[base..base + NAME_CAPACITY])?;
        let offset = u32::from_le_bytes([
            image[base + 16],
            image[base + 17],
            image[base + 18],
            image[base + 19],
        ]) as usize;
        let length = u32::from_le_bytes([
            image[base + 20],
            image[base + 21],
            image[base + 22],
            image[base + 23],
        ]) as usize;

        let end = offset
            .checked_add(length)
            .ok_or(RamdiskError::OutOfBounds)?;
        if end > image.len() {
            return Err(RamdiskError::OutOfBounds);
        }
        if entries.iter().any(|known| known.name == name) {
            return Err(RamdiskError::DuplicateName);
        }
        entries.push(Entry {
            name,
            offset,
            length,
        });
    }
    Ok(entries)
}

/// Sucht einen Eintrag nach Namen.
pub fn find<'a>(entries: &'a [Entry], name: &str) -> Option<&'a Entry> {
    entries.iter().find(|entry| entry.name == name)
}

/// Liest den Namen aus seinen 16 Byte.
///
/// Erlaubt sind ASCII-Zeichen ohne Null und ohne Steuerzeichen; die Fuellung
/// besteht aus Nullen. Alles andere waere ein Name, den niemand tippen kann.
fn decode_name(raw: &[u8]) -> Result<String, RamdiskError> {
    let end = raw.iter().position(|byte| *byte == 0).unwrap_or(raw.len());
    let bytes = &raw[..end];
    if bytes.is_empty() {
        return Err(RamdiskError::InvalidName);
    }
    if !bytes.iter().all(|byte| (0x20..0x7F).contains(byte)) {
        return Err(RamdiskError::InvalidName);
    }
    // Die Bytes sind auf ASCII geprueft, deshalb ist der Weg ueber `str` sicher.
    let text = core::str::from_utf8(bytes).map_err(|_| RamdiskError::InvalidName)?;
    Ok(String::from(text))
}

/// Baut ein Abbild aus Name/Nutzdaten-Paaren. Fuer Tests und Werkzeuge.
///
/// Die Nutzdaten beginnen direkt hinter dem Verzeichnis, in der Reihenfolge der
/// uebergebenen Paare. Namen, die nicht in 16 Byte passen, werden abgelehnt.
pub fn build(entries: &[(&str, &[u8])]) -> Result<Vec<u8>, RamdiskError> {
    if entries.is_empty() {
        return Err(RamdiskError::Empty);
    }
    let table_end = HEADER_LEN + entries.len() * ENTRY_LEN;
    let mut image: Vec<u8> = alloc::vec![0u8; table_end];
    image[0..4].copy_from_slice(&MAGIC);
    image[4..6].copy_from_slice(&VERSION.to_le_bytes());
    image[6..8].copy_from_slice(&(entries.len() as u16).to_le_bytes());

    for (index, (name, data)) in entries.iter().enumerate() {
        let mut raw = [0u8; NAME_CAPACITY];
        let name_bytes = name.as_bytes();
        if name_bytes.is_empty() || name_bytes.len() > NAME_CAPACITY {
            return Err(RamdiskError::InvalidName);
        }
        if !name_bytes.iter().all(|byte| (0x20..0x7F).contains(byte)) {
            return Err(RamdiskError::InvalidName);
        }
        if entries[..index].iter().any(|(known, _)| known == name) {
            return Err(RamdiskError::DuplicateName);
        }
        raw[..name_bytes.len()].copy_from_slice(name_bytes);

        let offset = image.len();
        image.extend_from_slice(data);

        let base = HEADER_LEN + index * ENTRY_LEN;
        image[base..base + NAME_CAPACITY].copy_from_slice(&raw);
        image[base + 16..base + 20].copy_from_slice(&(offset as u32).to_le_bytes());
        image[base + 20..base + 24].copy_from_slice(&(data.len() as u32).to_le_bytes());
    }
    Ok(image)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_built_image_can_be_read_back() {
        let image = build(&[("init", b"PROGRAMM-EINS"), ("shell", b"PROGRAMM-ZWEI")]).unwrap();
        let entries = parse(&image).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "init");
        assert_eq!(entries[0].data(&image), Some(&b"PROGRAMM-EINS"[..]));
        assert_eq!(entries[1].name, "shell");
        assert_eq!(entries[1].data(&image), Some(&b"PROGRAMM-ZWEI"[..]));
    }

    #[test]
    fn find_returns_the_named_entry_only() {
        let image = build(&[("init", b"A"), ("shell", b"B")]).unwrap();
        let entries = parse(&image).unwrap();
        assert_eq!(
            find(&entries, "shell").map(|e| e.name.as_str()),
            Some("shell")
        );
        assert!(find(&entries, "gibtsnicht").is_none());
    }

    #[test]
    fn a_raw_elf_is_not_a_container() {
        // Ein nacktes Programm beginnt mit 0x7F 'E' 'L' 'F', nicht mit SRAM.
        assert!(!is_container(&[0x7F, b'E', b'L', b'F']));
        assert!(is_container(&MAGIC));
    }

    #[test]
    fn a_foreign_magic_is_rejected() {
        let mut image = build(&[("init", b"A")]).unwrap();
        image[0..4].copy_from_slice(b"NOPE");
        assert_eq!(parse(&image), Err(RamdiskError::BadMagic));
    }

    #[test]
    fn a_future_version_is_rejected_instead_of_misread() {
        let mut image = build(&[("init", b"A")]).unwrap();
        image[4..6].copy_from_slice(&2u16.to_le_bytes());
        assert_eq!(parse(&image), Err(RamdiskError::UnsupportedVersion(2)));
    }

    #[test]
    fn an_empty_directory_is_an_error_not_an_empty_list() {
        let mut image = build(&[("init", b"A")]).unwrap();
        image[6..8].copy_from_slice(&0u16.to_le_bytes());
        assert_eq!(parse(&image), Err(RamdiskError::Empty));
    }

    #[test]
    fn a_truncated_table_is_rejected() {
        let image = build(&[("init", b"A"), ("zwei", b"B")]).unwrap();
        // Verzeichnis fuer zwei Eintraege angekuendigt, aber nur einer vorhanden.
        assert_eq!(
            parse(&image[..HEADER_LEN + ENTRY_LEN]),
            Err(RamdiskError::Truncated)
        );
    }

    #[test]
    fn an_entry_pointing_past_the_end_is_rejected() {
        let mut image = build(&[("init", b"A")]).unwrap();
        let base = HEADER_LEN;
        image[base + 20..base + 24].copy_from_slice(&9_999u32.to_le_bytes());
        assert_eq!(parse(&image), Err(RamdiskError::OutOfBounds));
    }

    #[test]
    fn duplicate_names_are_rejected_when_building() {
        assert_eq!(
            build(&[("init", b"A"), ("init", b"B")]),
            Err(RamdiskError::DuplicateName)
        );
    }

    #[test]
    fn a_name_that_does_not_fit_is_rejected() {
        assert_eq!(
            build(&[("diesernameistzulangfuersfeld", b"A")]),
            Err(RamdiskError::InvalidName)
        );
    }

    #[test]
    fn a_name_with_control_bytes_is_rejected() {
        let mut image = build(&[("init", b"A")]).unwrap();
        image[HEADER_LEN] = 0x01;
        assert_eq!(parse(&image), Err(RamdiskError::InvalidName));
    }

    #[test]
    fn an_entry_with_an_empty_payload_is_allowed_but_empty_lists_nothing() {
        let image = build(&[("leer", b"")]).unwrap();
        let entries = parse(&image).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].length, 0);
        assert_eq!(entries[0].data(&image), Some(&[][..]));
    }
}
