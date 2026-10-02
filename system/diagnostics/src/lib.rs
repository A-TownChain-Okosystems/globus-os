//! GlobusOS System Diagnostics & Recovery Service (GSDS).
//!
//! This crate defines the deterministic, machine-readable diagnostics contract.
//! Hardware probes and repair executors remain outside this crate and must be
//! injected through explicit traits/capability boundaries.

#![no_std]

extern crate alloc;

pub mod executor;
pub mod evidence;

pub use evidence::{EvidenceLedger, EvidenceRecord, EvidenceStage};
pub use executor::{Capability, RepairAuthorization, RepairExecutor, RepairPolicy, RepairReceipt};

use alloc::{string::String, vec::Vec};

/// Problem severity used by the diagnostics UI and policy engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum Severity {
    Info = 0,
    Warning = 1,
    Error = 2,
    Critical = 3,
}

/// System area associated with a diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Domain {
    Hardware,
    Driver,
    Kernel,
    Memory,
    Storage,
    Network,
    User,
    Application,
    Update,
    Security,
    AiService,
    Blockchain,
}

/// Allowed repair authorization level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum RepairLevel {
    Information = 0,
    AutomaticSafeFix = 1,
    UserConfirmation = 2,
    AdministratorConfirmation = 3,
    RecoveryEnvironment = 4,
    EmergencyRecovery = 5,
}

/// Health state for one subsystem.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthState {
    Healthy,
    Degraded,
    Faulted,
    Unknown,
}

/// A normalized system health entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HealthEntry {
    pub domain: Domain,
    pub state: HealthState,
    pub score: u8,
}

impl HealthEntry {
    pub const fn new(domain: Domain, state: HealthState, score: u8) -> Self {
        Self {
            domain,
            state,
            score: if score > 100 { 100 } else { score },
        }
    }
}

/// Aggregate health snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HealthSnapshot {
    pub overall: u8,
    pub entries: Vec<HealthEntry>,
}

impl HealthSnapshot {
    pub fn from_entries(entries: Vec<HealthEntry>) -> Self {
        if entries.is_empty() {
            return Self {
                overall: 0,
                entries,
            };
        }
        let total: u32 = entries
            .iter()
            .map(|entry| u32::from(entry.score))
            .sum();
        Self {
            overall: (total / entries.len() as u32) as u8,
            entries,
        }
    }

    pub fn state(&self) -> HealthState {
        if self
            .entries
            .iter()
            .any(|e| e.state == HealthState::Faulted)
        {
            return HealthState::Faulted;
        }
        if self
            .entries
            .iter()
            .any(|e| e.state == HealthState::Degraded)
        {
            return HealthState::Degraded;
        }
        if self.entries.is_empty() {
            HealthState::Unknown
        } else {
            HealthState::Healthy
        }
    }
}

/// A detected problem with a stable identifier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    pub id: u64,
    pub code: String,
    pub domain: Domain,
    pub severity: Severity,
    pub title: String,
    pub detail: String,
    pub repair_level: RepairLevel,
    pub resolved: bool,
}

impl Problem {
    pub fn new(
        id: u64,
        code: impl Into<String>,
        domain: Domain,
        severity: Severity,
        title: impl Into<String>,
        detail: impl Into<String>,
        repair_level: RepairLevel,
    ) -> Self {
        Self {
            id,
            code: code.into(),
            domain,
            severity,
            title: title.into(),
            detail: detail.into(),
            repair_level,
            resolved: false,
        }
    }
}

/// Explicit repair actions. Execution belongs to a capability-authorized service.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepairAction {
    RestartService { service: String },
    RestartApplication { application: String },
    RollbackUpdate { update_id: String },
    RestoreConfiguration { snapshot_id: String },
    RestoreSnapshot { snapshot_id: String },
    EnterSafeMode,
    RecoverDriver { driver: String },
    RepairFilesystem { volume: String },
    FullSystemRecovery,
}

/// A repair request. Execution is deliberately separate from diagnosis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepairRequest {
    pub problem_id: u64,
    pub action: RepairAction,
    pub required_level: RepairLevel,
    pub user_approved: bool,
}

/// Result returned by a repair executor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepairResult {
    Applied,
    RejectedByPolicy,
    NotSupported,
    Failed,
    VerificationFailed,
}

/// Crash classification used by Crash Center.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrashKind {
    Application,
    Service,
    Driver,
    KernelPanic,
    GpuTimeout,
    Hardware,
}

