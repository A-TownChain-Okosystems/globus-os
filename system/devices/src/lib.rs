//! Isolated device-service registry and hardware contracts.

pub mod acpi;
pub mod amd_iommu;
pub mod apic;
pub mod block;
pub mod block_cache;
pub mod block_manager;
pub mod boot;
pub mod ethernet;
pub mod ethernet_dma;
pub mod interrupt;
pub mod iommu;
pub mod mmio;
pub mod msi;
pub mod nvme;
pub mod nvme_controller;
pub mod nvme_mmio;
pub mod nvme_queue;
pub mod pci;
pub mod pci_ecam;
pub mod pci_enum;
pub mod registry;
pub mod smp;
pub mod timer;

pub use block_cache::{BlockCache, CacheError};
pub use block_manager::{BlockDeviceId, BlockDeviceRegistry};
pub use registry::DeviceRegistry;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DeviceId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceClass {
    Storage,
    Network,
    Display,
    Input,
    Audio,
    Gpu,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    pub id: DeviceId,
    pub name: String,
    pub class: DeviceClass,
}
