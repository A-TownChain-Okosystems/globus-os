use crate::{RepairAction, RepairLevel, RepairResult};

const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
const FNV_PRIME: u64 = 0x00000100000001b3;

fn mix(hash: u64, byte: u8) -> u64 { (hash ^ byte as u64).wrapping_mul(FNV_PRIME) }

fn mix_u64(mut hash: u64, value: u64) -> u64 {
    for byte in value.to_be_bytes() { hash = mix(hash, byte); }
    hash
}

fn mix_u8(hash: u64, value: u8) -> u64 { mix(hash, value) }


/// Deterministic evidence for one repair lifecycle transition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EvidenceStage {
    Requested,
    Authorized,
    Executed,
    Verified,
    Rejected,
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EvidenceRecord {
    pub sequence: u64,
    pub previous_hash: u64,
    pub hash: u64,
    pub problem_id: u64,
    pub stage: EvidenceStage,
    pub action: RepairAction,
    pub required_level: RepairLevel,
    pub result: RepairResult,
}

impl EvidenceRecord {
    pub const fn new(
        sequence: u64,
        problem_id: u64,
        stage: EvidenceStage,
        action: RepairAction,
        required_level: RepairLevel,
        result: RepairResult,
    ) -> Self {
        let mut hash = FNV_OFFSET_BASIS;
        hash = mix_u64(hash, sequence);
        hash = mix_u64(hash, problem_id);
        hash = mix_u8(hash, stage as u8);
        hash = mix_u8(hash, action as u8);
        hash = mix_u8(hash, required_level as u8);
        hash = mix_u8(hash, result as u8);
        Self { sequence, previous_hash: 0, hash, problem_id, stage, action, required_level, result }
    }

    fn with_previous(mut self, previous_hash: u64) -> Self {
        let mut hash = FNV_OFFSET_BASIS;
        hash = mix_u64(hash, previous_hash);
        hash = mix_u64(hash, self.sequence);
        hash = mix_u64(hash, self.problem_id);
        hash = mix_u8(hash, self.stage as u8);
        hash = mix_u8(hash, self.action as u8);
        hash = mix_u8(hash, self.required_level as u8);
        hash = mix_u8(hash, self.result as u8);
        self.previous_hash = previous_hash;
        self.hash = hash;
        self
    }
}

pub const MAX_EVIDENCE_RECORDS: usize = 1024;

pub struct EvidenceLedger {
    records: alloc::vec::Vec<EvidenceRecord>,
    next_sequence: u64,
}

impl EvidenceLedger {
    pub fn new() -> Self { Self { records: alloc::vec::Vec::new(), next_sequence: 0 } }

    pub fn append(
        &mut self,
        problem_id: u64,
        stage: EvidenceStage,
        action: RepairAction,
        required_level: RepairLevel,
        result: RepairResult,
    ) -> EvidenceRecord {
        let previous_hash = self.records.last().map_or(0, |record| record.hash);
        let record = EvidenceRecord::new(
            self.next_sequence,
            problem_id,
            stage,
            action,
            required_level,
            result,
        ).with_previous(previous_hash);
        self.next_sequence = self.next_sequence.saturating_add(1);
        if self.records.len() == MAX_EVIDENCE_RECORDS {
            self.records.remove(0);
        }
        self.records.push(record);
        record
    }

    pub fn records(&self) -> &[EvidenceRecord] { &self.records }
}

impl Default for EvidenceLedger {
    fn default() -> Self { Self::new() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sequence_is_monotonic_and_bounded() {
        let mut ledger = EvidenceLedger::new();
        for _ in 0..(MAX_EVIDENCE_RECORDS + 2) {
            ledger.append(7, EvidenceStage::Requested, RepairAction::RestartService, RepairLevel::Information, RepairResult::Applied);
        }
        assert_eq!(ledger.records().len(), MAX_EVIDENCE_RECORDS);
        assert_eq!(ledger.records()[0].sequence, 2);
        assert_eq!(ledger.records()[1].previous_hash, ledger.records()[0].hash);
        assert_eq!(ledger.records()[MAX_EVIDENCE_RECORDS - 1].sequence, MAX_EVIDENCE_RECORDS as u64 + 1);
    }
}
