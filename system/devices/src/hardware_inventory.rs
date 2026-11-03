//! Common hardware inventory contract for discovery services.
//!
//! This module contains data and policy primitives only. It does not access
//! hardware, map MMIO, assign interrupts, or grant DMA authority. Those actions
//! remain behind privileged HAL/service interfaces.

use alloc::string::String;
use alloc::vec::Vec;

use crate::{DeviceClass, DeviceId};

/// Stable bus or platform locator reported by a trusted discovery adapter.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum HardwareLocation {
    Cpu {
        logical_id: u32,
    },
    Pci {
        segment: u16,
        bus: u8,
        device: u8,
        function: u8,
    },
    Acpi {
        hardware_id: String,
        unique_id: Option<String>,
    },
    Usb {
        controller_id: u64,
        port_path: Vec<u8>,
    },
    Firmware {
        kind: String,
        instance: u32,
    },
}

impl HardwareLocation {
    /// Reject impossible PCI BDF coordinates before they enter the inventory.
    pub fn is_valid(&self) -> bool {
        match self {
            Self::Pci {
                device, function, ..
            } => *device < 32 && *function < 8,
            Self::Usb { port_path, .. } => !port_path.is_empty(),
            Self::Acpi { hardware_id, .. } => !hardware_id.is_empty(),
            Self::Firmware { kind, .. } => !kind.is_empty(),
            Self::Cpu { .. } => true,
        }
    }
}

/// Discovery and driver lifecycle are deliberately separate states.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HardwareStatus {
    Discovered,
    Supported,
    Ready,
    Failed,
}

/// One normalized hardware record. Capability names are descriptive metadata,
/// not proof that a feature is enabled or safe to use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HardwareRecord {
    pub id: DeviceId,
    pub class: DeviceClass,
    pub location: HardwareLocation,
    pub vendor_id: Option<u32>,
    pub device_id: Option<u32>,
    pub revision: Option<u8>,
    pub status: HardwareStatus,
    pub driver: Option<String>,
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InventoryError {
    InvalidLocation,
    DuplicateDeviceId,
    DuplicateLocation,
    DeviceNotFound,
}

/// Deterministic in-memory registry. Discovery backends must validate raw bus
/// data before calling this API; registration never probes hardware itself.
#[derive(Debug, Default, Clone)]
pub struct HardwareInventory {
    records: Vec<HardwareRecord>,
}

impl HardwareInventory {
    pub const fn new() -> Self {
        Self {
            records: Vec::new(),
        }
    }

    pub fn register(&mut self, record: HardwareRecord) -> Result<(), InventoryError> {
        if !record.location.is_valid() {
            return Err(InventoryError::InvalidLocation);
        }
        if self.records.iter().any(|existing| existing.id == record.id) {
            return Err(InventoryError::DuplicateDeviceId);
        }
        if self
            .records
            .iter()
            .any(|existing| existing.location == record.location)
        {
            return Err(InventoryError::DuplicateLocation);
        }
        self.records.push(record);
        Ok(())
    }

    pub fn set_status(
        &mut self,
        id: DeviceId,
        status: HardwareStatus,
        driver: Option<String>,
    ) -> Result<(), InventoryError> {
        let record = self
            .records
            .iter_mut()
            .find(|record| record.id == id)
            .ok_or(InventoryError::DeviceNotFound)?;
        record.status = status;
        record.driver = driver;
        Ok(())
    }

    pub fn get(&self, id: DeviceId) -> Option<&HardwareRecord> {
        self.records.iter().find(|record| record.id == id)
    }

    pub fn records(&self) -> &[HardwareRecord] {
        &self.records
    }

    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pci_record(id: u64, device: u8, function: u8) -> HardwareRecord {
        HardwareRecord {
            id: DeviceId(id),
            class: DeviceClass::Other,
            location: HardwareLocation::Pci {
                segment: 0,
                bus: 0,
                device,
                function,
            },
            vendor_id: Some(0x1234),
            device_id: Some(0x5678),
            revision: Some(1),
            status: HardwareStatus::Discovered,
            driver: None,
            capabilities: Vec::new(),
        }
    }

    #[test]
    fn inventory_registers_unique_devices_and_locations() {
        let mut inventory = HardwareInventory::new();
        assert_eq!(inventory.register(pci_record(1, 2, 0)), Ok(()));
        assert_eq!(
            inventory.register(pci_record(2, 2, 0)),
            Err(InventoryError::DuplicateLocation)
        );
        assert_eq!(
            inventory.register(pci_record(1, 3, 0)),
            Err(InventoryError::DuplicateDeviceId)
        );
        assert_eq!(inventory.len(), 1);
    }

    #[test]
    fn invalid_pci_coordinates_are_rejected() {
        let mut inventory = HardwareInventory::new();
        assert_eq!(
            inventory.register(pci_record(1, 32, 0)),
            Err(InventoryError::InvalidLocation)
        );
        assert_eq!(
            inventory.register(pci_record(1, 0, 8)),
            Err(InventoryError::InvalidLocation)
        );
        assert!(inventory.is_empty());
    }

    #[test]
    fn discovered_supported_and_ready_are_distinct_states() {
        let mut inventory = HardwareInventory::new();
        assert_eq!(inventory.register(pci_record(7, 1, 0)), Ok(()));
        assert_eq!(
            inventory.get(DeviceId(7)).unwrap().status,
            HardwareStatus::Discovered
        );
        assert_eq!(
            inventory.set_status(
                DeviceId(7),
                HardwareStatus::Supported,
                Some(String::from("test-driver"))
            ),
            Ok(())
        );
        assert_eq!(
            inventory.get(DeviceId(7)).unwrap().status,
            HardwareStatus::Supported
        );
        assert_eq!(
            inventory.set_status(
                DeviceId(7),
                HardwareStatus::Ready,
                Some(String::from("test-driver"))
            ),
            Ok(())
        );
        assert_eq!(
            inventory.get(DeviceId(7)).unwrap().status,
            HardwareStatus::Ready
        );
    }

    #[test]
    fn status_update_for_unknown_device_fails_closed() {
        let mut inventory = HardwareInventory::new();
        assert_eq!(
            inventory.set_status(DeviceId(99), HardwareStatus::Ready, None),
            Err(InventoryError::DeviceNotFound)
        );
    }
}
