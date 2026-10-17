//! Deterministic device enumeration and driver binding registry.

use std::collections::BTreeMap;

use crate::{Device, DeviceClass, DeviceId};

#[derive(Debug, Default)]
pub struct DeviceRegistry {
    devices: Vec<Device>,
    drivers: BTreeMap<DeviceId, String>,
}

impl DeviceRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, device: Device) -> bool {
        if self.devices.iter().any(|d| d.id == device.id) {
            return false;
        }
        self.devices.push(device);
        self.devices.sort_by_key(|d| d.id);
        true
    }

    pub fn bind_driver(&mut self, id: DeviceId, driver: String) -> bool {
        if !self.devices.iter().any(|device| device.id == id) || driver.is_empty() {
            return false;
        }
        self.drivers.insert(id, driver);
        true
    }

    pub fn driver_for(&self, id: DeviceId) -> Option<&str> {
        self.drivers.get(&id).map(String::as_str)
    }

    pub fn by_class(&self, class: DeviceClass) -> impl Iterator<Item = &Device> {
        self.devices.iter().filter(move |d| d.class == class)
    }

    pub fn get(&self, id: DeviceId) -> Option<&Device> {
        self.devices.iter().find(|d| d.id == id)
    }

    pub fn devices(&self) -> &[Device] {
        &self.devices
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(id: u64, class: DeviceClass) -> Device {
        Device {
            id: DeviceId(id),
            name: format!("device-{id}"),
            class,
        }
    }

    #[test]
    fn registration_is_deterministic() {
        let mut registry = DeviceRegistry::new();
        assert!(registry.register(device(2, DeviceClass::Network)));
        assert!(registry.register(device(1, DeviceClass::Storage)));
        assert_eq!(registry.devices()[0].id, DeviceId(1));
        assert!(registry.bind_driver(DeviceId(2), "ethernet".into()));
        assert_eq!(registry.driver_for(DeviceId(2)), Some("ethernet"));
    }

    #[test]
    fn driver_binding_requires_registered_device_and_nonempty_driver() {
        let mut registry = DeviceRegistry::new();
        assert!(!registry.bind_driver(DeviceId(9), "ethernet".into()));
        assert!(registry.register(device(9, DeviceClass::Network)));
        assert!(!registry.bind_driver(DeviceId(9), String::new()));
        assert_eq!(registry.driver_for(DeviceId(9)), None);
    }
}
