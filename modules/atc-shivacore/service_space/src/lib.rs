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

pub mod blockchain;
pub mod consensus;
pub mod genesis;
pub mod genesis_bridge;
pub mod gossip_bridge;
pub mod atcnet;
pub mod did;
pub mod remote_caps;
pub mod knowledge_graph;
pub mod security_audit;
pub mod identity_key_service;
pub mod identity_key_hal;
