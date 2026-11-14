//! Integrated GlobusOS userspace runtime.

mod core_api;
mod error_reporting;
pub use core_api::{ProcessHandle, RuntimeCore, RuntimeError};
pub use error_reporting::{SystemErrorView, latest_error, present};

use aurora_core::{AuroraError, AuroraRequest, AuroraResponse, RequestStatus, StateMachine};
use globus_diagnostics::EventLog;
use globus_identity::{IdentitySession, LoginState, UserId, WalletAddress};
use globus_services::ServiceState;
use globus_system_core::{BOOT_PLAN, BootStep, SystemState, validate_boot_plan};
use shivacore_service_space::genesis::{
    GENESIS_CHAIN_ID, GenesisAllocation, GenesisConfig, GenesisValidator, LockType,
};
use shivacore_service_space::genesis_bridge::GenesisBridge;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// System and service state as the runtime reports them.
pub struct RuntimeStatus {
    /// Kernel-side system state.
    pub system: SystemState,
    /// Service subsystem state.
    pub services: ServiceState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Login session of a userspace user.
pub struct LoginContext {
    /// The authenticated identity session.
    pub session: IdentitySession,
}

impl LoginContext {
    /// Opens an authenticated session that expires after `ttl_seconds`.
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

    /// True while the session has not expired.
    pub fn active(&self, now_unix: u64) -> bool {
        self.session.is_active(now_unix)
    }
    /// Ends the session.
    pub fn lock(&mut self) {
        self.session.lock();
    }
}

/// Result of a successful userspace boot.
pub struct BootedRuntime {
    /// Status the runtime reports after boot.
    pub status: RuntimeStatus,
    /// Boot plan that was validated during boot.
    pub boot_plan: &'static [BootStep],
    /// Aurora request state machine of this runtime.
    pub aurora: StateMachine,
    /// Genesis bridge initialised from the devnet config.
    pub blockchain: GenesisBridge,
    /// Event log for crash and driver events.
    pub diagnostics: EventLog,
    /// ABI facade handed to userspace.
    pub core: RuntimeCore,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Why userspace boot stopped.
pub enum RuntimeBootError {
    /// The boot plan failed validation.
    InvalidBootPlan,
    /// Genesis initialisation failed.
    Genesis(aurora_core::AuroraError),
}

/// Deterministic devnet genesis: four validators, fixed allocations and timestamp.
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

/// A desktop session bound to a platform-validated software framebuffer.
pub struct DesktopRuntime<'a> {
    display: globus_graphics::display::Framebuffer<'a>,
    /// Desktop session holding windows and focus.
    pub session: globus_graphics::desktop::DesktopSession,
    /// Shell that draws the desktop.
    pub shell: globus_graphics::shell::DesktopShell,
}

impl DesktopRuntime<'_> {
    /// Redraws the shell into the provided framebuffer.
    pub fn render(&mut self) {
        self.display
            .render_shell(self.session.windows(), &self.shell);
    }

    /// Current viewport of the desktop.
    pub fn viewport(&self) -> globus_graphics::desktop::DesktopViewport {
        self.session.viewport()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Why the desktop refused to start.
pub enum DesktopStartError {
    /// Core services are not Ready yet.
    ServicesUnavailable,
}

/// Starts the desktop only after core services report Ready and the caller has
/// supplied a validated framebuffer. Hardware discovery and physical mapping
/// remain the responsibility of the platform boot/display driver.
pub fn start_desktop<'a>(
    runtime: &BootedRuntime,
    display: globus_graphics::display::Framebuffer<'a>,
) -> Result<DesktopRuntime<'a>, DesktopStartError> {
    if runtime.status.services != ServiceState::Ready {
        return Err(DesktopStartError::ServicesUnavailable);
    }
    let viewport = globus_graphics::desktop::DesktopViewport {
        width: display.width(),
        height: display.height(),
    };
    let mut session = globus_graphics::desktop::DesktopSession::new(viewport);
    session.unlock();
    let mut shell = globus_graphics::shell::DesktopShell::new();
    shell.register_app("Terminal", "globus-terminal", true);
    shell.register_app("Settings", "globus-settings", true);
    Ok(DesktopRuntime {
        display,
        session,
        shell,
    })
}

/// Validates the boot plan, initialises genesis and returns the booted runtime.
pub fn boot_userspace() -> Result<BootedRuntime, RuntimeBootError> {
    if !validate_boot_plan() {
        return Err(RuntimeBootError::InvalidBootPlan);
    }
    let config = devnet_genesis_config();
    let blockchain =
        GenesisBridge::init_from_config(&config).map_err(|_| RuntimeBootError::InvalidBootPlan)?;
    Ok(BootedRuntime {
        status: RuntimeStatus {
            system: SystemState::MultiUser,
            services: ServiceState::Ready,
        },
        boot_plan: BOOT_PLAN,
        aurora: StateMachine::new(),
        blockchain,
        diagnostics: EventLog::new(),
        core: RuntimeCore::new(),
    })
}

/// Runs one Aurora request through Queued, Running, Verifying and Completed.
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

/// Status before boot completes: system booting, services defined.
pub fn initial_status() -> RuntimeStatus {
    RuntimeStatus {
        system: SystemState::Booting,
        services: ServiceState::Defined,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use globus_identity::{UserId, create_wallet};

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
    fn userspace_boot_starts_ai_blockchain_and_diagnostics() {
        let runtime = boot_userspace().unwrap();
        assert_eq!(runtime.status.system, SystemState::MultiUser);
        assert_eq!(runtime.status.services, ServiceState::Ready);
        assert_eq!(runtime.blockchain.chain.chain_id(), GENESIS_CHAIN_ID);
        assert_eq!(runtime.blockchain.chain.block_count(), 1);
        assert_eq!(runtime.blockchain.validators.active_count(), 4);
        assert_eq!(runtime.core.abi_version(), libshivacore::ABI_VERSION);
        assert!(runtime.diagnostics.latest().is_none());
    }
}
