//! NVMe MMIO register and controller initialization contracts.

pub const CAP: u64 = 0x00; pub const VS: u64 = 0x08; pub const CC: u64 = 0x14; pub const CSTS: u64 = 0x1c; pub const AQA: u64 = 0x24; pub const ASQ: u64 = 0x28; pub const ACQ: u64 = 0x30;
pub const CC_EN: u32 = 1; pub const CSTS_RDY: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NvmeControllerConfig { pub admin_queue_depth: u16, pub io_queue_depth: u16, pub page_size: u32 }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NvmeControllerError { InvalidConfig, Timeout, ControllerFatal, QueueSetupFailed, IdentifyFailed }

pub trait NvmeMmio {
    fn read32(&self, offset: u64) -> u32;
    fn read64(&self, offset: u64) -> u64;
    fn write32(&mut self, offset: u64, value: u32);
    fn write64(&mut self, offset: u64, value: u64);
}

pub fn validate_config(c: NvmeControllerConfig) -> Result<(), NvmeControllerError> {
    if !c.admin_queue_depth.is_power_of_two() || !(2..=4096).contains(&c.admin_queue_depth) || !c.io_queue_depth.is_power_of_two() || !(2..=4096).contains(&c.io_queue_depth) || !c.page_size.is_power_of_two() || !(4096..=65536).contains(&c.page_size) { return Err(NvmeControllerError::InvalidConfig); }
    Ok(())
}
