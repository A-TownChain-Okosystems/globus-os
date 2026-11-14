//! Deterministic device enumeration and driver binding registry.

use crate::{Device, DeviceClass, DeviceId};
use alloc::string::String;
use alloc::vec::Vec;

#[derive(Debug, Default)]
/// Erkannte Geraete und ihre Treiberbindung.
pub struct DeviceRegistry {
    devices: Vec<Device>,
}

impl DeviceRegistry {
    /// Leere Registry.
    pub fn new() -> Self {
        Self::default()
    }
    /// Nimmt ein Geraet auf; false bei doppelter Kennung.
    pub fn register(&mut self, device: Device) -> bool {
        if self.devices.iter().any(|d| d.id == device.id) {
            return false;
        }
        self.devices.push(device);
        self.devices.sort_by_key(|d| d.id);
        true
    }
    /// Bindet einen Treiber an ein Geraet; false, wenn die Kennung unbekannt ist.
    pub fn bind_driver(&mut self, id: DeviceId, driver: String) -> bool {
        let Some(device) = self.devices.iter_mut().find(|d| d.id == id) else {
            return false;
        };
        if driver.is_empty() {
            return false;
        }
        device.driver = driver;
        true
    }
    /// Geraete einer Klasse.
    pub fn by_class(&self, class: DeviceClass) -> impl Iterator<Item = &Device> {
        self.devices.iter().filter(move |d| d.class == class)
    }
    /// Geraet zu einer Kennung.
    pub fn get(&self, id: DeviceId) -> Option<&Device> {
        self.devices.iter().find(|d| d.id == id)
    }
    /// Alle Geraete.
    pub fn devices(&self) -> &[Device] {
        &self.devices
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn registration_is_deterministic() {
        let mut r = DeviceRegistry::new();
        assert!(r.register(Device {
            id: DeviceId(2),
            class: DeviceClass::Network,
            driver: "".into()
        }));
        assert!(r.register(Device {
            id: DeviceId(1),
            class: DeviceClass::Storage,
            driver: "".into()
        }));
        assert_eq!(r.devices()[0].id, DeviceId(1));
        assert!(r.bind_driver(DeviceId(2), "ethernet".into()));
    }
}
