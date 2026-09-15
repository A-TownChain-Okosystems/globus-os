//! Generic block-device contract used by NVMe and persistent storage.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockGeometry { pub block_size: u32, pub block_count: u64 }

impl BlockGeometry {
    pub fn capacity_bytes(&self) -> Option<u64> { u64::from(self.block_size).checked_mul(self.block_count) }
    pub fn valid(&self) -> bool { self.block_size.is_power_of_two() && (512..=65536).contains(&self.block_size) && self.block_count > 0 }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockRequest { pub lba: u64, pub blocks: u32, pub write: bool }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockError { InvalidGeometry, OutOfRange, ZeroLength, Overflow, NotReady, Io }

pub fn validate_request(g: BlockGeometry, r: BlockRequest) -> Result<(), BlockError> {
    if !g.valid() { return Err(BlockError::InvalidGeometry); }
    if r.blocks == 0 { return Err(BlockError::ZeroLength); }
    let end = r.lba.checked_add(u64::from(r.blocks)).ok_or(BlockError::Overflow)?;
    if end > g.block_count { return Err(BlockError::OutOfRange); }
    Ok(())
}

pub trait BlockDevice {
    fn geometry(&self) -> BlockGeometry;
    fn read(&mut self, request: BlockRequest, destination: &mut [u8]) -> Result<(), BlockError>;
    fn write(&mut self, request: BlockRequest, source: &[u8]) -> Result<(), BlockError>;
}

#[cfg(test)] mod tests { use super::*; #[test] fn rejects_past_end() { let g=BlockGeometry{block_size:4096,block_count:10}; assert_eq!(validate_request(g,BlockRequest{lba:9,blocks:2,write:false}),Err(BlockError::OutOfRange)); } }
