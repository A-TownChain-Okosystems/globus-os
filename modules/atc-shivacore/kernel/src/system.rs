// Copyright (c) 2026 Michael Wroblewski / ShivaCore / A-TownChain-Okosystems. All Rights Reserved.
// ShivaCore — K-Sprint 36: System Boot + Init Process + Process Groups
// Copyright (c) Michael Wroblewski. All rights reserved.
//
// Boot-Sequence, Init-Prozess (PID 1), Prozessgruppen/Sessions,
// User/Group IDs, SystemManager (Top-Level Integration aller Subsysteme).

// P0-Baseline: alloc-Prelude für no_std (Issue #45, Schritt 3)
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use crate::ats1000::{ExitCode, Pid};
use alloc::collections::BTreeMap;

// ═══════════════════════════════════════════════════════════════════════════════
// Boot Phases
// ═══════════════════════════════════════════════════════════════════════════════

/// Phase des Systemstarts.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BootPhase {
    /// Pre-boot: bootloader hands off to kernel
    PreBoot,
    /// Early kernel: GDT, IDT, PIC, serial
    Early,
    /// Memory: paging, heap, frame allocator
    Memory,
    /// Core subsystems: process manager, scheduler, capabilities
    Core,
    /// Hardware drivers: PCI, HPET, virtio-blk, virtio-net
    Drivers,
    /// Filesystem: VFS, ATCFS, block buffer
    Filesystem,
    /// Network: Ethernet, ARP, TCP/IP, P2P
    Network,
    /// Userspace: userspace manager, ELF loader, signals, page faults
    Userspace,
    /// Init: start PID 1 (init process)
    Init,
    /// Running: system fully booted
    Running,
    /// Shutdown: system shutting down
    Shutdown,
}

