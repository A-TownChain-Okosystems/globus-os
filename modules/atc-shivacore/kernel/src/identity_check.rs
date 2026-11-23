// Copyright (c) 2026 Michael Wroblewski / ShivaCore / A-TownChain-Okosystems. All Rights Reserved.
// ─────────────────────────────────────────────────────────────────────────
// IDENT-003 — das Prueftor des Anmeldefensters
//
// Der Datensatz gehoert dem Kernel (Ramdisk, `sys.identity`), das Passwort
// kommt aus dem Fenster. Geprueft wird hier: eine Stelle fuer die Frage
// "stimmt dieses Passwort", damit Fenster und Kernel nicht zwei Wahrheiten
// haben.
//
// Der Datensatz wird beim Boot einmal gelesen und hier abgelegt. Fehlt er,
// antwortet das Tor mit `NoRecord` und nicht mit "falsch" — der Unterschied
// ist wichtig: ein fehlender Datensatz ist ein Systemfehler, ein falsches
// Passwort eine Eingabe.
// ─────────────────────────────────────────────────────────────────────────

use globus_identity_core::{CredentialRecord, VerifyError};

/// Ergebnis einer Pruefung.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckOutcome {
    /// Passwort passt.
    Ok,
    /// Passwort passt nicht.
    WrongPassword,
    /// Kein Datensatz vorhanden (fail-closed).
    NoRecord,
    /// Die KDF konnte nicht rechnen (Speicher/Parameter).
    Unavailable,
}

impl CheckOutcome {
    /// Wert, den der Syscall an das Fenster zurueckgibt.
    pub fn code(self) -> u64 {
        match self {
            CheckOutcome::Ok => 0,
            CheckOutcome::WrongPassword => 1,
            CheckOutcome::NoRecord => 2,
            CheckOutcome::Unavailable => 4,
        }
    }
}

/// Resolve the canonical username from the validated multi-account catalog and
/// verify only that account's credential. The catalog lock is released before Argon2id.
pub fn check_catalog(username: &str, password: &[u8]) -> CheckOutcome {
    let Some(account) = crate::identity_catalog::account_for_username(username) else {
        // Keep unknown-user timing close to a real password verification. The
        // derived output is discarded and can never authenticate an account.
        let mut dummy_hash = [0u8; globus_identity_core::HASH_LEN];
        let dummy_salt = [0xA5u8; globus_identity_core::SALT_LEN];
        let result = globus_identity_core::hash_into(
            globus_identity_core::KdfParams::OWNER_DEFAULT,
            password,
            &dummy_salt,
            &mut dummy_hash,
        );
        dummy_hash.fill(0);
        return if result.is_ok() {
            CheckOutcome::WrongPassword
        } else {
            CheckOutcome::Unavailable
        };
    };
    match account.credential.verify(password) {
        Ok(()) => CheckOutcome::Ok,
        Err(VerifyError::WrongPassword) => CheckOutcome::WrongPassword,
        Err(VerifyError::KdfUnavailable) => CheckOutcome::Unavailable,
        Err(_) => CheckOutcome::Unavailable,
    }
}

/// Haelt den Datensatz und prueft gegen ihn.
#[derive(Debug, Default)]
pub struct IdentityGate {
    record: Option<CredentialRecord>,
}

impl IdentityGate {
    /// Leeres Tor: ohne Datensatz gibt es keine Anmeldung.
    pub const fn empty() -> Self {
        Self { record: None }
    }

    /// Uebernimmt den beim Boot gelesenen Datensatz.
    pub fn publish(&mut self, record: CredentialRecord) {
        self.record = Some(record);
    }

    /// Steht ein Datensatz bereit?
    pub fn has_record(&self) -> bool {
        self.record.is_some()
    }

    /// Prueft ein Passwort.
    pub fn check(&self, password: &[u8]) -> CheckOutcome {
        let Some(record) = self.record.as_ref() else {
            return CheckOutcome::NoRecord;
        };
        match record.verify(password) {
            Ok(()) => CheckOutcome::Ok,
            Err(VerifyError::WrongPassword) => CheckOutcome::WrongPassword,
            Err(_) => CheckOutcome::Unavailable,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use globus_identity_core::{HASH_LEN, KdfParams, MAGIC, RECORD_LEN, SALT_LEN, hash_into};

    fn record_for(password: &[u8]) -> CredentialRecord {
        let params = KdfParams::OWNER_DEFAULT;
        let salt = [0x2Fu8; SALT_LEN];
        let mut hash = [0u8; HASH_LEN];
        hash_into(params, password, &salt, &mut hash).expect("KDF");
        let mut bytes = [0u8; RECORD_LEN];
        bytes[0..4].copy_from_slice(&MAGIC);
        bytes[4] = 1;
        bytes[5] = 1;
        bytes[6..8].copy_from_slice(&(params.memory_kib as u16).to_le_bytes());
        bytes[8..12].copy_from_slice(&params.iterations.to_le_bytes());
        bytes[12] = params.parallelism as u8;
        bytes[16..32].copy_from_slice(&salt);
        bytes[32..64].copy_from_slice(&hash);
        CredentialRecord::parse(&bytes).expect("Datensatz")
    }

    #[test]
    fn ohne_datensatz_gibt_es_keine_anmeldung() {
        let gate = IdentityGate::empty();
        assert_eq!(gate.check(b"egal"), CheckOutcome::NoRecord);
        assert!(!gate.has_record());
        assert_eq!(CheckOutcome::NoRecord.code(), 2);
    }

    #[test]
    fn richtiges_passwort_wird_bestaetigt() {
        let mut gate = IdentityGate::empty();
        gate.publish(record_for(b"richtig"));
        assert!(gate.has_record());
        assert_eq!(gate.check(b"richtig"), CheckOutcome::Ok);
        assert_eq!(CheckOutcome::Ok.code(), 0);
    }

    #[test]
    fn falsches_passwort_wird_abgelehnt() {
        let mut gate = IdentityGate::empty();
        gate.publish(record_for(b"richtig"));
        assert_eq!(gate.check(b"falsch"), CheckOutcome::WrongPassword);
        assert_eq!(gate.check(b""), CheckOutcome::WrongPassword);
        assert_eq!(CheckOutcome::WrongPassword.code(), 1);
    }
}
