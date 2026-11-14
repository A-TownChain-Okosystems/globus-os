//! PCI MSI/MSI-X capability and interrupt-vector runtime.

use alloc::vec;
use alloc::vec::Vec;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Nachricht fuer einen Message-Signalled Interrupt.
pub struct MsiMessage {
    /// Zieladresse der Nachricht.
    pub address: u64,
    /// Nutzdaten mit Vektor und Modus.
    pub data: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Ein Eintrag der MSI-X-Tabelle.
pub struct MsixTableEntry {
    /// Nachricht des Eintrags.
    pub message: MsiMessage,
    /// True, wenn der Eintrag maskiert ist.
    pub masked: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Betriebsart der Interrupts eines Geraets.
pub enum InterruptMode {
    /// Klassische Interruptleitung.
    Legacy,
    /// Message-Signalled Interrupts.
    Msi { vectors: u16 },
    /// MSI-X.
    Msix { vectors: u16 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Fehler der MSI-Konfiguration.
pub enum MsiError {
    /// Die Vektorzahl ist unbrauchbar.
    InvalidVectorCount,
    /// Es sind keine Vektoren mehr frei.
    NoVectors,
    /// Der Vektor ist nicht vergeben.
    VectorUnavailable,
    /// Der Vektor liegt ausserhalb des Bereichs.
    InvalidVector,
}

/// True, wenn die Vektorzahl im erlaubten Bereich liegt.
pub fn valid_vector_count(count: u16) -> bool {
    count > 0 && count <= 2048
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Vergabe von Vektoren aus einem festen Bereich.
pub struct VectorAllocator {
    base: u16,
    allocated: Vec<u16>,
    limit: u16,
}

impl VectorAllocator {
    /// Legt den Bereich an und prueft Basis und Anzahl.
    pub fn new(base: u16, count: u16) -> Result<Self, MsiError> {
        if !valid_vector_count(count) || base.checked_add(count).is_none() {
            return Err(MsiError::InvalidVectorCount);
        }
        Ok(Self {
            base,
            allocated: Vec::new(),
            limit: count,
        })
    }
    /// Vergibt den naechsten freien Vektor.
    pub fn allocate(&mut self) -> Result<u16, MsiError> {
        for offset in 0..self.limit {
            let vector = self.base + offset;
            if !self.allocated.contains(&vector) {
                self.allocated.push(vector);
                self.allocated.sort();
                return Ok(vector);
            }
        }
        Err(MsiError::NoVectors)
    }
    /// Gibt einen Vektor zurueck.
    pub fn release(&mut self, vector: u16) -> Result<(), MsiError> {
        let before = self.allocated.len();
        self.allocated.retain(|v| *v != vector);
        if before == self.allocated.len() {
            Err(MsiError::InvalidVector)
        } else {
            Ok(())
        }
    }
    /// True, wenn der Vektor vergeben ist.
    pub fn is_allocated(&self, vector: u16) -> bool {
        self.allocated.contains(&vector)
    }
    /// Alle vergebenen Vektoren.
    pub fn allocated(&self) -> &[u16] {
        &self.allocated
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// MSI-X-Tabelle eines Geraets.
pub struct MsixTable {
    entries: Vec<MsixTableEntry>,
}
impl MsixTable {
    /// Legt eine Tabelle mit dieser Eintragszahl an.
    pub fn new(count: u16) -> Result<Self, MsiError> {
        if !valid_vector_count(count) {
            return Err(MsiError::InvalidVectorCount);
        }
        Ok(Self {
            entries: vec![
                MsixTableEntry {
                    message: MsiMessage {
                        address: 0,
                        data: 0
                    },
                    masked: true
                };
                count as usize
            ],
        })
    }
    /// Setzt die Nachricht eines Eintrags.
    pub fn set(&mut self, index: u16, message: MsiMessage) -> Result<(), MsiError> {
        let e = self
            .entries
            .get_mut(index as usize)
            .ok_or(MsiError::InvalidVector)?;
        e.message = message;
        Ok(())
    }
    /// Maskiert oder entmaskiert einen Eintrag.
    pub fn mask(&mut self, index: u16, masked: bool) -> Result<(), MsiError> {
        let e = self
            .entries
            .get_mut(index as usize)
            .ok_or(MsiError::InvalidVector)?;
        e.masked = masked;
        Ok(())
    }
    /// Eintrag an dieser Stelle.
    pub fn get(&self, index: u16) -> Result<MsixTableEntry, MsiError> {
        self.entries
            .get(index as usize)
            .copied()
            .ok_or(MsiError::InvalidVector)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn allocator_is_deterministic() {
        let mut a = VectorAllocator::new(32, 2).unwrap();
        assert_eq!(a.allocate().unwrap(), 32);
        assert_eq!(a.allocate().unwrap(), 33);
        assert_eq!(a.allocate(), Err(MsiError::NoVectors));
        a.release(32).unwrap();
        assert_eq!(a.allocate().unwrap(), 32);
    }
    #[test]
    fn msix_starts_masked() {
        let t = MsixTable::new(2).unwrap();
        assert!(t.get(0).unwrap().masked);
    }
}
