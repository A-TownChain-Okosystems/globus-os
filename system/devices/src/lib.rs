//! Isolated device-service registry.

pub mod acpi;
pub mod ethernet_dma;
pub mod iommu;
pub mod mmio;
pub mod nvme;
pub mod nvme_mmio;
pub mod nvme_queue;
pub mod pci;
pub mod pci_ecam;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DeviceId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceClass {
    Storage,
    Network,
    Display,
    Input,
    Audio,
    Usb,
    Gpu,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    pub id: DeviceId,
    pub class: DeviceClass,
    pub driver: String,
}
