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

pub mod ai;
#[cfg(test)]
pub mod allocator;
pub mod atcfs;
pub mod ats1000;
pub mod capability;
pub mod contract;
pub mod cross_subsystem;
pub mod diagnostics;
pub mod hal;
pub mod ipc;
pub mod kernel_init;
pub mod memory_manager;
pub mod mempool;
pub mod net;
pub mod p2p;
pub mod p2p_secure;
pub mod process;
pub mod scheduler;
pub mod security;
pub mod syscall;
pub mod tcpip;
pub mod timer;
pub mod vfs;
pub mod vm;
pub mod vmm;

#[cfg(test)]
pub mod lkm;
