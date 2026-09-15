//! Intel 8254x/e1000-class Ethernet DMA programming.
//!
//! This is a concrete reference driver for controllers exposing the e1000
//! descriptor/register model. Other NIC families must provide their own driver.

use super::mmio::MmioWindow;

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct LegacyRxDescriptor {
    pub buffer_address: u64,
    pub length: u16,
    pub checksum: u16,
    pub status: u8,
    pub errors: u8,
    pub special: u16,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct LegacyTxDescriptor {
    pub buffer_address: u64,
    pub length: u16,
    pub cso: u8,
    pub command: u8,
    pub status: u8,
    pub css: u8,
    pub special: u16,
}

pub struct E1000Dma {
    regs: MmioWindow,
}

impl E1000Dma {
    pub const CTRL: usize = 0x0000;
    pub const STATUS: usize = 0x0008;
    pub const RCTL: usize = 0x0100;
    pub const TCTL: usize = 0x0400;
    pub const RDBAL: usize = 0x2800;
    pub const RDBAH: usize = 0x2804;
    pub const RDLEN: usize = 0x2808;
    pub const RDH: usize = 0x2810;
    pub const RDT: usize = 0x2818;
    pub const TDBAL: usize = 0x3800;
    pub const TDBAH: usize = 0x3804;
    pub const TDLEN: usize = 0x3808;
    pub const TDH: usize = 0x3810;
    pub const TDT: usize = 0x3818;

    pub const CTRL_RST: u32 = 1 << 26;
    pub const RCTL_EN: u32 = 1 << 1;
    pub const RCTL_BAM: u32 = 1 << 15;
    pub const TCTL_EN: u32 = 1 << 1;
    pub const TCTL_PSP: u32 = 1 << 3;
    pub const TX_CMD_EOP: u8 = 1 << 0;
    pub const TX_CMD_IFCS: u8 = 1 << 1;
    pub const TX_CMD_RS: u8 = 1 << 3;
    pub const TX_STATUS_DD: u8 = 1 << 0;

    pub const unsafe fn new(base: usize, len: usize) -> Self {
        Self { regs: MmioWindow::new(base, len) }
    }

    pub fn reset(&self) {
        self.regs.write32(Self::CTRL, Self::CTRL_RST);
        for _ in 0..1_000_000 {
            if self.regs.read32(Self::CTRL) & Self::CTRL_RST == 0 {
                return;
            }
            core::hint::spin_loop();
        }
        panic!("e1000 reset timed out");
    }

    /// Programs a physically contiguous descriptor ring. The ring length must
    /// satisfy the controller's descriptor alignment constraints.
    pub fn configure_rx(&self, ring_physical: u64, ring_bytes: u32, count: u16) {
        assert!(count >= 8);
        self.regs.write32(Self::RDBAL, ring_physical as u32);
        self.regs.write32(Self::RDBAH, (ring_physical >> 32) as u32);
        self.regs.write32(Self::RDLEN, ring_bytes);
        self.regs.write32(Self::RDH, 0);
        self.regs.write32(Self::RDT, u32::from(count - 1));
        self.regs.write32(Self::RCTL, Self::RCTL_EN | Self::RCTL_BAM);
        self.regs.fence();
    }

    pub fn configure_tx(&self, ring_physical: u64, ring_bytes: u32) {
        self.regs.write32(Self::TDBAL, ring_physical as u32);
        self.regs.write32(Self::TDBAH, (ring_physical >> 32) as u32);
        self.regs.write32(Self::TDLEN, ring_bytes);
        self.regs.write32(Self::TDH, 0);
        self.regs.write32(Self::TDT, 0);
        self.regs.write32(Self::TCTL, Self::TCTL_EN | Self::TCTL_PSP);
        self.regs.fence();
    }

    /// Publishes one TX descriptor by advancing TDT after all descriptor fields
    /// and packet bytes are visible to the device.
    pub fn publish_tx(&self, index: u16, ring_count: u16) {
        assert!(index < ring_count);
        self.regs.fence();
        self.regs.write32(Self::TDT, u32::from(index));
    }
}
