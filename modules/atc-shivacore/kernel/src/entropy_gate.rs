// Copyright (c) 2026 Michael Wroblewski / ShivaCore / A-TownChain-Okosystems. All Rights Reserved.
// ─────────────────────────────────────────────────────────────────────────
// DIE KERNEL-SEITE DER ENTROPIE (REQ-REC-13, REQ-REC-15, REQ-REC-16, REQ-REC-18)
//
// Bisher war der Entropiekern eine Bibliothek ohne Aufrufer im Image. Hier
// bekommt er seinen Platz im Kernel:
//
//   * `status()`  — die Frage, die die Registrierung VOR jeder Erzeugung stellt
//   * `fill()`    — Ausgabe nur im Zustand `Ready`, sonst ein Fehler
//   * `on_resume()` — Neuspeisung nach S3/S4 und nach dem Laden eines Images
//   * `status_label()` — der Text, den das Fenster zeigt statt eines leeren Felds
//
// WAS HIER NOCH FEHLT UND NICHT VORGEGEBEN WIRD: ein Auslöser. Der Kernel hat
// heute keine ACPI-Behandlung für S3/S4 und keinen Registrierungsaufruf, der
// `status()` fragt. `on_resume` ist deshalb vorhanden und geprüft, aber
// aufgerufen wird es noch von niemandem. Genauso die Quellen: `Rdrand` und
// `TscJitter` übersetzen, ihre Güte belegt in dieser Umgebung aber nichts
// (16.3: ohne KVM ist die "Hardwarequelle" ein Modell des Hypervisors).
//
// Die Zustandsführung selbst ist ohne Hardware prüfbar: `resume_into` nimmt
// einen beliebigen Kern entgegen, deshalb laufen die Tests hier auf dem Host
// mit eingespeisten Quellen.
// ─────────────────────────────────────────────────────────────────────────

use globus_entropy_core::{EntropyError, EntropyPool, EntropySource, EntropyState, SourceError};
use spin::Mutex;

/// Der eine Entropiekern des Systems.
pub static ENTROPY: Mutex<EntropyPool> = Mutex::new(EntropyPool::new());

/// Aktueller Zustand — REQ-REC-13: die Registrierung fragt das vor jeder
/// Erzeugung und bricht bei allem außer `Ready` ab.
pub fn status() -> EntropyState {
    ENTROPY.lock().state()
}

/// Kurzform für Aufrufer, die nur eine Entscheidung brauchen.
pub fn ready() -> bool {
    status() == EntropyState::Ready
}

/// Gibt Entropie heraus. Außerhalb von `Ready` gibt es keine, auch keine Nullen.
pub fn fill(out: &mut [u8]) -> Result<(), EntropyError> {
    ENTROPY.lock().fill(out)
}

/// Neuspeisung nach S3/S4 und nach dem Laden eines Images (REQ-REC-16).
///
/// Reihenfolge: verwerfen, dann Quellen lesen, dann prüfen. Bis der Selbsttest
/// bestanden ist, steht der Zustand auf `Uninitialized` beziehungsweise `Failed`.
pub fn on_resume(
    hardware: &mut impl EntropySource,
    jitter: &mut impl EntropySource,
) -> EntropyState {
    resume_into(&mut ENTROPY.lock(), hardware, jitter)
}

/// Dieselbe Abfolge auf einem übergebenen Kern — damit prüfbar ohne Hardware.
pub fn resume_into(
    pool: &mut EntropyPool,
    hardware: &mut impl EntropySource,
    jitter: &mut impl EntropySource,
) -> EntropyState {
    pool.invalidate();
    pool.reseed(hardware, jitter)
}

/// Der Text, den das Fenster zeigt (REQ-REC-18) — kein leeres Feld, kein
/// stiller Fehlschlag. Der Wortlaut stammt aus der Spezifikation.
pub const fn status_label(state: EntropyState) -> &'static str {
    match state {
        EntropyState::Ready => "Zufallsquelle bereit",
        EntropyState::Uninitialized => "Zufallsquelle nicht bereit — keine Kontoanlage möglich",
        EntropyState::Failed => "Zufallsquelle nicht bereit — keine Kontoanlage möglich",
    }
}

