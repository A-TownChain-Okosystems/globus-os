// Copyright (c) 2026 Michael Wroblewski / ShivaCore / A-TownChain-Okosystems. All Rights Reserved.
// ─────────────────────────────────────────────────────────────────────────
// ENTROPY-GATE — der Durchsetzungspunkt von REQ-REC-13
//
// Die Spezifikation (docs/identity/SPEC-RECOVERY-001.md, 16.2 und 16.4) verlangt
// zweierlei, und das zweite fehlte bisher:
//
//   1. einen Entropiekern mit Zustandsmaschine  — steht in `globus-entropy-core`
//   2. einen Punkt, der ihn BEVOR der Phrase fragt und bei allem ausser `Ready`
//      abbricht                                   — steht hier
//
// Ohne diese Datei waere der Kern eine Bibliothek ohne Aufrufer: er koennte
// nicht verhindern, dass im Image eine Phrase aus einer ungeprueften Quelle
// entsteht. Die Reihenfolge ist deshalb bindend: Zustand pruefen, dann
// Entropie holen, dann Phrase erzeugen. Ein Fehlschlag endet mit
// `EntropyUnavailable` und ohne Phrase.
//
// GRENZE, die dazugehoert: Auf dem Host sind beide Quellen zwei unabhaengige
// Zuege aus dem Betriebssystem-Zufall (`OsRng`). Die Trennung in Hardware und
// Jitter ist fuer das Image gebaut, wo RDRAND/RDSEED und der Zeitgebersteuerstand
// getrennt anliegen. Ein gruener Host-Lauf belegt also die Zustandsmaschine und
// die Verdrahtung, nicht die Guete einer Hardwarequelle (16.3).
// ─────────────────────────────────────────────────────────────────────────

use argon2::password_hash::rand_core::{OsRng, RngCore};
use globus_entropy_core::{EntropyPool, EntropySource, EntropyState, SourceError};

use crate::IdentityError;

/// Entropiequelle auf Betriebssystem-Ebene.
///
/// Im Image tritt an ihre Stelle eine Quelle mit RDRAND/RDSEED beziehungsweise
/// Zeitgeber-Jitter. Der Vertrag ist derselbe: entweder vollstaendig fuellen
/// oder einen Fehler melden, nie eine halbe Lieferung.
pub struct OsEntropy;

impl EntropySource for OsEntropy {
    fn fill(&mut self, out: &mut [u8]) -> Result<(), SourceError> {
        OsRng
            .try_fill_bytes(out)
            .map_err(|_| SourceError::Unavailable)
    }
}

/// Durchsetzungspunkt: haelt den Entropiekern und gibt nur im Zustand `Ready`
/// Material heraus.
pub struct EntropyGate {
    pool: EntropyPool,
}

impl Default for EntropyGate {
    fn default() -> Self {
        Self::new()
    }
}

impl EntropyGate {
    /// Tor ohne geprueftes Material (`Uninitialized`).
    pub const fn new() -> Self {
        Self {
            pool: EntropyPool::new(),
        }
    }

    /// Tor, das sofort aus zwei Betriebssystem-Zuegen gespeist wird.
    pub fn from_host() -> Self {
        let mut gate = Self::new();
        let mut hardware = OsEntropy;
        let mut jitter = OsEntropy;
        gate.initialize(&mut hardware, &mut jitter);
        gate
    }

    /// Speist und prueft das Tor. Ergebnis ist der neue Zustand.
    pub fn initialize(
        &mut self,
        hardware: &mut impl EntropySource,
        jitter: &mut impl EntropySource,
    ) -> EntropyState {
        self.pool.init(hardware, jitter)
    }

    /// Aktueller Zustand — die Registrierung fragt ihn vor jeder Erzeugung.
    pub const fn status(&self) -> EntropyState {
        self.pool.state()
    }

    /// Holt Material. Ausserhalb von `Ready` gibt es keins, auch keine Nullen.
    pub fn fill(&mut self, out: &mut [u8]) -> Result<(), IdentityError> {
        self.pool
            .fill(out)
            .map_err(|_| IdentityError::EntropyUnavailable)
    }

