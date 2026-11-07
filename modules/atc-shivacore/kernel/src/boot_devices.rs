// Copyright (c) 2026 Michael Wroblewski / ShivaCore / A-TownChain-Okosystems. All Rights Reserved.
// ─────────────────────────────────────────────────────────────────────────
// SC-006 REQ-SC006-11 — Der Geraetevorrat des Bootlaufs
//
// Die Enumeration hat die Geraete gefunden und protokolliert. Das Log ist aber
// kein Ort, an dem jemand nachfragen kann: der Service Space braucht die Liste
// zur Laufzeit, nicht als Textzeile.
//
// Diese Datei ist der Ort dazwischen. Der Boot schreibt hinein, wer die Liste
// braucht liest heraus.
//
// Zwei Regeln:
//
//   * **Schreiben ist einmalig.** `publish` ersetzt den Vorrat, statt
//     anzuhängen. Zweimal scannen ergibt dieselbe Liste, nicht das Doppelte.
//   * **Lesen gibt eine Kopie.** Der Vorrat ist kernel-eigen; wer ihn liest,
//     bekommt seine eigene Liste und kann sie nicht veraendern.
//
// Was hier nicht passiert: keine Rechte. Der Vorrat sagt, was da ist. Wer es
// benutzen darf, entscheidet die Device-Cap.
// ─────────────────────────────────────────────────────────────────────────

use alloc::vec::Vec;

use crate::acpi_devices::AcpiDevice;
use crate::pci_scan::PciDevice;

/// Der Vorrat an Geraeten, die der Boot gefunden hat.
///
/// Als eigene Struktur und nicht als globaler Zustand, damit Tests mehrere
/// unabhaengige Vorraete bauen koennen — ein globaler Speicher waere zwischen
/// parallel laufenden Tests nicht zu trennen.
pub struct DeviceStore {
    devices: spin::Mutex<Vec<PciDevice>>,
}

impl DeviceStore {
    /// Ein leerer Vorrat. `const`, damit er als Statik dienen kann.
    pub const fn new() -> Self {
        Self {
            devices: spin::Mutex::new(Vec::new()),
        }
    }

    /// Den Vorrat setzen und die Anzahl zurueckgeben.
    ///
    /// Ersetzt, was vorher da war. Ein zweiter Scan mit derselben Hardware
    /// ergibt also wieder dieselbe Liste und nicht das Doppelte.
    pub fn publish(&self, devices: Vec<PciDevice>) -> usize {
        let count = devices.len();
        *self.devices.lock() = devices;
        count
    }

    /// Eine Kopie des Vorrats.
    pub fn snapshot(&self) -> Vec<PciDevice> {
        self.devices.lock().clone()
    }

