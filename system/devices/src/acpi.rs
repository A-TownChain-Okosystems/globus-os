//! Minimal ACPI table discovery primitives for hardware bring-up.
//!
//! The UEFI loader supplies the RSDP physical address. The kernel maps ACPI
//! tables read-only before calling these routines. No ACPI table is trusted
//! until its checksum and length have been validated.

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
///
/// Device-scope decoding is intentionally deferred until the IOMMU policy
/// layer has established the isolation domain for each PCI requester ID.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_short_and_invalid_tables() {
        assert!(validate_table(&[]).is_none());
        assert!(parse_dmar(&[]).is_none());
    }
}
