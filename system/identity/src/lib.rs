//! GlobusOS identity and wallet boundary.
//!
//! The wallet flow follows the ecosystem's documented ATC wallet format:
//! 256-bit entropy -> 24-word BIP39 mnemonic -> PBKDF2-HMAC-SHA512 seed ->
//! HMAC-SHA256 private key -> SHA-256 public key -> ATC address.
//!
//! Recovery material is deliberately kept in zeroizing containers and is never
//! included in ordinary identity metadata or formatting implementations.

use bip39::{Language, Mnemonic};
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use zeroize::{Zeroize, Zeroizing};

type HmacSha256 = Hmac<Sha256>;

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

/// Sensitive recovery material. It is zeroized when dropped and cannot be printed.
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

/// A 24-word recovery phrase held in zeroizing memory.
pub struct RecoveryPhrase(Zeroizing<String>);

impl RecoveryPhrase {
    /// Explicitly expose the phrase to a trusted UI boundary for one-time display.
    /// Callers must not log, persist, transmit, or send the returned value to AI services.
    pub fn expose(&self) -> &str { &self.0 }

    pub fn word_count(&self) -> usize { self.0.split_whitespace().count() }
}

impl Drop for RecoveryPhrase {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

/// Result of local wallet creation. The private key is intentionally not returned.
pub struct WalletCreation {
    pub recovery_phrase: RecoveryPhrase,
    pub wallet_address: WalletAddress,
    pub binding: IdentityBinding,
}

impl WalletCreation {
    /// Explicitly consume the result and return the recovery phrase to a trusted UI.
    pub fn recovery_phrase(&self) -> &str { self.recovery_phrase.expose() }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdentityError {
    InvalidUserId,
    WeakSeed,
    InvalidWalletAddress,
    EmptyNetwork,
    InvalidMnemonic,
    WalletDerivationFailed,
}

/// Create a new ATC wallet locally using 256 bits of cryptographic entropy.
///
/// The mnemonic is generated locally. No network access or server-side identity
/// operation is involved. The caller is responsible for displaying the phrase once
/// and then destroying the temporary `WalletCreation` value.
pub fn create_wallet(
    user_id: UserId,
    chain_id: u64,
    network: impl Into<String>,
    key_version: u16,
) -> Result<WalletCreation, IdentityError> {
    let network = network.into();
    if network.is_empty() {
        return Err(IdentityError::EmptyNetwork);
    }

    let mnemonic = Mnemonic::generate_in(Language::English, 24)
        .map_err(|_| IdentityError::WalletDerivationFailed)?;
    let phrase = mnemonic.to_string();
    let seed = mnemonic.to_seed_normalized("");
    let private_key = derive_private_key(&seed)?;
    let public_key = derive_public_key(&private_key);
    let wallet_address = public_key_to_address(&public_key)?;

    let binding = IdentityBinding {
        user_id,
        wallet_address: wallet_address.clone(),
        chain_id,
        network,
        key_version,
    };

    Ok(WalletCreation {
        recovery_phrase: RecoveryPhrase(Zeroizing::new(phrase)),
        wallet_address,
        binding,
    })
}

/// Restore an ATC wallet from a previously generated 24-word phrase.
pub fn restore_wallet(
    user_id: UserId,
    phrase: &str,
    chain_id: u64,
    network: impl Into<String>,
    key_version: u16,
) -> Result<IdentityBinding, IdentityError> {
    let network = network.into();
    if network.is_empty() {
        return Err(IdentityError::EmptyNetwork);
    }
    let mnemonic = Mnemonic::parse_in_normalized(Language::English, phrase)
        .map_err(|_| IdentityError::InvalidMnemonic)?;
    if mnemonic.word_count() != 24 {
        return Err(IdentityError::InvalidMnemonic);
    }

    let seed = mnemonic.to_seed_normalized("");
    let private_key = derive_private_key(&seed)?;
    let public_key = derive_public_key(&private_key);
    let wallet_address = public_key_to_address(&public_key)?;

    Ok(IdentityBinding {
        user_id,
        wallet_address,
        chain_id,
        network,
        key_version,
    })
}

fn derive_private_key(seed: &[u8]) -> Result<[u8; 32], IdentityError> {
    let mut mac = HmacSha256::new_from_slice(b"A-TownChain")
        .map_err(|_| IdentityError::WalletDerivationFailed)?;
    mac.update(seed);
    let output = mac.finalize().into_bytes();
    let mut key = [0u8; 32];
    key.copy_from_slice(&output);
    Ok(key)
}

fn derive_public_key(private_key: &[u8; 32]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(private_key);
    hasher.finalize().into()
}

fn public_key_to_address(public_key: &[u8; 32]) -> Result<WalletAddress, IdentityError> {
    let step1 = Sha256::digest(public_key);
    let step2 = Sha256::digest(step1);
    let checksum = Sha256::digest(step2);
    let address = format!(
        "ATC{}{}",
        hex_upper(&step2[..14]),
        hex_upper(&checksum[..2]),
    );
    if address.len() != 35 {
        return Err(IdentityError::InvalidWalletAddress);
    }
    Ok(WalletAddress(address))
}

fn hex_upper(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut output = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_rejects_empty_user_id() {
        assert_eq!(UserId::new("").unwrap_err(), IdentityError::InvalidUserId);
    }

    #[test]
    fn generated_wallet_has_24_words_and_atc_address() {
        let user = UserId::new("test-user").unwrap();
        let wallet = create_wallet(user, 600, "devnet", 1).unwrap();
        assert_eq!(wallet.recovery_phrase.word_count(), 24);
        assert_eq!(wallet.wallet_address.as_str().len(), 35);
        assert!(wallet.wallet_address.as_str().starts_with("ATC"));
    }

    #[test]
    fn wallet_restore_reproduces_address() {
        let user = UserId::new("test-user").unwrap();
        let wallet = create_wallet(user.clone(), 600, "devnet", 1).unwrap();
        let restored = restore_wallet(user, wallet.recovery_phrase(), 600, "devnet", 1).unwrap();
        assert_eq!(restored.wallet_address, wallet.wallet_address);
    }

    #[test]
    fn weak_recovery_material_is_rejected() {
        assert_eq!(RecoveryMaterial::from_seed(vec![0; 16]).unwrap_err(), IdentityError::WeakSeed);
    }

    #[test]
    fn address_has_expected_format() {
        let user = UserId::new("format-test").unwrap();
        let wallet = create_wallet(user, 600, "devnet", 1).unwrap();
        assert!(wallet.wallet_address.as_str().chars().skip(3).all(|c| c.is_ascii_hexdigit() && c.is_ascii_uppercase() || c.is_ascii_digit()));
    }
}
