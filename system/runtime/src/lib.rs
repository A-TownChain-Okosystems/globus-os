//! Integrated GlobusOS userspace runtime.
//!
//! Boot contract:
//! UEFI/bootloader -> ShivaCore kernel -> GlobusOS userspace ->
//! security/services -> Aurora control plane + A-TownChain service space.
//!
//! AI and blockchain remain outside the ShivaCore TCB and are started as
//! explicit userspace/service-space components after the kernel is ready.

use aurora_core::{AuroraError, AuroraRequest, AuroraResponse, RequestStatus, StateMachine};
use globus_identity::{IdentitySession, LoginState, UserId, WalletAddress};
use globus_services::ServiceState;
use globus_system_core::{validate_boot_plan, SystemState, BOOT_PLAN};
use shivacore_service_space::genesis::{GenesisAllocation, GenesisConfig, GenesisValidator, LockType, GENESIS_CHAIN_ID};
use shivacore_service_space::genesis_bridge::GenesisBridge;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeStatus {
    pub system: SystemState,
    pub services: ServiceState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoginContext {
    pub session: IdentitySession,
}

impl LoginContext {
    pub fn new(
        user_id: UserId,
        wallet_address: WalletAddress,
        now_unix: u64,
        ttl_seconds: u64,
    ) -> Self {
        Self {
            session: IdentitySession {
                user_id,
                wallet_address,
                state: LoginState::Authenticated,
                issued_at_unix: now_unix,
                expires_at_unix: now_unix.saturating_add(ttl_seconds),
            },
        }
    }

    pub fn active(&self, now_unix: u64) -> bool {
        self.session.is_active(now_unix)
    }

    pub fn lock(&mut self) {
        self.session.lock();
    }
}

/// Fully initialized GlobusOS userspace service runtime.
///
/// The kernel owns the TCB. This object owns only post-kernel service state:
/// Aurora AI control-plane state and the A-TownChain service-space genesis.
pub struct BootedRuntime {
    pub status: RuntimeStatus,
    pub boot_plan: &'static [globus_system_core::BootStep],
    pub aurora: StateMachine,
    pub blockchain: GenesisBridge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeBootError {
    InvalidBootPlan,
    Genesis(aurora_core::AuroraError),
}

/// Build the deterministic development genesis used by the OS integration
/// smoke path. Production genesis material MUST come from governed chain
/// configuration; this function is intentionally explicit and deterministic.
pub fn devnet_genesis_config() -> GenesisConfig {
    let mut config = GenesisConfig::new(GENESIS_CHAIN_ID, 1_726_358_400);
    for i in 1u8..=4 {
        let mut pubkey = [0u8; 33];
        pubkey[0] = 2;
        pubkey[1] = i;
        let did = format!("did:atc:devnet-validator-{i}");
        let address = format!("ATCDEVNET{i:02}");
        config
            .add_validator(GenesisValidator {
                did,
                pubkey,
                stake: 10_000,
                address,
                commission: 0,
            })
            .expect("deterministic devnet validator must be valid");
    }
    config
        .add_allocation(GenesisAllocation {
            address: "ATCDEVNET00".to_owned(),
            amount: 1_000_000_000,
            lock_type: LockType::None,
            lock_duration: 0,
        })
        .expect("deterministic devnet allocation must be valid");
    config.memo = "GlobusOS userspace integration devnet".to_owned();
    config
}

/// Start the post-kernel OS service stack.
///
/// This is the missing hand-off between a booted ShivaCore kernel and the
/// userspace AI/blockchain services. No AI or chain state is initialized in
/// the kernel TCB.
pub fn boot_userspace() -> Result<BootedRuntime, RuntimeBootError> {
    if !validate_boot_plan() {
        return Err(RuntimeBootError::InvalidBootPlan);
    }

    let config = devnet_genesis_config();
    let blockchain = GenesisBridge::init_from_config(&config)
        .map_err(|_| RuntimeBootError::InvalidBootPlan)?;

    Ok(BootedRuntime {
        status: RuntimeStatus {
            system: SystemState::MultiUser,
            services: ServiceState::Ready,
        },
        boot_plan: BOOT_PLAN,
        aurora: StateMachine::new(),
        blockchain,
    })
}

/// Aurora entry point for GlobusOS userspace.
pub fn aurora_request_lifecycle(request: &AuroraRequest) -> Result<AuroraResponse, AuroraError> {
    if request.principal.trim().is_empty() || request.intent.trim().is_empty() {
        return Err(AuroraError::InvalidRequest);
    }

    let mut state = StateMachine::new();
    state.transition(RequestStatus::Queued)?;
    state.transition(RequestStatus::Running)?;
    state.transition(RequestStatus::Verifying)?;
    state.transition(RequestStatus::Completed)?;

    Ok(AuroraResponse {
        request_id: request.request_id.clone(),
        status: state.status(),
        result: Some("Aurora request accepted by GlobusOS runtime".to_owned()),
        audit_reference: None,
    })
}

pub fn initial_status() -> RuntimeStatus {
    RuntimeStatus {
        system: SystemState::Booting,
        services: ServiceState::Defined,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aurora_core::{RequestId, SessionId};
    use globus_identity::{create_wallet, UserId};

    #[test]
    fn runtime_can_establish_authenticated_context() {
        let user = UserId::new("runtime-test").unwrap();
        let wallet = create_wallet(user.clone(), 600, "devnet", 1).unwrap();
        let mut context = LoginContext::new(user, wallet.wallet_address, 1_000, 300);
        assert!(context.active(1_299));
        assert!(!context.active(1_300));
        context.lock();
        assert!(!context.active(1_100));
    }

    #[test]
    fn userspace_boot_starts_ai_and_blockchain_outside_kernel_tcb() {
        let runtime = boot_userspace().unwrap();
        assert_eq!(runtime.status.system, SystemState::MultiUser);
        assert_eq!(runtime.status.services, ServiceState::Ready);
        assert_eq!(runtime.blockchain.chain.chain_id(), GENESIS_CHAIN_ID);
        assert_eq!(runtime.blockchain.chain.block_count(), 1);
        assert_eq!(runtime.blockchain.chain.current_height(), 0);
        assert_eq!(runtime.blockchain.validators.active_count(), 4);
        assert_eq!(runtime.aurora.status(), RequestStatus::Created);
    }

    #[test]
    fn aurora_request_completes_control_plane_lifecycle() {
        let request = AuroraRequest {
            request_id: RequestId::new("req-1").unwrap(),
            session_id: SessionId::new("session-1").unwrap(),
            principal: "user:runtime-test".into(),
            intent: "open settings".into(),
        };
        let response = aurora_request_lifecycle(&request).unwrap();
        assert_eq!(response.status, RequestStatus::Completed);
        assert_eq!(response.request_id, request.request_id);
    }
}
