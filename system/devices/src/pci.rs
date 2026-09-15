//! PCI/PCIe discovery and MMIO contracts.
//!
//! Raw config-space access and BAR mapping remain privileged HAL operations.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PciAddress { pub segment: u16, pub bus: u8, pub device: u8, pub function: u8 }

impl PciAddress {
    pub const fn new(segment: u16, bus: u8, device: u8, function: u8) -> Self { Self { segment, bus, device, function } }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PciId { pub vendor: u16, pub device: u16, pub class: u8, pub subclass: u8, pub prog_if: u8 }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BarKind { Memory32, Memory64, Io }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PciBar { pub index: u8, pub kind: BarKind, pub base: u64, pub size: u64, pub prefetchable: bool }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PciError { InvalidAddress, ConfigAccessFailed, InvalidBar, MappingDenied }

pub trait PciConfigAccess {
    fn read_u32(&self, address: PciAddress, offset: u16) -> Result<u32, PciError>;
    fn write_u32(&self, address: PciAddress, offset: u16, value: u32) -> Result<(), PciError>;
}

pub trait MmioMapper {
    type Mapping;
    fn map_device_bar(&self, bar: PciBar) -> Result<Self::Mapping, PciError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn supports_64bit_bar_addresses() {
        let bar = PciBar { index: 0, kind: BarKind::Memory64, base: 0x1_0000_0000, size: 0x1000, prefetchable: false };
        assert!(bar.base > u32::MAX as u64);
    }
}