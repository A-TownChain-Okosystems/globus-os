use std::string::String;

use globus_diagnostics::{CrashEvent, ErrorCode, EventLog, Severity};

/// Userspace-facing, immutable presentation of a kernel/service fault.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemErrorView {
    /// Stable error code as shown to the user.
    pub code: &'static str,
    /// Severity as classified by kernel or service.
    pub severity: Severity,
    /// Short human-readable title.
    pub title: &'static str,
    /// Explanation of what happened.
    pub message: String,
    /// Driver that raised the fault; empty when none is involved.
    pub driver: String,
    /// Device the fault belongs to, 0 when unknown.
    pub device_id: u64,
    /// True when the system recovered without user action.
    pub recovered: bool,
}

/// Converts a crash event into the immutable userspace view.
pub fn present(event: &CrashEvent) -> SystemErrorView {
    let (title, message) = match event.code {
        ErrorCode::DriverCrashed => (
            "Driver crashed",
            format!(
                "The driver '{}' crashed and was isolated to protect the system.",
                event.driver
            ),
        ),
        ErrorCode::DriverIsolated => (
            "Driver isolated",
            format!(
                "The driver '{}' was isolated after a fault; the rest of the system remains running.",
                event.driver
            ),
        ),
        ErrorCode::DriverRecoveryFailed => (
            "Driver recovery failed",
            format!(
                "The driver '{}' could not be recovered and remains offline.",
                event.driver
            ),
        ),
    };
    SystemErrorView {
        code: event.code.as_str(),
        severity: event.severity,
        title,
        message,
        driver: event.driver.clone(),
        device_id: event.device_id,
        recovered: event.recovered,
    }
}

/// Most recent crash event in the log, if any.
pub fn latest_error(log: &EventLog) -> Option<SystemErrorView> {
    log.latest().map(present)
}
