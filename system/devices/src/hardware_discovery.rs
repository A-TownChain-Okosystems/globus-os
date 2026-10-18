//! PCI/ACPI-backed hardware inventory discovery and resource policy.
//!
//! This module only reads PCI configuration through the supplied privileged
//! HAL adapter. It never maps BARs, routes interrupts, or grants DMA access.
//! Those operations must be performed by the HAL or an authorized device
//! service after the returned metadata has been validated.

use alloc::string::String;
use alloc::vec::Vec;

use crate::acpi::{self, AcpiError, McfgWindow};
use crate::hardware_inventory::{
    HardwareInventory, HardwareLocation, HardwareRecord, HardwareStatus, InventoryError,
};
use crate::interrupt::InterruptVector;
use crate::iommu::{self, DmaRegion, IommuError};
use crate::pci::{self, BarKind, PciAddress, PciBar, PciConfigAccess, PciError, PciId};
use crate::{DeviceClass, DeviceId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscoveryError {
    InvalidMcfg(AcpiError),
    InvalidWindow,
    Config(PciError),
    Inventory(InventoryError),
}

/// Validate an MCFG table and decode its ECAM allocation windows.
/// Overlapping bus ranges in the same segment are rejected to avoid ambiguous
/// routing of a BDF to multiple physical ECAM regions.
pub fn parse_mcfg_windows(bytes: &[u8]) -> Result<Vec<McfgWindow>, DiscoveryError> {
    acpi::validate_mcfg_table(bytes).map_err(DiscoveryError::InvalidMcfg)?;
    const FIXED: usize = 44;
    const ENTRY: usize = 16;
    let length = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]) as usize;
    let mut windows = Vec::new();
    for raw in bytes[FIXED..length].chunks_exact(ENTRY) {
        let window = McfgWindow {
            base_address: u64::from_le_bytes(raw[0..8].try_into().map_err(|_| {
                DiscoveryError::InvalidMcfg(AcpiError::InvalidLength)
            })?),
            start_bus: raw[8],
            end_bus: raw[9],
            segment: u16::from_le_bytes([raw[10], raw[11]]),
        };
        window.validate().map_err(DiscoveryError::InvalidMcfg)?;
        if windows.iter().any(|existing: &McfgWindow| {
            existing.segment == window.segment
                && existing.start_bus <= window.end_bus
                && window.start_bus <= existing.end_bus
        }) {
            return Err(DiscoveryError::InvalidWindow);
        }
        windows.push(window);
    }
    Ok(windows)
}

/// Convert PCI class metadata into the inventory's broad device classes.
pub const fn class_from_pci(id: PciId) -> DeviceClass {
    match (id.class, id.subclass) {
        (0x01, _) => DeviceClass::Storage,
        (0x02, _) => DeviceClass::Network,
        (0x03, _) => DeviceClass::Display,
        (0x04, _) => DeviceClass::Audio,
        (0x0C, 0x03) => DeviceClass::Usb,
        _ => DeviceClass::Other,
    }
}

/// Deterministic inventory ID derived from the complete PCI segment:BDF tuple.
pub const fn device_id(address: PciAddress) -> DeviceId {
    DeviceId(
        ((address.segment as u64) << 16)
            | ((address.bus as u64) << 8)
            | ((address.device as u64) << 3)
            | address.function as u64,
    )
}

