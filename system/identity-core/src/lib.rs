// Copyright (c) 2026 Michael Wroblewski / ShivaCore / A-TownChain-Okosystems. All Rights Reserved.
// ─────────────────────────────────────────────────────────────────────────
// IDENT-001 — Pruefkern fuer die Anmeldung (Entscheidung des Owners, 14.11.2026)
//
// Der Kern ist `no_std` + `alloc` und wird von Host und Image gemeinsam benutzt.
// Damit gibt es genau einen Verifizierer: was auf dem Host ein Passwort
// annimmt, nimmt das Image genauso an — und umgekehrt.
//
// Entscheidungen, die hier festgehalten sind:
//   1. KDF: Argon2id, 19 MiB, t = 2, p = 1 — und die Werte stehen IM DATENSATZ,
//      damit ein spaeterer Wechsel keine Neuausstellung braucht.
//   2. Datensatz: eigene Datei auf der Ramdisk. Der Parser ist fail-closed:
//      Laenge, Kennung, Fassung, Verfahren, Reserviertfeld und Wertebereiche
//      werden geprueft, bevor irgendetwas gerechnet wird.
//   3. Enter bestaetigt nur die Pruefung. Der Kern kennt keine Sitzung, keine
//      PID und keinen Scheduler — das ist die ABI-Seite, nicht die Anmeldung.
//
// EHRLICHE GRENZE: 19 MiB Arbeitsspeicher sind kein Nebenwert. Auf dem Host ist
// das ein Haufen, im Image braucht es dafuer einen eigenen Bereich aus dem
// Frame-Vorrat (der Kernel-Haufen ist 512 KiB). Die Messung im Image steht
// deshalb noch aus; hier ist der Kern, den sie messen wird.
// ─────────────────────────────────────────────────────────────────────────

#![cfg_attr(not(test), no_std)]

extern crate alloc;

use argon2::{Algorithm, Argon2, Params, Version};

/// Kennung des Datensatzes ("ATID").
pub const MAGIC: [u8; 4] = *b"ATID";
/// Fassung des Datensatzes.
pub const FORMAT_VERSION: u8 = 1;
/// Verfahrenskennung: 1 = Argon2id.
pub const KDF_ARGON2ID: u8 = 1;
/// Laenge des Datensatzes in Byte. Andere Laengen sind kein Datensatz.
pub const RECORD_LEN: usize = 64;
/// Laenge des Salzes.
pub const SALT_LEN: usize = 16;
/// Laenge des Hashes.
pub const HASH_LEN: usize = 32;

/// Grenzen, innerhalb derer ein Datensatz gelten darf.
///
/// Fail-closed: ein Datensatz, der mehr Speicher oder mehr Durchgaenge verlangt
/// als hier steht, wird abgelehnt, statt den Kernel in eine teure Rechnung zu
/// schicken. 19 MiB ist die Festlegung des Owners und liegt innerhalb.
pub const MIN_MEMORY_KIB: u32 = 8 * 1024;
pub const MAX_MEMORY_KIB: u32 = 32 * 1024;
pub const MIN_ITERATIONS: u32 = 1;
pub const MAX_ITERATIONS: u32 = 10;
pub const MIN_PARALLELISM: u32 = 1;
pub const MAX_PARALLELISM: u32 = 4;

/// Warum eine Anmeldung nicht bestaetigt wurde.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerifyError {
    /// Kein Datensatz (fehlende Datei) — fail-closed, keine Anmeldung.
    Missing,
    /// Falsche Laenge: abgeschnitten oder aufgefuellt.
    BadLength,
    /// Kennung stimmt nicht.
    BadMagic,
    /// Unbekannte Fassung (keine stille Deutung).
    UnsupportedVersion,
    /// Unbekanntes Verfahren.
    UnsupportedKdf,
    /// Reserviertfeld nicht null.
    ReservedNotZero,
    /// Werte ausserhalb der erlaubten Grenzen.
    ParamsOutOfRange,
    /// Salz oder Hash sind leer/ungenutzt.
    EmptySecret,
    /// Passwort passt nicht.
    WrongPassword,
    /// Die KDF konnte nicht rechnen (Speicher nicht verfuegbar).
    KdfUnavailable,
}

