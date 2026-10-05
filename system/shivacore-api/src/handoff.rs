//! Kernel-to-userspace boot handoff adapter.
//!
//! The adapter is the single validation boundary between ShivaCore's stable
//! ABI and the GlobusOS service runtime. It does not grant authority by
//! default: every capability is explicitly declared and validated.

use crate::{AbiError, AbiHandshake, CapabilityHandle, CapabilityRight, ABI_VERSION};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum BootstrapCapability {
    Ipc = 0,
    Process = 1,
    Memory = 2,
    Device = 3,
    Storage = 4,
    Network = 5,
    Identity = 6,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CapabilityGrant {
    pub capability: CapabilityHandle,
    pub object: u64,
    pub right: CapabilityRight,
    pub bootstrap: BootstrapCapability,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CapabilityHandoff {
    pub ipc: CapabilityHandle,
    pub process: CapabilityHandle,
    pub memory: CapabilityHandle,
    pub grants: [CapabilityGrant; 7],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InitialTaskRole {
    RootServer = 0,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InitialTaskAuthorization {
    pub role: InitialTaskRole,
    pub image_id: [u8; 32],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KernelBootHandoff {
    pub abi: AbiHandshake,
    pub initial_task: InitialTaskAuthorization,
    pub capabilities: CapabilityHandoff,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ValidatedHandoff {
    pub abi_version: u32,
    pub initial_task: InitialTaskAuthorization,
    pub capabilities: CapabilityHandoff,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandoffError {
    Abi(AbiError),
    MissingBootstrapCapability(BootstrapCapability),
    DuplicateCapability(BootstrapCapability),
    InvalidHandle,
    InvalidCapabilityRight(BootstrapCapability),
    InvalidInitialTaskAuthorization,
}

impl From<AbiError> for HandoffError {
    fn from(error: AbiError) -> Self {
        Self::Abi(error)
    }
}

/// Validates and accepts the kernel-provided handoff.
///
/// The validation is fail-closed: a major ABI mismatch, zero handle,
/// duplicate bootstrap role, or unsafe bootstrap right is rejected before
/// GlobusOS can enter its service-start phase.
pub const fn accept_kernel_handoff(
    handoff: KernelBootHandoff,
) -> Result<ValidatedHandoff, HandoffError> {
    if !handoff.abi.compatible() {
        return Err(HandoffError::Abi(AbiError::AbiVersionMismatch));
    }

    if handoff.initial_task.image_id == [0; 32] {
        return Err(HandoffError::InvalidInitialTaskAuthorization);
    }

    let grants = handoff.capabilities.grants;
    let mut seen = [false; 7];

    let mut i = 0;
    while i < grants.len() {
        let grant = grants[i];
        if grant.capability.0 == 0 || grant.object == 0 {
            return Err(HandoffError::InvalidHandle);
        }

        let role = grant.bootstrap as usize;
        if seen[role] {
            return Err(HandoffError::DuplicateCapability(grant.bootstrap));
        }
        seen[role] = true;

        // Bootstrap authority is intentionally bounded. No service receives
        // Grant/Revoke/Admin-like authority through this handoff.
        if matches!(grant.right, CapabilityRight::Grant | CapabilityRight::Revoke) {
            return Err(HandoffError::InvalidCapabilityRight(grant.bootstrap));
        }

        i += 1;
    }

    if !seen[BootstrapCapability::Ipc as usize] {
        return Err(HandoffError::MissingBootstrapCapability(
            BootstrapCapability::Ipc,
        ));
    }
    if !seen[BootstrapCapability::Process as usize] {
        return Err(HandoffError::MissingBootstrapCapability(
            BootstrapCapability::Process,
        ));
    }
    if !seen[BootstrapCapability::Memory as usize] {
        return Err(HandoffError::MissingBootstrapCapability(
            BootstrapCapability::Memory,
        ));
    }

    Ok(ValidatedHandoff {
        abi_version: ABI_VERSION,
        initial_task: handoff.initial_task,
        capabilities: handoff.capabilities,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_handoff() -> KernelBootHandoff {
        let roles = [
            BootstrapCapability::Ipc,
            BootstrapCapability::Process,
            BootstrapCapability::Memory,
            BootstrapCapability::Device,
            BootstrapCapability::Storage,
            BootstrapCapability::Network,
            BootstrapCapability::Identity,
        ];
        let mut grants = [CapabilityGrant {
            capability: CapabilityHandle(1),
            object: 1,
            right: CapabilityRight::Inspect,
            bootstrap: BootstrapCapability::Ipc,
        }; 7];

        let mut i = 0;
        while i < grants.len() {
            grants[i] = CapabilityGrant {
                capability: CapabilityHandle((i + 1) as u64),
                object: (i + 1) as u64,
                right: CapabilityRight::Inspect,
                bootstrap: roles[i],
            };
            i += 1;
        }

        KernelBootHandoff {
            abi: AbiHandshake::CURRENT,
            initial_task: InitialTaskAuthorization {
                role: InitialTaskRole::RootServer,
                image_id: [0x47; 32],
            },
            capabilities: CapabilityHandoff {
                ipc: CapabilityHandle(1),
                process: CapabilityHandle(2),
                memory: CapabilityHandle(3),
                grants,
            },
        }
    }

    #[test]
    fn valid_handoff_is_accepted() {
        let accepted = accept_kernel_handoff(valid_handoff()).unwrap();
        assert_eq!(accepted.abi_version, ABI_VERSION);
    }

    #[test]
    fn zero_initial_task_identity_fails_closed() {
        let mut handoff = valid_handoff();
        handoff.initial_task.image_id = [0; 32];
        assert_eq!(
            accept_kernel_handoff(handoff),
            Err(HandoffError::InvalidInitialTaskAuthorization)
        );
    }

    #[test]
    fn initial_task_identity_is_preserved() {
        let handoff = valid_handoff();
        let accepted = accept_kernel_handoff(handoff).unwrap();
        assert_eq!(accepted.initial_task.role, InitialTaskRole::RootServer);
        assert_eq!(accepted.initial_task.image_id, [0x47; 32]);
    }

    #[test]
    fn major_version_mismatch_fails_closed() {
        let mut handoff = valid_handoff();
        handoff.abi = AbiHandshake { major: 2, minor: 0 };
        assert_eq!(
            accept_kernel_handoff(handoff),
            Err(HandoffError::Abi(AbiError::AbiVersionMismatch))
        );
    }

    #[test]
    fn missing_process_capability_is_rejected() {
        let mut handoff = valid_handoff();
        handoff.capabilities.grants[1].bootstrap = BootstrapCapability::Ipc;
        assert_eq!(
            accept_kernel_handoff(handoff),
            Err(HandoffError::DuplicateCapability(BootstrapCapability::Ipc))
        );
    }

    #[test]
    fn zero_handles_are_rejected() {
        let mut handoff = valid_handoff();
        handoff.capabilities.grants[0].capability = CapabilityHandle(0);
        assert_eq!(
            accept_kernel_handoff(handoff),
            Err(HandoffError::InvalidHandle)
        );
    }

    #[test]
    fn grant_authority_is_not_bootstrapped() {
        let mut handoff = valid_handoff();
        handoff.capabilities.grants[0].right = CapabilityRight::Grant;
        assert_eq!(
            accept_kernel_handoff(handoff),
            Err(HandoffError::InvalidCapabilityRight(BootstrapCapability::Ipc))
        );
    }
}
