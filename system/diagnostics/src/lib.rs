//! Stable, machine-readable system fault reporting shared by ShivaCore and GlobusOS.

#![no_std]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Severity {
    Info = 0,
    Warning = 1,
    Error = 2,
    Critical = 3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum ErrorCode {
    DriverCrashed = 0x0007,
    DriverIsolated = 0x0008,
    DriverRecoveryFailed = 0x0009,
}

impl ErrorCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DriverCrashed => "SC-DRV-0007",
            Self::DriverIsolated => "SC-DRV-0008",
            Self::DriverRecoveryFailed => "SC-DRV-0009",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Component {
    Kernel,
    Driver,
    Device,
    Service,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrashEvent {
    pub sequence: u64,
    pub code: ErrorCode,
    pub severity: Severity,
    pub component: Component,
    pub driver: String,
    pub device_id: u64,
    pub timestamp_ns: u64,
    pub recovered: bool,
}

impl CrashEvent {
    pub fn driver_crash(sequence: u64, driver: &str, device_id: u64, timestamp_ns: u64) -> Self {
        Self {
            sequence,
            code: ErrorCode::DriverCrashed,
            severity: Severity::Critical,
            component: Component::Driver,
            driver: String::from(driver),
            device_id,
            timestamp_ns,
            recovered: false,
        }
    }
}

#[derive(Debug, Default)]
pub struct EventLog {
    next_sequence: u64,
    events: Vec<CrashEvent>,
}

impl EventLog {
    pub const MAX_EVENTS: usize = 256;

    pub const fn new() -> Self {
        Self {
            next_sequence: 1,
            events: Vec::new(),
        }
    }

    pub fn record_driver_crash(&mut self, driver: &str, device_id: u64, timestamp_ns: u64) -> u64 {
        let sequence = self.next_sequence;
        self.next_sequence = self.next_sequence.saturating_add(1);
        if self.events.len() == Self::MAX_EVENTS {
            self.events.remove(0);
        }
        self.events.push(CrashEvent::driver_crash(
            sequence,
            driver,
            device_id,
            timestamp_ns,
        ));
        sequence
    }

    pub fn mark_recovered(&mut self, sequence: u64) -> bool {
        let Some(event) = self.events.iter_mut().find(|e| e.sequence == sequence) else {
            return false;
        };
        event.recovered = true;
        event.code = ErrorCode::DriverIsolated;
        event.severity = Severity::Error;
        true
    }

    pub fn events(&self) -> &[CrashEvent] {
        &self.events
    }
    pub fn latest(&self) -> Option<&CrashEvent> {
        self.events.last()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn driver_crash_has_stable_code() {
        let mut log = EventLog::new();
        let seq = log.record_driver_crash("e1000", 42, 100);
        assert_eq!(seq, 1);
        let event = log.latest().unwrap();
        assert_eq!(event.code.as_str(), "SC-DRV-0007");
        assert_eq!(event.driver, "e1000");
        assert!(!event.recovered);
    }

    #[test]
    fn recovery_changes_event_state() {
        let mut log = EventLog::new();
        let seq = log.record_driver_crash("nvme", 7, 100);
        assert!(log.mark_recovered(seq));
        let event = log.latest().unwrap();
        assert_eq!(event.code, ErrorCode::DriverIsolated);
        assert!(event.recovered);
    }
}
