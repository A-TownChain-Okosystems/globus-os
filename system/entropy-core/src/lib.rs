// Copyright (c) 2026 Michael Wroblewski / ShivaCore / A-TownChain-Okosystems. All Rights Reserved.
// ─────────────────────────────────────────────────────────────────────────
// ENTROPY-001 — gemeinsamer Entropiekern fuer Host und Image (R-1)
//
// Bezug: `docs/identity/SPEC-RECOVERY-001.md`, Abschnitt 16, REQ-REC-13 bis
// REQ-REC-18. Der Kern ist `no_std` und ohne `unsafe`, damit er im Kernel und im
// Host-Build derselbe ist. Er trifft eine Architekturentscheidung, die hier
// begruendet steht, weil sie sonst in jeder Datei neu diskutiert wird:
//
//   Die Entropie-Einheit liegt NICHT kernelintern, sondern als eigener
//   `no_std`-Baustein in `system/` und wird vom Kernel und von `identity`
//   gemeinsam benutzt.
//
// Begruendung, technisch und nicht aesthetisch:
//
//   1. Es gibt heute schon zwei Nutzer mit gegensaetzlicher Umgebung: der Kernel
//      (kein `std`, RDRAND/RDSEED und Zeitgeber-Jitter) und `system/identity`
//      (Host, `OsRng`). Eine kernelinterne Einheit muesste im Host nachgebaut
//      werden — zwei Wahrheiten ueber dieselbe Zustandsmaschine, genau der
//      Fehler, den die Architektur-Grundregel verbietet.
//   2. Das Repository hat fuer diese Lage bereits ein Vorbild: `globus-identity-core`
//      ist `no_std` + `alloc` und wird vom Kernel (`modules/atc-shivacore/kernel`,
//      Abhaengigkeit `globus-identity-core`) und von `system/identity` zusammen
//      benutzt. Der Kernel haengt schon an einem Baustein aus `system/`.
//   3. Die Abhaengigkeitsrichtung bleibt abwaerts: `system/entropy-core` haengt
//      an nichts aus `modules/`. Der Kernel zeigt auf `system/`, nie umgekehrt.
//   4. Die Logik ist deterministisch pruefbar, wenn die Quelle einspeisbar ist.
//      Kernelinterner Code ist nur ueber QEMU zu sehen; dieser Kern wird gegen
//      gesteuerte Quellen getestet (konstant, festhaengendes Bit, Wiederholung),
//      was die Anforderungen REC-T-41 bis REC-T-45 verlangen.
//
// Was hier bewusst NICHT passiert:
//   - Keine Hardware. Der Zugriff auf RDRAND/RDSEED und den Zeitgebersteuerstand
//     kommt ueber `EntropySource` von aussen. Der Kernel implementiert ihn mit
//     `unsafe` an genau einer Stelle, dieser Kern bleibt frei davon.
//   - Keine Zwischenspeicherung von Entropie, keine Protokollierung. Quelle,
//     Mischmaterial und Schluessel werden nach Gebrauch genullt.
//   - Keine Aussage ueber die Guete einer Quelle. Ein bestandener Selbsttest ist
//     eine Untergrenze, kein Beweis (16.3 der Spezifikation).
//
// EHRLICHE GRENZE: Dieser Kern hat heute KEINEN Aufrufer im Produktivpfad.
// `system/identity` erzeugt Phrasen weiterhin aus `OsRng`, der Kernel hat keinen
// Registrierungspfad. Der Durchsetzungspunkt (Registrierung fragt
// `state()` und bricht bei allem ausser `Ready` ab) ist der naechste Schritt;
// ohne ihn verhindert der Kern so wenig wie eine Bibliothek ohne Aufrufer.
// ─────────────────────────────────────────────────────────────────────────

//! Gemeinsamer Entropiekern fuer Kernel und Host (SPEC-RECOVERY-001 §16, R-1).
//!
//! Der Kern ist `no_std` und ohne `unsafe`: er traegt die Zustandsmaschine
//! `Uninitialized`/`Ready`/`Failed`, den Selbsttest nach REQ-REC-14 und einen
//! ChaCha20-DRBG, dessen Schluessel aus Hardware- und Jitterquelle gemischt
//! wird. Der Hardwarezugriff kommt ausschliesslich ueber [`EntropySource`],
//! damit derselbe Code im Kernel und im Host-Build laeuft.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

