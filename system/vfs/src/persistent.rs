//! Minimal persistent filesystem metadata layer over a generic block device.

use globus_devices::block::{BlockDevice, BlockError, BlockRequest};

const MAGIC: &[u8; 8] = b"GLOBFS01";
const HEADER_SIZE: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Superblock {
    pub block_size: u32,
    pub total_blocks: u64,
    pub metadata_start: u64,
    pub metadata_blocks: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FsError {
    Io,
    InvalidGeometry,
    InvalidSuperblock,
    Buffer,
    Overflow,
}

fn checksum(bytes: &[u8]) -> u32 {
    let mut hash = 0x811c9dc5u32;
    for byte in bytes { hash ^= u32::from(*byte); hash = hash.wrapping_mul(0x01000193); }
    hash
}

impl Superblock {
    pub fn encode(&self, out: &mut [u8]) -> Result<(), FsError> {
        if out.len() < HEADER_SIZE { return Err(FsError::Buffer); }
        out[..HEADER_SIZE].fill(0);
        out[..8].copy_from_slice(MAGIC);
        out[8..12].copy_from_slice(&self.block_size.to_le_bytes());
        out[12..20].copy_from_slice(&self.total_blocks.to_le_bytes());
        out[20..28].copy_from_slice(&self.metadata_start.to_le_bytes());
        out[28..32].copy_from_slice(&checksum(&out[..28]).to_le_bytes());
        Ok(())
    }

    pub fn decode(input: &[u8]) -> Result<Self, FsError> {
        if input.len() < HEADER_SIZE || &input[..8] != MAGIC { return Err(FsError::InvalidSuperblock); }
        let expected = u32::from_le_bytes(input[28..32].try_into().map_err(|_| FsError::Buffer)?);
        if checksum(&input[..28]) != expected { return Err(FsError::InvalidSuperblock); }
        let block_size = u32::from_le_bytes(input[8..12].try_into().map_err(|_| FsError::Buffer)?);
        let total_blocks = u64::from_le_bytes(input[12..20].try_into().map_err(|_| FsError::Buffer)?);
        let metadata_start = u64::from_le_bytes(input[20..28].try_into().map_err(|_| FsError::Buffer)?);
        if !block_size.is_power_of_two() || !(512..=65536).contains(&block_size) || total_blocks == 0 || metadata_start >= total_blocks { return Err(FsError::InvalidSuperblock); }
        Ok(Self { block_size, total_blocks, metadata_start, metadata_blocks: 1 })
    }
}

pub struct PersistentFs<D> { device: D, superblock: Superblock }

impl<D: BlockDevice> PersistentFs<D> {
    pub fn format(mut device: D, metadata_blocks: u64) -> Result<Self, FsError> {
        let geometry = device.geometry();
        if !geometry.valid() || metadata_blocks == 0 || metadata_blocks >= geometry.block_count { return Err(FsError::InvalidGeometry); }
        let superblock = Superblock { block_size: geometry.block_size, total_blocks: geometry.block_count, metadata_start: 1, metadata_blocks };
        let mut block = vec![0u8; geometry.block_size as usize];
        superblock.encode(&mut block)?;
        device.write(BlockRequest { lba: 0, blocks: 1, write: true }, &block).map_err(|_| FsError::Io)?;
        Ok(Self { device, superblock })
    }

    pub fn mount(mut device: D) -> Result<Self, FsError> {
        let geometry = device.geometry();
        let mut block = vec![0u8; geometry.block_size as usize];
        device.read(BlockRequest { lba: 0, blocks: 1, write: false }, &mut block).map_err(|_| FsError::Io)?;
        let superblock = Superblock::decode(&block)?;
        if superblock.block_size != geometry.block_size || superblock.total_blocks != geometry.block_count { return Err(FsError::InvalidSuperblock); }
        Ok(Self { device, superblock })
    }

    pub fn superblock(&self) -> Superblock { self.superblock }
    pub fn device_geometry(&self) -> globus_devices::block::BlockGeometry { self.device.geometry() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use globus_devices::block::{BlockGeometry, MemoryBlockDevice};

    #[test]
    fn format_and_mount_round_trip() {
        let device = MemoryBlockDevice::new(BlockGeometry { block_size: 512, block_count: 32 }).unwrap();
        let fs = PersistentFs::format(device, 4).unwrap();
        let device = fs.device;
        let mounted = PersistentFs::mount(device).unwrap();
        assert_eq!(mounted.superblock().metadata_blocks, 4);
    }
}
