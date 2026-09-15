//! Protective GPT parser for persistent-disk discovery.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GptHeader { pub first_usable_lba: u64, pub last_usable_lba: u64, pub partition_entry_lba: u64, pub partition_count: u32, pub partition_size: u32 }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GptError { TooSmall, BadSignature, BadHeaderSize, BadCrc, InvalidRange, InvalidEntries }

fn crc32(bytes: &[u8]) -> u32 { let mut crc=0xffff_ffffu32; for &b in bytes { crc ^= u32::from(b); for _ in 0..8 { crc=(crc>>1)^((crc & 1).wrapping_mul(0xedb8_8320)); } } !crc }

pub fn parse_header(sector: &[u8]) -> Result<GptHeader, GptError> {
    if sector.len() < 92 { return Err(GptError::TooSmall); }
    if &sector[0..8] != b"EFI PART" { return Err(GptError::BadSignature); }
    let size=u32::from_le_bytes(sector[12..16].try_into().unwrap()); if !(92..=512).contains(&size) || size as usize > sector.len() { return Err(GptError::BadHeaderSize); }
    let stored=u32::from_le_bytes(sector[16..20].try_into().unwrap()); let mut h=sector[..size as usize].to_vec(); h[16..20].fill(0); if crc32(&h)!=stored { return Err(GptError::BadCrc); }
    let first=u64::from_le_bytes(sector[48..56].try_into().unwrap()); let last=u64::from_le_bytes(sector[56..64].try_into().unwrap()); let entries=u32::from_le_bytes(sector[80..84].try_into().unwrap()); let entry_size=u32::from_le_bytes(sector[84..88].try_into().unwrap());
    if first>last || entries==0 || !(128..=4096).contains(&entry_size) || !entry_size.is_multiple_of(8) { return Err(GptError::InvalidRange); }
    Ok(GptHeader{first_usable_lba:first,last_usable_lba:last,partition_entry_lba:u64::from_le_bytes(sector[72..80].try_into().unwrap()),partition_count:entries,partition_size:entry_size})
}