/// KDF-Parameter eines Datensatzes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KdfParams {
    /// Speicher in KiB.
    pub memory_kib: u32,
    /// Durchgaenge.
    pub iterations: u32,
    /// Parallelitaet.
    pub parallelism: u32,
}

impl KdfParams {
    /// Die Festlegung des Owners: Argon2id, 19 MiB, t = 2, p = 1.
    pub const OWNER_DEFAULT: KdfParams = KdfParams {
        memory_kib: 19 * 1024,
        iterations: 2,
        parallelism: 1,
    };

    /// Prueft die Grenzen (fail-closed).
    pub fn validate(&self) -> Result<(), VerifyError> {
        if !(MIN_MEMORY_KIB..=MAX_MEMORY_KIB).contains(&self.memory_kib)
            || !self.memory_kib.is_multiple_of(1024)
        {
            return Err(VerifyError::ParamsOutOfRange);
        }
        if !(MIN_ITERATIONS..=MAX_ITERATIONS).contains(&self.iterations) {
            return Err(VerifyError::ParamsOutOfRange);
        }
        if !(MIN_PARALLELISM..=MAX_PARALLELISM).contains(&self.parallelism) {
            return Err(VerifyError::ParamsOutOfRange);
        }
        Ok(())
    }
}

/// Ein geprueft gelesener Datensatz.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CredentialRecord {
    /// Parameter aus dem Datensatz selbst (nicht aus dem Programm).
    pub params: KdfParams,
    /// Salz.
    pub salt: [u8; SALT_LEN],
    /// Erwarteter Hash.
    pub hash: [u8; HASH_LEN],
}

impl CredentialRecord {
    /// Liest einen Datensatz fail-closed.
    ///
    /// Reihenfolge bindend: Laenge, Kennung, Fassung, Verfahren, Reserviertfeld,
    /// Wertebereiche, Geheimnisse. Erst danach steht ein Datensatz.
    pub fn parse(bytes: &[u8]) -> Result<Self, VerifyError> {
        if bytes.is_empty() {
            return Err(VerifyError::Missing);
        }
        if bytes.len() != RECORD_LEN {
            return Err(VerifyError::BadLength);
        }
        if bytes[0..4] != MAGIC {
            return Err(VerifyError::BadMagic);
        }
        if bytes[4] != FORMAT_VERSION {
            return Err(VerifyError::UnsupportedVersion);
        }
        if bytes[5] != KDF_ARGON2ID {
            return Err(VerifyError::UnsupportedKdf);
        }
        let memory_kib = u16::from_le_bytes([bytes[6], bytes[7]]) as u32;
        let iterations =
            u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
        let parallelism = bytes[12] as u32;
        if bytes[13..16] != [0, 0, 0] {
            return Err(VerifyError::ReservedNotZero);
        }
        let params = KdfParams {
            memory_kib,
            iterations,
            parallelism,
        };
        params.validate()?;
        let mut salt = [0u8; SALT_LEN];
        salt.copy_from_slice(&bytes[16..32]);
        if salt == [0u8; SALT_LEN] {
            return Err(VerifyError::EmptySecret);
        }
        let mut hash = [0u8; HASH_LEN];
        hash.copy_from_slice(&bytes[32..64]);
        if hash == [0u8; HASH_LEN] {
            return Err(VerifyError::EmptySecret);
        }
        Ok(Self {
            params,
            salt,
            hash,
        })
    }