/// Hardwarequelle über `RDRAND`.
///
/// Nur auf x86_64 vorhanden; ohne CPU-Unterstützung meldet `fill` einen Ausfall,
/// damit der Selbsttest `Failed` setzt statt still weiterzulaufen (REQ-REC-15).
pub struct Rdrand;

impl Rdrand {
    /// Steht die Anweisung auf dieser CPU zur Verfügung (CPUID.01H:ECX.RDRAND)?
    pub fn available() -> bool {
        #[cfg(target_arch = "x86_64")]
        {
            // `__cpuid` ist in dieser Toolchain bereits sicher aufrufbar: die
            // Abfrage nach Blatt 1 und Bit 30 ist der dokumentierte Weg.
            core::arch::x86_64::__cpuid(1).ecx & (1 << 30) != 0
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            false
        }
    }
}

impl EntropySource for Rdrand {
    fn fill(&mut self, out: &mut [u8]) -> Result<(), SourceError> {
        #[cfg(target_arch = "x86_64")]
        {
            if !Self::available() {
                return Err(SourceError::Unavailable);
            }
            for chunk in out.chunks_mut(8) {
                let mut value: u64 = 0;
                // RDRAND kann laut Intel vorübergehend nichts liefern; einige
                // wenige Versuche sind zulässig, danach gilt die Quelle als
                // ausgefallen.
                let mut ok = false;
                for _ in 0..10 {
                    // SAFETY: RDRAND schreibt nur in das übergebene Register und
                    // setzt CF bei Erfolg; der Zeiger ist gültig.
                    if unsafe { core::arch::x86_64::_rdrand64_step(&mut value) } == 1 {
                        ok = true;
                        break;
                    }
                }
                if !ok {
                    return Err(SourceError::Unavailable);
                }
                let bytes = value.to_le_bytes();
                chunk.copy_from_slice(&bytes[..chunk.len()]);
            }
            Ok(())
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            let _ = out;
            Err(SourceError::Unavailable)
        }
    }
}

/// Zeitgeber-Jitter: der Steuerstand des TSC, wiederholt gelesen, in den
/// Differenzen vermischt.
///
/// Das ist die zweite Quelle, die REQ-REC-15 verlangt, damit ein Ausfall der
/// Hardwarequelle nicht stillschweigend weiterläuft. Die Güte dieses Jitters ist
/// begrenzt und wird durch den Selbsttest geprüft, nicht angenommen.
pub struct TscJitter {
    rounds: u32,
}

impl Default for TscJitter {
    fn default() -> Self {
        Self::new()
    }
}

impl TscJitter {
    /// Standardzahl an Lesungen des Zeitgeberstands.
    pub const fn new() -> Self {
        Self { rounds: 1_024 }
    }

    /// Abweichende Zahl an Lesungen (kleinere Werte nur für Prüfungen).
    pub const fn with_rounds(rounds: u32) -> Self {
        Self { rounds }
    }
}