    /// Wahr, wenn die Ausgabemenge dieser Speisung erschoepft ist.
    pub const fn needs_reseed(&self) -> bool {
        self.pool.needs_reseed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use globus_entropy_core::{EntropyFault, HEALTH_BYTES};

    /// Quelle mit fester Folge, damit der Selbsttest steuerbar ist.
    struct Fixed {
        data: Vec<u8>,
    }

    impl EntropySource for Fixed {
        fn fill(&mut self, out: &mut [u8]) -> Result<(), SourceError> {
            if self.data.len() < out.len() {
                return Err(SourceError::Short);
            }
            out.copy_from_slice(&self.data[..out.len()]);
            Ok(())
        }
    }

    fn stream(seed: u64, len: usize) -> Vec<u8> {
        let mut value = if seed == 0 {
            0x9E37_79B9_7F4A_7C15
        } else {
            seed
        };
        let mut out = Vec::with_capacity(len);
        for _ in 0..len {
            value ^= value << 13;
            value ^= value >> 7;
            value ^= value << 17;
            out.push((value >> 24) as u8);
        }
        out
    }

    fn healthy(seed: u64) -> Fixed {
        Fixed {
            data: stream(seed, HEALTH_BYTES),
        }
    }

    #[test]
    fn uninitialisiertes_tor_gibt_kein_material() {
        let mut gate = EntropyGate::new();
        assert_eq!(gate.status(), EntropyState::Uninitialized);
        let mut out = [0xAAu8; 32];
        assert_eq!(gate.fill(&mut out), Err(IdentityError::EntropyUnavailable));
        assert_eq!(out, [0xAAu8; 32], "kein Byte darf geschrieben werden");
    }

    #[test]
    fn gesunde_quellen_ergeben_ready_und_material() {
        let mut gate = EntropyGate::new();
        assert_eq!(
            gate.initialize(&mut healthy(1), &mut healthy(2)),
            EntropyState::Ready
        );
        let mut out = [0u8; 32];
        assert_eq!(gate.fill(&mut out), Ok(()));
        assert_ne!(out, [0u8; 32]);
    }

    #[test]
    fn konstante_quelle_verweigert_das_tor() {
        let mut gate = EntropyGate::new();
        let mut flat = Fixed {
            data: vec![0u8; HEALTH_BYTES],
        };
        assert_eq!(
            gate.initialize(&mut healthy(3), &mut flat),
            EntropyState::Failed
        );
        assert_eq!(gate.status(), EntropyState::Failed);
        let mut out = [0u8; 32];
        assert_eq!(gate.fill(&mut out), Err(IdentityError::EntropyUnavailable));
    }

    #[test]
    fn nicht_verfuegbare_quelle_verweigert_das_tor() {
        struct Dead;
        impl EntropySource for Dead {
            fn fill(&mut self, _out: &mut [u8]) -> Result<(), SourceError> {
                Err(SourceError::Unavailable)
            }
        }
        let mut gate = EntropyGate::new();
        assert_eq!(
            gate.initialize(&mut Dead, &mut healthy(4)),
            EntropyState::Failed
        );
        assert_eq!(
            gate.fill(&mut [0u8; 8]),
            Err(IdentityError::EntropyUnavailable)
        );
    }

    #[test]
    fn host_tor_ist_ready_und_bleibt_nach_verbrauch_ready() {
        let mut gate = EntropyGate::from_host();
        assert_eq!(gate.status(), EntropyState::Ready);
        let mut first = [0u8; 32];
        let mut second = [0u8; 32];
        gate.fill(&mut first).unwrap();
        gate.fill(&mut second).unwrap();
        assert_ne!(first, second, "zwei Zuege duerfen nicht gleich sein");
        assert_eq!(gate.status(), EntropyState::Ready);
        assert!(!gate.needs_reseed());
    }

    #[test]
    fn fehlerbefund_steht_im_kern_und_nicht_im_tor() {
        // Das Tor meldet nur den Zustand; den Grund traegt der Kern, damit eine
        // Oberflaeche spaeter "Zufallsquelle nicht bereit" anzeigen kann.
        let mut pool = EntropyPool::new();
        let mut flat = Fixed {
            data: vec![0x5Au8; HEALTH_BYTES],
        };
        assert_eq!(pool.init(&mut healthy(5), &mut flat), EntropyState::Failed);
        assert!(matches!(pool.fault(), Some(EntropyFault::Health(_))));
    }
}
