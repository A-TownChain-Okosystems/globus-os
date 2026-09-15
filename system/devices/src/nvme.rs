//! NVMe service contract. Queue/doorbell/MMIO operations belong to the driver process.

use super::pci::{DmaPolicy, PciAddress};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NvmeController {
    pub pci: PciAddress,
    pub namespace_count: u32,
    pub dma: DmaPolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NvmeNamespace {
    pub controller: PciAddress,
    pub namespace_id: u32,
    pub block_size: u32,
    pub block_count: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NvmeQueueConfig {
    pub submission_depth: u16,
    pub completion_depth: u16,
}

impl NvmeQueueConfig {
    pub const DEFAULT: Self = Self {
        submission_depth: 64,
        completion_depth: 64,
    };
}
