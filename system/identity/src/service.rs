//! Stateful identity-service orchestration.
//!
//! This layer owns account/session state but deliberately does not persist or expose
//! recovery secrets. A production persistence backend must be added behind this boundary.

use super::{create_wallet, restore_wallet, AccountProfile, IdentityBinding, IdentityError, IdentitySession, LoginState, UserId, WalletCreation, WalletAddress};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdentityServiceError {
    Identity(IdentityError),
    AlreadyRegistered,
    NotRegistered,
    InvalidCredentials,
}

impl From<IdentityError> for IdentityServiceError {
    fn from(value: IdentityError) -> Self { Self::Identity(value) }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistrationResult { pub profile: AccountProfile, pub wallet_address: WalletAddress }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoginResult { pub session: IdentitySession }

pub struct IdentityService {
    profile: Option<AccountProfile>,
    credential_digest: Option<[u8; 32]>,
    session: Option<IdentitySession>,
}

impl IdentityService {
    pub const fn new() -> Self { Self { profile: None, credential_digest: None, session: None } }

    pub fn register(&mut self, user_id: UserId, display_name: impl Into<String>, credential: &[u8], chain_id: u64, network: impl Into<String>, key_version: u16) -> Result<(RegistrationResult, WalletCreation), IdentityServiceError> {
        if self.profile.is_some() { return Err(IdentityServiceError::AlreadyRegistered); }
        if credential.is_empty() { return Err(IdentityServiceError::InvalidCredentials); }
        let wallet = create_wallet(user_id, chain_id, network, key_version)?;
        let profile = AccountProfile::new(wallet.binding.user_id.clone(), display_name, wallet.binding.clone())?;
        self.credential_digest = Some(digest(credential));
        let result = RegistrationResult { wallet_address: profile.wallet_address.clone(), profile: profile.clone() };
        self.profile = Some(profile);
        Ok((result, wallet))
    }

    pub fn restore(&mut self, user_id: UserId, display_name: impl Into<String>, phrase: &str, credential: &[u8], chain_id: u64, network: impl Into<String>, key_version: u16) -> Result<RegistrationResult, IdentityServiceError> {
        if credential.is_empty() { return Err(IdentityServiceError::InvalidCredentials); }
        let binding = restore_wallet(user_id, phrase, chain_id, network, key_version)?;
        let profile = AccountProfile::new(binding.user_id.clone(), display_name, binding)?;
        self.credential_digest = Some(digest(credential));
        let result = RegistrationResult { wallet_address: profile.wallet_address.clone(), profile: profile.clone() };
        self.profile = Some(profile);
        self.session = None;
        Ok(result)
    }

    pub fn login(&mut self, credential: &[u8], now_unix: u64, ttl_seconds: u64) -> Result<LoginResult, IdentityServiceError> {
        let profile = self.profile.as_ref().ok_or(IdentityServiceError::NotRegistered)?;
        let expected = self.credential_digest.ok_or(IdentityServiceError::InvalidCredentials)?;
        if digest(credential) != expected { return Err(IdentityServiceError::InvalidCredentials); }
        let session = IdentitySession { user_id: profile.user_id.clone(), wallet_address: profile.wallet_address.clone(), state: LoginState::Authenticated, issued_at_unix: now_unix, expires_at_unix: now_unix.saturating_add(ttl_seconds) };
        self.session = Some(session.clone());
        Ok(LoginResult { session })
    }

    pub fn logout(&mut self) { self.session = None; }
    pub fn lock(&mut self) { if let Some(session) = &mut self.session { session.lock(); } }
    pub fn session(&self, now_unix: u64) -> Option<&IdentitySession> { self.session.as_ref().filter(|s| s.is_active(now_unix)) }
    pub fn profile(&self) -> Option<&AccountProfile> { self.profile.as_ref() }
    pub fn binding(&self) -> Option<&IdentityBinding> { self.profile.as_ref().map(|p| &p.binding) }
}

fn digest(value: &[u8]) -> [u8; 32] { use sha2::{Digest, Sha256}; Sha256::digest(value).into() }
impl Default for IdentityService { fn default() -> Self { Self::new() } }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn register_login_lock_logout_flow() {
        let user = UserId::new("alice").unwrap(); let mut service = IdentityService::new();
        let (registered, wallet) = service.register(user, "Alice", b"local-credential", 600, "devnet", 1).unwrap();
        assert_eq!(registered.wallet_address, wallet.wallet_address);
        let login = service.login(b"local-credential", 100, 300).unwrap();
        assert_eq!(login.session.state, LoginState::Authenticated); assert!(service.session(200).is_some());
        service.lock(); assert!(service.session(200).is_none()); service.logout(); assert!(service.session(200).is_none());
    }
    #[test]
    fn wrong_credential_is_denied() {
        let user = UserId::new("alice").unwrap(); let mut service = IdentityService::new();
        service.register(user, "Alice", b"correct", 600, "devnet", 1).unwrap();
        assert_eq!(service.login(b"wrong", 100, 300).unwrap_err(), IdentityServiceError::InvalidCredentials);
    }
    #[test]
    fn recovery_restores_wallet_identity() {
        let user = UserId::new("alice").unwrap(); let mut source = IdentityService::new();
        let (_, wallet) = source.register(user.clone(), "Alice", b"credential", 600, "devnet", 1).unwrap();
        let phrase = wallet.recovery_phrase().to_owned(); let mut restored = IdentityService::new();
        let result = restored.restore(user, "Alice", &phrase, b"credential", 600, "devnet", 1).unwrap();
        assert_eq!(result.wallet_address, wallet.wallet_address);
    }
}
