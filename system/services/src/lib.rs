//! Deterministic service lifecycle and dependency model.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceState {
    Defined,
    Starting,
    Ready,
    Failed,
    Stopped,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Declared service: name, dependencies and whether it is critical.
pub struct ServiceSpec {
    pub name: String,
    pub dependencies: Vec<String>,
    pub critical: bool,
}

/// Fixed start order of the user-facing services.
pub fn boot_order() -> [&'static str; 8] {
    [
        "security", "devices", "storage", "network", "runtime", "graphics", "audio", "aurora",
    ]
}
