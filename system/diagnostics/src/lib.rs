//! Deterministic, offline-capable system diagnostics and recovery contracts.

#![no_std]

extern crate alloc;
use alloc::string::String;
use alloc::vec::Vec;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Severity { Info = 0, Warning = 1, Error = 2, Critical = 3 }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Component { Hardware, Driver, Kernel, Memory, Storage, Network, User, Application, Update, Security, AiService, Blockchain, Service }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum RecoveryLevel { Information = 0, AutomaticSafeFix = 1, UserConfirmation = 2, AdministratorConfirmation = 3, RecoveryEnvironment = 4, EmergencyRecovery = 5 }

impl RecoveryLevel {
    pub const fn requires_confirmation(self) -> bool {
        matches!(self, Self::UserConfirmation | Self::AdministratorConfirmation | Self::RecoveryEnvironment | Self::EmergencyRecovery)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum RepairAction { RestartService, RestartApplication, RollbackUpdate, RestoreConfiguration, RestoreSnapshot, EnterSafeMode, RecoverDriver, RepairFilesystem, FullSystemRecovery }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum HealthState { Healthy = 0, Degraded = 1, Unhealthy = 2, Critical = 3 }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MetricSnapshot { pub cpu_percent: u8, pub ram_percent: u8, pub gpu_percent: u8, pub disk_percent: u8 }

impl MetricSnapshot {
    pub const fn new(cpu_percent: u8, ram_percent: u8, gpu_percent: u8, disk_percent: u8) -> Option<Self> {
        if cpu_percent > 100 || ram_percent > 100 || gpu_percent > 100 || disk_percent > 100 { None } else { Some(Self { cpu_percent, ram_percent, gpu_percent, disk_percent }) }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem { pub id: u64, pub severity: Severity, pub component: Component, pub title: String, pub detail: String, pub recommendation: Option<RepairAction>, pub recovery_level: RecoveryLevel }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrashReport {
    pub crash_id: u64, pub timestamp_ns: u64, pub application: Option<String>, pub process: Option<String>, pub thread: Option<String>,
    pub stack_trace: Option<String>, pub kernel_version: String, pub driver_version: Option<String>, pub hardware: String,
    pub recent_changes: String, pub system_logs: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event { pub sequence: u64, pub severity: Severity, pub component: Component, pub code: u16, pub message: String, pub timestamp_ns: u64 }

#[derive(Debug, Default)]
pub struct EventLog { next_sequence: u64, events: Vec<Event> }

impl EventLog {
    pub const MAX_EVENTS: usize = 256;
    pub const fn new() -> Self { Self { next_sequence: 1, events: Vec::new() } }
    pub fn record(&mut self, severity: Severity, component: Component, code: u16, message: &str, timestamp_ns: u64) -> u64 {
        let sequence = self.next_sequence; self.next_sequence = self.next_sequence.saturating_add(1);
        if self.events.len() == Self::MAX_EVENTS { self.events.remove(0); }
        self.events.push(Event { sequence, severity, component, code, message: String::from(message), timestamp_ns }); sequence
    }
    pub fn events(&self) -> &[Event] { &self.events }
    pub fn latest(&self) -> Option<&Event> { self.events.last() }
}

#[derive(Debug, Default)]
pub struct DiagnosticsEngine { next_problem: u64, problems: Vec<Problem>, crashes: Vec<CrashReport>, events: EventLog }

impl DiagnosticsEngine {
    pub const MAX_PROBLEMS: usize = 256;
    pub const MAX_CRASH_REPORTS: usize = 64;
    pub const fn new() -> Self { Self { next_problem: 1, problems: Vec::new(), crashes: Vec::new(), events: EventLog::new() } }
    pub fn scan(&mut self, metrics: MetricSnapshot, timestamp_ns: u64) -> Option<u64> {
        let mut first = None;
        if metrics.ram_percent >= 90 { first = Some(self.add_problem(Severity::Warning, Component::Memory, "High memory usage", "RAM usage is at or above 90 percent.", Some(RepairAction::RestartApplication), RecoveryLevel::UserConfirmation, timestamp_ns)); }
        if metrics.disk_percent >= 95 { let id = self.add_problem(Severity::Warning, Component::Storage, "Storage pressure", "Disk usage is at or above 95 percent.", Some(RepairAction::RestoreConfiguration), RecoveryLevel::UserConfirmation, timestamp_ns); if first.is_none() { first = Some(id); } }
        if metrics.cpu_percent >= 95 { let id = self.add_problem(Severity::Warning, Component::Kernel, "Sustained CPU pressure", "CPU utilization is at or above 95 percent.", None, RecoveryLevel::Information, timestamp_ns); if first.is_none() { first = Some(id); } }
        first
    }
    pub fn add_problem(&mut self, severity: Severity, component: Component, title: &str, detail: &str, recommendation: Option<RepairAction>, recovery_level: RecoveryLevel, timestamp_ns: u64) -> u64 {
        let id = self.next_problem; self.next_problem = self.next_problem.saturating_add(1);
        if self.problems.len() == Self::MAX_PROBLEMS { self.problems.remove(0); }
        self.problems.push(Problem { id, severity, component, title: String::from(title), detail: String::from(detail), recommendation, recovery_level });
        self.events.record(severity, component, 0x1000, title, timestamp_ns); id
    }
    pub fn record_crash(&mut self, report: CrashReport) {
        if self.crashes.len() == Self::MAX_CRASH_REPORTS { self.crashes.remove(0); }
        self.events.record(Severity::Critical, Component::Application, 0x2000, "Crash report recorded", report.timestamp_ns);
        self.crashes.push(report);
    }
    pub fn health_score(&self, metrics: MetricSnapshot) -> u8 {
        let average = (metrics.cpu_percent as u16 + metrics.ram_percent as u16 + metrics.gpu_percent as u16 + metrics.disk_percent as u16) / 4;
        100u8.saturating_sub(average as u8)
    }
    pub fn health_state(&self, score: u8) -> HealthState { match score { 90..=100 => HealthState::Healthy, 70..=89 => HealthState::Degraded, 40..=69 => HealthState::Unhealthy, _ => HealthState::Critical } }
    pub fn problems(&self) -> &[Problem] { &self.problems }
    pub fn crashes(&self) -> &[CrashReport] { &self.crashes }
    pub fn events(&self) -> &EventLog { &self.events }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn metrics() -> MetricSnapshot { MetricSnapshot::new(40, 91, 30, 60).unwrap() }
    #[test] fn scan_detects_high_memory_without_network() { let mut engine = DiagnosticsEngine::new(); let id = engine.scan(metrics(), 100).unwrap(); assert_eq!(id, 1); assert_eq!(engine.problems()[0].component, Component::Memory); assert!(engine.problems()[0].recovery_level.requires_confirmation()); }
    #[test] fn metric_validation_rejects_values_above_hundred() { assert!(MetricSnapshot::new(101, 0, 0, 0).is_none()); }
    #[test] fn health_score_is_deterministic() { let engine = DiagnosticsEngine::new(); assert_eq!(engine.health_score(metrics()), 69); assert_eq!(engine.health_state(69), HealthState::Unhealthy); }
    #[test] fn crash_report_is_retained() { let mut engine = DiagnosticsEngine::new(); engine.record_crash(CrashReport { crash_id: 7, timestamp_ns: 10, application: Some(String::from("shell")), process: Some(String::from("shell")), thread: None, stack_trace: None, kernel_version: String::from("0.1.0"), driver_version: None, hardware: String::from("test"), recent_changes: String::from("none"), system_logs: String::from("test") }); assert_eq!(engine.crashes()[0].crash_id, 7); assert_eq!(engine.events().latest().unwrap().code, 0x2000); }
}