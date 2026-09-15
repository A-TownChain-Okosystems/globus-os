//! Intel VT-d register programming primitives.
//!
//! AMD IOMMU has a different register and table model and is intentionally kept
//! behind a separate implementation. The common DMA policy remains unchanged.

use super::mmio::MmioWindow;

pub struct IntelVtd {
    regs: MmioWindow,
}

impl IntelVtd {
    pub const GCMD: usize = 0x18;
    pub const GSTS: usize = 0x1c;
    pub const RTADDR: usize = 0x20;
    pub const CCMD: usize = 0x28;
    pub const IOTLB: usize = 0x08;

    pub const GCMD_SRTP: u32 = 1 << 30;
    pub const GCMD_TE: u32 = 1 << 31;
    pub const GSTS_SRTP: u32 = 1 << 30;
    pub const GSTS_TE: u32 = 1 << 31;

    /// `base` must be the virtual address of the VT-d register page mapped as
    /// device memory by the kernel.
    pub const unsafe fn new(base: usize, len: usize) -> Self {
        Self {
            regs: MmioWindow::new(base, len),
        }
    }

    pub fn version(&self) -> u32 {
        self.regs.read32(0x00)
    }

    pub fn capabilities(&self) -> u64 {
        self.regs.read64(0x08)
    }

    /// Programs the root-table physical address and requests hardware to load it.
    /// The root table must already be physically allocated and initialized.
    pub fn set_root_table(&self, physical_address: u64) {
        assert_eq!(physical_address & 0xfff, 0);
        self.regs.write64(Self::RTADDR, physical_address);
        self.regs.write32(Self::GCMD, Self::GCMD_SRTP);
        self.wait_for(Self::GSTS, Self::GSTS_SRTP, true);
    }

    /// Enables DMA translation after root/context tables are installed.
    pub fn enable_translation(&self) {
        let command = self.regs.read32(Self::GCMD) | Self::GCMD_TE;
        self.regs.write32(Self::GCMD, command);
        self.wait_for(Self::GSTS, Self::GSTS_TE, true);
    }

    /// Disables DMA translation before tearing down tables.
    pub fn disable_translation(&self) {
        let command = self.regs.read32(Self::GCMD) & !Self::GCMD_TE;
        self.regs.write32(Self::GCMD, command);
        self.wait_for(Self::GSTS, Self::GSTS_TE, false);
    }

    fn wait_for(&self, offset: usize, mask: u32, set: bool) {
        for _ in 0..1_000_000 {
            let value = self.regs.read32(offset) & mask;
            if (value != 0) == set {
                return;
            }
            core::hint::spin_loop();
        }
        panic!("VT-d register transition timed out");
    }
}