/// Machine-readable crash evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrashReport {
    pub id: u64,
    pub kind: CrashKind,
    pub timestamp_ns: u64,
    pub component: String,
    pub process: Option<String>,
    pub thread: Option<String>,
    pub stack_trace: String,
    pub kernel_version: String,
    pub driver_version: Option<String>,
    pub hardware: String,
    pub recent_changes: Vec<String>,
    pub logs: Vec<String>,
}

/// Deterministic, bounded diagnostics state.
#[derive(Debug)]
pub struct DiagnosticsEngine {
    evidence: EvidenceLedger,
    next_problem_id: u64,
    next_crash_id: u64,
    problems: Vec<Problem>,
    crashes: Vec<CrashReport>,
    health: HealthSnapshot,
}

impl DiagnosticsEngine {
    pub const MAX_PROBLEMS: usize = 256;
    pub const MAX_CRASHES: usize = 64;

    pub fn new() -> Self {
        Self {
            evidence: EvidenceLedger::new(),
            next_problem_id: 1,
            next_crash_id: 1,
            problems: Vec::new(),
            crashes: Vec::new(),
            health: HealthSnapshot::from_entries(Vec::new()),
        }
    }

    /// Runs a supplied set of deterministic observations.
    pub fn scan(&mut self, entries: Vec<HealthEntry>) -> &HealthSnapshot {
        self.health = HealthSnapshot::from_entries(entries);
        &self.health
    }

    pub fn health(&self) -> &HealthSnapshot {
        &self.health
    }

    pub fn evidence(&self) -> &[EvidenceRecord] {
        self.evidence.records()
    }

    pub fn problems(&self) -> &[Problem] {
        &self.problems
    }

    pub fn crashes(&self) -> &[CrashReport] {
        &self.crashes
    }

    pub fn add_problem(
        &mut self,
        code: impl Into<String>,
        domain: Domain,
        severity: Severity,
        title: impl Into<String>,
        detail: impl Into<String>,
        repair_level: RepairLevel,
    ) -> u64 {
        let id = self.next_problem_id;
        self.next_problem_id = self.next_problem_id.saturating_add(1);
        if self.problems.len() == Self::MAX_PROBLEMS {
            self.problems.remove(0);
        }
        self.problems.push(Problem::new(
            id,
            code,
            domain,
            severity,
            title,
            detail,
            repair_level,
        ));
        id
    }

    pub fn explain(&self, problem_id: u64) -> Option<&Problem> {
        self.problems
            .iter()
            .find(|problem| problem.id == problem_id)
    }

    pub fn propose_repair(
        &self,
        problem_id: u64,
        action: RepairAction,
    ) -> Option<RepairRequest> {
        let problem = self.explain(problem_id)?;
        Some(RepairRequest {
            problem_id,
            action,
            required_level: problem.repair_level,
            user_approved: false,
        })
    }

    /// Executes an authorized repair through the injected executor and verifies the result.
    pub fn repair<E: crate::executor::RepairExecutor>(
        &mut self,
        request: &crate::executor::RepairRequest,
        authorization: &crate::executor::RepairAuthorization,
        executor: &mut E,
    ) -> crate::executor::RepairReceipt {
        let capability = match crate::executor::RepairPolicy::authorize(request, authorization) {
            Ok(capability) => capability,
            Err(result) => {
                return crate::executor::RepairReceipt {
                    problem_id: request.problem_id,
                    capability: crate::executor::Capability::ObserveDiagnostics,
                    result,
                };
            }
        };
        if self.explain(request.problem_id).is_none() {
            return crate::executor::RepairReceipt {
                problem_id: request.problem_id,
                capability,
                result: crate::executor::RepairResult::Failed,
            };
        }
        let result = executor.execute(capability, &request.action);
        if result == crate::executor::RepairResult::Applied {
            let verified = self.verify_repair(request.problem_id, result);
            if !verified {
                return crate::executor::RepairReceipt {
                    problem_id: request.problem_id,
                    capability,
                    result: crate::executor::RepairResult::VerificationFailed,
                };
            }
        }
        crate::executor::RepairReceipt {
            problem_id: request.problem_id,
            capability,
            result,
        }
    }

    /// Marks a problem resolved only after an external executor verifies it.
    pub fn verify_repair(&mut self, problem_id: u64, result: RepairResult) -> bool {
        if result != RepairResult::Applied {
            return false;
        }
        if let Some(problem) = self.problems.iter_mut().find(|p| p.id == problem_id) {
            problem.resolved = true;
            true
        } else {
            false
        }
    }

