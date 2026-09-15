//! AMD IOMMU hardware-facing register programming primitives.
//!
//! The kernel must map the IOMMU MMIO window as device memory and provide a
//! physically allocated device table. Device-table entries and command-buffer
//! construction remain policy-owned by the kernel/IOMMU service.

use super::mmio::MmioWindow;

pub struct AmdIommu {
    regs: MmioWindow,
}

impl AmdIommu {
    pub const DEVICE_TABLE_BASE: usize = 0x00;
    pub const COMMAND: usize = 0x08;
    pub const CONTROL: usize = 0x18;
    pub const STATUS: usize = 0x2020;

    pub const CONTROL_IOMMU_ENABLE: u64 = 1 << 0;
    pub const COMMAND_COMPLETION_WAIT: u32 = 1 << 0;
    pub const COMMAND_INVALIDATE_IOTLB: u32 = 1 << 3;
    pub const COMMAND_INVALIDATE_DEVICE: u32 = 1 << 2;

    /// `base` must point at the kernel-mapped AMD IOMMU MMIO window.
    pub const unsafe fn new(base: usize, len: usize) -> Self {
        Self { regs: MmioWindow::new(base, len) }
    }

    pub fn version(&self) -> u32 {
        self.regs.read32(0x30)
    }

    pub fn capabilities(&self) -> u64 {
        self.regs.read64(0x10)
    }

    pub fn set_device_table(&self, physical_address: u64) {
        assert_eq!(physical_address & 0xfff, 0);
        self.regs.write64(Self::DEVICE_TABLE_BASE, physical_address);
        self.regs.fence();
    }

    pub fn enable(&self) {
        let control = self.regs.read64(Self::CONTROL) | Self::CONTROL_IOMMU_ENABLE;
        self.regs.write64(Self::CONTROL, control);
        self.regs.fence();
    }

    pub fn disable(&self) {
        let control = self.regs.read64(Self::CONTROL) & !Self::CONTROL_IOMMU_ENABLE;
        self.regs.write64(Self::CONTROL, control);
        self.regs.fence();
    }

    pub fn invalidate_device(&self) {
        self.regs.write32(Self::COMMAND, Self::COMMAND_INVALIDATE_DEVICE);
        self.regs.fence();
    }

    pub fn invalidate_iotlb(&self) {
        self.regs.write32(Self::COMMAND, Self::COMMAND_INVALIDATE_IOTLB);
        self.regs.fence();
    }
}