use chacha20::ChaCha20;
use chacha20::cipher::{KeyIvInit, StreamCipher};
use zeroize::Zeroize;

/// Anzahl Bytes, die eine Quelle fuer den Selbsttest liefern muss (REQ-REC-14).
pub const HEALTH_BYTES: usize = 4096;

/// Kleinste Anzahl verschiedener Bytewerte im Selbsttest (REQ-REC-14 c).
pub const MIN_DISTINCT_BYTES: usize = 64;

/// Laenge des Schluesselmaterials, das aus einer Quelle uebernommen wird.
pub const TAIL_LEN: usize = 32;

/// Groesste Ausgabemenge einer Initialisierung, danach ist Neuspeisung faellig.
///
/// 1 MiB ist eine Festlegung, keine Herstellerangabe: sie ist klein genug, dass
/// ein Neuspeisen billig bleibt, und gross genug, dass eine Phrase (32 Byte) sie
/// nie erreicht.
pub const RESEED_INTERVAL_BYTES: u64 = 1024 * 1024;

/// Laenge eines Pruefblocks im Selbsttest.
const BLOCK_LEN: usize = 32;

/// Groesse des Stapelpuffers fuer eine Quellenabfrage.
const CHUNK_LEN: usize = 256;

/// Zustand der Entropiequelle (16.1 der Spezifikation, REQ-REC-13).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntropyState {
    /// Quelle noch nicht geprueft — keine Phrase, kein Konto.
    Uninitialized,
    /// Selbsttest bestanden, Ausgabe mischt Hardware und Jitter.
    Ready,
    /// Quelle liefert nichts Brauchbares oder widerspruechlich — keine Phrase.
    Failed,
}

/// Warum eine Quelle nicht verwendet wird.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthFault {
    /// Weniger als [`HEALTH_BYTES`] geliefert.
    TooShort {
        /// Tatsaechlich gelieferte Bytes.
        got: usize,
    },
    /// Alle Bytes gleich — die Quelle liefert eine Konstante.
    Constant,
    /// Eine Bitposition ist nie 0 oder nie 1.
    StuckBit {
        /// Betroffene Bitposition, 0 = niedrigstes Bit.
        bit: u8,
    },
    /// Zu wenige verschiedene Bytewerte.
    TooFewDistinct {
        /// Gezaehlte verschiedene Bytewerte.
        seen: usize,
    },
    /// Zwei aufeinanderfolgende Pruefbloecke sind gleich.
    RepeatedBlock,
}

/// Fehler beim Einlesen einer Quelle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceError {
    /// Quelle nicht verfuegbar (im Image: kein RDRAND/RDSEED, kein Jitter).
    Unavailable,
    /// Quelle lieferte weniger Bytes als angefordert.
    Short,
}

/// Eine einspeisbare Entropiequelle.
///
/// Der Kernel implementiert sie mit RDRAND/RDSEED und Zeitgeber-Jitter, der Host
/// mit `OsRng`. Alles, was hier entschieden wird, gilt fuer beide gleich.
pub trait EntropySource {
    /// Fuellt `out` vollstaendig oder meldet einen Fehler. Teilmengen sind
    /// ausdruecklich kein Erfolg: eine halb gefuellte Anfrage ist ein Fehler.
    fn fill(&mut self, out: &mut [u8]) -> Result<(), SourceError>;
}

/// Grund, aus dem der Kern nicht liefert.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntropyError {
    /// Zustand ist nicht `Ready` — fail-closed, kein Byte wird geschrieben.
    NotReady,
    /// Ausgabemenge einer Initialisierung erschoepft, Neuspeisung noetig.
    ReseedingRequired,
}

/// Letzter Befund, damit eine Oberflaeche nicht raten muss (REQ-REC-18).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntropyFault {
    /// Quelle hat den Dienst verweigert.
    Source(SourceError),
    /// Quelle hat geliefert, aber den Selbsttest nicht bestanden.
    Health(HealthFault),
}

