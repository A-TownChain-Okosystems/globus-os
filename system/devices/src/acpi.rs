//! ACPI table discovery contracts and checksum validation.

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
