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

/// ATCNet-Protokoll mit Nachrichtentypen, Nutzlasten und Verbindungsverwaltung.
pub mod atcnet;
/// Block und Kette mit Vorschlagspipeline.
pub mod blockchain;
/// Konsens mit Proof of History, Stimmen und Ereignisgraph.
pub mod consensus;
/// Containernetz mit Namensraeumen, Schnittstellen, Routen, Weiterleitung und Firewall-Regeln.
pub mod container_net;
/// Ausfuehrer fuer Vertraege.
pub mod contract;
/// Registratur der Geraete mit Anspruechen und Abgleich gegen die Enumerationsquelle.
pub mod device_registry;
/// DID und Signaturen. Nur mit Feature `crypto` — `sha2` laesst sich fuer
/// `x86_64-unknown-none` nicht uebersetzen (LLVM-Absturz, siehe Cargo.toml).
#[cfg(feature = "crypto")]
pub mod did;
/// Genesis-Konfiguration, Genesis-Zustand und Genesis-Vertrag.
pub mod genesis;
/// Brueckenkette aus der Genesis-Konfiguration mit Validatorensatz und Zuteilungen.
pub mod genesis_bridge;
/// Gossip-Bruecke zwischen Genesis-Kette und Netzwerk.
pub mod gossip_bridge;
/// Hardwaretreiber der Service-Schicht.
pub mod hw_drivers;
/// Haltegrenze fuer identitaetsgebundene Schluessel; ohne Software-Rueckfall.
pub mod identity_key_hal;
/// Capability-gebundener Dienst um identitaetsgebundene Schluessel; ohne Schluesselmaterial.
pub mod identity_key_service;
/// Knowledge Graph als Triple-Speicher mit Indizes und Capability-Pruefung.
pub mod knowledge_graph;
/// Transaktionsspeicher, Zustandstabelle, Nonce-Verwaltung und Transaktionspruefer.
pub mod mempool;
/// Netzwerkgeraet und Frame-Ebene.
pub mod net;
/// Peer-Verwaltung des P2P-Netzes.
pub mod p2p;
/// Gesicherter Peer-Kanal mit Signatur und Domaenentrennung.
pub mod p2p_secure;
/// Entfernte Capabilities brauchen `did::SoftwareSigner`, also `crypto`.
#[cfg(feature = "crypto")]
pub mod remote_caps;
/// Sicherheitsaudit ueber die Brueckenkette.
pub mod security_audit;
/// Initialisierung der Nicht-TCB-Subsysteme mit Phasenprotokoll.
pub mod service_init;
/// Socket-Schnittstelle auf dem Netzwerkgeraet.
pub mod sockets;
/// TCP/IP-Schicht.
pub mod tcpip;
/// VM-Laufzeit mit Instruktionsausfuehrung und Gaskosten.
pub mod vm;
