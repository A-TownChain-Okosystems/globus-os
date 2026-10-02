use crate::{RepairAction, RepairLevel, RepairResult};

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
        Self { sequence, problem_id, stage, action, required_level, result }
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
        let record = EvidenceRecord::new(
            self.next_sequence,
            problem_id,
            stage,
            action,
            required_level,
            result,
        );
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
        assert_eq!(ledger.records()[MAX_EVIDENCE_RECORDS - 1].sequence, MAX_EVIDENCE_RECORDS as u64 + 1);
    }
}
