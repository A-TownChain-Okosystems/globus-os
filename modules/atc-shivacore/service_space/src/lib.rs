// Copyright (c) 2026 Michael Wroblewski / ShivaCore / A-TownChain-Okosystems. All Rights Reserved.
//! ShivaCore Service Space (AD-012/AD-028) — Service-Submodule AUSGELAGERT aus dem Kernel-Crate.
//! Diese Schicht baut auf dem Kernel auf (shivacore), niemals umgekehrt.
#![cfg_attr(not(test), no_std)]
#![allow(dead_code)]
#![allow(unused_imports)]
#![allow(unused_variables)]
#![allow(unused_mut)]

#[cfg(test)]
extern crate std;

extern crate alloc;

pub mod atcnet;
pub mod blockchain;
pub mod consensus;
pub mod container_net;
pub mod contract;
pub mod device_registry;
/// DID und Signaturen. Nur mit Feature `crypto` — `sha2` laesst sich fuer
/// `x86_64-unknown-none` nicht uebersetzen (LLVM-Absturz, siehe Cargo.toml).
#[cfg(feature = "crypto")]
pub mod did;
pub mod genesis;
pub mod genesis_bridge;
pub mod gossip_bridge;
pub mod hw_drivers;
pub mod identity_key_hal;
pub mod identity_key_service;
pub mod knowledge_graph;
pub mod mempool;
pub mod net;
pub mod p2p;
pub mod p2p_secure;
/// Entfernte Capabilities brauchen `did::SoftwareSigner`, also `crypto`.
#[cfg(feature = "crypto")]
pub mod remote_caps;
pub mod security_audit;
pub mod service_init;
pub mod sockets;
pub mod tcpip;
pub mod vm;
