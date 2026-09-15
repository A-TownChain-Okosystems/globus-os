//! PCI Express Enhanced Configuration Access Mechanism (ECAM).

use super::{mmio::MmioWindow, pci::PciAddress};

pub struct PciEcam {
    window: MmioWindow,
    segment: u16,
    bus_start: u8,
}

impl PciEcam {
    /// Creates an ECAM accessor for a firmware-described ECAM MMIO window.
    pub const unsafe fn new(window: MmioWindow, segment: u16, bus_start: u8) -> Self {
        Self { window, segment, bus_start }
    }

    fn offset(&self, address: PciAddress, register: u16) -> usize {
        assert_eq!(address.segment, self.segment);
        assert!(register < 0x1000 && register & 3 == 0);
        let bus = address.bus.checked_sub(self.bus_start).expect("PCI bus outside ECAM range") as usize;
        (bus << 20) | ((address.device as usize) << 15) | ((address.function as usize) << 12) | register as usize
    }

    pub fn read32(&self, address: PciAddress, register: u16) -> u32 {
        self.window.read32(self.offset(address, register))
    }

    pub fn write32(&self, address: PciAddress, register: u16, value: u32) {
        self.window.write32(self.offset(address, register), value);
    }

    pub fn enable_bus_master(&self, address: PciAddress) {
        let command = self.read32(address, 0x04);
        self.write32(address, 0x04, command | (1 << 2));
        self.window.fence();
    }

    pub fn vendor_device(&self, address: PciAddress) -> u32 {
        self.read32(address, 0x00)
    }

    pub fn class_code(&self, address: PciAddress) -> u32 {
        self.read32(address, 0x08)
    }
}
