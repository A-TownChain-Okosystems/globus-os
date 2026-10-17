//! Package metadata and cryptographic verification policy.
//!
//! Package trust requires an artifact digest match and an Ed25519 signature from
//! an explicitly configured trusted public key. Non-empty metadata is not trust.

use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Package {
    pub name: String,
    pub version: String,
    /// SHA-256 digest encoded as `sha256:` followed by 64 lowercase hex digits.
    pub digest: String,
    /// Ed25519 signature encoded as 128 hexadecimal digits.
    pub signature: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verification {
    Trusted,
    Untrusted,
}

/// Verify artifact bytes and the package's signed metadata against trusted keys.
///
/// The signed message is domain-separated and binds the package name, version,
/// and declared digest. Public keys must come from a trusted configuration
/// source; this function intentionally does not discover or trust keys itself.
pub fn verify_package(pkg: &Package, artifact: &[u8], trusted_keys: &[[u8; 32]]) -> Verification {
    if pkg.name.trim().is_empty() || pkg.version.trim().is_empty() || trusted_keys.is_empty() {
        return Verification::Untrusted;
    }

    let Some(declared_digest) = pkg.digest.strip_prefix("sha256:") else {
        return Verification::Untrusted;
    };
    if declared_digest.len() != 64
        || !declared_digest
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Verification::Untrusted;
    }

    let actual_digest = Sha256::digest(artifact);
    let Ok(declared_digest_bytes) = hex::decode(declared_digest) else {
        return Verification::Untrusted;
    };
    if actual_digest.as_slice() != declared_digest_bytes.as_slice() {
        return Verification::Untrusted;
    }

    let Ok(signature_bytes) = hex::decode(&pkg.signature) else {
        return Verification::Untrusted;
    };
    let Ok(signature) = Signature::from_slice(&signature_bytes) else {
        return Verification::Untrusted;
    };
    let message = signed_metadata(pkg);

    for key_bytes in trusted_keys {
        let Ok(key) = VerifyingKey::from_bytes(key_bytes) else {
            continue;
        };
        if key.verify(&message, &signature).is_ok() {
            return Verification::Trusted;
        }
    }

    Verification::Untrusted
}

fn signed_metadata(pkg: &Package) -> Vec<u8> {
    let mut message = b"GLOBUSOS-PACKAGE-SIGNATURE-V1\0".to_vec();
    message.extend_from_slice(pkg.name.as_bytes());
    message.push(0);
    message.extend_from_slice(pkg.version.as_bytes());
    message.push(0);
    message.extend_from_slice(pkg.digest.as_bytes());
    message
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    fn signed_package(artifact: &[u8], signing_key: &SigningKey) -> Package {
        let digest = format!("sha256:{}", hex::encode(Sha256::digest(artifact)));
        let mut pkg = Package {
            name: "example".into(),
            version: "1.2.3".into(),
            digest,
            signature: String::new(),
        };
        pkg.signature = hex::encode(signing_key.sign(&signed_metadata(&pkg)).to_bytes());
        pkg
    }

    #[test]
    fn trusts_matching_artifact_signed_by_configured_key() {
        let key = SigningKey::from_bytes(&[7_u8; 32]);
        let artifact = b"trusted package contents";
        let pkg = signed_package(artifact, &key);
        assert_eq!(
            verify_package(&pkg, artifact, &[key.verifying_key().to_bytes()]),
            Verification::Trusted
        );
    }

    #[test]
    fn rejects_modified_artifact() {
        let key = SigningKey::from_bytes(&[7_u8; 32]);
        let pkg = signed_package(b"original", &key);
        assert_eq!(
            verify_package(&pkg, b"modified", &[key.verifying_key().to_bytes()]),
            Verification::Untrusted
        );
    }

    #[test]
    fn rejects_signature_from_untrusted_key() {
        let signer = SigningKey::from_bytes(&[7_u8; 32]);
        let other = SigningKey::from_bytes(&[8_u8; 32]);
        let artifact = b"contents";
        let pkg = signed_package(artifact, &signer);
        assert_eq!(
            verify_package(&pkg, artifact, &[other.verifying_key().to_bytes()]),
            Verification::Untrusted
        );
    }

    #[test]
    fn rejects_malformed_or_missing_metadata() {
        let key = SigningKey::from_bytes(&[7_u8; 32]);
        let artifact = b"contents";
        let mut pkg = signed_package(artifact, &key);
        pkg.digest = "not-a-digest".into();
        assert_eq!(
            verify_package(&pkg, artifact, &[key.verifying_key().to_bytes()]),
            Verification::Untrusted
        );

        let pkg = signed_package(artifact, &key);
        assert_eq!(verify_package(&pkg, artifact, &[]), Verification::Untrusted);
    }

    #[test]
    fn rejects_metadata_tampering() {
        let key = SigningKey::from_bytes(&[7_u8; 32]);
        let artifact = b"contents";
        let mut pkg = signed_package(artifact, &key);
        pkg.version = "9.9.9".into();
        assert_eq!(
            verify_package(&pkg, artifact, &[key.verifying_key().to_bytes()]),
            Verification::Untrusted
        );
    }
}
