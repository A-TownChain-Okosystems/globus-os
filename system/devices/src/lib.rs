//! Isolated device-service registry and hardware contracts.
//!
//! `no_std` ab 07.11.2026: Diese Schicht ist ein Vertrag, kein Dienst. Der
//! Service Space (no_std) haengt daran, und ein std-Crate haette ihn vom
//! Bare-Metal-Ziel ausgeschlossen (`can't find crate for std`). Tests duerfen
//! weiter std benutzen.
#![cfg_attr(not(test), no_std)]

extern crate alloc;

use alloc::string::String;

/// ACPI-Tabellen und ihre Pruefsummen.
pub mod acpi;
/// Umleitungstabelle des IO-APIC.
pub mod apic;
/// Blockgeraete, Geometrie und Auftraege.
pub mod block;
/// Cache fuer einen Block ueber einem Geraet.
pub mod block_cache;
/// Verwaltung mehrerer Blockgeraete.
pub mod block_manager;
/// Speicherkarte und Bildpuffer aus der Firmware.
pub mod boot;
/// Treiberzustaende und ihre Aufsicht.
pub mod driver_lifecycle;
/// Ethernet-Rahmen und Geraetevertrag.
pub mod ethernet;
/// Interruptrouten und Maskierung.
pub mod interrupt;
/// Speicherabbildung fuer Geraete.
pub mod iommu;
/// Message-Signalled Interrupts.
pub mod msi;
/// NVMe-Kommandos und Queues.
pub mod nvme;
/// NVMe-Controller und sein Lebenszyklus.
pub mod nvme_controller;
/// Registerfenster als MMIO-Backend fuer Controller.
pub mod nvme_mmio;
/// PCI-Konfigurationsraum und Geraeteerkennung.
pub mod pci;
/// Erkannte Geraete und Treiberbindung.
pub mod registry;
/// CPU-Topologie und Startzustand.
pub mod smp;
/// Monotoner Timer und Ticks.
pub mod timer;

pub use block_cache::{BlockCache, CacheError};
pub use block_manager::{BlockDeviceId, BlockDeviceRegistry};
pub use registry::DeviceRegistry;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
/// Kennung eines erkannten Geraets.
pub struct DeviceId(pub u64);
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Klasse eines Geraets.
pub enum DeviceClass {
    /// Massenspeicher.
    Storage,
    /// Netzwerk.
    Network,
    /// Anzeige.
    Display,
    /// Eingabe.
    Input,
    /// Audio.
    Audio,
    /// USB.
    Usb,
    /// Grafikprozessor.
    Gpu,
    /// Sonstige Klasse.
    Other,
}
#[derive(Debug, Clone, PartialEq, Eq)]
/// Ein erkanntes Geraet.
pub struct Device {
    /// Kennung des Geraets.
    pub id: DeviceId,
    /// Klasse des Geraets.
    pub class: DeviceClass,
    /// Gebundener Treiber, leer solange ungebunden.
    pub driver: String,
}
