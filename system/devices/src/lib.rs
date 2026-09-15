//! Isolated device-service registry.

pub mod nvme;
pub mod pci;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DeviceId(pub u64);
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceClass { Storage, Network, Display, Input, Audio, Usb, Gpu, Other }
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device { pub id: DeviceId, pub class: DeviceClass, pub driver: String }