/// Scan only bus ranges covered by validated MCFG windows. Reads configuration
/// through a HAL-owned adapter; no direct port-I/O or ECAM dereference occurs here.
pub fn discover_pci<A: PciConfigAccess>(
    access: &A,
    windows: &[McfgWindow],
    inventory: &mut HardwareInventory,
) -> Result<usize, DiscoveryError> {
    let mut discovered = 0;
    for (index, window) in windows.iter().enumerate() {
        window.validate().map_err(DiscoveryError::InvalidMcfg)?;
        if windows[..index].iter().any(|previous| {
            previous.segment == window.segment
                && previous.start_bus <= window.end_bus
                && window.start_bus <= previous.end_bus
        }) {
            return Err(DiscoveryError::InvalidWindow);
        }
        for bus in window.start_bus..=window.end_bus {
            for device in 0..32u8 {
                let first = PciAddress::new(window.segment, bus, device, 0);
                let identity = read_config(access, first, 0)?;
                if identity as u16 == 0xFFFF {
                    continue;
                }
                let header = read_config(access, first, 0x0C)?;
                let functions = if ((header >> 16) as u8) & 0x80 != 0 { 8 } else { 1 };
                for function in 0..functions {
                    let address = PciAddress::new(window.segment, bus, device, function);
                    let identity = if function == 0 {
                        identity
                    } else {
                        read_config(access, address, 0)?
                    };
                    let vendor = identity as u16;
                    if vendor == 0xFFFF {
                        continue;
                    }
                    let device_id_raw = (identity >> 16) as u16;
                    let class_register = read_config(access, address, 0x08)?;
                    let revision = class_register as u8;
                    let prog_if = (class_register >> 8) as u8;
                    let subclass = (class_register >> 16) as u8;
                    let class = (class_register >> 24) as u8;
                    let id = pci::decode_id(class, subclass, prog_if, vendor, device_id_raw);
                    let record = HardwareRecord {
                        id: device_id(address),
                        class: class_from_pci(id),
                        location: HardwareLocation::Pci {
                            segment: address.segment,
                            bus: address.bus,
                            device: address.device,
                            function: address.function,
                        },
                        vendor_id: Some(vendor as u32),
                        device_id: Some(device_id_raw as u32),
                        revision: Some(revision),
                        status: HardwareStatus::Discovered,
                        driver: None,
                        capabilities: alloc::vec![
                            String::from("pci"),
                            alloc::format!("pci-class-{class:02x}"),
                        ],
                    };
                    inventory.register(record).map_err(DiscoveryError::Inventory)?;
                    discovered += 1;
                }
            }
        }
    }
    Ok(discovered)
}

fn read_config<A: PciConfigAccess>(
    access: &A,
    address: PciAddress,
    offset: u16,
) -> Result<u32, DiscoveryError> {
    if !address.valid() {
        return Err(DiscoveryError::Config(PciError::InvalidAddress));
    }
    access
        .read_u32(address, offset)
        .map_err(DiscoveryError::Config)
}

/// Validate BAR metadata without mapping it. MMIO mapping is a separate,
/// privileged operation and must be requested only after this succeeds.
pub fn validate_bar_resource(bar: PciBar) -> Result<(), PciError> {
    pci::validate_bar(bar)?;
    match bar.kind {
        BarKind::Memory32 if bar.base > u32::MAX as u64 => Err(PciError::InvalidBar),
        BarKind::Io if bar.base > u16::MAX as u64 => Err(PciError::InvalidBar),
        _ => Ok(()),
    }
}

/// Validate an interrupt vector separately from BAR and DMA validation.
pub fn validate_interrupt_vector(vector: u16) -> Result<InterruptVector, ResourceError> {
    let vector = InterruptVector(vector);
    if vector.valid() {
        Ok(vector)
    } else {
        Err(ResourceError::InvalidInterrupt)
    }
}

/// Validate DMA address arithmetic only. This does not authorize a device,
/// establish an IOMMU domain, pin pages, or install hardware mappings.
pub fn validate_dma_region(region: DmaRegion) -> Result<(), IommuError> {
    iommu::validate_region(region)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceError {
    InvalidInterrupt,
}

/// Driver profiles can match exact IDs and/or PCI class codes. A profile match
/// means "candidate supported", not that initialization or capabilities succeeded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DriverProfile {
    pub name: &'static str,
    pub vendor_id: Option<u16>,
    pub device_id: Option<u16>,
    pub class: Option<u8>,
    pub subclass: Option<u8>,
    pub required_capabilities: &'static [&'static str],
}

