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

pub mod addr_space;
#[cfg(test)]
pub mod allocator;
pub mod apic;
pub mod atcfs;
pub mod ats1000;
pub mod capability;
pub mod cross_subsystem;
pub mod diagnostics;
pub mod dma_auth;
pub mod elf_loader;
pub mod fault;
pub mod hal;
pub mod input_queue;
pub mod ioapic;
pub mod ipc;
pub mod irq;
pub mod irq_bridge;
pub mod irq_routing;
pub mod kernel_caps;
pub mod kernel_init;
pub mod key_events;
pub mod keymap;
pub mod memory_manager;
pub mod process;
pub mod scheduler;
pub mod security;
pub mod syscall;
pub mod syscall_gate;
pub mod system;
pub mod timer;
pub mod user_sched;
pub mod userspace;
pub mod vfs;
pub mod vmm;

pub mod boot_devices;
#[cfg(test)]
pub mod lkm;
pub mod pci_scan;
pub mod ramdisk;
pub mod spawn;
