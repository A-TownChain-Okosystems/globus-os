//! GlobusOS system integration boundary.

pub mod boot;

use globus_ipc::Endpoint;
use globus_memory::AddressSpace;
use globus_process::ProcessId;
use globus_security::Capability;

pub use boot::{BOOT_PLAN, BootService, BootStep, validate_boot_plan};

/// Kernel-facing identity of a GlobusOS service.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServiceIdentity {
    /// Process that hosts the service.
    pub process: ProcessId,
    /// IPC endpoint other services talk to.
    pub endpoint: Endpoint,
    /// Address space the service runs in.
    pub address_space: AddressSpace,
    /// Capability under which the service may act.
    pub capability: Capability,
}

/// Explicit system lifecycle states.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemState {
    /// Kernel is starting, no service is up yet.
    Booting,
    /// Boot plan is running, services are starting.
    Initializing,
    /// All planned services are up; user sessions allowed.
    MultiUser,
    /// Degraded operation after a failed boot step.
    Recovery,
    /// Services are being stopped in reverse order.
    ShuttingDown,
}

impl SystemState {
    /// Returns true only in the multi-user state, when user services are up.
pub fn is_operational(self) -> bool {
        matches!(self, Self::MultiUser)
    }
}
