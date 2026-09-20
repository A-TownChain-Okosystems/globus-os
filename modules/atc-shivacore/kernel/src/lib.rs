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

#[cfg(feature = "full-stack")]
pub mod ai;
#[cfg(test)]
pub mod allocator;
#[cfg(feature = "full-stack")]
pub mod atcfs;
pub mod ats1000;
#[cfg(feature = "full-stack")]
pub mod capability;
#[cfg(feature = "full-stack")]
pub mod contract;
#[cfg(feature = "full-stack")]
pub mod cross_subsystem;
#[cfg(feature = "full-stack")]
pub mod diagnostics;
pub mod elf_loader;
pub mod hal;
pub mod ipc;
#[cfg(feature = "full-stack")]
pub mod kernel_init;
#[cfg(feature = "full-stack")]
pub mod memory_manager;
#[cfg(feature = "full-stack")]
pub mod mempool;
#[cfg(feature = "full-stack")]
pub mod net;
#[cfg(feature = "full-stack")]
pub mod p2p;
#[cfg(feature = "full-stack")]
pub mod p2p_secure;
#[cfg(feature = "full-stack")]
pub mod process;
#[cfg(feature = "full-stack")]
pub mod scheduler;
#[cfg(feature = "full-stack")]
pub mod security;
#[cfg(feature = "full-stack")]
pub mod syscall;
#[cfg(feature = "full-stack")]
pub mod system;
#[cfg(feature = "full-stack")]
pub mod tcpip;
#[cfg(feature = "full-stack")]
pub mod timer;
pub mod user_sched;
pub mod userspace;
#[cfg(feature = "full-stack")]
pub mod vfs;
#[cfg(feature = "full-stack")]
pub mod vm;
#[cfg(feature = "full-stack")]
pub mod vmm;

#[cfg(test)]
pub mod lkm;