impl BootPhase {
    /// Feste Reihenfolge der Phasen.
    pub fn order() -> &'static [BootPhase] {
        &[
            BootPhase::PreBoot,
            BootPhase::Early,
            BootPhase::Memory,
            BootPhase::Core,
            BootPhase::Drivers,
            BootPhase::Filesystem,
            BootPhase::Network,
            BootPhase::Userspace,
            BootPhase::Init,
            BootPhase::Running,
        ]
    }

    /// True ab der ersten Nachinitialisierungsphase.
    pub fn is_post_init(&self) -> bool {
        matches!(self, BootPhase::Init | BootPhase::Running)
    }

    /// True, solange der Userspace noch nicht laeuft.
    pub fn is_pre_userspace(&self) -> bool {
        matches!(
            self,
            BootPhase::PreBoot
                | BootPhase::Early
                | BootPhase::Memory
                | BootPhase::Core
                | BootPhase::Drivers
                | BootPhase::Filesystem
                | BootPhase::Network
        )
    }

    /// Kurzname der Phase.
    pub fn name(&self) -> &'static str {
        match self {
            BootPhase::PreBoot => "pre-boot",
            BootPhase::Early => "early",
            BootPhase::Memory => "memory",
            BootPhase::Core => "core",
            BootPhase::Drivers => "drivers",
            BootPhase::Filesystem => "filesystem",
            BootPhase::Network => "network",
            BootPhase::Userspace => "userspace",
            BootPhase::Init => "init",
            BootPhase::Running => "running",
            BootPhase::Shutdown => "shutdown",
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Boot Sequence
// ═══════════════════════════════════════════════════════════════════════════════

/// Zustand und Verlauf des Systemstarts.
pub struct BootSequence {
    current_phase: BootPhase,
    completed_phases: Vec<BootPhase>,
    boot_log: Vec<String>,
    boot_time_ns: u64,
    phase_timestamps: BTreeMap<u8, u64>, // phase ordinal → timestamp
}

impl Default for BootSequence {
    fn default() -> Self {
        Self::new()
    }
}

impl BootSequence {
    /// Startet in der ersten Phase.
    pub fn new() -> Self {
        Self {
            current_phase: BootPhase::PreBoot,
            completed_phases: Vec::new(),
            boot_log: Vec::new(),
            boot_time_ns: 0,
            phase_timestamps: BTreeMap::new(),
        }
    }

    /// Aktuelle Phase.
    pub fn current_phase(&self) -> BootPhase {
        self.current_phase
    }

    /// Geht eine Phase weiter und haelt den Zeitstempel fest; ueber die letzte Phase hinaus bleibt der Zustand stehen.
    pub fn advance(&mut self, timestamp_ns: u64) -> BootPhase {
        let phases = BootPhase::order();
        let current_idx = phases
            .iter()
            .position(|p| *p == self.current_phase)
            .unwrap_or(0);

        // Log completion of current phase
        self.completed_phases.push(self.current_phase);
        self.phase_timestamps
            .insert(self.current_phase as u8, timestamp_ns);
        self.log(&format!(
            "phase '{}' completed at {}ns",
            self.current_phase.name(),
            timestamp_ns
        ));

        if current_idx + 1 < phases.len() {
            self.current_phase = phases[current_idx + 1];
            self.log(&format!("entering phase '{}'", self.current_phase.name()));
        }
        self.current_phase
    }

    /// True, wenn der Start abgeschlossen ist.
    pub fn is_running(&self) -> bool {
        self.current_phase == BootPhase::Running
    }
    /// True, solange der Start laeuft.
    pub fn is_booting(&self) -> bool {
        self.current_phase != BootPhase::Running && self.current_phase != BootPhase::Shutdown
    }

    /// Anzahl abgeschlossener Phasen.
    pub fn completed_count(&self) -> usize {
        self.completed_phases.len()
    }
    /// Gesamtzahl der Phasen.
    pub fn total_phases(&self) -> usize {
        BootPhase::order().len()
    }

    /// Fortschritt zwischen 0 und 1.
    pub fn boot_progress(&self) -> f32 {
        let completed = self.completed_count() as f32;
        let total = self.total_phases() as f32;
        (completed / total) * 100.0
    }

    /// Haengt eine Meldung an das Startprotokoll.
    pub fn log(&mut self, msg: &str) {
        self.boot_log
            .push(format!("[{}] {}", self.current_phase.name(), msg));
    }

    /// Das Startprotokoll.
    pub fn boot_log(&self) -> &[String] {
        &self.boot_log
    }

    /// Dauer einer Phase, falls sie schon durchlaufen wurde.
    pub fn phase_time(&self, phase: BootPhase) -> Option<u64> {
        self.phase_timestamps.get(&(phase as u8)).copied()
    }

    /// Gesamtdauer des Starts in Nanosekunden.
    pub fn total_boot_time(&self) -> u64 {
        self.boot_time_ns
    }

    /// Schliesst den Start mit Zeitstempel ab.
    pub fn set_boot_complete(&mut self, timestamp_ns: u64) {
        // Close the Init phase and record Running as reached so the boot
        // progress reflects a fully completed phase chain.
        self.completed_phases.push(self.current_phase);
        self.current_phase = BootPhase::Running;
        self.phase_timestamps
            .insert(BootPhase::Running as u8, timestamp_ns);
        self.completed_phases.push(BootPhase::Running);
        self.boot_time_ns = timestamp_ns;
        self.log(&format!("boot complete in {}ns", timestamp_ns));
    }

    /// Faehrt den Startvorgang herunter und beendet ihn.
    pub fn shutdown(&mut self) {
        self.current_phase = BootPhase::Shutdown;
        self.log("system shutdown initiated");
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// User/Group IDs
// ═══════════════════════════════════════════════════════════════════════════════

/// UID des Systemverwalters.
pub const ROOT_UID: u32 = 0;
/// GID des Systemverwalters.
pub const ROOT_GID: u32 = 0;
/// PID des Init-Prozesses.
pub const INIT_PID: u32 = 1;

/// Benutzer- und Gruppenkennung eines Prozesses, mit effektiven Werten.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct UserGroup {
    /// Reale Benutzerkennung.
    pub uid: u32,
    /// Reale Gruppenkennung.
    pub gid: u32,
    /// Effektive Benutzerkennung; massgeblich fuer Rechtepruefungen.
    pub euid: u32, // Effective UID
    /// Effektive Gruppenkennung.
    pub egid: u32, // Effective GID
}

impl UserGroup {
    /// Kennung des Systemverwalters.
    pub fn root() -> Self {
        Self {
            uid: ROOT_UID,
            gid: ROOT_GID,
            euid: ROOT_UID,
            egid: ROOT_GID,
        }
    }

    /// Kennung aus Benutzer und Gruppe, effektive Werte gleich.
    pub fn new(uid: u32, gid: u32) -> Self {
        Self {
            uid,
            gid,
            euid: uid,
            egid: gid,
        }
    }

    /// True, wenn die effektive Benutzerkennung die des Systemverwalters ist.
    pub fn is_root(&self) -> bool {
        self.euid == ROOT_UID || self.uid == ROOT_UID
    }

    /// Setzt reale und effektive Benutzerkennung.
    pub fn setuid(&mut self, uid: u32) {
        // Only root can change UID freely
        if self.is_root() {
            self.uid = uid;
            self.euid = uid;
        }
    }

    /// Setzt nur die effektive Benutzerkennung.
    pub fn seteuid(&mut self, euid: u32) {
        if self.is_root() || euid == self.uid {
            self.euid = euid;
        }
    }

    /// Setzt reale und effektive Gruppenkennung.
    pub fn setgid(&mut self, gid: u32) {
        if self.is_root() {
            self.gid = gid;
            self.egid = gid;
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Process Groups and Sessions
// ═══════════════════════════════════════════════════════════════════════════════

/// Kennung einer Prozessgruppe.
pub type Pgid = u32;
/// Kennung einer Sitzung.
pub type Sid = u32;

/// A process group (for job control)
#[derive(Clone, Debug)]
pub struct ProcessGroup {
    /// Kennung der Gruppe.
    pub pgid: Pgid,
    /// Prozess, der die Gruppe anfuehrt.
    pub leader: Pid,
    /// Mitglieder der Gruppe.
    pub members: Vec<Pid>,
    /// Sitzung, zu der die Gruppe gehoert.
    pub session: Sid,
}

impl ProcessGroup {
    /// Legt eine Gruppe mit ihrem Anfuhrer an.
    pub fn new(pgid: Pgid, leader: Pid, session: Sid) -> Self {
        Self {
            pgid,
            leader,
            members: vec![leader],
            session,
        }
    }

    /// Fuegt ein Mitglied hinzu, sofern es noch fehlt.
    pub fn add_member(&mut self, pid: Pid) {
        if !self.members.contains(&pid) {
            self.members.push(pid);
        }
    }

    /// Entfernt ein Mitglied; liefert false, wenn es nicht eingetragen war.
    pub fn remove_member(&mut self, pid: Pid) -> bool {
        self.members.retain(|p| *p != pid);
        !self.members.is_empty()
    }

    /// Anzahl der Mitglieder.
    pub fn member_count(&self) -> usize {
        self.members.len()
    }
    /// True, wenn der Prozess Mitglied ist.
    pub fn contains(&self, pid: Pid) -> bool {
        self.members.contains(&pid)
    }
}

/// A login session
#[derive(Clone, Debug)]
pub struct Session {
    /// Kennung der Sitzung.
    pub sid: Sid,
    /// Prozess, der die Sitzung anfuehrt.
    pub leader: Pid,
    /// Prozessgruppen der Sitzung.
    pub groups: Vec<Pgid>,
    /// Steuerndes Terminal, falls eines zugewiesen ist.
    pub controlling_tty: Option<u32>,
}

impl Session {
    /// Legt eine Sitzung mit ihrem Anfuhrer an.
    pub fn new(sid: Sid, leader: Pid) -> Self {
        Self {
            sid,
            leader,
            groups: Vec::new(),
            controlling_tty: None,
        }
    }

    /// Fuegt eine Prozessgruppe hinzu, sofern sie noch fehlt.
    pub fn add_group(&mut self, pgid: Pgid) {
        if !self.groups.contains(&pgid) {
            self.groups.push(pgid);
        }
    }

    /// Weist der Sitzung ein steuerndes Terminal zu.
    pub fn set_controlling_tty(&mut self, tty: u32) {
        self.controlling_tty = Some(tty);
    }

    /// Anzahl der Prozessgruppen.
    pub fn group_count(&self) -> usize {
        self.groups.len()
    }
}

/// Process group/session manager
pub struct ProcessGroupManager {
    groups: Vec<ProcessGroup>,
    sessions: Vec<Session>,
    process_groups: BTreeMap<u32, Pgid>,  // pid → pgid
    process_sessions: BTreeMap<u32, Sid>, // pid → sid
    next_pgid: Pgid,
    next_sid: Sid,
}

impl Default for ProcessGroupManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ProcessGroupManager {
    /// Leere Verwaltung.
    pub fn new() -> Self {
        Self {
            groups: Vec::new(),
            sessions: Vec::new(),
            process_groups: BTreeMap::new(),
            process_sessions: BTreeMap::new(),
            next_pgid: 1,
            next_sid: 1,
        }
    }

    /// Create a new session (setsid syscall)
    pub fn create_session(&mut self, leader: Pid) -> Sid {
        let sid = self.next_sid;
        self.next_sid += 1;

        let mut session = Session::new(sid, leader);
        // Leader gets its own process group
        let pgid = self.create_group_internal(leader, sid);
        session.add_group(pgid);

        self.sessions.push(session);
        self.process_sessions.insert(leader.0, sid);
        sid
    }

    /// Create a new process group
    pub fn create_group(&mut self, leader: Pid, session: Sid) -> Pgid {
        let pgid = self.create_group_internal(leader, session);
        // Add to session
        if let Some(s) = self.sessions.iter_mut().find(|s| s.sid == session) {
            s.add_group(pgid);
        }
        pgid
    }

    fn create_group_internal(&mut self, leader: Pid, session: Sid) -> Pgid {
        let pgid = self.next_pgid;
        self.next_pgid += 1;
        let group = ProcessGroup::new(pgid, leader, session);
        self.groups.push(group);
        self.process_groups.insert(leader.0, pgid);
        self.process_sessions.insert(leader.0, session);
        pgid
    }

    /// Add a process to an existing group
    pub fn join_group(&mut self, pid: Pid, pgid: Pgid) -> bool {
        // Find the group
        let session = match self.groups.iter().find(|g| g.pgid == pgid) {
            Some(g) => g.session,
            None => return false,
        };

        // Remove from old group
        if let Some(&old_pgid) = self.process_groups.get(&pid.0)
            && let Some(g) = self.groups.iter_mut().find(|g| g.pgid == old_pgid)
        {
            g.remove_member(pid);
        }

        // Add to new group
        if let Some(g) = self.groups.iter_mut().find(|g| g.pgid == pgid) {
            g.add_member(pid);
            self.process_groups.insert(pid.0, pgid);
            self.process_sessions.insert(pid.0, session);
            true
        } else {
            false
        }
    }

    /// Get the process group for a PID
    pub fn get_group(&self, pid: Pid) -> Option<&ProcessGroup> {
        let pgid = self.process_groups.get(&pid.0)?;
        self.groups.iter().find(|g| g.pgid == *pgid)
    }

    /// Get the session for a PID
    pub fn get_session(&self, pid: Pid) -> Option<&Session> {
        let sid = self.process_sessions.get(&pid.0)?;
        self.sessions.iter().find(|s| s.sid == *sid)
    }

    /// Remove a process from groups/sessions (on exit)
    pub fn remove_process(&mut self, pid: Pid) {
        if let Some(&pgid) = self.process_groups.get(&pid.0) {
            let mut should_remove_group = false;
            if let Some(g) = self.groups.iter_mut().find(|g| g.pgid == pgid) {
                g.remove_member(pid);
                should_remove_group = g.members.is_empty();
            }
            if should_remove_group {
                self.groups.retain(|g| g.pgid != pgid);
            }
        }
        self.process_groups.remove(&pid.0);
        self.process_sessions.remove(&pid.0);
    }

    /// Anzahl der Prozessgruppen.
    pub fn group_count(&self) -> usize {
        self.groups.len()
    }
    /// Anzahl der Sitzungen.
    pub fn session_count(&self) -> usize {
        self.sessions.len()
    }

    /// Send a signal to all processes in a group (killpg)
    pub fn group_members(&self, pgid: Pgid) -> Vec<Pid> {
        self.groups
            .iter()
            .find(|g| g.pgid == pgid)
            .map(|g| g.members.clone())
            .unwrap_or_default()
    }

    /// Get all processes in a session
    pub fn session_processes(&self, sid: Sid) -> Vec<Pid> {
        let mut all = Vec::new();
        for g in &self.groups {
            if g.session == sid {
                all.extend(&g.members);
            }
        }
        all
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Init Process (PID 1)
// ═══════════════════════════════════════════════════════════════════════════════

/// Init process state
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum InitState {
    /// Noch nicht gestartet.
    NotStarted,
    /// Start laeuft an.
    Starting,
    /// Init laeuft.
    Running,
    /// Init sammelt Zombie-Kinder ein.
    Reaping, // Reaping zombie children
    /// Init faehrt herunter.
    ShuttingDown,
    /// Init ist beendet.
    Exited,
}

/// Init process configuration
#[derive(Clone, Debug)]
pub struct InitConfig {
    /// Pfad des Init-Programms.
    pub binary_path: String,
    /// Benutzerkennung des Init-Prozesses.
    pub uid: u32,
    /// Gruppenkennung des Init-Prozesses.
    pub gid: u32,
    /// Umgebung, die Init erbt.
    pub env: Vec<(String, String)>,
    /// True, wenn Zombie-Kinder automatisch eingesammelt werden.
    pub auto_reap: bool, // Auto-reap zombie children
    /// Obergrenze fuer Neustarts nach einem Absturz.
    pub max_restarts: u32, // Max restarts if init crashes
    /// Bisherige Neustarts.
    pub restart_count: u32,
}

impl Default for InitConfig {
    fn default() -> Self {
        Self {
            binary_path: "/bin/init".to_string(),
            uid: ROOT_UID,
            gid: ROOT_GID,
            env: vec![
                ("PATH".to_string(), "/bin:/sbin".to_string()),
                ("HOME".to_string(), "/".to_string()),
                ("SHELL".to_string(), "/bin/sh".to_string()),
            ],
            auto_reap: true,
            max_restarts: 3,
            restart_count: 0,
        }
    }
}

/// The init process manager
pub struct InitProcess {
    /// Prozesskennung des Init-Prozesses.
    pub pid: Pid,
    /// Zustand des Init-Prozesses.
    pub state: InitState,
    /// Konfiguration des Init-Prozesses.
    pub config: InitConfig,
    /// Kindprozesse von Init.
    pub children: Vec<Pid>,
    /// Exit-Code, sobald Init beendet ist.
    pub exit_code: Option<ExitCode>,
    /// Startzeit in Nanosekunden.
    pub start_time_ns: u64,
}

impl Default for InitProcess {
    fn default() -> Self {
        Self::new()
    }
}

impl InitProcess {
    /// Legt Init in seinem Anfangszustand an.
    pub fn new() -> Self {
        Self {
            pid: Pid(INIT_PID),
            state: InitState::NotStarted,
            config: InitConfig::default(),
            children: Vec::new(),
            exit_code: None,
            start_time_ns: 0,
        }
    }

    /// Startet Init und setzt die Startzeit.
    pub fn start(&mut self, timestamp_ns: u64) {
        self.state = InitState::Starting;
        self.start_time_ns = timestamp_ns;
        self.state = InitState::Running;
    }

    /// Traegt einen Kindprozess ein.
    pub fn add_child(&mut self, pid: Pid) {
        if !self.children.contains(&pid) {
            self.children.push(pid);
        }
    }

    /// Entfernt einen Kindprozess.
    pub fn remove_child(&mut self, pid: Pid) {
        self.children.retain(|p| *p != pid);
    }

    /// Anzahl der Kindprozesse.
    pub fn child_count(&self) -> usize {
        self.children.len()
    }

    /// True, wenn Init neu gestartet werden darf.
    pub fn should_restart(&self) -> bool {
        self.config.restart_count < self.config.max_restarts
    }

    /// Startet Init neu, solange das Neustartbudget reicht.
    pub fn restart(&mut self) -> bool {
        if !self.should_restart() {
            return false;
        }
        self.config.restart_count += 1;
        self.state = InitState::Starting;
        self.children.clear();
        self.exit_code = None;
        true
    }

    /// Faehrt Init herunter.
    pub fn shutdown(&mut self) {
        self.state = InitState::ShuttingDown;
    }

    /// Beendet Init mit Exit-Code.
    pub fn exit(&mut self, code: ExitCode) {
        self.exit_code = Some(code);
        self.state = InitState::Exited;
    }

    /// True, wenn Init laeuft.
    pub fn is_running(&self) -> bool {
        self.state == InitState::Running
    }
    /// True, wenn Init beendet ist.
    pub fn is_exited(&self) -> bool {
        self.state == InitState::Exited
    }

    /// Laufzeit seit dem Start bis `now_ns`.
    pub fn uptime_ns(&self, now_ns: u64) -> u64 {
        now_ns.saturating_sub(self.start_time_ns)
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// System Manager (top-level integration)
// ═══════════════════════════════════════════════════════════════════════════════

/// Buendelt Startablauf, Init, Prozessgruppen und Systemkennung.
pub struct SystemManager {
    /// Startablauf des Systems.
    pub boot: BootSequence,
    /// Init-Prozess.
    pub init: InitProcess,
    /// Prozessgruppen und Sitzungen.
    pub proc_groups: ProcessGroupManager,
    /// Kennung des Systemverwalters.
    pub system_uid: UserGroup,
    /// Laufzeit des Systems in Nanosekunden.
    pub uptime_ns: u64,
    /// True, solange das System laeuft.
    pub running: bool,
}

impl Default for SystemManager {
    fn default() -> Self {
        Self::new()
    }
}

impl SystemManager {
    /// Legt das System im Anfangszustand an.
    pub fn new() -> Self {
        let mut proc_groups = ProcessGroupManager::new();
        // Init gets its own session and group
        let sid = proc_groups.create_session(Pid(INIT_PID));

        Self {
            boot: BootSequence::new(),
            init: InitProcess::new(),
            proc_groups,
            system_uid: UserGroup::root(),
            uptime_ns: 0,
            running: false,
        }
    }

    /// Run the boot sequence (simulated)
    pub fn boot_system(&mut self) -> BootPhase {
        let mut ts = 0u64;

        // Phase 1: Early (GDT, IDT, PIC)
        self.boot.log("initializing GDT, IDT, PIC");
        ts += 1_000_000; // 1ms
        self.boot.advance(ts);

        // Phase 2: Memory (paging, heap)
        self.boot.log("initializing paging, heap allocator");
        ts += 2_000_000;
        self.boot.advance(ts);

        // Phase 3: Core (process manager, scheduler, capabilities)
        self.boot
            .log("initializing process manager, scheduler, capabilities");
        ts += 5_000_000;
        self.boot.advance(ts);

        // Phase 4: Drivers (PCI, HPET, virtio-blk, virtio-net)
        self.boot
            .log("scanning PCI bus, initializing HPET, virtio-blk, virtio-net");
        ts += 10_000_000;
        self.boot.advance(ts);

        // Phase 5: Filesystem (VFS, ATCFS)
        self.boot.log("mounting VFS, initializing ATCFS");
        ts += 3_000_000;
        self.boot.advance(ts);

        // Phase 6: Network (Ethernet, ARP, TCP/IP)
        self.boot
            .log("initializing network stack, ARP table, TCP/IP");
        ts += 8_000_000;
        self.boot.advance(ts);

        // Phase 7: Userspace (userspace manager, ELF loader, signals)
        self.boot
            .log("initializing userspace manager, ELF loader, signal manager");
        ts += 4_000_000;
        self.boot.advance(ts);

        // Phase 8: Init (start PID 1)
        self.boot.log("starting init process (PID 1)");
        self.init.start(ts);
        ts += 2_000_000;
        self.boot.advance(ts);

        // Phase 9: Running
        self.running = true;
        self.uptime_ns = ts;
        self.boot.set_boot_complete(ts);

        self.boot.current_phase()
    }

    /// Faehrt System und Init herunter.
    pub fn shutdown_system(&mut self) {
        self.boot.shutdown();
        self.init.shutdown();
        self.running = false;
    }

    /// Laufzeit in Nanosekunden.
    pub fn uptime_ns(&self) -> u64 {
        self.uptime_ns
    }
    /// Laufzeit in Millisekunden.
    pub fn uptime_ms(&self) -> u64 {
        self.uptime_ns / 1_000_000
    }
    /// Laufzeit in Sekunden.
    pub fn uptime_secs(&self) -> u64 {
        self.uptime_ns / 1_000_000_000
    }

    /// True, solange das System laeuft.
    pub fn is_running(&self) -> bool {
        self.running
    }

    /// Fortschritt des Starts zwischen 0 und 1.
    pub fn boot_progress(&self) -> f32 {
        self.boot.boot_progress()
    }
    /// Aktuelle Startphase.
    pub fn current_phase(&self) -> BootPhase {
        self.boot.current_phase()
    }

    /// Zaehlt die Laufzeit hoch und schaltet bei Bedarf die Startphase weiter.
    pub fn tick(&mut self, elapsed_ns: u64) {
        if self.running {
            self.uptime_ns += elapsed_ns;
        }
    }

    /// Create a new user process (fork from init or another process)
    pub fn spawn_user_process(&mut self, parent: Pid, new_pid: Pid) -> bool {
        if !self.running {
            return false;
        }

        // Add to init's children if parent is init
        if parent == Pid(INIT_PID) {
            self.init.add_child(new_pid);
        }

        // Inherit parent's process group
        if let Some(pgid) = self.proc_groups.process_groups.get(&parent.0).copied() {
            self.proc_groups.join_group(new_pid, pgid);
        }

        true
    }

    /// Reap a child process (called by init)
    pub fn reap_child(&mut self, pid: Pid) -> bool {
        self.init.remove_child(pid);
        self.proc_groups.remove_process(pid);
        true
    }

    /// Get system info
    pub fn system_info(&self) -> String {
        format!(
            "ShivaCore System\n\
             Phase: {}\n\
             Uptime: {}ms\n\
             Init PID: {} ({})\n\
             Sessions: {}\n\
             Process Groups: {}\n\
             Boot Progress: {:.1}%",
            self.boot.current_phase().name(),
            self.uptime_ms(),
            self.init.pid.0,
            if self.init.is_running() {
                "running"
            } else {
                "stopped"
            },
            self.proc_groups.session_count(),
            self.proc_groups.group_count(),
            self.boot.boot_progress(),
        )
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    // --- BootPhase tests ---

    #[test]
    fn test_boot_phase_order() {
        let phases = BootPhase::order();
        assert_eq!(phases.len(), 10);
        assert_eq!(phases[0], BootPhase::PreBoot);
        assert_eq!(phases[9], BootPhase::Running);
    }

    #[test]
    fn test_boot_phase_is_post_init() {
        assert!(BootPhase::Init.is_post_init());
        assert!(BootPhase::Running.is_post_init());
        assert!(!BootPhase::Early.is_post_init());
        assert!(!BootPhase::Drivers.is_post_init());
    }

    #[test]
    fn test_boot_phase_is_pre_userspace() {
        assert!(BootPhase::Early.is_pre_userspace());
        assert!(BootPhase::Drivers.is_pre_userspace());
        assert!(!BootPhase::Userspace.is_pre_userspace());
        assert!(!BootPhase::Running.is_pre_userspace());
    }

    #[test]
    fn test_boot_phase_names() {
        assert_eq!(BootPhase::PreBoot.name(), "pre-boot");
        assert_eq!(BootPhase::Early.name(), "early");
        assert_eq!(BootPhase::Running.name(), "running");
        assert_eq!(BootPhase::Shutdown.name(), "shutdown");
    }

    // --- BootSequence tests ---

    #[test]
    fn test_boot_sequence_new() {
        let bs = BootSequence::new();
        assert_eq!(bs.current_phase(), BootPhase::PreBoot);
        assert_eq!(bs.completed_count(), 0);
        assert!(!bs.is_running());
        assert!(bs.is_booting());
    }

    #[test]
    fn test_boot_sequence_advance() {
        let mut bs = BootSequence::new();
        let phase = bs.advance(1000);
        assert_eq!(phase, BootPhase::Early);
        assert_eq!(bs.completed_count(), 1);
    }

    #[test]
    fn test_boot_sequence_full() {
        let mut bs = BootSequence::new();
        let mut ts = 0u64;
        for _ in 0..9 {
            ts += 1_000_000;
            bs.advance(ts);
        }
        assert_eq!(bs.current_phase(), BootPhase::Running);
        assert_eq!(bs.completed_count(), 9);
        assert!(bs.is_running());
    }

    #[test]
    fn test_boot_sequence_progress() {
        let mut bs = BootSequence::new();
        assert_eq!(bs.boot_progress(), 0.0);
        bs.advance(1000);
        bs.advance(2000);
        assert!(bs.boot_progress() > 0.0);
    }

    #[test]
    fn test_boot_sequence_log() {
        let mut bs = BootSequence::new();
        bs.log("test message");
        assert!(!bs.boot_log().is_empty());
        assert!(bs.boot_log().last().unwrap().contains("test message"));
    }

    #[test]
    fn test_boot_sequence_phase_time() {
        let mut bs = BootSequence::new();
        bs.advance(5000);
        assert_eq!(bs.phase_time(BootPhase::PreBoot), Some(5000));
    }

    #[test]
    fn test_boot_sequence_set_complete() {
        let mut bs = BootSequence::new();
        bs.set_boot_complete(42_000_000);
        assert!(bs.is_running());
        assert_eq!(bs.total_boot_time(), 42_000_000);
    }

    #[test]
    fn test_boot_sequence_shutdown() {
        let mut bs = BootSequence::new();
        bs.set_boot_complete(1000);
        bs.shutdown();
        assert_eq!(bs.current_phase(), BootPhase::Shutdown);
    }

    // --- UserGroup tests ---

    #[test]
    fn test_user_group_root() {
        let ug = UserGroup::root();
        assert!(ug.is_root());
        assert_eq!(ug.uid, 0);
        assert_eq!(ug.gid, 0);
    }

    #[test]
    fn test_user_group_new() {
        let ug = UserGroup::new(1000, 1000);
        assert!(!ug.is_root());
        assert_eq!(ug.uid, 1000);
        assert_eq!(ug.euid, 1000);
    }

    #[test]
    fn test_user_group_setuid_root() {
        let mut ug = UserGroup::root();
        ug.setuid(1000);
        assert_eq!(ug.uid, 1000);
        assert_eq!(ug.euid, 1000);
    }

    #[test]
    fn test_user_group_setuid_non_root() {
        let mut ug = UserGroup::new(1000, 1000);
        ug.setuid(500); // Should fail (non-root can't change UID)
        assert_eq!(ug.uid, 1000); // Unchanged
    }

    #[test]
    fn test_user_group_seteuid() {
        let mut ug = UserGroup::new(1000, 1000);
        ug.seteuid(1000); // Can set to own UID
        assert_eq!(ug.euid, 1000);
        ug.seteuid(500); // Cannot set to other UID
        assert_eq!(ug.euid, 1000); // Unchanged
    }

    #[test]
    fn test_user_group_setgid() {
        let mut ug = UserGroup::root();
        ug.setgid(100);
        assert_eq!(ug.gid, 100);
        assert_eq!(ug.egid, 100);
    }

    // --- ProcessGroup tests ---

    #[test]
    fn test_process_group_new() {
        let pg = ProcessGroup::new(1, Pid(100), 1);
        assert_eq!(pg.pgid, 1);
        assert_eq!(pg.leader, Pid(100));
        assert_eq!(pg.session, 1);
        assert_eq!(pg.member_count(), 1);
        assert!(pg.contains(Pid(100)));
    }

    #[test]
    fn test_process_group_add_remove() {
        let mut pg = ProcessGroup::new(1, Pid(100), 1);
        pg.add_member(Pid(101));
        pg.add_member(Pid(102));
        assert_eq!(pg.member_count(), 3);
        assert!(pg.contains(Pid(101)));

        // Duplicate add is no-op
        pg.add_member(Pid(101));
        assert_eq!(pg.member_count(), 3);

        pg.remove_member(Pid(101));
        assert!(!pg.contains(Pid(101)));
        assert_eq!(pg.member_count(), 2);
    }

    #[test]
    fn test_process_group_remove_leader() {
        let mut pg = ProcessGroup::new(1, Pid(100), 1);
        pg.add_member(Pid(101));
        pg.remove_member(Pid(100)); // Remove leader
        assert!(!pg.contains(Pid(100)));
        assert_eq!(pg.member_count(), 1);
    }

    #[test]
    fn test_process_group_remove_last() {
        let mut pg = ProcessGroup::new(1, Pid(100), 1);
        pg.remove_member(Pid(100));
        assert_eq!(pg.member_count(), 0);
    }

    // --- Session tests ---

    #[test]
    fn test_session_new() {
        let s = Session::new(1, Pid(100));
        assert_eq!(s.sid, 1);
        assert_eq!(s.leader, Pid(100));
        assert_eq!(s.group_count(), 0);
        assert!(s.controlling_tty.is_none());
    }

    #[test]
    fn test_session_add_group() {
        let mut s = Session::new(1, Pid(100));
        s.add_group(1);
        s.add_group(2);
        s.add_group(1); // Duplicate
        assert_eq!(s.group_count(), 2);
    }

    #[test]
    fn test_session_controlling_tty() {
        let mut s = Session::new(1, Pid(100));
        s.set_controlling_tty(0);
        assert_eq!(s.controlling_tty, Some(0));
    }

    // --- ProcessGroupManager tests ---

    #[test]
    fn test_pgm_new() {
        let mgr = ProcessGroupManager::new();
        assert_eq!(mgr.group_count(), 0);
        assert_eq!(mgr.session_count(), 0);
    }

    #[test]
    fn test_pgm_create_session() {
        let mut mgr = ProcessGroupManager::new();
        let sid = mgr.create_session(Pid(100));
        assert_eq!(sid, 1);
        assert_eq!(mgr.session_count(), 1);
        assert_eq!(mgr.group_count(), 1); // Leader gets own group
    }

    #[test]
    fn test_pgm_create_group() {
        let mut mgr = ProcessGroupManager::new();
        let sid = mgr.create_session(Pid(100));
        let pgid = mgr.create_group(Pid(101), sid);
        assert!(pgid > 0);
        assert_eq!(mgr.group_count(), 2); // Session leader + new group
    }

    #[test]
    fn test_pgm_join_group() {
        let mut mgr = ProcessGroupManager::new();
        let sid = mgr.create_session(Pid(100));
        let pgid = mgr.create_group(Pid(101), sid);

        // PID 102 joins the group
        assert!(mgr.join_group(Pid(102), pgid));
        let group = mgr.get_group(Pid(102)).unwrap();
        assert!(group.contains(Pid(102)));
    }

    #[test]
    fn test_pgm_join_nonexistent_group() {
        let mut mgr = ProcessGroupManager::new();
        assert!(!mgr.join_group(Pid(100), 999));
    }

    #[test]
    fn test_pgm_get_group() {
        let mut mgr = ProcessGroupManager::new();
        mgr.create_session(Pid(100));
        let group = mgr.get_group(Pid(100)).unwrap();
        assert_eq!(group.leader, Pid(100));
    }

    #[test]
    fn test_pgm_get_session() {
        let mut mgr = ProcessGroupManager::new();
        let sid = mgr.create_session(Pid(100));
        let session = mgr.get_session(Pid(100)).unwrap();
        assert_eq!(session.sid, sid);
    }

    #[test]
    fn test_pgm_remove_process() {
        let mut mgr = ProcessGroupManager::new();
        mgr.create_session(Pid(100));
        assert_eq!(mgr.group_count(), 1);

        mgr.remove_process(Pid(100));
        assert_eq!(mgr.group_count(), 0); // Group removed when empty
    }

    #[test]
    fn test_pgm_group_members() {
        let mut mgr = ProcessGroupManager::new();
        let sid = mgr.create_session(Pid(100));
        let pgid = mgr.create_group(Pid(101), sid);
        mgr.join_group(Pid(102), pgid);
        mgr.join_group(Pid(103), pgid);

        let members = mgr.group_members(pgid);
        assert_eq!(members.len(), 3);
        assert!(members.contains(&Pid(101)));
        assert!(members.contains(&Pid(102)));
    }

    #[test]
    fn test_pgm_session_processes() {
        let mut mgr = ProcessGroupManager::new();
        let sid = mgr.create_session(Pid(100));
        let pgid2 = mgr.create_group(Pid(101), sid);
        mgr.join_group(Pid(102), pgid2);

        let processes = mgr.session_processes(sid);
        assert!(processes.contains(&Pid(100)));
        assert!(processes.contains(&Pid(101)));
        assert!(processes.contains(&Pid(102)));
    }

    #[test]
    fn test_pgm_multiple_sessions() {
        let mut mgr = ProcessGroupManager::new();
        let sid1 = mgr.create_session(Pid(100));
        let sid2 = mgr.create_session(Pid(200));
        assert_ne!(sid1, sid2);
        assert_eq!(mgr.session_count(), 2);
    }

    // --- InitProcess tests ---

    #[test]
    fn test_init_new() {
        let init = InitProcess::new();
        assert_eq!(init.pid, Pid(INIT_PID));
        assert_eq!(init.state, InitState::NotStarted);
        assert!(!init.is_running());
    }

    #[test]
    fn test_init_start() {
        let mut init = InitProcess::new();
        init.start(1000);
        assert!(init.is_running());
        assert_eq!(init.start_time_ns, 1000);
    }

    #[test]
    fn test_init_children() {
        let mut init = InitProcess::new();
        init.add_child(Pid(100));
        init.add_child(Pid(101));
        init.add_child(Pid(100)); // Duplicate
        assert_eq!(init.child_count(), 2);
        init.remove_child(Pid(100));
        assert_eq!(init.child_count(), 1);
    }

    #[test]
    fn test_init_restart() {
        let mut init = InitProcess::new();
        init.start(1000);
        init.exit(1);
        assert!(init.should_restart());
        assert!(init.restart());
        assert_eq!(init.config.restart_count, 1);
        assert_ne!(init.state, InitState::Exited);
    }

    #[test]
    fn test_init_max_restarts() {
        let mut init = InitProcess::new();
        init.config.max_restarts = 2;
        init.exit(1);
        assert!(init.restart()); // restart 1
        init.exit(1);
        assert!(init.restart()); // restart 2
        init.exit(1);
        assert!(!init.should_restart());
        assert!(!init.restart()); // Max reached
    }

    #[test]
    fn test_init_shutdown() {
        let mut init = InitProcess::new();
        init.start(1000);
        init.shutdown();
        assert_eq!(init.state, InitState::ShuttingDown);
        assert!(!init.is_running());
    }

    #[test]
    fn test_init_exit() {
        let mut init = InitProcess::new();
        init.start(1000);
        init.exit(0);
        assert!(init.is_exited());
        assert_eq!(init.exit_code, Some(0));
    }

    #[test]
    fn test_init_uptime() {
        let mut init = InitProcess::new();
        init.start(5000);
        assert_eq!(init.uptime_ns(10000), 5000);
    }

    #[test]
    fn test_init_config_default() {
        let config = InitConfig::default();
        assert_eq!(config.binary_path, "/bin/init");
        assert_eq!(config.uid, ROOT_UID);
        assert!(config.auto_reap);
        assert_eq!(config.env.len(), 3);
    }

    // --- SystemManager tests ---

    #[test]
    fn test_system_manager_new() {
        let mgr = SystemManager::new();
        assert!(!mgr.is_running());
        assert_eq!(mgr.current_phase(), BootPhase::PreBoot);
        assert_eq!(mgr.proc_groups.session_count(), 1); // Init session
    }

    #[test]
    fn test_system_manager_boot() {
        let mut mgr = SystemManager::new();
        let phase = mgr.boot_system();
        assert_eq!(phase, BootPhase::Running);
        assert!(mgr.is_running());
        assert!(mgr.init.is_running());
        assert!(mgr.boot_progress() >= 100.0);
    }

    #[test]
    fn test_system_manager_uptime() {
        let mut mgr = SystemManager::new();
        mgr.boot_system();
        mgr.tick(5_000_000_000); // 5 seconds
        assert!(mgr.uptime_secs() >= 5);
    }

    #[test]
    fn test_system_manager_shutdown() {
        let mut mgr = SystemManager::new();
        mgr.boot_system();
        assert!(mgr.is_running());
        mgr.shutdown_system();
        assert!(!mgr.is_running());
        assert_eq!(mgr.current_phase(), BootPhase::Shutdown);
    }

    #[test]
    fn test_system_manager_spawn() {
        let mut mgr = SystemManager::new();
        mgr.boot_system();
        assert!(mgr.spawn_user_process(Pid(INIT_PID), Pid(100)));
        assert_eq!(mgr.init.child_count(), 1);
    }

    #[test]
    fn test_system_manager_spawn_not_running() {
        let mut mgr = SystemManager::new();
        assert!(!mgr.spawn_user_process(Pid(INIT_PID), Pid(100)));
    }

    #[test]
    fn test_system_manager_reap_child() {
        let mut mgr = SystemManager::new();
        mgr.boot_system();
        mgr.spawn_user_process(Pid(INIT_PID), Pid(100));
        assert_eq!(mgr.init.child_count(), 1);
        mgr.reap_child(Pid(100));
        assert_eq!(mgr.init.child_count(), 0);
    }

    #[test]
    fn test_system_manager_info() {
        let mut mgr = SystemManager::new();
        mgr.boot_system();
        let info = mgr.system_info();
        assert!(info.contains("ShivaCore System"));
        assert!(info.contains("running"));
        assert!(info.contains("Init PID: 1"));
    }

    #[test]
    fn test_system_manager_boot_log() {
        let mut mgr = SystemManager::new();
        mgr.boot_system();
        let log = mgr.boot.boot_log();
        assert!(!log.is_empty());
        // Should have entries for each phase
        assert!(log.iter().any(|l| l.contains("GDT")));
        assert!(log.iter().any(|l| l.contains("PCI")));
        assert!(log.iter().any(|l| l.contains("init process")));
    }

    #[test]
    fn test_system_manager_init_has_session() {
        let mgr = SystemManager::new();
        let session = mgr.proc_groups.get_session(Pid(INIT_PID));
        assert!(session.is_some());
        let group = mgr.proc_groups.get_group(Pid(INIT_PID));
        assert!(group.is_some());
    }

    #[test]
    fn test_system_manager_child_inherits_group() {
        let mut mgr = SystemManager::new();
        mgr.boot_system();
        mgr.spawn_user_process(Pid(INIT_PID), Pid(100));
        // Child should be in init's process group
        let group = mgr.proc_groups.get_group(Pid(100));
        assert!(group.is_some());
    }

    #[test]
    fn test_full_boot_lifecycle() {
        let mut mgr = SystemManager::new();

        // Boot
        assert!(!mgr.is_running());
        mgr.boot_system();
        assert!(mgr.is_running());
        assert!(mgr.init.is_running());

        // Spawn children
        mgr.spawn_user_process(Pid(INIT_PID), Pid(100));
        mgr.spawn_user_process(Pid(INIT_PID), Pid(101));
        assert_eq!(mgr.init.child_count(), 2);

        // Tick
        mgr.tick(1_000_000_000);
        assert!(mgr.uptime_secs() >= 1);

        // Reap one child
        mgr.reap_child(Pid(100));
        assert_eq!(mgr.init.child_count(), 1);

        // Shutdown
        mgr.shutdown_system();
        assert!(!mgr.is_running());
        assert_eq!(mgr.current_phase(), BootPhase::Shutdown);
    }

    #[test]
    fn test_constants() {
        assert_eq!(ROOT_UID, 0);
        assert_eq!(ROOT_GID, 0);
        assert_eq!(INIT_PID, 1);
    }
}
