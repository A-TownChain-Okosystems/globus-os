//! Checksummed write-ahead journal records for crash-safe metadata updates.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Fehler beim Schreiben oder Pruefen eines Journal-Eintrags.
pub enum JournalError {
    /// Der Puffer ist zu klein fuer einen Journal-Eintrag.
    Buffer,
    /// Reihenfolge, Feldwert oder Laenge ist ungueltig.
    Invalid,
    /// Die Sequenznummer laeuft ueber.
    SequenceOverflow,
    /// Das Journal ist voll.
    Full,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Art der Journal-Operation.
pub enum JournalOp {
    /// Erste Operation (Bytewert 1): der Zielblock wird geschrieben.
    Write = 1,
    /// Zweite Operation (Bytewert 2): der Zielblock wird freigegeben. Ausgewertet wird die Operation von der Dateisystemschicht; das Journal unterscheidet nur die zwei Bytewerte.
    Clear = 2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Ein Journal-Eintrag in seiner Plattenform.
pub struct JournalRecord {
    /// Fortlaufende Sequenznummer.
    pub sequence: u64,
    /// Block, auf den die Operation zielt.
    pub target_block: u64,
    /// Art der Operation.
    pub op: JournalOp,
    /// Pruefsumme ueber den Eintrag ohne dieses Feld.
    pub checksum: u32,
}

/// Feste Groesse eines Journal-Eintrags in Byte.
pub const JOURNAL_RECORD_SIZE: usize = 32;

fn checksum(bytes: &[u8]) -> u32 {
    let mut hash = 0x811c9dc5u32;
    for byte in bytes {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(0x01000193);
    }
    hash
}

impl JournalRecord {
    /// Baut einen Eintrag und berechnet seine Pruefsumme.
    pub fn new(sequence: u64, target_block: u64, op: JournalOp) -> Self {
        let mut record = Self {
            sequence,
            target_block,
            op,
            checksum: 0,
        };
        let mut bytes = [0u8; JOURNAL_RECORD_SIZE];
        record.encode_unchecked(&mut bytes);
        record.checksum = checksum(&bytes[..20]);
        record
    }

    fn encode_unchecked(&self, out: &mut [u8]) {
        out.fill(0);
        out[..8].copy_from_slice(&self.sequence.to_le_bytes());
        out[8..16].copy_from_slice(&self.target_block.to_le_bytes());
        out[16] = self.op as u8;
        out[20..24].copy_from_slice(&self.checksum.to_le_bytes());
    }

    /// Schreibt den Eintrag in den Puffer.
    pub fn encode(&self, out: &mut [u8]) -> Result<(), JournalError> {
        if out.len() < JOURNAL_RECORD_SIZE {
            return Err(JournalError::Buffer);
        }
        self.encode_unchecked(&mut out[..JOURNAL_RECORD_SIZE]);
        Ok(())
    }

    /// Liest einen Eintrag und prueft Pruefsumme und reservierte Bytes.
    pub fn decode(input: &[u8]) -> Result<Self, JournalError> {
        if input.len() < JOURNAL_RECORD_SIZE {
            return Err(JournalError::Buffer);
        }
        if input[17..20].iter().any(|b| *b != 0)
            || input[24..JOURNAL_RECORD_SIZE].iter().any(|b| *b != 0)
        {
            return Err(JournalError::Invalid);
        }
        let sequence = u64::from_le_bytes(input[..8].try_into().map_err(|_| JournalError::Buffer)?);
        let target_block =
            u64::from_le_bytes(input[8..16].try_into().map_err(|_| JournalError::Buffer)?);
        let op = match input[16] {
            1 => JournalOp::Write,
            2 => JournalOp::Clear,
            _ => return Err(JournalError::Invalid),
        };
        let expected =
            u32::from_le_bytes(input[20..24].try_into().map_err(|_| JournalError::Buffer)?);
        if checksum(&input[..20]) != expected {
            return Err(JournalError::Invalid);
        }
        Ok(Self {
            sequence,
            target_block,
            op,
            checksum: expected,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Journal mit fester Kapazitaet.
pub struct Journal {
    capacity: usize,
    records: Vec<JournalRecord>,
    next_sequence: u64,
}

impl Journal {
    /// Journal mit nicht verschwindender Kapazitaet.
    pub fn new(capacity: usize) -> Result<Self, JournalError> {
        if capacity == 0 {
            return Err(JournalError::Invalid);
        }
        Ok(Self {
            capacity,
            records: Vec::with_capacity(capacity),
            next_sequence: 1,
        })
    }

    /// Alle Eintraege in Einfuegereihenfolge.
    pub fn records(&self) -> &[JournalRecord] {
        &self.records
    }

    /// Noch nicht abgeschlossene Eintraege.
    pub fn pending(&self) -> &[JournalRecord] {
        &self.records
    }

    /// Haengt eine Operation an und liefert den Eintrag.
    pub fn append(
        &mut self,
        target_block: u64,
        op: JournalOp,
    ) -> Result<JournalRecord, JournalError> {
        if self.records.len() == self.capacity {
            return Err(JournalError::Full);
        }
        let sequence = self.next_sequence;
        self.next_sequence = self
            .next_sequence
            .checked_add(1)
            .ok_or(JournalError::SequenceOverflow)?;
        let record = JournalRecord::new(sequence, target_block, op);
        self.records.push(record);
        Ok(record)
    }

    /// Leert das Journal nach erfolgreichem Replay.
    pub fn clear(&mut self) {
        self.records.clear();
    }

    /// Prueft die Eintraege der Reihe nach und liefert den letzten Zielblock.
    pub fn recover(&self) -> Result<u64, JournalError> {
        let mut previous = 0;
        for record in &self.records {
            if record.sequence <= previous {
                return Err(JournalError::Invalid);
            }
            previous = record.sequence;
        }
        Ok(previous)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_round_trip() {
        let record = JournalRecord::new(7, 42, JournalOp::Write);
        let mut bytes = [0u8; JOURNAL_RECORD_SIZE];
        record.encode(&mut bytes).unwrap();
        assert_eq!(JournalRecord::decode(&bytes).unwrap(), record);
    }

    #[test]
    fn corruption_is_rejected() {
        let record = JournalRecord::new(1, 2, JournalOp::Write);
        let mut bytes = [0u8; JOURNAL_RECORD_SIZE];
        record.encode(&mut bytes).unwrap();
        bytes[8] ^= 1;
        assert_eq!(JournalRecord::decode(&bytes), Err(JournalError::Invalid));
    }

    #[test]
    fn journal_is_bounded_and_ordered() {
        let mut journal = Journal::new(2).unwrap();
        assert_eq!(journal.append(10, JournalOp::Write).unwrap().sequence, 1);
        assert_eq!(journal.append(11, JournalOp::Clear).unwrap().sequence, 2);
        assert_eq!(
            journal.append(12, JournalOp::Write),
            Err(JournalError::Full)
        );
        assert_eq!(journal.recover().unwrap(), 2);
    }
}
