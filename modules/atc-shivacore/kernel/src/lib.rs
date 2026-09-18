// Copyright (c) 2026 Michael Wroblewski / ShivaCore / A-TownChain-Okosystems. All Rights Reserved.
//! ShivaCore Kernel — Library Crate für Test-Ausführung
//!
//! Re-exportiert alle Kernel-Module für Unit- und Integrationstests.
#![cfg_attr(not(test), no_std)]
#![allow(dead_code)]
#![allow(unused_imports)]
#![allow(unused_variables)]
#![allow(unused_mut)]

#[cfg(test)]
extern crate std;

extern crate alloc;

#[cfg(test)]
pub mod allocator;
pub mod ats1000;
pub mod hal;
// [K29-Build] ausgeschlossen: pub mod framebuffer;
// [K29-Build] ausgeschlossen: pub mod gdt;
// [K29-Build] ausgeschlossen: pub mod interrupts;
// [K29-Build] ausgeschlossen: pub mod memory;
// [K29-Build] ausgeschlossen: pub mod serial;
pub mod net; // [K12] Netz-Primitive (HAL-Ebene) — verbleibt im Kernel (AD-028-Justierung)
             // [AD-028] Service-Space-Migration: blockchain, consensus, genesis, genesis_bridge,
             // gossip_bridge, atcnet, net, did, remote_caps, knowledge_graph, security_audit
             // sind in den Service-Space-Crate (service_space/) migriert — Kernel haelt nur Primitive.
pub mod atcfs;
pub mod capability;
pub mod ipc;
pub mod memory_manager;
pub mod process;
pub mod scheduler;
pub mod vfs;
// [K29-Build] ausgeschlossen: pub mod syscall;
pub mod timer;
// [K29-Build] ausgeschlossen: pub mod block;
pub mod ai;
pub mod contract;
pub mod cross_subsystem;
pub mod kernel_init;
pub mod mempool;
pub mod p2p;
pub mod p2p_secure; // [K14-Upgrade] ATC-PROTO-P2P-001 v1.0.0 (SCR-0028)
pub mod security;
pub mod tcpip;
pub mod vm;
pub mod vmm; // [M1.2] hardened VMM validation core
             // [K29-Build] ausgeschlossen: pub mod cow;
             // [K29-Build] ausgeschlossen: pub mod tracing;
             // [K29-Build] ausgeschlossen: pub mod container_net;
             // LKM is std-backed module-management logic; compile it in host-side kernel tests.
#[cfg(test)]
pub mod lkm;
// [K29-Build] ausgeschlossen: pub mod module_security;
// [K29-Build] ausgeschlossen: pub mod fs_journal;