impl EntropySource for TscJitter {
    fn fill(&mut self, out: &mut [u8]) -> Result<(), SourceError> {
        #[cfg(target_arch = "x86_64")]
        {
            // SAFETY: RDTSC ist eine reine Leseoperation des Zeitgeberstands.
            let mut previous = unsafe { core::arch::x86_64::_rdtsc() };
            let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
            for _ in 0..self.rounds {
                // SAFETY: wie oben, ohne Seiteneffekte.
                let now = unsafe { core::arch::x86_64::_rdtsc() };
                let delta = now.wrapping_sub(previous);
                previous = now;
                state ^= delta;
                state = state.rotate_left(7).wrapping_mul(0x2545_F491_4F6C_DD1D);
            }
            let mut mask = state;
            for byte in out.iter_mut() {
                *byte ^= mask as u8;
                mask = mask.rotate_right(11).wrapping_mul(0x9E37_79B9_7F4A_7C15);
            }
            Ok(())
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            let _ = out;
            Err(SourceError::Unavailable)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use globus_entropy_core::HEALTH_BYTES;

    /// Quelle mit fester Folge — der Selbsttest wird damit steuerbar.
    struct Fixed {
        byte: u8,
        vary: bool,
    }

    impl EntropySource for Fixed {
        fn fill(&mut self, out: &mut [u8]) -> Result<(), SourceError> {
            if self.vary {
                let mut value = self.byte as u64 | 1;
                for byte in out.iter_mut() {
                    value ^= value << 13;
                    value ^= value >> 7;
                    value ^= value << 17;
                    *byte = (value >> 24) as u8;
                }
            } else {
                for byte in out.iter_mut() {
                    *byte = self.byte;
                }
            }
            Ok(())
        }
    }

    fn healthy(byte: u8) -> Fixed {
        Fixed { byte, vary: true }
    }

    fn flat(byte: u8) -> Fixed {
        Fixed { byte, vary: false }
    }

    #[test]
    fn frischer_kern_ist_nicht_bereit() {
        let pool = EntropyPool::new();
        assert_eq!(pool.state(), EntropyState::Uninitialized);
        assert_eq!(
            status_label(pool.state()),
            "Zufallsquelle nicht bereit — keine Kontoanlage möglich"
        );
    }

    #[test]
    fn aufwachen_speist_neu_und_gibt_frei() {
        let mut pool = EntropyPool::new();
        assert_eq!(
            resume_into(&mut pool, &mut healthy(0x11), &mut healthy(0x22)),
            EntropyState::Ready
        );
        let mut out = [0u8; 32];
        assert_eq!(pool.fill(&mut out), Ok(()));
        assert_ne!(out, [0u8; 32]);
        assert_eq!(status_label(pool.state()), "Zufallsquelle bereit");
    }

    #[test]
    fn aufwachen_mit_toter_hardware_sperrt() {
        let mut pool = EntropyPool::new();
        // Eine konstante "Hardwarequelle" ist keine Entropie.
        assert_eq!(
            resume_into(&mut pool, &mut flat(0x00), &mut healthy(0x33)),
            EntropyState::Failed
        );
        assert_eq!(pool.fill(&mut [0u8; 32]), Err(EntropyError::NotReady));
        assert_eq!(
            status_label(pool.state()),
            "Zufallsquelle nicht bereit — keine Kontoanlage möglich"
        );
    }

    #[test]
    fn aufwachen_mit_totem_jitter_sperrt() {
        let mut pool = EntropyPool::new();
        assert_eq!(
            resume_into(&mut pool, &mut healthy(0x44), &mut flat(0xFF)),
            EntropyState::Failed
        );
        assert_eq!(pool.fill(&mut [0u8; 8]), Err(EntropyError::NotReady));
    }

    #[test]
    fn zweite_neuspeisung_laesst_den_alten_schluessel_nicht_weiterlaufen() {
        let mut pool = EntropyPool::new();
        assert_eq!(
            resume_into(&mut pool, &mut healthy(0x55), &mut healthy(0x66)),
            EntropyState::Ready
        );
        let mut first = [0u8; 32];
        pool.fill(&mut first).unwrap();

        // Aufwachen, Quelle inzwischen defekt: gesperrt, kein alter Schlüssel.
        assert_eq!(
            resume_into(&mut pool, &mut flat(0x00), &mut healthy(0x77)),
            EntropyState::Failed
        );
        assert_eq!(pool.fill(&mut [0u8; 8]), Err(EntropyError::NotReady));

        // Erneute Neuspeisung mit gesunder Quelle: wieder bereit, anderer Strom.
        assert_eq!(
            resume_into(&mut pool, &mut healthy(0x88), &mut healthy(0x99)),
            EntropyState::Ready
        );
        let mut second = [0u8; 32];
        pool.fill(&mut second).unwrap();
        assert_ne!(first, second);
    }

    #[test]
    fn quellen_stehen_bereit_oder_melden_ausfall() {
        // Aufruf der echten Quellen: entweder sie liefern, oder sie melden einen
        // Ausfall. Ein stiller Teilerfolg ist nicht zulässig.
        let mut out = [0u8; HEALTH_BYTES];
        match Rdrand.fill(&mut out) {
            Ok(()) => assert!(Rdrand::available()),
            // Ein Ausfall wird als Ausfall gemeldet, nicht als halbe Lieferung.
            Err(error) => assert!(matches!(error, SourceError::Unavailable)),
        }
    }

    #[test]
    fn tsc_jitter_liefert_oder_meldet() {
        let mut jitter = TscJitter::with_rounds(64);
        let mut out = [0u8; 64];
        match jitter.fill(&mut out) {
            Ok(()) => assert_ne!(out, [0u8; 64]),
            Err(error) => assert!(matches!(error, SourceError::Unavailable)),
        }
    }
}
