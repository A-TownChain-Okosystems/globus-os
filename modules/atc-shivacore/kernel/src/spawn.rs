// Copyright (c) 2026 Michael Wroblewski / ShivaCore / A-TownChain-Okosystems. All Rights Reserved.
// ─────────────────────────────────────────────────────────────────────────
// Spawn-Annahme (ABI 2.1): Init beantragt ein Programm aus dem Ramdisk-Verzeichnis.
//
// Der Kernel liest das Verzeichnis heute selbst und startet, was er findet. Damit
// entscheidet der Kernel statt des Init. Dieser Pfad dreht es um: der Aufruf kommt
// per Syscall, der Kernel prueft und ordnet zu.
//
// EHRLICHE GRENZE: Die Seitentabellen eines neuen Prozesses brauchen Frames, und
// ein Frame-Allokator ist zur Syscall-Zeit nicht erreichbar (er lebt im Bootpfad).
// Deshalb endet die Annahme hier mit `ResourcesUnavailable` und einem Protokoll —
// sie erfindet keinen PID. Was fehlt, ist genau ein globaler Frame-Vorrat.
// ─────────────────────────────────────────────────────────────────────────

use crate::elf_loader::ElfParser;
use crate::ramdisk::{self, Entry};
use crate::userspace::UserAddressSpace;
use spin::Mutex;

/// Ergebnis einer Spawn-Annahme.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpawnPlan {
    pub name_len: usize,
    pub elf_len: usize,
    pub entry: u64,
}

/// Warum ein Spawn-Wunsch nicht angenommen wurde.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpawnError {
    /// Namensfeld leer.
    EmptyName,
    /// Kein Eintrag dieses Namens im Verzeichnis.
    NotFound,
    /// Bytes sind kein lesbares ELF oder nicht ET_EXEC.
    NotExecutable,
    /// Einstieg liegt ausserhalb des Nutzercode-Fensters.
    EntryOutsideWindow,
    /// Kein Verzeichnis im Speicher (Image ohne Ramdisk).
    NoDirectory,
}

/// Einstieg gegen das Nutzercode-Fenster pruefen (Spezifikation, nicht Vermutung).
pub fn validate_entry(entry: u64, space: &UserAddressSpace) -> Result<(), SpawnError> {
    if entry < space.code_base || entry >= space.code_base.saturating_add(space.code_size) {
        return Err(SpawnError::EntryOutsideWindow);
    }
    Ok(())
}

/// Name aus dem 16-Byte-Feld lesen (NUL-gepolstert).
pub fn name_from_field(field: &[u8; 16]) -> Result<&str, SpawnError> {
    let end = field.iter().position(|b| *b == 0).unwrap_or(field.len());
    if end == 0 {
        return Err(SpawnError::EmptyName);
    }
    core::str::from_utf8(&field[..end]).map_err(|_| SpawnError::EmptyName)
}

/// Annahme: Name aufloesen, Bytes pruefen, Einstieg gegen das Fenster halten.
pub fn plan(entries: &[Entry], image: &[u8], field: &[u8; 16]) -> Result<SpawnPlan, SpawnError> {
    let name = name_from_field(field)?;
    let entry = ramdisk::find(entries, name).ok_or(SpawnError::NotFound)?;
    let bytes = entry.data(image).ok_or(SpawnError::NotFound)?;
    let elf = ElfParser::parse(bytes).map_err(|_| SpawnError::NotExecutable)?;
    validate_entry(elf.entry_point(), &UserAddressSpace::default())?;
    Ok(SpawnPlan {
        name_len: name.len(),
        elf_len: bytes.len(),
        entry: elf.entry_point(),
    })
}

/// Verzeichnis fuer die Syscall-Zeit merken (Zeiger auf die Boot-Region).
static DIRECTORY: Mutex<Option<(usize, usize)>> = Mutex::new(None);

/// Vom Bootpfad aufzurufen, sobald das Verzeichnis geparst ist.
pub fn retain(image_ptr: usize, len: usize) {
    *DIRECTORY.lock() = Some((image_ptr, len));
}

