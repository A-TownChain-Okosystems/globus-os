//! Crash-consistent GlobusState persistent store.
//!
//! This is the first on-disk filesystem primitive for GlobusOS state/config data.
//! It uses a fixed-size superblock plus an append-only journal. Higher-level VFS
//! namespaces can build on the journal without depending on a host filesystem.

pub const BLOCK_SIZE: usize = 4096;
pub const SUPERBLOCK_MAGIC: u64 = 0x4753_5441_5445_3031;
pub const FORMAT_VERSION: u32 = 1;
pub const RECORD_MAGIC: u32 = 0x4753_5243;

pub trait BlockIo {
    fn block_count(&self) -> u64;
    fn read_block(&mut self, index: u64, block: &mut [u8; BLOCK_SIZE]) -> Result<(), FsError>;
    fn write_block(&mut self, index: u64, block: &[u8; BLOCK_SIZE]) -> Result<(), FsError>;
    fn flush(&mut self) -> Result<(), FsError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FsError {
    InvalidDevice,
    InvalidSuperblock,
    CorruptRecord,
    NoSpace,
    Io,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
struct Superblock {
    magic: u64,
    version: u32,
    block_size: u32,
    generation: u64,
    journal_start: u64,
    journal_blocks: u64,
    journal_tail: u64,
    checksum: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
struct RecordHeader {
    magic: u32,
    kind: u16,
    flags: u16,
    sequence: u64,
    key: u64,
    payload_len: u32,
    checksum: u32,
}

pub struct GlobusStateFs<D> {
    device: D,
    superblock: Superblock,
}

impl<D: BlockIo> GlobusStateFs<D> {
    pub fn format(mut device: D, journal_start: u64) -> Result<Self, FsError> {
        if device.block_count() < journal_start + 2 {
            return Err(FsError::InvalidDevice);
        }
        let mut block = [0u8; BLOCK_SIZE];
        let mut sb = Superblock {
            magic: SUPERBLOCK_MAGIC,
            version: FORMAT_VERSION,
            block_size: BLOCK_SIZE as u32,
            generation: 1,
            journal_start,
            journal_blocks: device.block_count() - journal_start,
            journal_tail: 0,
            checksum: 0,
        };
        sb.checksum = checksum(&as_bytes(&sb)[..40]);
        write_struct(&mut block, &sb);
        device.write_block(0, &block)?;
        device.flush()?;
        Ok(Self { device, superblock: sb })
    }

    pub fn mount(mut device: D) -> Result<Self, FsError> {
        let mut block = [0u8; BLOCK_SIZE];
        device.read_block(0, &mut block)?;
        let sb: Superblock = read_struct(&block)?;
        if sb.magic != SUPERBLOCK_MAGIC
            || sb.version != FORMAT_VERSION
            || sb.block_size != BLOCK_SIZE as u32
            || sb.checksum != checksum(&block[..40])
            || sb.journal_start >= device.block_count()
            || sb.journal_start + sb.journal_blocks > device.block_count()
        {
            return Err(FsError::InvalidSuperblock);
        }
        Ok(Self { device, superblock: sb })
    }

    pub fn append(&mut self, kind: u16, key: u64, payload: &[u8]) -> Result<u64, FsError> {
        if payload.len() > BLOCK_SIZE - core::mem::size_of::<RecordHeader>() {
            return Err(FsError::NoSpace);
        }
        let tail = self.superblock.journal_tail;
        if tail >= self.superblock.journal_blocks {
            return Err(FsError::NoSpace);
        }
        let sequence = self.superblock.generation;
        let mut block = [0u8; BLOCK_SIZE];
        let mut header = RecordHeader {
            magic: RECORD_MAGIC,
            kind,
            flags: 0,
            sequence,
            key,
            payload_len: payload.len() as u32,
            checksum: 0,
        };
        block[core::mem::size_of::<RecordHeader>()
            ..core::mem::size_of::<RecordHeader>() + payload.len()]
            .copy_from_slice(payload);
        header.checksum = checksum(&block[..core::mem::size_of::<RecordHeader>() - 4 + payload.len()]);
        write_struct(&mut block, &header);
        self.device.write_block(self.superblock.journal_start + tail, &block)?;
        self.device.flush()?;

        self.superblock.journal_tail += 1;
        self.superblock.generation += 1;
        self.persist_superblock()?;
        Ok(sequence)
    }

    pub fn replay<F: FnMut(u16, u64, &[u8])>(&mut self, mut apply: F) -> Result<(), FsError> {
        let header_size = core::mem::size_of::<RecordHeader>();
        let mut block = [0u8; BLOCK_SIZE];
        for index in 0..self.superblock.journal_tail {
            self.device.read_block(self.superblock.journal_start + index, &mut block)?;
            let header: RecordHeader = read_struct(&block)?;
            let end = header_size.checked_add(header.payload_len as usize).ok_or(FsError::CorruptRecord)?;
            if header.magic != RECORD_MAGIC || end > BLOCK_SIZE {
                return Err(FsError::CorruptRecord);
            }
            let mut check = block;
            check[header_size - 4..header_size].fill(0);
            if checksum(&check[..header_size - 4 + header.payload_len as usize]) != header.checksum {
                return Err(FsError::CorruptRecord);
            }
            apply(header.kind, header.key, &block[header_size..end]);
        }
        Ok(())
    }

    fn persist_superblock(&mut self) -> Result<(), FsError> {
        let mut block = [0u8; BLOCK_SIZE];
        let mut sb = self.superblock;
        sb.checksum = 0;
        sb.checksum = checksum(&as_bytes(&sb)[..40]);
        write_struct(&mut block, &sb);
        self.device.write_block(0, &block)?;
        self.device.flush()
    }
}

fn checksum(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for byte in data {
        crc ^= *byte as u32;
        for _ in 0..8 {
            let mask = 0u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}

fn as_bytes<T>(value: &T) -> &[u8] {
    unsafe { core::slice::from_raw_parts((value as *const T).cast(), core::mem::size_of::<T>()) }
}

fn write_struct<T: Copy>(block: &mut [u8; BLOCK_SIZE], value: &T) {
    block[..core::mem::size_of::<T>()].copy_from_slice(as_bytes(value));
}

fn read_struct<T: Copy>(block: &[u8; BLOCK_SIZE]) -> Result<T, FsError> {
    if core::mem::size_of::<T>() > BLOCK_SIZE {
        return Err(FsError::CorruptRecord);
    }
    Ok(unsafe { core::ptr::read_unaligned(block.as_ptr().cast::<T>()) })
}