/// Laufender Selbsttest ueber eine Folge von Bytes (REQ-REC-14).
///
/// Der Pruefer arbeitet stueckweise, damit im Kernel kein 4-KiB-Stapelpuffer
/// noetig ist. Er merkt sich nur, was er fuer das Urteil braucht: gesehene
/// Bitwerte, gesehene Bytewerte und den letzten Pruefblock.
pub struct HealthAccumulator {
    total: usize,
    bit_one: [bool; 8],
    bit_zero: [bool; 8],
    distinct: [bool; 256],
    distinct_count: usize,
    block: [u8; BLOCK_LEN],
    block_fill: usize,
    previous: [u8; BLOCK_LEN],
    has_previous: bool,
    repeated_blocks: usize,
}

impl Default for HealthAccumulator {
    fn default() -> Self {
        Self::new()
    }
}

impl HealthAccumulator {
    /// Leerer Pruefer.
    pub const fn new() -> Self {
        Self {
            total: 0,
            bit_one: [false; 8],
            bit_zero: [false; 8],
            distinct: [false; 256],
            distinct_count: 0,
            block: [0; BLOCK_LEN],
            block_fill: 0,
            previous: [0; BLOCK_LEN],
            has_previous: false,
            repeated_blocks: 0,
        }
    }

    /// Nimmt weitere Bytes auf.
    pub fn push(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.total = self.total.saturating_add(1);
            for bit in 0..8 {
                if byte & (1 << bit) != 0 {
                    self.bit_one[bit] = true;
                } else {
                    self.bit_zero[bit] = true;
                }
            }
            let index = byte as usize;
            if !self.distinct[index] {
                self.distinct[index] = true;
                self.distinct_count = self.distinct_count.saturating_add(1);
            }
            self.block[self.block_fill] = byte;
            self.block_fill += 1;
            if self.block_fill == BLOCK_LEN {
                self.close_block();
            }
        }
    }

    fn close_block(&mut self) {
        if self.has_previous && self.block == self.previous {
            self.repeated_blocks = self.repeated_blocks.saturating_add(1);
        }
        self.previous = self.block;
        self.has_previous = true;
        self.block_fill = 0;
    }

    /// Bisher aufgenommene Bytes.
    pub const fn total(&self) -> usize {
        self.total
    }

    /// Urteil ueber die bisher aufgenommene Folge.
    ///
    /// Reihenfolge nach Aussagekraft: Laenge, Wiederholung, festhaengende Bits,
    /// Vielfalt. `Constant` ist ein Sonderfall von `StuckBit` und wird deshalb
    /// zuerst genannt, weil er der haeufigste Fehler emulierter Quellen ist.
    pub fn verdict(&self) -> Result<(), HealthFault> {
        if self.total < HEALTH_BYTES {
            return Err(HealthFault::TooShort { got: self.total });
        }
        if self.distinct_count == 1 {
            return Err(HealthFault::Constant);
        }
        if self.repeated_blocks > 0 {
            return Err(HealthFault::RepeatedBlock);
        }
        for bit in 0..8 {
            if !self.bit_one[bit] || !self.bit_zero[bit] {
                return Err(HealthFault::StuckBit { bit: bit as u8 });
            }
        }
        if self.distinct_count < MIN_DISTINCT_BYTES {
            return Err(HealthFault::TooFewDistinct {
                seen: self.distinct_count,
            });
        }
        Ok(())
    }
}

/// Entropiekern: Zustandsmaschine, Selbsttest und DRBG in einem.
pub struct EntropyPool {
    state: EntropyState,
    cipher: Option<ChaCha20>,
    produced: u64,
    generation: u64,
    fault: Option<EntropyFault>,
}

impl Default for EntropyPool {
    fn default() -> Self {
        Self::new()
    }
}

impl EntropyPool {
    /// Kern im Zustand `Uninitialized` — es gibt noch keine Ausgabe.
    pub const fn new() -> Self {
        Self {
            state: EntropyState::Uninitialized,
            cipher: None,
            produced: 0,
            generation: 0,
            fault: None,
        }
    }

    /// Aktueller Zustand (REQ-REC-13: die Registrierung fragt das hier).
    pub const fn state(&self) -> EntropyState {
        self.state
    }

