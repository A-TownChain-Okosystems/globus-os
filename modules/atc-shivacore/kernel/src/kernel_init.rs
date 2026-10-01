// Copyright (c) 2026 Michael Wroblewski / ShivaCore / A-TownChain-Okosystems. All Rights Reserved.
//! ShivaCore Kernel initialization sequence.
//!
//! Kernel-owned initialization stops at hardware, IPC, filesystem,
//! security and other TCB responsibilities. Network protocol stacks and
//! blockchain/contract execution are initialized in Service Space.

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

use crate::ai::AiEngine;
use crate::atcfs::AtcFileSystem;
use crate::capability::CapabilityTable;
use crate::ipc::IpcSubsystem;
use crate::memory_manager::{MemorySubsystem, HEAP_END, HEAP_SIZE, HEAP_START};
use crate::process::ProcessManager;
use crate::scheduler::DaHeftScheduler;
use crate::security::SecurityManager;
use crate::vfs::Vfs;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InitStatus {
    NotStarted,
    Initializing,
    Ready,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootPhase {
    Heap,
    Memory,
    Capabilities,
    Processes,
    Scheduler,
    Ipc,
    FileSystem,
    Network,
    Security,
    Ai,
    Done,
}

impl BootPhase {
    pub fn label(&self) -> &str {
        match self {
            BootPhase::Heap => "L0 Heap (linked_list_allocator)",
            BootPhase::Memory => "L1 MemorySubsystem (Heap-Bridge)",
            BootPhase::Capabilities => "L2 CapabilityTable",
            BootPhase::Processes => "L3 ProcessManager",
            BootPhase::Scheduler => "L4 DA-HEFT Scheduler",
            BootPhase::Ipc => "L5 IPC Channels",
            BootPhase::FileSystem => "L6 ATCFS + VFS",
            BootPhase::Network => "L7 Network link interface (Service Space protocols)",
            BootPhase::Security => "L8 Security/Audit/MultiSig",
            BootPhase::Ai => "L10 AI Subsystem (Aurora AI)",
            BootPhase::Done => "Boot Complete",
        }
    }
}

pub struct KernelState {
    pub memory: MemorySubsystem,
    pub processes: ProcessManager,
    pub scheduler: DaHeftScheduler,
    pub ipc: IpcSubsystem,
    pub fs: AtcFileSystem,
    pub vfs: Vfs,
    pub security: SecurityManager,
    pub ai: AiEngine,
    pub init_log: Vec<(BootPhase, InitStatus)>,
}

impl KernelState {
    pub fn boot() -> Result<Self, BootError> {
        let mut log = Vec::new();

        log.push((BootPhase::Heap, InitStatus::Ready));

        log.push((BootPhase::Memory, InitStatus::Initializing));
        let memory = MemorySubsystem::new();
        if memory.stats().heap_base != HEAP_START {
            log.push((BootPhase::Memory, InitStatus::Failed));
            return Err(BootError::HeapConfigMismatch);
        }
        log.push((BootPhase::Memory, InitStatus::Ready));

        log.push((BootPhase::Capabilities, InitStatus::Initializing));
        let processes = ProcessManager::new();
        log.push((BootPhase::Capabilities, InitStatus::Ready));
        log.push((BootPhase::Processes, InitStatus::Ready));

        log.push((BootPhase::Scheduler, InitStatus::Initializing));
        let scheduler = DaHeftScheduler::new();
        log.push((BootPhase::Scheduler, InitStatus::Ready));

        log.push((BootPhase::Ipc, InitStatus::Initializing));
        let ipc = IpcSubsystem::new();
        log.push((BootPhase::Ipc, InitStatus::Ready));

        log.push((BootPhase::FileSystem, InitStatus::Initializing));
        let fs = AtcFileSystem::new();
        if !fs.exists("/") {
            log.push((BootPhase::FileSystem, InitStatus::Failed));
            return Err(BootError::FsInitFailed);
        }
        let caps = Arc::new(spin::Mutex::new(CapabilityTable::new()));
        let vfs = Vfs::new(caps);
        log.push((BootPhase::FileSystem, InitStatus::Ready));

        log.push((BootPhase::Network, InitStatus::Ready));

        log.push((BootPhase::Security, InitStatus::Initializing));
        let security = SecurityManager::new();
        log.push((BootPhase::Security, InitStatus::Ready));

        log.push((BootPhase::Ai, InitStatus::Initializing));
        let ai = AiEngine::new();
        log.push((BootPhase::Ai, InitStatus::Ready));

        log.push((BootPhase::Done, InitStatus::Ready));

        Ok(Self {
            memory,
            processes,
            scheduler,
            ipc,
            fs,
            vfs,
            security,
            ai,
            init_log: log,
        })
    }

    pub fn boot_log(&self) -> String {
        let mut out = String::from("=== ShivaCore Kernel Boot ===\n");
        for (phase, status) in &self.init_log {
            let icon = match status {
                InitStatus::Ready => "OK",
                InitStatus::Initializing => "..",
                InitStatus::Failed => "FAIL",
                InitStatus::NotStarted => "--",
            };
            out.push_str(&format!("  [{}] {}\n", icon, phase.label()));
        }
        out.push_str(&format!(
            "\n  Memory: {} regions, {} bytes allocated\n",
            self.memory.stats().active_regions,
            self.memory.stats().total_allocated
        ));
        out.push_str(&format!("  FS: {} nodes\n", self.fs.ls("/").len()));
        out.push_str(&format!("  AI: {} models\n", self.ai.model_count()));
        out.push_str("=== Boot Complete ===\n");
        out
    }

    pub fn smoke_test(&mut self) -> Result<(), BootError> {
        let region = self
            .memory
            .allocate(crate::ats1000::Pid(1), 1024)
            .map_err(|_| BootError::SmokeTestFailed)?;

        let caps = &self.memory.caps;
        self.fs
            .write_file(
                caps,
                "/tmp/smoke_test.txt",
                b"ShivaCore boot OK",
                crate::ats1000::Pid(1),
            )
            .map_err(|_| BootError::SmokeTestFailed)?;

        let (_cid, node) = self
            .fs
            .read_file(caps, "/tmp/smoke_test.txt", crate::ats1000::Pid(1))
            .map_err(|_| BootError::SmokeTestFailed)?;
        assert_eq!(node.size, 17);

        self.memory
            .deallocate(crate::ats1000::Pid(1), region.region_id)
            .map_err(|_| BootError::SmokeTestFailed)?;

        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootError {
    HeapConfigMismatch,
    FsInitFailed,
    SmokeTestFailed,
}

pub fn validate_integration() -> Result<(), String> {
    if HEAP_START != 0x4444_4444_0000 {
        return Err(format!("HEAP_START mismatch: 0x{:x}", HEAP_START));
    }
    if HEAP_SIZE != 100 * 1024 {
        return Err(format!("HEAP_SIZE mismatch: {}", HEAP_SIZE));
    }
    if HEAP_END != HEAP_START + HEAP_SIZE {
        return Err(format!("HEAP_END mismatch: 0x{:x}", HEAP_END));
    }
    Ok(())
}

pub fn kernel_version() -> &'static str {
    "ShivaCore Kernel v0.0.24 — service-space boundary enforcement"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kernel_boot() {
        let state = KernelState::boot().unwrap();
        assert_eq!(
            state.init_log.last().unwrap(),
            &(BootPhase::Done, InitStatus::Ready)
        );
    }

    #[test]
    fn test_boot_log_output() {
        let state = KernelState::boot().unwrap();
        let log = state.boot_log();
        assert!(log.contains("ShivaCore Kernel Boot"));
        assert!(log.contains("MemorySubsystem"));
        assert!(log.contains("ATCFS"));
        assert!(log.contains("Network link interface"));
        assert!(log.contains("Security"));
        assert!(log.contains("AI"));
        assert!(log.contains("Boot Complete"));
        assert!(!log.contains("P2P:"));
        assert!(!log.contains("Mempool:"));
        assert!(!log.contains("VM:"));
    }

    #[test]
    fn test_smoke_test() {
        let mut state = KernelState::boot().unwrap();
        state.smoke_test().unwrap();
        assert_eq!(state.memory.stats().total_allocated, 0);
    }

    #[test]
    fn test_validate_integration() {
        assert!(validate_integration().is_ok());
    }

    #[test]
    fn test_kernel_version() {
        assert!(kernel_version().contains("ShivaCore"));
        assert!(kernel_version().contains("service-space"));
    }

    #[test]
    fn test_boot_phases_all_ready() {
        let state = KernelState::boot().unwrap();
        for (_, status) in &state.init_log {
            assert_ne!(*status, InitStatus::Failed);
        }
    }

    #[test]
    fn test_fs_root_exists_after_boot() {
        let state = KernelState::boot().unwrap();
        assert!(state.fs.exists("/"));
        assert!(state.fs.exists("/atc"));
    }

    #[test]
    fn test_service_space_boundary() {
        let state = KernelState::boot().unwrap();
        assert_eq!(
            state
                .init_log
                .iter()
                .find(|(p, _)| *p == BootPhase::Network)
                .unwrap()
                .1,
            InitStatus::Ready
        );
    }

    #[test]
    fn test_ai_initialized() {
        let state = KernelState::boot().unwrap();
        assert_eq!(state.ai.model_count(), 0);
    }

    #[test]
    fn test_security_initialized() {
        let state = KernelState::boot().unwrap();
        assert_eq!(
            state
                .init_log
                .iter()
                .find(|(p, _)| *p == BootPhase::Security)
                .unwrap()
                .1,
            InitStatus::Ready
        );
    }

    #[test]
    fn test_ipc_initialized() {
        let state = KernelState::boot().unwrap();
        assert!(state.init_log.iter().any(|(p, _)| *p == BootPhase::Ipc));
    }

    #[test]
    fn test_scheduler_initialized() {
        let state = KernelState::boot().unwrap();
        assert!(state
            .init_log
            .iter()
            .any(|(p, _)| *p == BootPhase::Scheduler));
    }

    #[test]
    fn test_vfs_initialized() {
        let state = KernelState::boot().unwrap();
        assert!(state
            .init_log
            .iter()
            .any(|(p, _)| *p == BootPhase::FileSystem));
    }
}