pub fn match_driver_profile<'a>(
    pci_id: PciId,
    observed_capabilities: &[&str],
    profiles: &'a [DriverProfile],
) -> Option<&'a DriverProfile> {
    profiles.iter().find(|profile| {
        profile.vendor_id.is_none_or(|v| v == pci_id.vendor)
            && profile.device_id.is_none_or(|d| d == pci_id.device)
            && profile.class.is_none_or(|c| c == pci_id.class)
            && profile.subclass.is_none_or(|s| s == pci_id.subclass)
            && profile
                .required_capabilities
                .iter()
                .all(|required| observed_capabilities.contains(required))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::collections::BTreeMap;

    #[derive(Default)]
    struct FakePci {
        registers: BTreeMap<(PciAddress, u16), u32>,
    }

    impl PciConfigAccess for FakePci {
        fn read_u32(&self, address: PciAddress, offset: u16) -> Result<u32, PciError> {
            if !address.valid() || offset >= 4096 || offset & 3 != 0 {
                return Err(PciError::InvalidAddress);
            }
            Ok(*self.registers.get(&(address, offset)).unwrap_or(&0xFFFF_FFFF))
        }
        fn write_u32(&self, _: PciAddress, _: u16, _: u32) -> Result<(), PciError> {
            Err(PciError::MappingDenied)
        }
    }

    fn put(fake: &mut FakePci, address: PciAddress, offset: u16, value: u32) {
        fake.registers.insert((address, offset), value);
    }

    #[test]
    fn simulated_pci_device_is_registered_once_with_bdf_and_ids() {
        let address = PciAddress::new(0, 0, 2, 0);
        let mut fake = FakePci::default();
        put(&mut fake, address, 0, (0x2922u32 << 16) | 0x8086);
        put(&mut fake, address, 8, 0x0106_0001);
        put(&mut fake, address, 0x0C, 0);
        let mut inventory = HardwareInventory::new();
        let windows = [McfgWindow {
            base_address: 0xE000_0000,
            segment: 0,
            start_bus: 0,
            end_bus: 0,
        }];
        assert_eq!(discover_pci(&fake, &windows, &mut inventory), Ok(1));
        let record = inventory.get(device_id(address)).unwrap();
        assert_eq!(record.vendor_id, Some(0x8086));
        assert_eq!(record.device_id, Some(0x2922));
        assert_eq!(record.class, DeviceClass::Storage);
        assert_eq!(discover_pci(&fake, &windows, &mut inventory), Err(
            DiscoveryError::Inventory(InventoryError::DuplicateDeviceId)
        ));
    }

    #[test]
    fn malformed_mcfg_is_rejected_before_scanning() {
        assert!(matches!(parse_mcfg_windows(b"bad"), Err(DiscoveryError::InvalidMcfg(_))));
    }

    #[test]
    fn overlapping_mcfg_windows_in_same_segment_are_rejected() {
        let windows = [
            McfgWindow { base_address: 0xE000_0000, segment: 0, start_bus: 0, end_bus: 2 },
            McfgWindow { base_address: 0xE100_0000, segment: 0, start_bus: 2, end_bus: 3 },
        ];
        // Window validation is also performed at discovery entry.
        let mut inventory = HardwareInventory::new();
        let fake = FakePci::default();
        assert_eq!(discover_pci(&fake, &windows, &mut inventory), Err(DiscoveryError::InvalidWindow));
    }

    #[test]
    fn resource_validation_keeps_bar_interrupt_and_dma_checks_separate() {
        assert!(validate_bar_resource(PciBar {
            index: 0, kind: BarKind::Memory32, base: 0x1_0000_0000,
            size: 0x1000, prefetchable: false,
        }).is_err());
        assert!(validate_interrupt_vector(31).is_err());
        assert!(validate_interrupt_vector(32).is_ok());
        assert!(validate_dma_region(DmaRegion {
            iova: u64::MAX - 3, physical: 0x1000, length: 8, writable: true,
        }).is_err());
    }

    #[test]
    fn driver_profile_requires_ids_class_and_capabilities_to_match() {
        let id = pci::decode_id(2, 0, 0, 0x1234, 0x5678);
        let profiles = [DriverProfile {
            name: "test-net",
            vendor_id: Some(0x1234),
            device_id: Some(0x5678),
            class: Some(2),
            subclass: Some(0),
            required_capabilities: &["msi"],
        }];
        assert_eq!(match_driver_profile(id, &[], &profiles), None);
        assert_eq!(match_driver_profile(id, &["msi"], &profiles).unwrap().name, "test-net");
    }
}
