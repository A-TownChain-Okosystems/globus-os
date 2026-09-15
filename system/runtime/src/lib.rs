//! Integrated GlobusOS userspace runtime.

use globus_services::ServiceState;
use globus_system_core::SystemState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeStatus { pub system: SystemState, pub services: ServiceState }

pub fn initial_status() -> RuntimeStatus {
    RuntimeStatus { system: SystemState::Booting, services: ServiceState::Defined }
}
