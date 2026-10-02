//! Capability-authorized repair execution boundary.
//!
//! Diagnostics may propose a repair, but only a caller holding the required
//! authorization may execute it. The executor is injected by the OS service
//! layer; this crate never performs privileged operations itself.

use super::{RepairAction, RepairLevel, RepairRequest, RepairResult};

/// Explicit capability required for a repair class.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Capability {
    ObserveDiagnostics,
    RestartService,
    RestartApplication,
    RollbackUpdate,
    RestoreConfiguration,
    RestoreSnapshot,
    EnterSafeMode,
    RecoverDriver,
    RepairFilesystem,
    FullSystemRecovery,
}

impl Capability {
    pub const fn for_action(action: &RepairAction) -> Self {
        match action {
            RepairAction::RestartService { .. } => Self::RestartService,
            RepairAction::RestartApplication { .. } => Self::RestartApplication,
            RepairAction::RollbackUpdate { .. } => Self::RollbackUpdate,
            RepairAction::RestoreConfiguration { .. } => Self::RestoreConfiguration,
            RepairAction::RestoreSnapshot { .. } => Self::RestoreSnapshot,
            RepairAction::EnterSafeMode => Self::EnterSafeMode,
            RepairAction::RecoverDriver { .. } => Self::RecoverDriver,
            RepairAction::RepairFilesystem { .. } => Self::RepairFilesystem,
            RepairAction::FullSystemRecovery => Self::FullSystemRecovery,
        }
    }
}

/// Authorization presented by the privileged OS service.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RepairAuthorization {
    pub granted_level: RepairLevel,
    pub user_approved: bool,
    pub administrator_approved: bool,
    pub recovery_environment: bool,
    pub emergency_authorized: bool,
}

impl RepairAuthorization {
    pub const fn new(granted_level: RepairLevel) -> Self {
        Self {
            granted_level,
            user_approved: false,
            administrator_approved: false,
            recovery_environment: false,
            emergency_authorized: false,
        }
    }

    pub const fn allows(&self, request: &RepairRequest) -> bool {
        if self.granted_level as u8 < request.required_level as u8 {
            return false;
        }
        match request.required_level {
            RepairLevel::Information | RepairLevel::AutomaticSafeFix => true,
            RepairLevel::UserConfirmation => self.user_approved,
            RepairLevel::AdministratorConfirmation => {
                self.user_approved && self.administrator_approved
            }
            RepairLevel::RecoveryEnvironment => {
                self.user_approved && self.administrator_approved && self.recovery_environment
            }
            RepairLevel::EmergencyRecovery => {
                self.user_approved
                    && self.administrator_approved
                    && self.recovery_environment
                    && self.emergency_authorized
            }
        }
    }
}

/// Policy check performed before privileged execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RepairPolicy;

impl RepairPolicy {
    pub const fn authorize(
        request: &RepairRequest,
        authorization: &RepairAuthorization,
    ) -> Result<Capability, RepairResult> {
        if !authorization.allows(request) {
            return Err(RepairResult::RejectedByPolicy);
        }
        Ok(Capability::for_action(&request.action))
    }
}

/// OS-provided implementation that performs the actual privileged operation.
pub trait RepairExecutor {
    fn execute(
        &mut self,
        capability: Capability,
        action: &RepairAction,
    ) -> RepairResult;
}

/// Immutable evidence returned after an authorized execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RepairReceipt {
    pub problem_id: u64,
    pub capability: Capability,
    pub result: RepairResult,
}

impl RepairReceipt {
    pub const fn verified(problem_id: u64, capability: Capability) -> Self {
        Self {
            problem_id,
            capability,
            result: RepairResult::Applied,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Domain, Severity};
    use alloc::string::String;

    struct StubExecutor {
        calls: usize,
        last: Option<Capability>,
    }

    impl RepairExecutor for StubExecutor {
        fn execute(&mut self, capability: Capability, _action: &RepairAction) -> RepairResult {
            self.calls += 1;
            self.last = Some(capability);
            RepairResult::Applied
        }
    }

    #[test]
    fn user_confirmation_is_required() {
        let request = RepairRequest {
            problem_id: 7,
            action: RepairAction::RestartService {
                service: String::from("network"),
            },
            required_level: RepairLevel::UserConfirmation,
            user_approved: false,
        };
        let auth = RepairAuthorization::new(RepairLevel::UserConfirmation);
        assert_eq!(
            RepairPolicy::authorize(&request, &auth),
            Err(RepairResult::RejectedByPolicy)
        );
    }

    #[test]
    fn administrator_recovery_requires_all_authority_flags() {
        let request = RepairRequest {
            problem_id: 8,
            action: RepairAction::FullSystemRecovery,
            required_level: RepairLevel::EmergencyRecovery,
            user_approved: false,
        };
        let mut auth = RepairAuthorization::new(RepairLevel::EmergencyRecovery);
        auth.user_approved = true;
        auth.administrator_approved = true;
        auth.recovery_environment = true;
        assert!(!auth.allows(&request));
        auth.emergency_authorized = true;
        assert!(auth.allows(&request));
    }

    #[test]
    fn capability_is_derived_from_action() {
        let request = RepairRequest {
            problem_id: 1,
            action: RepairAction::RecoverDriver {
                driver: String::from("gpu"),
            },
            required_level: RepairLevel::AdministratorConfirmation,
            user_approved: true,
        };
        let mut auth = RepairAuthorization::new(RepairLevel::AdministratorConfirmation);
        auth.user_approved = true;
        auth.administrator_approved = true;
        assert_eq!(
            RepairPolicy::authorize(&request, &auth),
            Ok(Capability::RecoverDriver)
        );
    }

    #[test]
    fn executor_is_explicitly_injected() {
        let mut executor = StubExecutor { calls: 0, last: None };
        let request = RepairRequest {
            problem_id: 3,
            action: RepairAction::RestartApplication {
                application: String::from("shell"),
            },
            required_level: RepairLevel::UserConfirmation,
            user_approved: true,
        };
        let mut auth = RepairAuthorization::new(RepairLevel::UserConfirmation);
        auth.user_approved = true;
        let capability = RepairPolicy::authorize(&request, &auth).unwrap();
        assert_eq!(executor.execute(capability, &request.action), RepairResult::Applied);
        assert_eq!(executor.calls, 1);
        assert_eq!(executor.last, Some(Capability::RestartApplication));
        let _ = (Domain::Network, Severity::Info);
    }
}
