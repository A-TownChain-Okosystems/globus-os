// Copyright (c) 2026 Michael Wroblewski / ShivaCore / A-TownChain-Okosystems. All Rights Reserved.
//! ShivaCore Kernel — Library Crate für Test-Ausführung
#![cfg_attr(not(test), no_std)]
#![allow(dead_code)]
#![allow(unused_imports)]
#![allow(unused_variables)]
#![allow(unused_mut)]

extern crate alloc;
#[cfg(test)]
extern crate std;

// Enumerationstypen liegen in der gemeinsamen API-Crate (`libshivacore`), nicht
// mehr im Kernel: der Service Space braucht sie und haengt an `shivacore`, was
// den Kernel daran hindert, den Service Space einzubinden. Der Re-Export haelt
// die alten Pfade `shivacore::madt::…` am Leben.
pub use libshivacore::{acpi_devices, madt};

/// Adressraum je Prozess: Seitentabellen, Fenster und die Rechte je Eintrag.
pub mod addr_space;
/// Physikalischer Framespeicher; nur im Testbuild eingebunden.
#[cfg(test)]
pub mod allocator;
/// Local APIC: Basisadresse, Register und Interrupt-Zustellung.
pub mod apic;
/// ATC-FS, das inhaltsadressierte Dateisystem des Kernels.
pub mod atcfs;
/// ATS-1000-Kernelschnittstelle mit Prozesskennung, Beendigungsgrund und Prozessverwaltung.
pub mod ats1000;
/// Die eine Instanz, die ueber Berechtigungen entscheidet.
pub mod cap_authority;
/// Capability-System mit Rechten, Tabellen und Pruefungen.
pub mod capability;
/// Eigener Adressraum je Prozess mit Seitentabellen und Fenstern.
pub mod console_owner;
/// Zusammenspiel der Teilsysteme in einem Ablauf.
pub mod cross_subsystem;
/// Grenze zwischen Fehlerbehandlung und Ereignismeldung.
pub mod diagnostics;
/// Beurteilung von DMA-Bereichen, bevor ein Geraet darauf zugreift.
pub mod dma_auth;
/// Laden von ELF64-Dateien und die Signalverwaltung.
pub mod elf_loader;
/// Lebensdauer von Kanaelen und Handles: oeffnen, teilen, schliessen, widerrufen.
pub mod entropy_gate;
/// Ausnahmen und Fehlerwege aus dem Userspace in den Kernel.
pub mod fault;
/// Hardware-Abstraktion; derzeit die Prozessorinformation.
pub mod hal;
/// Pruefung des Identitaetsdatensatzes im Kernel: Ergebnis, Kennung und Gate.
pub mod identity_catalog;
/// Bootseitiger Mehrkonten-Katalog aus dem bestehenden Identity-Verzeichnis.
pub mod identity_check;
/// Lesen des Identitaetsdatensatzes aus Eintraegen und Abbild, mit Fehlerwegen.
pub mod identity_file;
/// Begrenzte Eingabewarteschlange zwischen Interrupt-Handler und Konsument.
pub mod input_queue;
/// IO-APIC: indirekte Register und Umleitung von Geraeteinterrupts.
pub mod ioapic;
/// Interprozesskommunikation mit Nachrichten und Warteschlangen.
pub mod ipc;
/// Lebensdauer von Handles und Kanaelen: oeffnen, teilen, schliessen, widerrufen.
pub mod ipc_lifecycle;
/// IRQ-Vermittlung mit ihren Fehlern.
pub mod irq;
/// Verbindung von der IRQ-Tabelle zum Maskenregister.
pub mod irq_bridge;
/// Zuordnung von Geraete-IRQs zu Vektoren.
pub mod irq_routing;
/// Keccak-256 in reinem Rust.
pub mod keccak;
/// Kernel-Abos auf IRQ-Leitungen.
pub mod kernel_caps;
/// Ablauf des Kernelstarts.
pub mod kernel_init;
/// Tastenereignisse vom Kernel zum Abonnenten; Entwurf, nicht verdrahtet.
pub mod key_events;
/// Scancode zu Tastencode fuer die PS/2-Tastatur (Satz 1).
pub mod keymap;
/// Umsetzung der Speicherverwaltungsschnittstelle.
pub mod memory_manager;
/// Prozessverwaltung mit Prozesszustand und Prozessliste.
pub mod process;
/// Verzoegerte Freigabe von Seitentabellen-Frames.
pub mod reclaim;
/// Ablaufplanung nach DA-HEFT mit Prioritaeten und Zeitscheiben.
pub mod scheduler;
/// Mehrfachsignatur, Schluessel und Sicherheitspruefungen.
pub mod security;
/// Stabile Systemaufruf-Grenze.
pub mod syscall;
/// Der einzige Uebergang vom Userspace in den Kernel.
pub mod syscall_gate;
/// Systemstart in Phasen, mit Zustand und Auswertung.
pub mod system;
/// Zeitquelle (HPET, PIT, TSC oder simuliert) und Zeitgeber.
pub mod timer;
/// Ablaufplanung der Ring-3-Prozesse mit Kontextwechsel.
pub mod user_sched;
/// User-Prozesse, Adressraum-Layout und Segmentselektoren.
pub mod userspace;
/// Dateibaum im Speicher mit Capability-Pruefung.
pub mod vfs;
/// Validierung und Seitentabellen des virtuellen Speichers.
pub mod vmm;

/// Vorrat der beim Boot gefundenen Geraete.
pub mod boot_devices;
/// Ladbare Kernelmodule mit ihrem Ladezustand; nur im Testbuild eingebunden.
#[cfg(test)]
pub mod lkm;
/// Konfigurationsraum-Adressen und Geraetesuche auf dem PCI-Bus.
pub mod pci_scan;
/// Ramdisk mit Verzeichnis und Kennung.
pub mod ramdisk;
/// Textausgabe auf dem Bildschirm mit fester Zeichenbreite.
pub mod screen;
/// Diensteverwaltung: Beenden heisst fuer einen Dienst Neustart.
pub mod services;
/// Annahme und Ergebnis eines Prozessstarts.
pub mod spawn;