    /// Letzter Befund, falls der Zustand `Failed` ist.
    pub const fn fault(&self) -> Option<EntropyFault> {
        self.fault
    }

    /// Wahr, wenn die Ausgabemenge dieser Initialisierung erschoepft ist.
    pub const fn needs_reseed(&self) -> bool {
        self.produced >= RESEED_INTERVAL_BYTES
    }

    /// Zaehler der Speisungen. Jede Speisung bekommt einen eigenen Nonce, damit
    /// zwei Speisungen nie denselben Schluesselstrom erzeugen.
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    /// Verwirft alles und faellt auf `Uninitialized` zurueck (REQ-REC-16).
    ///
    /// Aufzurufen nach dem Aufwachen aus S3/S4 und nach dem Laden eines Images:
    /// danach gibt es **kein** Material, bis eine Neuspeisung den Selbsttest
    /// bestanden hat. Das ist die Stelle, an der eine alte Instanz nicht
    /// weiterlaeuft, weil ein Zustand "ungefaehr bereit" nicht existiert.
    pub fn invalidate(&mut self) {
        self.clear();
    }

    /// Speist neu und faellt dabei zuerst auf `Uninitialized` zurueck.
    ///
    /// Reihenfolge ist bindend: verwerfen, dann Quelle lesen, dann pruefen. Ein
    /// gescheiterter Lauf hinterlaesst `Failed` und kein Schluesselmaterial —
    /// niemals den Schluessel der vorigen Speisung.
    pub fn reseed(
        &mut self,
        hardware: &mut impl EntropySource,
        jitter: &mut impl EntropySource,
    ) -> EntropyState {
        self.clear();
        self.init(hardware, jitter)
    }

    /// Initialisiert den Kern aus zwei unabhaengigen Quellen.
    ///
    /// Beide Quellen werden einzeln dem Selbsttest unterzogen. Erst wenn beide
    /// bestehen, wird gemischt und der Zustand `Ready`. Jeder andere Ausgang
    /// endet in `Failed` oder bleibt `Uninitialized` — fail-closed, ohne
    /// Ausgabe und ohne Ausnahme.
    pub fn init(
        &mut self,
        hardware: &mut impl EntropySource,
        jitter: &mut impl EntropySource,
    ) -> EntropyState {
        self.clear();
        self.generation = self.generation.wrapping_add(1);

        let (mut hardware_tail, hardware_health) = match gather(hardware) {
            Ok(pair) => pair,
            Err(error) => {
                self.fault = Some(EntropyFault::Source(error));
                self.state = EntropyState::Failed;
                return self.state;
            }
        };
        if let Err(fault) = hardware_health.verdict() {
            hardware_tail.zeroize();
            self.fault = Some(EntropyFault::Health(fault));
            self.state = EntropyState::Failed;
            return self.state;
        }

        let (mut jitter_tail, jitter_health) = match gather(jitter) {
            Ok(pair) => pair,
            Err(error) => {
                hardware_tail.zeroize();
                self.fault = Some(EntropyFault::Source(error));
                self.state = EntropyState::Failed;
                return self.state;
            }
        };
        if let Err(fault) = jitter_health.verdict() {
            hardware_tail.zeroize();
            jitter_tail.zeroize();
            self.fault = Some(EntropyFault::Health(fault));
            self.state = EntropyState::Failed;
            return self.state;
        }

        let mut key = [0u8; TAIL_LEN];
        for (index, byte) in key.iter_mut().enumerate() {
            *byte = hardware_tail[index] ^ jitter_tail[index];
        }
        hardware_tail.zeroize();
        jitter_tail.zeroize();

        let mut nonce = [0u8; 12];
        nonce[..8].copy_from_slice(&self.generation.to_be_bytes());
        self.cipher = Some(ChaCha20::new((&key).into(), (&nonce).into()));
        key.zeroize();
        nonce.zeroize();

        self.produced = 0;
        self.fault = None;
        self.state = EntropyState::Ready;
        self.state
    }

