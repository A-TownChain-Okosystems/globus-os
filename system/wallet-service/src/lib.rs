//! Capability-gated wallet service boundary for GlobusOS.
//!
//! The service never exposes private keys. Signing is delegated to the configured
//! keystore and requires an authenticated identity plus an explicit signing capability.

use globus_identity::{
    IdentitySession, KeyId, KeyStore, KeystoreError, LoginState, SignRequest, Signature,
};
use globus_ipc::{WalletSignMessage, WalletSignResponse};


/// Trusted host-side policy for wallet signing authorization.
///
/// The implementation must be supplied by the trusted service composition root. IPC
/// callers never provide grants or authorization policy as part of a signing request.
pub trait WalletAuthorizer {
    fn authorize_signing(&self, user_id: &str, key_id: &str, now_unix: u64) -> bool;
}

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

pub struct WalletService<K, A> {
    keystore: K,
    authorizer: A,
}

impl<K: KeyStore, A: WalletAuthorizer> WalletService<K, A> {
    /// Construct the service with a trusted host-side authorization policy.
    pub fn new(keystore: K, authorizer: A) -> Self {
        Self { keystore, authorizer }
    }

    /// Handle the typed IPC signing boundary. Recovery phrases and private keys are not
    /// representable by this IPC request type.
    pub fn sign_ipc(
        &self,
        session: &IdentitySession,
        now_unix: u64,
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
        let (signature, evidence) = self.sign(session, now_unix, sign_request)?;
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
        if !self.authorizer.authorize_signing(
            session.user_id.as_str(),
            request.key_id.as_str(),
            now_unix,
        ) {
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

    struct TestAuthorizer(bool);
    impl WalletAuthorizer for TestAuthorizer {
        fn authorize_signing(&self, user_id: &str, key_id: &str, _now_unix: u64) -> bool {
            self.0 && user_id == "test-user" && key_id == "wallet-primary"
        }
    }

    #[test]
    fn signing_requires_trusted_authorizer_approval() {
        let session = session();
        let request = SignRequest {
            key_id: KeyId::new("wallet-primary").unwrap(),
            domain: "ATC-TX-V1".into(),
            message: b"payload".to_vec(),
        };
        let denied_service = WalletService::new(TestStore, TestAuthorizer(false));
        assert_eq!(
            denied_service.sign(&session, 10, request.clone()).unwrap_err(),
            WalletServiceError::Unauthorized
        );

        let allowed_service = WalletService::new(TestStore, TestAuthorizer(true));
        let (signature, evidence) = allowed_service.sign(&session, 10, request).unwrap();
        assert_eq!(signature.bytes, b"payload");
        assert!(evidence.authorized);
    }

    #[test]
    fn authorizer_is_bound_to_user_and_key() {
        let service = WalletService::new(TestStore, TestAuthorizer(true));
        let session = session();
        let wrong_key = SignRequest {
            key_id: KeyId::new("other-key").unwrap(),
            domain: "ATC-TX-V1".into(),
            message: b"payload".to_vec(),
        };
        assert_eq!(
            service.sign(&session, 10, wrong_key).unwrap_err(),
            WalletServiceError::Unauthorized
        );
    }

    #[test]
    fn ipc_rejects_cross_identity_requests() {
        let service = WalletService::new(TestStore, TestAuthorizer(true));
        let request = WalletSignMessage {
            user_id: "other-user".into(),
            key_id: "wallet-primary".into(),
            domain: "ATC-TX-V1".into(),
            message: b"payload".to_vec(),
        };
        assert_eq!(
            service.sign_ipc(&session(), 10, request).unwrap_err(),
            WalletServiceError::IdentityMismatch
        );
    }
}
