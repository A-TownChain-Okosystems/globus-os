//! Architecture-neutral local/IO APIC programming contract.

use crate::interrupt::InterruptVector;
use alloc::vec::Vec;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Kennung eines APIC.
pub struct ApicId(pub u8);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Umleitung eines Interrupts auf einen Ziel-APIC.
pub struct IoApicRedirection {
    /// Interruptvektor.
    pub vector: InterruptVector,
    /// Ziel-APIC.
    pub destination: ApicId,
    /// True, wenn der Eintrag maskiert ist.
    pub masked: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Fehler der APIC-Programmierung.
pub enum ApicError {
    /// Die APIC-Kennung ist ungueltig.
    InvalidId,
    /// Der Vektor liegt ausserhalb des erlaubten Bereichs.
    InvalidVector,
    /// Fuer diesen Vektor existiert bereits ein Eintrag.
    DuplicateVector,
    /// Fuer diesen Vektor existiert kein Eintrag.
    MissingVector,
}

#[derive(Debug, Default)]
/// Umleitungstabelle des IO-APIC.
pub struct ApicController {
    entries: Vec<IoApicRedirection>,
}

impl ApicController {
    /// Leere Umleitungstabelle.
    pub fn new() -> Self {
        Self::default()
    }

    /// Setzt oder ersetzt die Umleitung eines Vektors.
    pub fn set_redirection(&mut self, entry: IoApicRedirection) -> Result<(), ApicError> {
        if entry.destination.0 == u8::MAX {
            return Err(ApicError::InvalidId);
        }
        if !entry.vector.valid() {
            return Err(ApicError::InvalidVector);
        }
        if self.entries.iter().any(|e| e.vector == entry.vector) {
            return Err(ApicError::DuplicateVector);
        }
        self.entries.push(entry);
        self.entries.sort_by_key(|e| e.vector.0);
        Ok(())
    }

    /// Umleitung eines Vektors.
    pub fn get(&self, vector: InterruptVector) -> Result<IoApicRedirection, ApicError> {
        self.entries
            .iter()
            .find(|e| e.vector == vector)
            .copied()
            .ok_or(ApicError::MissingVector)
    }

    /// Maskiert oder entmaskiert einen Vektor.
    pub fn mask(&mut self, vector: InterruptVector, masked: bool) -> Result<(), ApicError> {
        let entry = self
            .entries
            .iter_mut()
            .find(|e| e.vector == vector)
            .ok_or(ApicError::MissingVector)?;
        entry.masked = masked;
        Ok(())
    }

    /// Alle Umleitungen in Einfuegereihenfolge.
    pub fn entries(&self) -> &[IoApicRedirection] {
        &self.entries
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn redirection_is_deterministic() {
        let mut a = ApicController::new();
        a.set_redirection(IoApicRedirection {
            vector: InterruptVector(48),
            destination: ApicId(1),
            masked: true,
        })
        .unwrap();
        assert!(a.get(InterruptVector(48)).unwrap().masked);
        a.mask(InterruptVector(48), false).unwrap();
        assert!(!a.get(InterruptVector(48)).unwrap().masked);
    }
}
