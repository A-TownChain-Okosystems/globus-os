//! GlobusOS identity and wallet boundary.
//!
//! This crate deliberately does not expose private keys or recovery material through
//! `Debug`, `Display`, serialization, or ordinary identity metadata. Production wallet
//! generation must use an OS/hardware CSPRNG and a reviewed mnemonic implementation.

use sha2::{Digest, Sha256};
use zeroize::{Zeroize, Zeroizing};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserId(String);

impl UserId {
    pub fn new(value: impl Into<String>) -> Result<Self, IdentityError> {
        let value = value.into();
        if value.is_empty() || value.len() > 128 {
            return Err(IdentityError::InvalidUserId);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str { &self.0 }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalletAddress(String);

impl WalletAddress {
    pub fn as_str(&self) -> &str { &self.0 }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentityBinding {
    pub user_id: UserId,
    pub wallet_address: WalletAddress,
    pub chain_id: u64,
    pub network: String,
    pub key_version: u16,
}

/// Sensitive wallet material. It is zeroized when dropped and cannot be printed.
pub struct RecoveryMaterial {
    seed: Zeroizing<Vec<u8>>,
}

impl RecoveryMaterial {
    pub fn from_seed(seed: Vec<u8>) -> Result<Self, IdentityError> {
        if seed.len() < 32 {
            return Err(IdentityError::WeakSeed);
        }
        Ok(Self { seed: Zeroizing::new(seed) })
    }

    pub fn seed_len(&self) -> usize { self.seed.len() }
}

impl Drop for RecoveryMaterial {
    fn drop(&mut self) {
        self.seed.zeroize();
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdentityError {
    InvalidUserId,
    WeakSeed,
    InvalidWalletAddress,
    EmptyNetwork,
}

/// Deterministically derives a public wallet-address placeholder from public identity
/// inputs. This is NOT a production ATC wallet derivation algorithm.
///
/// The explicit placeholder prevents accidentally shipping an invented chain crypto
/// scheme. Replace it only after the canonical A-TownChain wallet/key standard is bound.
pub fn derive_address_placeholder(user_id: &UserId, chain_id: u64, network: &str) -> Result<WalletAddress, IdentityError> {
    if network.is_empty() {
        return Err(IdentityError::EmptyNetwork);
    }
    let mut hasher = Sha256::new();
    hasher.update(b"GLOBUSOS-WALLET-ADDRESS-V1");
    hasher.update(chain_id.to_le_bytes());
    hasher.update(network.as_bytes());
    hasher.update(user_id.as_str().as_bytes());
    let digest = hasher.finalize();
    Ok(WalletAddress(format!("ATC1{:x}", digest)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_rejects_empty_user_id() {
        assert_eq!(UserId::new("").unwrap_err(), IdentityError::InvalidUserId);
    }

    #[test]
    fn address_is_stable_for_same_public_inputs() {
        let user = UserId::new("test-user").unwrap();
        let a = derive_address_placeholder(&user, 600, "devnet").unwrap();
        let b = derive_address_placeholder(&user, 600, "devnet").unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn weak_recovery_material_is_rejected() {
        assert_eq!(RecoveryMaterial::from_seed(vec![0; 16]).unwrap_err(), IdentityError::WeakSeed);
    }
}
