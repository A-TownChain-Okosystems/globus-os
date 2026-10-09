//! Minimal ACPI table discovery primitives for hardware bring-up and checksum validation.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AcpiTableHeader {
    pub signature: [u8; 4],
    pub length: u32,
    pub revision: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DmarInfo {
    pub host_address_width: u8,
    pub flags: u8,
    pub remapping_units: usize,
}

pub fn validate_table(bytes: &[u8]) -> Option<AcpiTableHeader> {
    if bytes.len() < 36 || bytes.len() < 4 {
        return None;
    }
    let length = u32::from_le_bytes(bytes[4..8].try_into().ok()?);
    if length < 36 || length as usize > bytes.len() {
        return None;
    }
    let checksum = bytes[..length as usize]
        .iter()
        .fold(0u8, |sum, value| sum.wrapping_add(*value));
    if checksum != 0 {
        return None;
    }
    Some(AcpiTableHeader {
        signature: bytes[0..4].try_into().ok()?,
        length,
        revision: bytes[8],
    })
}

/// Parse the fixed DMAR header and count DRHD/ATSR/RMRR/ANDD structures.
pub fn parse_dmar(bytes: &[u8]) -> Option<DmarInfo> {
    let header = validate_table(bytes)?;
    if &header.signature != b"DMAR" || bytes.len() < 48 {
        return None;
    }
    let host_address_width = bytes[36];
    let flags = bytes[37];
    let mut offset = 48usize;
    let mut remapping_units = 0usize;
    while offset + 4 <= header.length as usize {
        let typ = u16::from_le_bytes(bytes[offset..offset + 2].try_into().ok()?);
        let len = u16::from_le_bytes(bytes[offset + 2..offset + 4].try_into().ok()?) as usize;
        if len < 4 || offset + len > header.length as usize {
            return None;
        }
        if typ == 0 {
            remapping_units += 1;
        }
        offset += len;
    }
    Some(DmarInfo {
        host_address_width,
        flags,
        remapping_units,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RsdpV1 {
    pub revision: u8,
    pub rsdt_address: u32,
    pub checksum: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RsdpV2 {
    pub revision: u8,
    pub rsdt_address: u32,
    pub xsdt_address: u64,
    pub length: u32,
    pub checksum: u8,
    pub extended_checksum: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcpiError {
    InvalidSignature,
    InvalidChecksum,
    InvalidLength,
    InvalidAddress,
}

pub fn checksum_valid(bytes: &[u8]) -> bool {
    bytes.iter().fold(0u8, |a, &b| a.wrapping_add(b)) == 0
}

pub fn validate_rsdp(bytes: &[u8]) -> Result<(), AcpiError> {
    if bytes.len() < 20 || &bytes[0..8] != b"RSD PTR " {
        return Err(AcpiError::InvalidSignature);
    }
    if !checksum_valid(&bytes[..20]) {
        return Err(AcpiError::InvalidChecksum);
    }
    if bytes[15] >= 2 {
        if bytes.len() < 36 {
            return Err(AcpiError::InvalidLength);
        }
        let length = u32::from_le_bytes(bytes[20..24].try_into().unwrap());
        if !(36..=4096).contains(&length) || length as usize > bytes.len() {
            return Err(AcpiError::InvalidLength);
        }
        if !checksum_valid(&bytes[..length as usize]) {
            return Err(AcpiError::InvalidChecksum);
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct McfgWindow {
    pub base_address: u64,
    pub segment: u16,
    pub start_bus: u8,
    pub end_bus: u8,
}

impl McfgWindow {
    pub fn ecam_address(&self, bus: u8, device: u8, function: u8, offset: u16) -> Option<u64> {
        if bus < self.start_bus
            || bus > self.end_bus
            || device >= 32
            || function >= 8
            || offset >= 4096
            || offset & 3 != 0
        {
            return None;
        }
        let bus_delta = u64::from(bus - self.start_bus);
        self.base_address.checked_add(
            (bus_delta << 20)
                | (u64::from(device) << 15)
                | (u64::from(function) << 12)
                | u64::from(offset),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_short_and_invalid_tables() {
        assert!(validate_table(&[]).is_none());
        assert!(parse_dmar(&[]).is_none());
    }

    #[test]
    fn ecam_address_is_bounded() {
        let w = McfgWindow {
            base_address: 0xE000_0000,
            segment: 0,
            start_bus: 0,
            end_bus: 255,
        };
        assert_eq!(w.ecam_address(0, 0, 0, 0), Some(0xE000_0000));
        assert!(w.ecam_address(0, 32, 0, 0).is_none());
    }
}
