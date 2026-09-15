//! NVMe controller contracts for the device-service layer.
//!
//! This is a hardware-neutral contract. MMIO register programming, queue
//! allocation and interrupt setup must be implemented by the privileged HAL.

pub const NVME_ADMIN_QUEUE_DEPTH: u16 = 32;
pub const NVME_DEFAULT_IO_QUEUE_DEPTH: u16 = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NvmeQueueId(pub u16);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NvmeDmaBuffer { pub address: u64, pub length: u32 }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NvmeCommand { pub opcode: u8, pub namespace: u32, pub data: Option<NvmeDmaBuffer>, pub command_id: u16 }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NvmeCompletion { pub command_id: u16, pub status: u16, pub result: u32 }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NvmeError { InvalidQueueDepth, InvalidBuffer, ControllerNotReady, SubmissionFailed }

pub const fn valid_queue_depth(depth: u16) -> bool { depth >= 2 && depth <= 4096 && depth.is_power_of_two() }

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn queue_depths_are_power_of_two() {
        assert!(valid_queue_depth(NVME_ADMIN_QUEUE_DEPTH));
        assert!(valid_queue_depth(NVME_DEFAULT_IO_QUEUE_DEPTH));
        assert!(!valid_queue_depth(255));
    }
}