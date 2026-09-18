// Copyright (c) 2026 Michael Wroblewski / ShivaCore / A-TownChain-Okosystems. All Rights Reserved.
//! ShivaCore Kernel — Library Crate für Test-Ausführung
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
pub mod net;
pub mod atcfs;
pub mod capability;
pub mod ipc;
pub mod memory_manager;
pub mod process;
pub mod scheduler;
pub mod vfs;
pub mod timer;
pub mod ai;
pub mod contract;
pub mod cross_subsystem;
pub mod kernel_init;
pub mod mempool;
pub mod p2p;
pub mod p2p_secure;
pub mod security;
pub mod tcpip;
pub mod vm;
pub mod vmm;
pub mod diagnostics;

#[cfg(test)]
pub mod lkm;
