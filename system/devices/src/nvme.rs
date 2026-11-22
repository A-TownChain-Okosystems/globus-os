//! NVMe controller contracts and deterministic queue runtime.

use alloc::vec::Vec;
/// Tiefe der Admin-Queue.
pub const NVME_ADMIN_QUEUE_DEPTH: u16 = 32;
/// Standardtiefe einer IO-Queue.
pub const NVME_DEFAULT_IO_QUEUE_DEPTH: u16 = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Kennung einer NVMe-Queue.
pub struct NvmeQueueId(pub u16);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Puffer im DMA-Adressraum des Controllers.
pub struct NvmeDmaBuffer {
    /// Physische Adresse des Puffers.
    pub address: u64,
    /// Laenge in Byte.
    pub length: u32,
}

impl NvmeDmaBuffer {
    /// True, wenn Adresse und Laenge brauchbar sind.
    pub fn valid(self) -> bool {
        self.length > 0 && self.address.checked_add(self.length as u64).is_some()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Ein NVMe-Kommando.
pub struct NvmeCommand {
    /// Opcode des Kommandos.
    pub opcode: u8,
    /// Namensraum, auf den sich das Kommando bezieht.
    pub namespace: u32,
    /// Datenpuffer, falls das Kommando einen braucht.
    pub data: Option<NvmeDmaBuffer>,
    /// Kennung zur Zuordnung der Fertigmeldung.
    /// Kennung des zugehoerigen Kommandos.
    pub command_id: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Eine Fertigmeldung des Controllers.
pub struct NvmeCompletion {
    /// Kennung des zugehoerigen Kommandos.
    pub command_id: u16,
    /// Statusfeld des Controllers.
    pub status: u16,
    /// Ergebniswert des Kommandos.
    pub result: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Fehler einer NVMe-Queue.
pub enum NvmeError {
    /// Die Tiefe ist 0 oder zu gross.
    InvalidQueueDepth,
    /// Der Puffer ist unbrauchbar.
    InvalidBuffer,
    /// Der Controller ist noch nicht bereit.
    ControllerNotReady,
    /// Das Kommando konnte nicht eingereiht werden.
    SubmissionFailed,
    /// Die Submission-Queue ist voll.
    QueueFull,
    /// Es liegt keine Fertigmeldung vor.
    CompletionUnavailable,
    /// Diese Kommandokennung ist bereits in der Queue.
    DuplicateCommand,
}

/// True, wenn die Tiefe im erlaubten Bereich liegt.
pub const fn valid_queue_depth(depth: u16) -> bool {
    depth >= 2 && depth <= 4096 && depth.is_power_of_two()
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Eine Submission- und Completion-Queue.
pub struct NvmeQueue {
    id: NvmeQueueId,
    depth: u16,
    submissions: Vec<NvmeCommand>,
    /// Vom Controller abgeholte, noch nicht fertig gemeldete Kommandos.
    inflight: Vec<NvmeCommand>,
    completions: Vec<NvmeCompletion>,
}

impl NvmeQueue {
    /// Legt eine Queue mit dieser Kennung und Tiefe an.
    pub fn new(id: NvmeQueueId, depth: u16) -> Result<Self, NvmeError> {
        if !valid_queue_depth(depth) {
            return Err(NvmeError::InvalidQueueDepth);
        }
        Ok(Self {
            id,
            depth,
            submissions: Vec::with_capacity(depth as usize),
            inflight: Vec::with_capacity(depth as usize),
            completions: Vec::with_capacity(depth as usize),
        })
    }
    /// Kennung der Queue.
    pub fn id(&self) -> NvmeQueueId {
        self.id
    }
    /// Tiefe der Queue.
    pub fn depth(&self) -> u16 {
        self.depth
    }
    /// Anzahl noch nicht abgeholter Kommandos.
    pub fn pending(&self) -> usize {
        self.submissions.len() + self.inflight.len()
    }
    /// Reiht ein Kommando ein und lehnt doppelte Kennungen ab.
    pub fn submit(&mut self, command: NvmeCommand) -> Result<(), NvmeError> {
        if self.submissions.len() + self.inflight.len() >= self.depth as usize {
            return Err(NvmeError::QueueFull);
        }
        if self
            .submissions
            .iter()
            .chain(self.inflight.iter())
            .any(|c| c.command_id == command.command_id)
            || self
                .completions
                .iter()
                .any(|c| c.command_id == command.command_id)
        {
            return Err(NvmeError::DuplicateCommand);
        }
        if command.command_id == u16::MAX {
            return Err(NvmeError::SubmissionFailed);
        }
        if let Some(buffer) = command.data
            && !buffer.valid()
        {
            return Err(NvmeError::InvalidBuffer);
        }
        self.submissions.push(command);
        Ok(())
    }
    /// Nimmt das naechste Kommando ab, das der Controller holen darf.
    pub fn take_submission(&mut self) -> Option<NvmeCommand> {
        if self.submissions.is_empty() {
            None
        } else {
            let command = self.submissions.remove(0);
            self.inflight.push(command);
            Some(command)
        }
    }
    /// Traegt eine Fertigmeldung ein.
    pub fn complete(&mut self, completion: NvmeCompletion) -> Result<(), NvmeError> {
        if self
            .completions
            .iter()
            .any(|c| c.command_id == completion.command_id)
        {
            return Err(NvmeError::DuplicateCommand);
        }
        let before = self.inflight.len();
        self.inflight
            .retain(|c| c.command_id != completion.command_id);
        let was_inflight = before != self.inflight.len();
        let was_queued = self
            .submissions
            .iter()
            .any(|c| c.command_id == completion.command_id);
        if !was_inflight && !was_queued {
            return Err(NvmeError::SubmissionFailed);
        }
        self.submissions
            .retain(|c| c.command_id != completion.command_id);
        self.completions.push(completion);
        Ok(())
    }
    /// Nimmt die naechste Fertigmeldung ab.
    pub fn poll_completion(&mut self) -> Option<NvmeCompletion> {
        if self.completions.is_empty() {
            None
        } else {
            Some(self.completions.remove(0))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn command(id: u16) -> NvmeCommand {
        NvmeCommand {
            opcode: 1,
            namespace: 1,
            data: Some(NvmeDmaBuffer {
                address: 0x1000,
                length: 512,
            }),
            command_id: id,
        }
    }
    #[test]
    fn queue_round_trip() {
        let mut q = NvmeQueue::new(NvmeQueueId(1), 4).unwrap();
        q.submit(command(1)).unwrap();
        assert_eq!(q.pending(), 1);
        let c = q.take_submission().unwrap();
        assert_eq!(c.command_id, 1);
    }
    #[test]
    fn completion_requires_pending_command() {
        let mut q = NvmeQueue::new(NvmeQueueId(1), 4).unwrap();
        assert_eq!(
            q.complete(NvmeCompletion {
                command_id: 1,
                status: 0,
                result: 0
            }),
            Err(NvmeError::SubmissionFailed)
        );
    }
    #[test]
    fn controller_may_complete_a_taken_command() {
        let mut q = NvmeQueue::new(NvmeQueueId(1), 4).unwrap();
        q.submit(command(7)).unwrap();
        let taken = q.take_submission().unwrap();
        assert_eq!(taken.command_id, 7);
        // Ein abgeholtes Kommando bleibt offen, bis es fertig gemeldet ist.
        assert_eq!(q.pending(), 1);
        q.complete(NvmeCompletion {
            command_id: 7,
            status: 0,
            result: 0,
        })
        .unwrap();
        assert_eq!(q.pending(), 0);
        assert_eq!(q.poll_completion().unwrap().command_id, 7);
    }
    #[test]
    fn second_completion_for_same_command_is_rejected() {
        let mut q = NvmeQueue::new(NvmeQueueId(1), 4).unwrap();
        q.submit(command(8)).unwrap();
        q.take_submission().unwrap();
        let done = NvmeCompletion {
            command_id: 8,
            status: 0,
            result: 0,
        };
        q.complete(done).unwrap();
        assert_eq!(q.complete(done), Err(NvmeError::DuplicateCommand));
    }
    #[test]
    fn rejects_invalid_dma() {
        let mut q = NvmeQueue::new(NvmeQueueId(1), 4).unwrap();
        assert_eq!(
            q.submit(NvmeCommand {
                opcode: 1,
                namespace: 1,
                data: Some(NvmeDmaBuffer {
                    address: u64::MAX,
                    length: 1
                }),
                command_id: 1
            }),
            Err(NvmeError::InvalidBuffer)
        );
    }
}