    /// Fuellt `out` aus dem DRBG.
    ///
    /// Schreibt bei jedem Fehler **kein** Byte, damit ein Aufrufer nicht
    /// versehentlich Nullen als Entropie weiterreicht.
    pub fn fill(&mut self, out: &mut [u8]) -> Result<(), EntropyError> {
        if self.state != EntropyState::Ready {
            return Err(EntropyError::NotReady);
        }
        let wanted = out.len() as u64;
        if self.produced.saturating_add(wanted) > RESEED_INTERVAL_BYTES {
            return Err(EntropyError::ReseedingRequired);
        }
        let Some(cipher) = self.cipher.as_mut() else {
            return Err(EntropyError::NotReady);
        };
        cipher.apply_keystream(out);
        self.produced = self.produced.saturating_add(wanted);
        Ok(())
    }

    fn clear(&mut self) {
        self.cipher = None;
        self.produced = 0;
        self.fault = None;
        self.state = EntropyState::Uninitialized;
    }
}

impl Drop for EntropyPool {
    fn drop(&mut self) {
        self.clear();
    }
}

/// Liest [`HEALTH_BYTES`] aus einer Quelle, prueft sie und gibt das letzte
/// Schluesselmaterial zurueck.
fn gather(
    source: &mut impl EntropySource,
) -> Result<([u8; TAIL_LEN], HealthAccumulator), SourceError> {
    let mut health = HealthAccumulator::new();
    let mut tail = [0u8; TAIL_LEN];
    let mut chunk = [0u8; CHUNK_LEN];
    while health.total() < HEALTH_BYTES {
        let remaining = HEALTH_BYTES - health.total();
        let take = remaining.min(CHUNK_LEN);
        let slice = &mut chunk[..take];
        source.fill(slice)?;
        health.push(slice);
        roll(&mut tail, slice);
    }
    Ok((tail, health))
}

