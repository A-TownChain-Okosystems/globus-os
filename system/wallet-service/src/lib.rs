//! Capability-gated wallet service boundary for GlobusOS.
//!
//! The service never exposes private keys. Signing is delegated to the configured
//! keystore and requires an authenticated identity plus an explicit signing capability.

use globus_identity::{
    IdentitySession, KeyId, KeyStore, KeystoreError, LoginState, SignRequest, Signature,
};
use globus_ipc::{WalletSignMessage, WalletSignResponse};
use globus_security::{Authorization, Grant, Right, authorize};

#[derive(Debug, Clone, PartialEq, Eq)]
/// Evidence recorded for every signing attempt, whether it succeeded or not.
pub struct AuditEvidence {
    /// Operation name, here the IPC method that was called.
    pub operation: &'static str,
    /// User the request claimed to act for.
    pub user_id: String,
    /// Key the request addressed.
    pub key_id: String,
    /// Domain separation label the signature was bound to.
    pub domain: String,
    /// SHA-256 digest of the signed message.
    pub message_digest: [u8; 32],
    /// Whether the capability check passed.
    pub authorized: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Failure modes of the wallet boundary.
pub enum WalletServiceError {
    /// No valid session was supplied.
    Unauthenticated,
    /// The session lacks the signing capability.
    Unauthorized,
    /// The requested user is not the session user.
    IdentityMismatch,
    /// The request is malformed, e.g. carries an unusable key id.
    InvalidRequest,
    /// The keystore rejected the operation.
    Keystore(KeystoreError),
}

impl From<KeystoreError> for WalletServiceError {
    fn from(value: KeystoreError) -> Self {
        Self::Keystore(value)
    }
}

/// Wallet boundary that never sees private keys; signing is delegated to `K`.
pub struct WalletService<K> {
    keystore: K,
}

impl<K: KeyStore> WalletService<K> {
    /// Creates the service over the given keystore.
    pub fn new(keystore: K) -> Self {
        Self { keystore }
    }

    /// Handle the typed IPC signing boundary. Recovery phrases and private keys are not
    /// representable by this IPC request type.
    pub fn sign_ipc(
        &self,
        session: &IdentitySession,
        now_unix: u64,
        grant: Option<Grant>,
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
        let (signature, evidence) = self.sign(session, now_unix, grant, sign_request)?;
        Ok((
            WalletSignResponse {
                algorithm: signature.algorithm,
                signature: signature.bytes,
                message_digest: evidence.message_digest,
            },
            evidence,
        ))
    }

    /// Sign a domain-separated message without exposing private-key material.
    pub fn sign(
        &self,
        session: &IdentitySession,
        now_unix: u64,
        grant: Option<Grant>,
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
        if authorize(grant, Right::Sign) != Authorization::Allowed {
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

    #[test]
    fn signing_requires_dedicated_capability() {
        let session = session();
        let key_id = KeyId::new("wallet-primary").unwrap();
        let request = SignRequest {
            key_id,
            domain: "ATC-TX-V1".into(),
            message: b"payload".to_vec(),
        };
        let service = WalletService::new(TestStore);
        assert_eq!(
            service
                .sign(&session, 10, None, request.clone())
                .unwrap_err(),
            WalletServiceError::Unauthorized
        );
        let execute_grant = Grant {
            capability: globus_security::Capability(1),
            right: Right::Execute,
        };
        assert_eq!(
            service
                .sign(&session, 10, Some(execute_grant), request.clone())
                .unwrap_err(),
            WalletServiceError::Unauthorized
        );
        let sign_grant = Grant {
            capability: globus_security::Capability(2),
            right: Right::Sign,
        };
        let (signature, evidence) = service
            .sign(&session, 10, Some(sign_grant), request)
            .unwrap();
        assert_eq!(signature.bytes, b"payload");
        assert!(evidence.authorized);
    }

    #[test]
    fn ipc_rejects_cross_identity_requests() {
        let service = WalletService::new(TestStore);
        let request = WalletSignMessage {
            user_id: "other-user".into(),
            key_id: "wallet-primary".into(),
            domain: "ATC-TX-V1".into(),
            message: b"payload".to_vec(),
        };
        let grant = Grant {
            capability: globus_security::Capability(1),
            right: Right::Sign,
        };
        assert_eq!(
            service
                .sign_ipc(&session(), 10, Some(grant), request)
                .unwrap_err(),
            WalletServiceError::IdentityMismatch
        );
    }
}