/// Annahme gegen das gemerkte Verzeichnis. None = kein Verzeichnis im Speicher.
pub fn plan_retained(field: &[u8; 16]) -> Result<SpawnPlan, SpawnError> {
    let retained = *DIRECTORY.lock();
    let (ptr, len) = retained.ok_or(SpawnError::NoDirectory)?;
    if ptr == 0 || len == 0 || len > 64 * 1024 * 1024 {
        return Err(SpawnError::NoDirectory);
    }
    // Die Region liegt im Boot-Abbild und bleibt gemappt; dieselbe Lesart wie im
    // Bootpfad (virtuelle Adresse, kein physical_memory_offset).
    let image: &[u8] = unsafe { core::slice::from_raw_parts(ptr as *const u8, len) };
    let entries = ramdisk::parse(image).map_err(|_| SpawnError::NoDirectory)?;
    plan(&entries, image, field)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(name: &str) -> [u8; 16] {
        let mut f = [0u8; 16];
        f[..name.len()].copy_from_slice(name.as_bytes());
        f
    }

    #[test]
    fn leerer_name_wird_abgelehnt() {
        assert_eq!(name_from_field(&[0u8; 16]), Err(SpawnError::EmptyName));
    }

    #[test]
    fn name_wird_bis_zur_null_lesen() {
        assert_eq!(name_from_field(&field("probe")), Ok("probe"));
    }

    #[test]
    fn volles_namensfeld_ohne_null_ist_gueltig() {
        let f = *b"0123456789abcdef";
        assert_eq!(name_from_field(&f), Ok("0123456789abcdef"));
    }

    #[test]
    fn unbekannter_name_wird_abgelehnt() {
        let image = ramdisk::build(&[("init", &[0x7f, b'E', b'L', b'F'])]).expect("Verzeichnis");
        let entries = ramdisk::parse(&image).expect("lesbar");
        assert_eq!(plan(&entries, &image, &field("probe")), Err(SpawnError::NotFound));
    }

    #[test]
    fn nicht_elf_wird_abgelehnt() {
        let image = ramdisk::build(&[("probe", &[1, 2, 3, 4])]).expect("Verzeichnis");
        let entries = ramdisk::parse(&image).expect("lesbar");
        assert_eq!(plan(&entries, &image, &field("probe")), Err(SpawnError::NotExecutable));
    }

    #[test]
    fn einstieg_ausserhalb_des_fensters_wird_abgelehnt() {
        let space = UserAddressSpace::default();
        assert_eq!(validate_entry(0x2011c0, &space), Err(SpawnError::EntryOutsideWindow));
        assert_eq!(validate_entry(0x500000, &space), Err(SpawnError::EntryOutsideWindow));
        assert_eq!(validate_entry(0x400000, &space), Ok(()));
    }

    #[test]
    fn ohne_verzeichnis_keine_annahme() {
        assert_eq!(plan_retained(&field("init")), Err(SpawnError::NoDirectory));
    }
}

/// Was zuletzt beantragt wurde (fuer die Protokollzeile des Kernels).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Attempt {
    pub seq: u64,
    pub pid: u64,
    pub plan: SpawnPlan,
    pub error: Option<SpawnError>,
}

static SEQ: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);
static REPORTED: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);
static LAST: Mutex<Option<Attempt>> = Mutex::new(None);

/// Vom Dispatcher aufzurufen: Entscheidung festhalten. Der Lib-Code druckt nicht
/// (das Makro lebt im Binaer-Crate), die Protokollzeile schreibt der Kernel.
pub fn note_attempt(pid: u64, plan: SpawnPlan, error: Option<SpawnError>) {
    let seq = SEQ.fetch_add(1, core::sync::atomic::Ordering::SeqCst) + 1;
    *LAST.lock() = Some(Attempt { seq, pid, plan, error });
}

/// Noch nicht protokollierte Entscheidung (genau einmal).
pub fn take_unreported() -> Option<Attempt> {
    let attempt = *LAST.lock();
    match attempt {
        Some(a) if a.seq > REPORTED.load(core::sync::atomic::Ordering::SeqCst) => {
            REPORTED.store(a.seq, core::sync::atomic::Ordering::SeqCst);
            Some(a)
        }
        _ => None,
    }
}

#[cfg(test)]
mod attempt_tests {
    use super::*;

    #[test]
    fn entscheidung_wird_genau_einmal_gemeldet() {
        let plan = SpawnPlan { name_len: 5, elf_len: 6608, entry: 0x400000 };
        note_attempt(1000, plan, None);
        let first = take_unreported().expect("erste Meldung");
        assert_eq!(first.pid, 1000);
        assert!(first.error.is_none());
        assert!(take_unreported().is_none(), "zweite Abfrage darf nichts liefern");
    }

    #[test]
    fn ablehnung_traegt_ihren_grund() {
        let plan = SpawnPlan { name_len: 0, elf_len: 0, entry: 0 };
        note_attempt(1001, plan, Some(SpawnError::NotFound));
        let reported = take_unreported().expect("Meldung");
        assert_eq!(reported.error, Some(SpawnError::NotFound));
    }
}