/// Schiebt die letzten [`TAIL_LEN`] Bytes durch ein festes Fenster.
fn roll(tail: &mut [u8; TAIL_LEN], bytes: &[u8]) {
    if bytes.len() >= TAIL_LEN {
        tail.copy_from_slice(&bytes[bytes.len() - TAIL_LEN..]);
        return;
    }
    let shift = bytes.len();
    tail.copy_within(shift.., 0);
    tail[TAIL_LEN - shift..].copy_from_slice(bytes);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Quelle mit festem Inhalt; `fill` liefert immer nur so viel, wie vorhanden
    /// ist, und meldet sonst `Short` — eine halbe Fuellung ist kein Erfolg.
    struct Fixed {
        data: Vec<u8>,
        fail: bool,
    }

    impl Fixed {
        fn from_bytes(data: Vec<u8>) -> Self {
            Self { data, fail: false }
        }

        fn failing() -> Self {
            Self {
                data: Vec::new(),
                fail: true,
            }
        }
    }

    impl EntropySource for Fixed {
        fn fill(&mut self, out: &mut [u8]) -> Result<(), SourceError> {
            if self.fail {
                return Err(SourceError::Unavailable);
            }
            if self.data.len() < out.len() {
                return Err(SourceError::Short);
            }
            out.copy_from_slice(&self.data[..out.len()]);
            Ok(())
        }
    }

    /// Deterministische Folge fuer Tests. Kein Anspruch auf kryptografische
    /// Guete, nur auf Reproduzierbarkeit und ausreichende Vielfalt.
    fn stream(seed: u64, len: usize) -> Vec<u8> {
        // xorshift64 braucht einen Wert ungleich 0; verschiedene Seeds duerfen
        // dabei nicht auf denselben Startwert abgebildet werden (32 | 1 == 33 | 1).
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

    fn good(seed: u64) -> Fixed {
        Fixed::from_bytes(stream(seed, HEALTH_BYTES))
    }

    #[test]
    fn neuer_kern_ist_uninitialisiert_und_liefert_nichts() {
        let mut pool = EntropyPool::new();
        assert_eq!(pool.state(), EntropyState::Uninitialized);
        let mut out = [0xAAu8; 32];
        assert_eq!(pool.fill(&mut out), Err(EntropyError::NotReady));
        assert_eq!(out, [0xAAu8; 32], "kein Byte darf geschrieben werden");
    }

    #[test]
    fn zwei_gesunde_quellen_ergeben_ready() {
        let mut pool = EntropyPool::new();
        let state = pool.init(&mut good(1), &mut good(2));
        assert_eq!(state, EntropyState::Ready);
        assert_eq!(pool.state(), EntropyState::Ready);
        assert_eq!(pool.fault(), None);
    }

    #[test]
    fn konstante_quelle_wird_abgelehnt() {
        let mut acc = HealthAccumulator::new();
        acc.push(&[0x5Au8; HEALTH_BYTES]);
        assert_eq!(acc.verdict(), Err(HealthFault::Constant));
    }

    #[test]
    fn festhaengendes_bit_wird_erkannt() {
        let mut data = stream(7, HEALTH_BYTES);
        for byte in data.iter_mut() {
            *byte |= 0b0000_0001;
        }
        let mut acc = HealthAccumulator::new();
        acc.push(&data);
        assert_eq!(acc.verdict(), Err(HealthFault::StuckBit { bit: 0 }));
    }

    #[test]
    fn zu_wenig_verschiedene_bytewerte_werden_abgelehnt() {
        // 32 verschiedene Werte, jeder Bitposition beides bietend (Wert und
        // Komplement): der Selbsttest muss an der Vielfalt scheitern, nicht an
        // einem festhaengenden Bit und nicht an der Wiederholung.
        let mut data = stream(9, HEALTH_BYTES);
        for byte in data.iter_mut() {
            let source = *byte;
            let base = source % 16;
            *byte = if source & 0x10 == 0 {
                base
            } else {
                base ^ 0xFF
            };
        }
        let mut acc = HealthAccumulator::new();
        acc.push(&data);
        assert_eq!(acc.verdict(), Err(HealthFault::TooFewDistinct { seen: 32 }));
    }

    #[test]
    fn wiederholter_block_wird_erkannt() {
        // Vielfalt und Bitverteilung sind in Ordnung, nur wiederholt sich jeder
        // zweite Block: genau der Fall, den eine haengende Quelle erzeugt.
        let mut data = stream(13, HEALTH_BYTES);
        for index in 1..(HEALTH_BYTES / BLOCK_LEN) {
            if index % 2 == 1 {
                let previous = (index - 1) * BLOCK_LEN;
                let current = index * BLOCK_LEN;
                let block: Vec<u8> = data[previous..previous + BLOCK_LEN].to_vec();
                data[current..current + BLOCK_LEN].copy_from_slice(&block);
            }
        }
        let mut acc = HealthAccumulator::new();
        acc.push(&data);
        assert_eq!(acc.verdict(), Err(HealthFault::RepeatedBlock));
    }

    #[test]
    fn zu_kurze_lieferung_wird_verworfen() {
        // Quelle liefert genau ein Byte zu wenig und meldet danach `Short`.
        struct Limited {
            available: usize,
        }
        impl EntropySource for Limited {
            fn fill(&mut self, out: &mut [u8]) -> Result<(), SourceError> {
                if self.available < out.len() {
                    return Err(SourceError::Short);
                }
                self.available -= out.len();
                for (index, byte) in out.iter_mut().enumerate() {
                    *byte = (index as u8).wrapping_mul(17).wrapping_add(3);
                }
                Ok(())
            }
        }
        let mut short = Limited {
            available: HEALTH_BYTES - 1,
        };
        let mut pool = EntropyPool::new();
        let state = pool.init(&mut short, &mut good(4));
        assert_eq!(state, EntropyState::Failed);
        assert_eq!(pool.fault(), Some(EntropyFault::Source(SourceError::Short)));
    }

    #[test]
    fn invalidiert_gibt_es_kein_material() {
        let mut pool = EntropyPool::new();
        assert_eq!(pool.init(&mut good(11), &mut good(12)), EntropyState::Ready);
        let mut out = [0u8; 32];
        pool.fill(&mut out).unwrap();

        // S3/S4 oder neues Image: alles verworfen, Zustand zurueck auf Anfang.
        pool.invalidate();
        assert_eq!(pool.state(), EntropyState::Uninitialized);
        let mut after = [0x5Au8; 32];
        assert_eq!(pool.fill(&mut after), Err(EntropyError::NotReady));
        assert_eq!(after, [0x5Au8; 32], "kein Byte darf geschrieben werden");
    }

    #[test]
    fn gescheiterte_neuspeisung_laesst_kein_altes_schluesselmaterial() {
        let mut pool = EntropyPool::new();
        assert_eq!(pool.init(&mut good(21), &mut good(22)), EntropyState::Ready);
        let mut first = [0u8; 32];
        pool.fill(&mut first).unwrap();
        let first_generation = pool.generation();

        // Quelle liefert nur noch Nullen: Neuspeisung scheitert, der alte
        // Schluessel darf nicht weiterlaufen.
        let mut flat = Fixed::from_bytes(vec![0u8; HEALTH_BYTES]);
        assert_eq!(pool.reseed(&mut flat, &mut good(23)), EntropyState::Failed);
        assert_eq!(pool.fill(&mut [0u8; 8]), Err(EntropyError::NotReady));
        assert!(matches!(pool.fault(), Some(EntropyFault::Health(_))));

        // Mit gesunder Quelle wieder Ready, aber mit anderem Schluesselstrom.
        assert_eq!(
            pool.reseed(&mut good(24), &mut good(25)),
            EntropyState::Ready
        );
        assert!(pool.generation() > first_generation);
        let mut second = [0u8; 32];
        pool.fill(&mut second).unwrap();
        assert_ne!(first, second);
    }

    #[test]
    fn neuspeisung_ohne_neue_quelle_bleibt_moeglich() {
        // Zweimal dieselbe Quelle, zwei Speisungen: der Nonce traegt die
        // Generation, also unterscheiden sich die Ausgaben trotzdem.
        let mut pool = EntropyPool::new();
        assert_eq!(
            pool.reseed(&mut good(31), &mut good(32)),
            EntropyState::Ready
        );
        let mut first = [0u8; 32];
        pool.fill(&mut first).unwrap();
        assert_eq!(
            pool.reseed(&mut good(31), &mut good(32)),
            EntropyState::Ready
        );
        let mut second = [0u8; 32];
        pool.fill(&mut second).unwrap();
        assert_ne!(first, second);
    }

    #[test]
    fn nicht_verfuegbare_quelle_setzt_failed() {
        let mut pool = EntropyPool::new();
        let state = pool.init(&mut Fixed::failing(), &mut good(5));
        assert_eq!(state, EntropyState::Failed);
        assert_eq!(
            pool.fault(),
            Some(EntropyFault::Source(SourceError::Unavailable))
        );
        let mut out = [0u8; 16];
        assert_eq!(pool.fill(&mut out), Err(EntropyError::NotReady));
    }

    #[test]
    fn erkrankte_jitterquelle_setzt_failed_obwohl_hardware_gesund_ist() {
        let mut flat = Fixed::from_bytes(vec![0u8; HEALTH_BYTES]);
        let mut pool = EntropyPool::new();
        assert_eq!(pool.init(&mut good(11), &mut flat), EntropyState::Failed);
        assert_eq!(
            pool.fault(),
            Some(EntropyFault::Health(HealthFault::Constant))
        );
    }

    #[test]
    fn gleiche_quellen_ergeben_gleiche_ausgabe() {
        let mut first = EntropyPool::new();
        let mut second = EntropyPool::new();
        first.init(&mut good(21), &mut good(22));
        second.init(&mut good(21), &mut good(22));
        let mut a = [0u8; 32];
        let mut b = [0u8; 32];
        first.fill(&mut a).unwrap();
        second.fill(&mut b).unwrap();
        assert_eq!(a, b, "der Kern muss deterministisch pruefbar sein");
    }

    #[test]
    fn andere_jitterquelle_aendert_die_ausgabe() {
        let mut first = EntropyPool::new();
        let mut second = EntropyPool::new();
        first.init(&mut good(31), &mut good(32));
        second.init(&mut good(31), &mut good(33));
        let mut a = [0u8; 32];
        let mut b = [0u8; 32];
        first.fill(&mut a).unwrap();
        second.fill(&mut b).unwrap();
        assert_ne!(a, b, "beide Quellen muessen in die Mischung eingehen");
    }

    #[test]
    fn ausgabe_ist_kein_durchreichen_der_quelle() {
        let hardware = stream(41, HEALTH_BYTES);
        let expected_tail = hardware[HEALTH_BYTES - TAIL_LEN..].to_vec();
        let mut source = Fixed::from_bytes(hardware);
        let mut pool = EntropyPool::new();
        pool.init(&mut source, &mut good(42));
        let mut out = [0u8; TAIL_LEN];
        pool.fill(&mut out).unwrap();
        assert_ne!(out.to_vec(), expected_tail, "kein Passthrough");
    }

    #[test]
    fn leere_anforderung_ist_erlaubt() {
        let mut pool = EntropyPool::new();
        pool.init(&mut good(51), &mut good(52));
        let mut empty: [u8; 0] = [];
        assert_eq!(pool.fill(&mut empty), Ok(()));
    }

    #[test]
    fn erschoepfte_ausgabemenge_verlangt_neuspeisung_und_schreibt_nichts() {
        let mut pool = EntropyPool::new();
        pool.init(&mut good(61), &mut good(62));
        let mut half = vec![0u8; (RESEED_INTERVAL_BYTES as usize) / 2];
        pool.fill(&mut half).unwrap();
        assert!(!pool.needs_reseed());
        pool.fill(&mut half).unwrap();
        assert!(pool.needs_reseed());
        let mut rest = [0x77u8; 8];
        assert_eq!(pool.fill(&mut rest), Err(EntropyError::ReseedingRequired));
        assert_eq!(rest, [0x77u8; 8], "kein Byte darf geschrieben werden");
    }

    #[test]
    fn neuspeisung_macht_wieder_ready_und_aendert_die_folge() {
        let mut pool = EntropyPool::new();
        pool.init(&mut good(71), &mut good(72));
        let mut first = [0u8; 32];
        pool.fill(&mut first).unwrap();
        let state = pool.init(&mut good(73), &mut good(74));
        assert_eq!(state, EntropyState::Ready);
        assert!(!pool.needs_reseed());
        let mut second = [0u8; 32];
        pool.fill(&mut second).unwrap();
        assert_ne!(first, second);
    }

    #[test]
    fn failed_bleibt_failed_bis_zur_neuspeisung() {
        let mut pool = EntropyPool::new();
        pool.init(&mut Fixed::failing(), &mut good(81));
        assert_eq!(pool.state(), EntropyState::Failed);
        let mut out = [0u8; 8];
        assert_eq!(pool.fill(&mut out), Err(EntropyError::NotReady));
        assert_eq!(pool.init(&mut good(82), &mut good(83)), EntropyState::Ready);
        assert_eq!(pool.fill(&mut out), Ok(()));
    }

    #[test]
    fn roll_fenster_haelt_die_letzten_bytes() {
        let mut tail = [0u8; TAIL_LEN];
        roll(&mut tail, &[1, 2, 3]);
        assert_eq!(&tail[TAIL_LEN - 3..], &[1, 2, 3]);
        roll(&mut tail, &[4, 5]);
        assert_eq!(&tail[TAIL_LEN - 5..], &[1, 2, 3, 4, 5]);
        let long = stream(91, 100);
        roll(&mut tail, &long);
        assert_eq!(&tail[..], &long[100 - TAIL_LEN..]);
    }

    #[test]
    fn abbruch_der_quelle_auf_halbem_weg_setzt_failed() {
        struct Dying {
            left: usize,
        }
        impl EntropySource for Dying {
            fn fill(&mut self, out: &mut [u8]) -> Result<(), SourceError> {
                if self.left < out.len() {
                    return Err(SourceError::Unavailable);
                }
                self.left -= out.len();
                for (index, byte) in out.iter_mut().enumerate() {
                    *byte = (index as u8).wrapping_mul(31).wrapping_add(7);
                }
                Ok(())
            }
        }
        let mut dying = Dying {
            left: CHUNK_LEN * 4,
        };
        let mut pool = EntropyPool::new();
        assert_eq!(pool.init(&mut dying, &mut good(92)), EntropyState::Failed);
        assert_eq!(
            pool.fault(),
            Some(EntropyFault::Source(SourceError::Unavailable))
        );
    }
}