    /// Prueft ein Passwort gegen diesen Datensatz.
    ///
    /// Rechnet mit den Parametern **aus dem Datensatz**. Vergleich ohne
    /// Fruehausstieg, damit die Laufzeit nicht verraet, wie weit man ist.
    pub fn verify(&self, password: &[u8]) -> Result<(), VerifyError> {
        let mut computed = [0u8; HASH_LEN];
        hash_into(self.params, password, &self.salt, &mut computed)?;
        let mut difference = 0u8;
        for (left, right) in computed.iter().zip(self.hash.iter()) {
            difference |= left ^ right;
        }
        if difference == 0 {
            Ok(())
        } else {
            Err(VerifyError::WrongPassword)
        }
    }
}

/// Rechnet einen Hash mit genau diesen Parametern.
///
/// Einziger Ort, an dem die KDF aufgerufen wird: `verify` benutzt sie, und die
/// Messung im Image benutzt dieselbe Funktion — keine zweite Rechnung.
pub fn hash_into(
    params: KdfParams,
    password: &[u8],
    salt: &[u8; SALT_LEN],
    out: &mut [u8; HASH_LEN],
) -> Result<(), VerifyError> {
    params.validate()?;
    let kdf = Params::new(
        params.memory_kib,
        params.iterations,
        params.parallelism,
        Some(HASH_LEN),
    )
    .map_err(|_| VerifyError::ParamsOutOfRange)?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, kdf);
    argon2
        .hash_password_into(password, salt, out)
        .map_err(|_| VerifyError::KdfUnavailable)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Baut einen Datensatz mit den Owner-Parametern. Der Salt ist fest, weil
    /// der Test sonst von einer Zufallsquelle abhinge.
    fn record_with(password: &[u8], params: KdfParams) -> [u8; RECORD_LEN] {
        let salt = [0x11u8; SALT_LEN];
        let argon = Argon2::new(
            Algorithm::Argon2id,
            Version::V0x13,
            Params::new(params.memory_kib, params.iterations, params.parallelism, Some(HASH_LEN))
                .expect("Parameter"),
        );
        let mut hash = [0u8; HASH_LEN];
        argon
            .hash_password_into(password, &salt, &mut hash)
            .expect("KDF");
        let mut bytes = [0u8; RECORD_LEN];
        bytes[0..4].copy_from_slice(&MAGIC);
        bytes[4] = FORMAT_VERSION;
        bytes[5] = KDF_ARGON2ID;
        bytes[6..8].copy_from_slice(&(params.memory_kib as u16).to_le_bytes());
        bytes[8..12].copy_from_slice(&params.iterations.to_le_bytes());
        bytes[12] = params.parallelism as u8;
        bytes[16..32].copy_from_slice(&salt);
        bytes[32..64].copy_from_slice(&hash);
        bytes
    }

    #[test]
    fn richtiges_passwort_wird_bestaetigt() {
        let bytes = record_with(b"korrekt-pferd-batterie", KdfParams::OWNER_DEFAULT);
        let record = CredentialRecord::parse(&bytes).expect("Datensatz");
        assert_eq!(record.params, KdfParams::OWNER_DEFAULT);
        assert_eq!(record.verify(b"korrekt-pferd-batterie"), Ok(()));
    }

    #[test]
    fn falsches_passwort_wird_abgelehnt() {
        let bytes = record_with(b"richtig", KdfParams::OWNER_DEFAULT);
        let record = CredentialRecord::parse(&bytes).expect("Datensatz");
        assert_eq!(record.verify(b"falsch"), Err(VerifyError::WrongPassword));
        assert_eq!(record.verify(b""), Err(VerifyError::WrongPassword));
    }

    #[test]
    fn fehlender_datensatz_ist_kein_datensatz() {
        assert_eq!(CredentialRecord::parse(&[]), Err(VerifyError::Missing));
    }

    #[test]
    fn abgeschnitten_oder_aufgefuellt_wird_abgelehnt() {
        let bytes = record_with(b"x", KdfParams::OWNER_DEFAULT);
        assert_eq!(
            CredentialRecord::parse(&bytes[..RECORD_LEN - 1]),
            Err(VerifyError::BadLength)
        );
        let mut long = alloc::vec::Vec::from(&bytes[..]);
        long.push(0);
        assert_eq!(CredentialRecord::parse(&long), Err(VerifyError::BadLength));
    }

    #[test]
    fn fremde_kennung_fremde_fassung_fremdes_verfahren() {
        let mut bytes = record_with(b"x", KdfParams::OWNER_DEFAULT);
        bytes[0] = b'X';
        assert_eq!(CredentialRecord::parse(&bytes), Err(VerifyError::BadMagic));

        let mut bytes = record_with(b"x", KdfParams::OWNER_DEFAULT);
        bytes[4] = FORMAT_VERSION + 1;
        assert_eq!(
            CredentialRecord::parse(&bytes),
            Err(VerifyError::UnsupportedVersion)
        );

        let mut bytes = record_with(b"x", KdfParams::OWNER_DEFAULT);
        bytes[5] = 9;
        assert_eq!(
            CredentialRecord::parse(&bytes),
            Err(VerifyError::UnsupportedKdf)
        );
    }

    #[test]
    fn manipulierte_parameter_werden_abgelehnt() {
        let mut bytes = record_with(b"x", KdfParams::OWNER_DEFAULT);
        bytes[13] = 1; // Reserviertfeld
        assert_eq!(
            CredentialRecord::parse(&bytes),
            Err(VerifyError::ReservedNotZero)
        );

        // 40 MiB: ausserhalb der Grenze, fail-closed statt teurer Rechnung.
        // (Das Feld ist u16 KiB, also bis 64 MiB — 40 MiB passen hinein und
        // liegen ueber MAX_MEMORY_KIB.)
        let mut bytes = record_with(b"x", KdfParams::OWNER_DEFAULT);
        bytes[6..8].copy_from_slice(&(40u32 * 1024).to_le_bytes()[0..2]);
        assert_eq!(
            CredentialRecord::parse(&bytes),
            Err(VerifyError::ParamsOutOfRange)
        );

        // 0 Durchgaenge.
        let mut bytes = record_with(b"x", KdfParams::OWNER_DEFAULT);
        bytes[8..12].copy_from_slice(&0u32.to_le_bytes());
        assert_eq!(
            CredentialRecord::parse(&bytes),
            Err(VerifyError::ParamsOutOfRange)
        );
    }

    #[test]
    fn leeres_salz_oder_hash_wird_abgelehnt() {
        let mut bytes = record_with(b"x", KdfParams::OWNER_DEFAULT);
        bytes[16..32].copy_from_slice(&[0u8; SALT_LEN]);
        assert_eq!(
            CredentialRecord::parse(&bytes),
            Err(VerifyError::EmptySecret)
        );

        let mut bytes = record_with(b"x", KdfParams::OWNER_DEFAULT);
        bytes[32..64].copy_from_slice(&[0u8; HASH_LEN]);
        assert_eq!(
            CredentialRecord::parse(&bytes),
            Err(VerifyError::EmptySecret)
        );
    }

    #[test]
    fn der_salt_aus_dem_datensatz_wird_benutzt() {
        // Zwei Datensaetze mit verschiedenem Salt: derselbe Hash darf nicht
        // passen, sonst waere der Salt nur Dekoration.
        let bytes = record_with(b"gleich", KdfParams::OWNER_DEFAULT);
        let mut record = CredentialRecord::parse(&bytes).expect("Datensatz");
        record.salt[0] ^= 0xFF;
        assert_eq!(record.verify(b"gleich"), Err(VerifyError::WrongPassword));
    }

    #[test]
    fn schwaechere_parameter_im_datensatz_werden_benutzt() {
        // Ein Datensatz mit anderen (aber erlaubten) Parametern bleibt gueltig:
        // die Werte kommen aus dem Datensatz, nicht aus dem Programm.
        let params = KdfParams {
            memory_kib: 8 * 1024,
            iterations: 3,
            parallelism: 1,
        };
        let bytes = record_with(b"alt", params);
        let record = CredentialRecord::parse(&bytes).expect("Datensatz");
        assert_eq!(record.params, params);
        assert_eq!(record.verify(b"alt"), Ok(()));
    }
}