    pub fn len(&self) -> usize {
        self.devices.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Default for DeviceStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Der Vorrat des laufenden Systems. Der Boot schreibt, der Service Space liest.
pub static BOOT_DEVICES: DeviceStore = DeviceStore::new();

/// Der Vorrat an Geraeten, die aus ACPI statt aus dem PCI-Konfigurationsraum
/// stammen (Local APIC, IOAPIC, HPET).
///
/// Eigener Typ und eigener Speicher, nicht derselbe wie `DeviceStore`: ein
/// ACPI-Geraet hat keine Bus/Geraet/Funktion-Kennung, und der PCI-Vorrat darf
/// nicht so tun, als haette es eine. Wer beide Quellen braucht, liest beide.
pub struct AcpiDeviceStore {
    devices: spin::Mutex<Vec<AcpiDevice>>,
}

impl AcpiDeviceStore {
    /// Ein leerer Vorrat. `const`, damit er als Statik dienen kann.
    pub const fn new() -> Self {
        Self {
            devices: spin::Mutex::new(Vec::new()),
        }
    }

    /// Den Vorrat setzen und die Anzahl zurueckgeben. Ersetzt statt anzuhaengen.
    pub fn publish(&self, devices: Vec<AcpiDevice>) -> usize {
        let count = devices.len();
        *self.devices.lock() = devices;
        count
    }

    /// Eine Kopie des Vorrats.
    pub fn snapshot(&self) -> Vec<AcpiDevice> {
        self.devices.lock().clone()
    }

    pub fn len(&self) -> usize {
        self.devices.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Die Geraetearten in fester Reihenfolge, fuer eine Logzeile.
    pub fn names(&self) -> Vec<&'static str> {
        self.devices
            .lock()
            .iter()
            .map(|device| device.name())
            .collect()
    }
}

impl Default for AcpiDeviceStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Der ACPI-Vorrat des laufenden Systems.
pub static BOOT_ACPI_DEVICES: AcpiDeviceStore = AcpiDeviceStore::new();

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pci_scan::{DeviceKind, PciAddress};

    fn device(bus: u8, dev: u8, class: u8, subclass: u8) -> PciDevice {
        PciDevice {
            address: PciAddress::new(bus, dev, 0),
            vendor: 0x8086,
            device: 0x1234,
            class,
            subclass,
            header_type: 0x00,
        }
    }

    #[test]
    fn a_fresh_store_is_empty() {
        let store = DeviceStore::new();
        assert!(store.is_empty());
        assert_eq!(store.len(), 0);
        assert!(store.snapshot().is_empty());
    }

    #[test]
    fn what_was_published_comes_back() {
        let store = DeviceStore::new();
        let count = store.publish(alloc::vec![
            device(0, 2, 0x03, 0x00),
            device(0, 3, 0x02, 0x00),
        ]);
        assert_eq!(count, 2);
        assert_eq!(store.len(), 2);
        let snapshot = store.snapshot();
        assert_eq!(snapshot.len(), 2);
        assert_eq!(snapshot[0].kind(), DeviceKind::Display);
        assert_eq!(snapshot[1].kind(), DeviceKind::Network);
    }

    #[test]
    fn publishing_again_replaces_instead_of_appending() {
        let store = DeviceStore::new();
        store.publish(alloc::vec![device(0, 2, 0x03, 0x00)]);
        store.publish(alloc::vec![
            device(0, 2, 0x03, 0x00),
            device(0, 3, 0x02, 0x00)
        ]);
        assert_eq!(store.len(), 2, "zweiter Scan darf nicht verdoppeln");
    }

    #[test]
    fn a_snapshot_is_a_copy_not_a_view() {
        let store = DeviceStore::new();
        store.publish(alloc::vec![device(0, 2, 0x03, 0x00)]);
        let mut snapshot = store.snapshot();
        snapshot.clear();
        assert_eq!(store.len(), 1, "der Vorrat bleibt unberuehrt");
    }

    #[test]
    fn the_store_can_be_emptied_by_publishing_nothing() {
        let store = DeviceStore::new();
        store.publish(alloc::vec![device(0, 2, 0x03, 0x00)]);
        assert_eq!(store.publish(Vec::new()), 0);
        assert!(store.is_empty());
    }

    fn lapic() -> AcpiDevice {
        crate::acpi_devices::AcpiDevice {
            kind: crate::acpi_devices::AcpiDeviceKind::LocalApic,
            address: 0xFEE0_0000,
            gsi_base: None,
        }
    }

    #[test]
    fn a_fresh_acpi_store_is_empty() {
        let store = AcpiDeviceStore::new();
        assert!(store.is_empty());
        assert!(store.names().is_empty());
    }

    #[test]
    fn the_acpi_store_names_what_it_holds() {
        let store = AcpiDeviceStore::new();
        let count = store.publish(alloc::vec![lapic()]);
        assert_eq!(count, 1);
        assert_eq!(store.names(), alloc::vec!["LAPIC"]);
    }

    #[test]
    fn the_acpi_store_replaces_instead_of_appending() {
        let store = AcpiDeviceStore::new();
        store.publish(alloc::vec![lapic()]);
        store.publish(alloc::vec![lapic()]);
        assert_eq!(store.len(), 1, "zweiter Lauf darf nicht verdoppeln");
    }

    #[test]
    fn the_acpi_snapshot_is_a_copy() {
        let store = AcpiDeviceStore::new();
        store.publish(alloc::vec![lapic()]);
        let mut snapshot = store.snapshot();
        snapshot.clear();
        assert_eq!(store.len(), 1);
    }
}
