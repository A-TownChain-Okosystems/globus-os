// Copyright (c) 2026 Michael Wroblewski / ShivaCore / A-TownChain-Okosystems. All Rights Reserved.
// ─────────────────────────────────────────────────────────────────────────
// IDENT-002 — Anmeldedatensatz aus der Ramdisk (Owner-Entscheidung 14.11.2026)
//
// Der Datensatz ist eine eigene Datei, kein Teil des Programms: eine
// Passwortaenderung ist damit kein neuer Kernel-Build. Gelesen wird er nur vom
// Kernel (`Reader::Kernel`), der Eintrag liegt im Namensraum `sys.` und ist
// fuer Dienste und Programme nicht vorhanden.
//
// Fail-closed heisst hier konkret: fehlt der Eintrag, ist er nicht lesbar oder
// ist er kein gueltiger Datensatz, gibt es eine Anmeldung — und einen Fehler,
// der das sagt. Es wird nie ein Ersatzdatensatz erfunden und nie ein leerer
// Hash akzeptiert.
// ─────────────────────────────────────────────────────────────────────────

use crate::ramdisk::{self, Entry, Reader};
use globus_identity_core::{CredentialRecord, VerifyError};

/// Name des Anmeldedatensatzes in der Ramdisk.
///
/// `sys.` ist der Systemnamensraum (nur der Kernel liest ihn); der Name passt
/// in die 16 Byte eines Verzeichniseintrags.
pub const CREDENTIAL_ENTRY: &str = "sys.identity";

/// Warum kein Datensatz bereitsteht.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialError {
    /// Kein Eintrag dieses Namens (oder nicht sichtbar).
    Missing,
    /// Der Eintrag passt nicht in das Abbild (Laenge/Offset).
    Unreadable,
    /// Der Inhalt ist kein gueltiger Datensatz.
    Invalid(VerifyError),
}

/// Liest den Datensatz aus dem Ramdisk-Verzeichnis.
pub fn load(
    entries: &[Entry],
    image: &[u8],
) -> Result<CredentialRecord, CredentialError> {
    let entry = ramdisk::lookup(entries, CREDENTIAL_ENTRY, Reader::Kernel)
        .ok_or(CredentialError::Missing)?;
    let bytes = entry.data(image).ok_or(CredentialError::Unreadable)?;
    CredentialRecord::parse(bytes).map_err(CredentialError::Invalid)
}

#[cfg(test)]
mod tests {
    use super::*;
    use globus_identity_core::{KdfParams, MAGIC, RECORD_LEN, SALT_LEN};

    /// Datensatz mit den Owner-Parametern. Der Hash ist hier beliebig: gelesen
    /// wird der Datensatz, nicht geprueft.
    fn record_bytes() -> [u8; RECORD_LEN] {
        let params = KdfParams::OWNER_DEFAULT;
        let mut bytes = [0u8; RECORD_LEN];
        bytes[0..4].copy_from_slice(&MAGIC);
        bytes[4] = 1;
        bytes[5] = 1;
        bytes[6..8].copy_from_slice(&(params.memory_kib as u16).to_le_bytes());
        bytes[8..12].copy_from_slice(&params.iterations.to_le_bytes());
        bytes[12] = params.parallelism as u8;
        bytes[16..32].copy_from_slice(&[0x77u8; SALT_LEN]);
        bytes[32..64].copy_from_slice(&[0x33u8; 32]);
        bytes
    }

    #[test]
    fn gueltiger_datensatz_wird_gelesen() {
        let record = record_bytes();
        let image = ramdisk::build(&[("init", &[0xF4]), (CREDENTIAL_ENTRY, &record)])
            .expect("Abbild");
        let entries = ramdisk::parse(&image).expect("Verzeichnis");
        let loaded = load(&entries, &image).expect("Datensatz");
        assert_eq!(loaded.params, KdfParams::OWNER_DEFAULT);
        assert_eq!(loaded.salt, [0x77u8; SALT_LEN]);
    }

    #[test]
    fn fehlender_datensatz_ist_fail_closed() {
        let image = ramdisk::build(&[("init", &[0xF4])]).expect("Abbild");
        let entries = ramdisk::parse(&image).expect("Verzeichnis");
        assert_eq!(load(&entries, &image), Err(CredentialError::Missing));
    }

    #[test]
    fn manipulierte_bytes_werden_abgelehnt() {
        let mut record = record_bytes();
        record[4] = 9; // unbekannte Fassung
        let image = ramdisk::build(&[(CREDENTIAL_ENTRY, &record)]).expect("Abbild");
        let entries = ramdisk::parse(&image).expect("Verzeichnis");
        assert_eq!(
            load(&entries, &image),
            Err(CredentialError::Invalid(VerifyError::UnsupportedVersion))
        );
    }

    #[test]
    fn abgeschnittener_eintrag_ist_kein_datensatz() {
        let record = &record_bytes()[..RECORD_LEN - 1];
        let image = ramdisk::build(&[(CREDENTIAL_ENTRY, record)]).expect("Abbild");
        let entries = ramdisk::parse(&image).expect("Verzeichnis");
        assert_eq!(
            load(&entries, &image),
            Err(CredentialError::Invalid(VerifyError::BadLength))
        );
    }

    #[test]
    fn ein_programm_sieht_den_datensatz_nicht() {
        let record = record_bytes();
        let image = ramdisk::build(&[("init", &[0xF4]), (CREDENTIAL_ENTRY, &record)])
            .expect("Abbild");
        let entries = ramdisk::parse(&image).expect("Verzeichnis");
        // Der Kernel liest ihn, ein Programm nicht — fuer das Programm ist er
        // nicht vorhanden, nicht "verweigert".
        assert!(ramdisk::lookup(&entries, CREDENTIAL_ENTRY, Reader::Kernel).is_some());
        assert!(ramdisk::lookup(&entries, CREDENTIAL_ENTRY, Reader::Program).is_none());
    }
}
