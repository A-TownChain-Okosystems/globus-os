//! Deterministic GlobusOS userspace initialization plan.
//!
//! This crate describes the order and authority boundaries of init. It does not
//! directly touch hardware; privileged operations are delegated to ShivaCore and
//! isolated device/system services.

use globus_ipc::Endpoint;
use globus_security::{Capability, Grant, Right};
use globus_services::ServiceState;
use globus_vfs::required_mounts;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InitStage {
    KernelHandoff,
    Ipc,
    Security,
    Storage,
    Devices,
    Network,
    Services,
    UserSession,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityHandoff {
    pub service: &'static str,
    pub endpoint: Endpoint,
    pub grant: Grant,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootPlan {
    pub stages: [InitStage; 8],
    pub mounts: usize,
    pub services: ServiceState,
}

pub fn default_plan() -> BootPlan {
    BootPlan {
        stages: [
            InitStage::KernelHandoff,
            InitStage::Ipc,
            InitStage::Security,
            InitStage::Storage,
            InitStage::Devices,
            InitStage::Network,
            InitStage::Services,
            InitStage::UserSession,
        ],
        mounts: required_mounts().len(),
        services: ServiceState::Defined,
    }
}

pub fn system_service_capability(service: &'static str, endpoint: Endpoint) -> CapabilityHandoff {
    CapabilityHandoff {
        service,
        endpoint,
        grant: Grant {
            capability: Capability(endpoint.0 as u128),
            right: Right::Admin,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boot_order_is_stable() {
        let plan = default_plan();
        assert_eq!(plan.stages[0], InitStage::KernelHandoff);
        assert_eq!(plan.stages[7], InitStage::UserSession);
        assert_eq!(plan.mounts, 6);
    }

    #[test]
    fn capability_handoff_is_explicit() {
        let handoff = system_service_capability("device-manager", Endpoint(7));
        assert_eq!(handoff.endpoint, Endpoint(7));
        assert_eq!(handoff.grant.capability, Capability(7));
        assert_eq!(handoff.grant.right, Right::Admin);
    }
}
