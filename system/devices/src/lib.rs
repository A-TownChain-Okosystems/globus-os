//! Isolated device-service registry and hardware contracts.
//!
//! `no_std` ab 07.11.2026: Diese Schicht ist ein Vertrag, kein Dienst. Der
//! Service Space (no_std) haengt daran, und ein std-Crate haette ihn vom
//! Bare-Metal-Ziel ausgeschlossen (`can't find crate for std`). Tests duerfen
//! weiter std benutzen.
#![cfg_attr(not(test), no_std)]

extern crate alloc;

use alloc::string::String;

pub mod acpi;
pub mod apic;
pub mod block;
pub mod block_cache;
pub mod block_manager;
pub mod boot;
pub mod driver_lifecycle;
pub mod ethernet;
pub mod interrupt;
pub mod iommu;
pub mod msi;
pub mod nvme;
pub mod nvme_controller;
pub mod pci;
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
