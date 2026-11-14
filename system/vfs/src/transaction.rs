//! Fail-closed filesystem transaction coordinator.

use crate::journal::{Journal, JournalError, JournalOp};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Fehler einer Mehrblock-Transaktion.
pub enum TransactionError {
    /// Das Journal hat abgelehnt.
    Journal(JournalError),
    /// Die Transaktion ist nicht mehr offen.
    Inactive,
    /// Mehr als 64 Schreibvorgaenge in einer Transaktion.
    TooManyWrites,
}
impl From<JournalError> for TransactionError {
    fn from(value: JournalError) -> Self {
        Self::Journal(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Ein vorgemerkter Schreibvorgang.
pub struct PendingWrite {
    /// Zielblock.
    pub block: u64,
    /// Pruefsumme des zu schreibenden Blocks.
    pub checksum: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Offene Transaktion ueber mehrere Bloecke.
pub struct Transaction {
    sequence: u64,
    writes: Vec<PendingWrite>,
    active: bool,
}
impl Transaction {
    /// Startet eine offene Transaktion mit dieser Sequenz.
    pub fn begin(sequence: u64) -> Self {
        Self {
            sequence,
            writes: Vec::new(),
            active: true,
        }
    }
    /// Merkt einen Schreibvorgang vor; hoechstens 64 je Transaktion.
    pub fn write(&mut self, block: u64, checksum: u32) -> Result<(), TransactionError> {
        if !self.active {
            return Err(TransactionError::Inactive);
        }
        if self.writes.len() >= 64 {
            return Err(TransactionError::TooManyWrites);
        }
        if self.writes.iter().any(|w| w.block == block) {
            return Ok(());
        }
        self.writes.push(PendingWrite { block, checksum });
        Ok(())
    }
    /// Schreibt die vorgemerkten Vorgaenge ins Journal und liefert die Zielsequenz.
    pub fn commit(mut self, journal: &mut Journal) -> Result<u64, TransactionError> {
        if !self.active {
            return Err(TransactionError::Inactive);
        }
        for write in &self.writes {
            journal.append(write.block, JournalOp::Write)?;
        }
        self.active = false;
        Ok(self.sequence)
    }
    /// Vorgemerkte Schreibvorgaenge.
    pub fn writes(&self) -> &[PendingWrite] {
        &self.writes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn commit_records_write_intent() {
        let mut tx = Transaction::begin(7);
        tx.write(12, 0x1234).unwrap();
        let mut j = Journal::new(4).unwrap();
        assert_eq!(tx.commit(&mut j).unwrap(), 7);
        assert_eq!(j.pending().len(), 1)
    }
    #[test]
    fn duplicate_blocks_are_coalesced() {
        let mut tx = Transaction::begin(1);
        tx.write(5, 1).unwrap();
        tx.write(5, 2).unwrap();
        assert_eq!(
            tx.writes(),
            &[PendingWrite {
                block: 5,
                checksum: 1
            }]
        )
    }
}
