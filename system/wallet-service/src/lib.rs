//! Capability-gated wallet service boundary for GlobusOS.
//!
//! The service never exposes private keys. Signing is delegated to the configured
//! keystore and requires an authenticated identity plus a kernel/security-registry
//! capability. IPC callers supply only a capability identifier; the service resolves
//! authority against its trusted registry and never accepts a caller-built Grant.

use globus_identity::{
    IdentitySession, KeyId, KeyStore, KeystoreError, LoginState, SignRequest, Signature,
};
use globus_ipc::{WalletSignMessage, WalletSignResponse};
use globus_security::{Authorization, Capability, CapabilityRegistry, Right, authorize};

/// Registry object ID representing the wallet-signing operation.
pub const WALLET_SIGN_RESOURCE_ID: u64 = 0x5741_4c4c_4554_5347;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditEvidence {
    pub operation: &'static str,
    pub user_id: String,
    pub key_id: String,
    pub domain: String,
    pub message_digest: [u8; 32],
    pub authorized: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WalletServiceError {
    Unauthenticated,
    Unauthorized,
    IdentityMismatch,
    InvalidRequest,
    Keystore(KeystoreError),
}

impl From<KeystoreError> for WalletServiceError {
    fn from(value: KeystoreError) -> Self {
        Self::Keystore(value)
    }
}

pub struct WalletService<K> {
    keystore: K,
    capabilities: CapabilityRegistry,
}

impl<K: KeyStore> WalletService<K> {
    /// Construct the service with a trusted capability registry. The registry must
    /// be supplied by the privileged security/kernel integration, not from an IPC
    /// request or other untrusted caller-controlled payload.
    pub fn new(keystore: K, capabilities: CapabilityRegistry) -> Self {
        Self { keystore, capabilities }
    }

    /// Handle the typed IPC signing boundary. Recovery phrases and private keys are not
    /// representable by this IPC request type. Only a capability ID crosses this boundary.
    pub fn sign_ipc(
        &self,
        session: &IdentitySession,
        now_unix: u64,
        capability: Capability,
        request: WalletSignMessage,
    ) -> Result<(WalletSignResponse, AuditEvidence), WalletServiceError> {
        if request.user_id != session.user_id.as_str() {
            return Err(WalletServiceError::IdentityMismatch);
        }
        let key_id = KeyId::new(request.key_id).map_err(|_| WalletServiceError::InvalidRequest)?;
        let sign_request = SignRequest {
            key_id,
            domain: request.domain,
            message: request.message,
        };
        let (signature, evidence) = self.sign(session, now_unix, capability, sign_request)?;
        Ok((
            WalletSignResponse {
                algorithm: signature.algorithm,
                signature: signature.bytes,
                message_digest: evidence.message_digest,
            },
            evidence,
        ))
    }

    /// Sign a domain-separated message only when the trusted registry authorizes the
    /// supplied capability ID for the wallet-signing resource.
    pub fn sign(
        &self,
        session: &IdentitySession,
        now_unix: u64,
        capability: Capability,
        request: SignRequest,
    ) -> Result<(Signature, AuditEvidence), WalletServiceError> {
        if !session.is_active(now_unix) || session.state != LoginState::Authenticated {
            return Err(WalletServiceError::Unauthenticated);
        }
        if request.domain.is_empty()
            || request.domain.len() > 128
            || request.message.len() > 1024 * 1024
        {
            return Err(WalletServiceError::InvalidRequest);
        }
        if authorize(
            &self.capabilities,
            capability,
            WALLET_SIGN_RESOURCE_ID,
            Right::Sign,
        ) != Authorization::Allowed
        {
            return Err(WalletServiceError::Unauthorized);
        }

        let digest = sha256(&request.message);
        let evidence = AuditEvidence {
            operation: "wallet.sign",
            user_id: session.user_id.as_str().to_owned(),
            key_id: request.key_id.as_str().to_owned(),
            domain: request.domain.clone(),
            message_digest: digest,
            authorized: true,
        };
        let signature = self.keystore.sign(request)?;
        Ok((signature, evidence))
    }
}

fn sha256(message: &[u8]) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    Sha256::digest(message).into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use globus_identity::{KeyMetadata, ProtectedKey};
    use zeroize::Zeroizing;

    struct TestStore;
    impl KeyStore for TestStore {
        fn provision(
            &mut self,
            key_id: KeyId,
            _secret: Zeroizing<Vec<u8>>,
            algorithm: String,
        ) -> Result<ProtectedKey, KeystoreError> {
            Ok(ProtectedKey {
                metadata: KeyMetadata {
                    key_id,
                    algorithm,
                    version: 1,
                    hardware_backed: false,
                },
            })
        }
        fn sign(&self, request: SignRequest) -> Result<Signature, KeystoreError> {
            Ok(Signature {
                algorithm: "test".into(),
                bytes: request.message,
            })
        }
        fn delete(&mut self, _key_id: &KeyId) -> Result<(), KeystoreError> {
            Ok(())
        }
    }

    fn session() -> IdentitySession {
        let user = globus_identity::UserId::new("test-user").unwrap();
        let wallet = globus_identity::create_wallet(user.clone(), 600, "devnet", 1).unwrap();
        IdentitySession {
            user_id: user,
            wallet_address: wallet.wallet_address,
            state: LoginState::Authenticated,
            issued_at_unix: 1,
            expires_at_unix: 100,
        }
    }

    fn registry_with_sign_capability() -> CapabilityRegistry {
        let mut registry = CapabilityRegistry::new();
        assert!(registry.grant(
            Capability(2),
            WALLET_SIGN_RESOURCE_ID,
            Right::Sign,
        ));
        registry
    }

    fn request() -> SignRequest {
        SignRequest {
            key_id: KeyId::new("wallet-primary").unwrap(),
            domain: "ATC-TX-V1".into(),
            message: b"payload".to_vec(),
        }
    }

    #[test]
    fn signing_requires_registry_backed_capability() {
        let service = WalletService::new(TestStore, registry_with_sign_capability());
        let (signature, evidence) = service.sign(&session(), 10, Capability(2), request()).unwrap();
        assert_eq!(signature.bytes, b"payload");
        assert!(evidence.authorized);
    }

    #[test]
    fn forged_or_unknown_capability_is_rejected() {
        let service = WalletService::new(TestStore, CapabilityRegistry::new());
        assert_eq!(
            service.sign(&session(), 10, Capability(2), request()).unwrap_err(),
            WalletServiceError::Unauthorized
        );
    }

    #[test]
    fn wrong_right_and_revoked_capabilities_are_rejected() {
        let mut registry = CapabilityRegistry::new();
        assert!(registry.grant(Capability(3), WALLET_SIGN_RESOURCE_ID, Right::Execute));
        assert!(registry.grant(Capability(4), WALLET_SIGN_RESOURCE_ID, Right::Sign));
        assert!(registry.revoke(Capability(4)));
        let service = WalletService::new(TestStore, registry);
        assert_eq!(
            service.sign(&session(), 10, Capability(3), request()).unwrap_err(),
            WalletServiceError::Unauthorized
        );
        assert_eq!(
            service.sign(&session(), 10, Capability(4), request()).unwrap_err(),
            WalletServiceError::Unauthorized
        );
    }

    #[test]
    fn ipc_rejects_cross_identity_requests() {
        let service = WalletService::new(TestStore, registry_with_sign_capability());
        let request = WalletSignMessage {
            user_id: "other-user".into(),
            key_id: "wallet-primary".into(),
            domain: "ATC-TX-V1".into(),
            message: b"payload".to_vec(),
        };
        assert_eq!(
            service.sign_ipc(&session(), 10, Capability(2), request).unwrap_err(),
            WalletServiceError::IdentityMismatch
        );
    }
}
