//! GlobusOS system integration boundary.

pub mod boot;

use globus_ipc::Endpoint;
use globus_memory::AddressSpace;
use globus_process::ProcessId;
use globus_security::Capability;

pub use boot::{validate_boot_plan, BootService, BootStep, BOOT_PLAN};

/// Kernel-facing identity of a GlobusOS service.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServiceIdentity {
    pub process: ProcessId,
    pub endpoint: Endpoint,
    pub address_space: AddressSpace,
    pub capability: Capability,
}

/// Explicit system lifecycle states.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemState {
    Booting,
    Initializing,
    MultiUser,
    Recovery,
    ShuttingDown,
}

impl SystemState {
    pub fn is_operational(self) -> bool {
        matches!(self, Self::MultiUser)
    }
}