    pub fn add_crash(
        &mut self,
        kind: CrashKind,
        timestamp_ns: u64,
        component: impl Into<String>,
        stack_trace: impl Into<String>,
        kernel_version: impl Into<String>,
        hardware: impl Into<String>,
    ) -> u64 {
        let id = self.next_crash_id;
        self.next_crash_id = self.next_crash_id.saturating_add(1);
        if self.crashes.len() == Self::MAX_CRASHES {
            self.crashes.remove(0);
        }
        self.crashes.push(CrashReport {
            id,
            kind,
            timestamp_ns,
            component: component.into(),
            process: None,
            thread: None,
            stack_trace: stack_trace.into(),
            kernel_version: kernel_version.into(),
            driver_version: None,
            hardware: hardware.into(),
            recent_changes: Vec::new(),
            logs: Vec::new(),
        });
        id
    }

    /// Generates a compact deterministic local report.
    pub fn report(&self) -> String {
        use core::fmt::Write;
        let mut report = String::new();
        let _ = writeln!(report, "GLOBUS SYSTEM HEALTH {}", self.health.overall);
        let _ = writeln!(report, "PROBLEMS {}", self.problems.len());
        let _ = writeln!(report, "CRASHES {}", self.crashes.len());
        for problem in &self.problems {
            let _ = writeln!(
                report,
                "PROBLEM {} {} {:?} {:?} resolved={}",
                problem.id, problem.code, problem.domain, problem.severity, problem.resolved
            );
        }
        report
    }
}

impl Default for DiagnosticsEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_is_deterministic() {
        let mut engine = DiagnosticsEngine::new();
        let snapshot = engine.scan(vec![
            HealthEntry::new(Domain::Hardware, HealthState::Healthy, 100),
            HealthEntry::new(Domain::Storage, HealthState::Degraded, 60),
        ]);
        assert_eq!(snapshot.overall, 80);
        assert_eq!(snapshot.state(), HealthState::Degraded);
    }

    #[test]
    fn repair_requires_external_verification() {
        let mut engine = DiagnosticsEngine::new();
        let id = engine.add_problem(
            "NET-0001",
            Domain::Network,
            Severity::Error,
            "Network service stopped",
            "Network hardware and configuration are available.",
            RepairLevel::UserConfirmation,
        );
        let request = engine
            .propose_repair(
                id,
                RepairAction::RestartService {
                    service: "globus-network".into(),
                },
            )
            .unwrap();
        assert!(!request.user_approved);
        assert!(!engine.problems()[0].resolved);
        assert!(engine.verify_repair(id, RepairResult::Applied));
        assert!(engine.problems()[0].resolved);
    }

    #[test]
    fn repair_pipeline_requires_policy_then_verification() {
        struct Executor;
        impl crate::executor::RepairExecutor for Executor {
            fn execute(
                &mut self,
                _capability: crate::executor::Capability,
                _action: &RepairAction,
            ) -> RepairResult {
                RepairResult::Applied
            }
        }

        let mut engine = DiagnosticsEngine::new();
        let id = engine.add_problem(
            "APP-0001",
            Domain::Application,
            Severity::Error,
            "Application stopped",
            "The process exited unexpectedly.",
            RepairLevel::UserConfirmation,
        );
        let request = engine
            .propose_repair(
                id,
                RepairAction::RestartApplication {
                    application: "shell".into(),
                },
            )
            .unwrap();
        let mut executor = Executor;
        let denied = engine.repair(
            &request,
            &crate::executor::RepairAuthorization::new(RepairLevel::UserConfirmation),
            &mut executor,
        );
        assert_eq!(denied.result, RepairResult::RejectedByPolicy);
        assert!(!engine.problems()[0].resolved);

        let mut authorization =
            crate::executor::RepairAuthorization::new(RepairLevel::UserConfirmation);
        authorization.user_approved = true;
        let applied = engine.repair(&request, &authorization, &mut executor);
        assert_eq!(applied.result, RepairResult::Applied);
        assert!(engine.problems()[0].resolved);
    }

    #[test]
    fn problem_history_is_bounded() {
        let mut engine = DiagnosticsEngine::new();
        for _ in 0..=DiagnosticsEngine::MAX_PROBLEMS {
            engine.add_problem(
                "TEST",
                Domain::Application,
                Severity::Info,
                "test",
                "test",
                RepairLevel::Information,
            );
        }
        assert_eq!(engine.problems().len(), DiagnosticsEngine::MAX_PROBLEMS);
    }
}
