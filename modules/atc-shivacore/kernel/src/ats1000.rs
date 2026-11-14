// Copyright (c) 2026 Michael Wroblewski / ShivaCore / A-TownChain-Okosystems. All Rights Reserved.
// ATS-1000 — ShivaCore Interface (siehe atc-kernel/docs/ATS_STANDARDS.md)
//
// Dieses Modul bildet die im Standard definierte KERNEL_API 1:1 als Rust-
// Traits/Stubs ab. In K-Sprint 0 sind die Funktionen noch nicht implementiert
// — sie werden Schritt für Schritt in K-Sprint 1-7 gefüllt:
//   K2 Speicher (alloc/free/mmap)   K3 Prozesse (spawn/kill/wait)
//   K5 Dateisystem (open/read/write/close)   K7 Netzwerk (connect/send/recv)
//
// Hinweis: Nutzt noch KEIN `alloc` (kein Heap bis K-Sprint 2) — bewusst nur
// feste Buffer/Slices, damit dieses Modul schon in K-Sprint 0 kompiliert.
//
// Ziel: Der Standard ist die Spezifikation, dieser Code ist die
// Referenzimplementierung — nicht umgekehrt.

#![allow(dead_code)]

/// Prozesskennung als Zahl; sie vergibt der Kernel, nicht der Prozess.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Pid(pub u32);
/// ATC-Adresse als 37 Byte, Form nach ATC-0002.
pub type Address = [u8; 37]; // ATC-Adresse, siehe ATC-0002
/// Rueckgabewert eines beendeten Prozesses.
pub type ExitCode = i32;

/// Art des Prozesses nach ATS-1000.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessType {
    /// Agent des ATC-Oekosystems.
    Agent,
    /// Dienstprozess.
    Service,
    /// Vertragsprozess.
    Contract,
    /// Systemprozess des Kernels.
    System,
    /// Validierungsprozess der Kette.
    Validator,
}

/// Ein Speicherbereich mit Anfang, Groesse und Eigentuemer.
#[derive(Debug, Clone, Copy)]
pub struct MemRegion {
    /// Anfangsadresse.
    pub addr: u64,
    /// Groesse in Byte.
    pub size: u64,
    /// Prozess, dem der Bereich gehoert.
    pub pid: Pid,
}

/// Eintrag der Prozessliste.
#[derive(Debug, Clone, Copy)]
pub struct ProcessInfo {
    /// Prozesskennung.
    pub pid: Pid,
    /// Art des Prozesses.
    pub ptype: ProcessType,
    /// Vorzugsstufe von 0 bis 255; 0 ist niedrig, 255 ist System (ATC-0008).
    pub priority: u8, // 0=niedrig, 255=System — ATC-0008
}

/// ATS-1000 KERNEL_API — Prozessverwaltung
/// Status: nicht umgesetzt. `process::ProcessManager` ist eine eigene
/// Verwaltung und setzt diesen Trait nicht um.
pub trait ProcessManager {
    /// Legt einen Prozess an und liefert seine Kennung.
    fn spawn(&mut self, ptype: ProcessType, priority: u8) -> Pid;
    /// Beendet einen Prozess; false, wenn die Kennung unbekannt ist.
    fn kill(&mut self, pid: Pid) -> bool;
    /// Wartet auf das Ende eines Prozesses und liefert seinen Rueckgabewert.
    fn wait(&mut self, pid: Pid) -> ExitCode;
    /// Die aktuelle Prozessliste.
    fn list_processes(&self) -> &[ProcessInfo];
}

/// ATS-1000 KERNEL_API — Speicher
/// Status: umgesetzt in `memory_manager::KernelMemoryManager`.
pub trait MemoryManager {
    /// Reserviert Speicher fuer einen Prozess; None, wenn die Anfrage nicht erfuellt werden kann.
    fn alloc(&mut self, size: u64, pid: Pid) -> Option<MemRegion>;
    /// Gibt einen Bereich frei; false, wenn er nicht zu diesem Prozess gehoert.
    fn free(&mut self, region: MemRegion) -> bool;
    /// Bildet einen Bereich an einer gewuenschten Adresse ab.
    fn mmap(&mut self, addr: u64, size: u64) -> Option<MemRegion>;
}

/// ATS-1000 KERNEL_API — Dateisystem (siehe ATS-1002/ATCFS)
/// Status: umgesetzt in `atcfs::AtcFileSystem`.
pub trait FileSystem {
    /// Oeffnet einen Pfad und liefert eine Dateikennung.
    fn open(&mut self, path: &str, mode: u8) -> Option<u64>;
    /// Liest in den Puffer und liefert die gelesene Anzahl.
    fn read(&mut self, fh: u64, buf: &mut [u8]) -> u64;
    /// Schreibt aus dem Puffer und liefert die geschriebene Anzahl.
    fn write(&mut self, fh: u64, data: &[u8]) -> u64;
    /// Schliesst eine Dateikennung.
    fn close(&mut self, fh: u64) -> bool;
}

/// ATS-1000 KERNEL_API — Netzwerk (siehe ATS-1004/ATCNet, ATC-0007)
/// Status: Stub. Wird in K-Sprint 7 implementiert.
pub trait NetworkStack {
    /// Verbindet zu einem Gegenknoten und liefert eine Verbindungskennung.
    fn connect(&mut self, peer_node_id: &[u8; 32]) -> Option<u64>;
    /// Verschickt eine Nachricht; false, wenn sie nicht angenommen wurde.
    fn send(&mut self, conn: u64, msg: &[u8]) -> bool;
    /// Nimmt eine Nachricht in den Puffer und liefert ihre Laenge.
    fn recv(&mut self, conn: u64, buf: &mut [u8]) -> u64;
}

/// Kernel-Garantien laut ATS-1000 — werden hier als Boot-Log dokumentiert,
/// sobald die jeweilige Eigenschaft technisch durchgesetzt wird.
pub const KERNEL_GUARANTEES: &[&str] = &[
    "Kein Single Point of Failure (dezentral) — ab K-Sprint 8 (P2P)",
    "Jeder Prozess laeuft isoliert in eigenem MemRegion — ab K-Sprint 6 (Userspace)",
    "Alle System-Calls sind auditierbar (auf-Chain) — ab K-Sprint 8",
    "Gas-basierte Ressourcen-Abrechnung — ab K-Sprint 6 (Syscalls)",
];
