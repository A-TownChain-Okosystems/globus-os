//! NVMe controller register programming (NVMe 1.x compatible core registers).

use super::mmio::MmioWindow;

pub struct NvmeMmio {
    regs: MmioWindow,
}

impl NvmeMmio {
    pub const CAP: usize = 0x00;
    pub const VS: usize = 0x08;
    pub const CC: usize = 0x14;
    pub const CSTS: usize = 0x1c;
    pub const AQA: usize = 0x24;
    pub const ASQ: usize = 0x28;
    pub const ACQ: usize = 0x30;

    pub const CC_EN: u32 = 1 << 0;
    pub const CC_SHN_NORMAL: u32 = 1 << 14;
    pub const CSTS_RDY: u32 = 1 << 0;
    pub const CSTS_CFS: u32 = 1 << 1;
    pub const CSTS_SHST_MASK: u32 = 0b11 << 2;
    pub const CSTS_SHST_COMPLETE: u32 = 0b10 << 2;

    pub const unsafe fn new(base: usize, len: usize) -> Self {
        Self {
            regs: MmioWindow::new(base, len),
        }
    }

    pub fn version(&self) -> u32 {
        self.regs.read32(Self::VS)
    }

    pub fn capabilities(&self) -> u64 {
        self.regs.read64(Self::CAP)
    }

    pub fn disable(&self) {
        let cc = self.regs.read32(Self::CC) & !Self::CC_EN;
        self.regs.write32(Self::CC, cc);
        self.wait_ready(false);
    }

    /// Programs the Admin Submission and Completion queues. The addresses must
    /// satisfy the controller's CAP.DSTRD and alignment requirements.
    pub fn configure_admin_queue(&self, depth: u16, asq_physical: u64, acq_physical: u64) {
        assert!(depth >= 2 && depth <= 4096);
        assert_eq!(asq_physical & 0xfff, 0);
        assert_eq!(acq_physical & 0xfff, 0);
        let entries = u32::from(depth - 1);
        self.regs.write32(Self::AQA, entries | (entries << 16));
        self.regs.write64(Self::ASQ, asq_physical);
        self.regs.write64(Self::ACQ, acq_physical);
        self.regs.fence();
    }

    pub fn enable(&self) {
        let cc = self.regs.read32(Self::CC) | Self::CC_EN;
        self.regs.write32(Self::CC, cc);
        self.wait_ready(true);
    }

    pub fn fatal_error(&self) -> bool {
        self.regs.read32(Self::CSTS) & Self::CSTS_CFS != 0
    }

    fn wait_ready(&self, expected: bool) {
        for _ in 0..1_000_000 {
            let ready = self.regs.read32(Self::CSTS) & Self::CSTS_RDY != 0;
            if ready == expected {
                return;
            }
            if self.fatal_error() {
                panic!("NVMe controller fatal status during state transition");
            }
            core::hint::spin_loop();
        }
        panic!("NVMe controller ready transition timed out");
    }
}
