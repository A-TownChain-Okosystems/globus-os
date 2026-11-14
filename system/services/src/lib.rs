//! Deterministic service lifecycle and dependency model.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceState {
    /// Declared but not started.
    Defined,
    /// Start requested, dependencies not all ready.
    Starting,
    /// Running and serving requests.
    Ready,
    /// Start or runtime failure; see the service log.
    Failed,
    /// Stopped on purpose, either by shutdown or by request.
    Stopped,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Declared service: name, dependencies and whether it is critical.
pub struct ServiceSpec {
    /// Unique service name, matching the boot order.
    pub name: String,
    /// Services that must be ready first.
    pub dependencies: Vec<String>,
    /// If true, a failure of this service fails the boot.
    pub critical: bool,
}

/// Fixed start order of the user-facing services.
pub fn boot_order() -> [&'static str; 8] {
    [
        "security", "devices", "storage", "network", "runtime", "graphics", "audio", "aurora",
    ]
}
