//! Integrated GlobusOS userspace runtime.

use globus_identity::{IdentitySession, LoginState, UserId, WalletAddress};
use globus_services::ServiceState;
use globus_system_core::SystemState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeStatus { pub system: SystemState, pub services: ServiceState }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoginContext {
    pub session: IdentitySession,
}

impl LoginContext {
    pub fn new(user_id: UserId, wallet_address: WalletAddress, now_unix: u64, ttl_seconds: u64) -> Self {
        Self { session: IdentitySession { user_id, wallet_address, state: LoginState::Authenticated, issued_at_unix: now_unix, expires_at_unix: now_unix.saturating_add(ttl_seconds) } }
    }

    pub fn active(&self, now_unix: u64) -> bool { self.session.is_active(now_unix) }
    pub fn lock(&mut self) { self.session.lock(); }
}

pub fn initial_status() -> RuntimeStatus {
    RuntimeStatus { system: SystemState::Booting, services: ServiceState::Defined }
}

#[cfg(test)]
mod tests {
    use super::*;
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
}
