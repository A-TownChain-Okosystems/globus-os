//! Single encrypted, atomic account-state repository for GlobusOS.
//!
//! This repository is the durable authority for username-to-UserId mappings and their
//! corresponding identity records. The account directory is a projection of this state;
//! it is not persisted as a second file. The current record schema preserves the existing
//! PHC credential representation and provides a strict, ephemeral PHC-to-ATID/GCR1 export.
//! Unsupported Argon2id parameters or encodings fail closed; CI equivalence remains required.

use argon2::password_hash::PasswordHash;

use crate::{
    AccountDirectoryEntry, AccountDirectoryError, AccountDirectoryStore, AccountProfile,
    AccountUsername, BlobStore, CredentialRecord, EncryptedBlobStore, IdentityBinding,
    IdentityKeyProvider, IdentityRecord, PersistenceError, SecureStoreError, UserId, WalletAddress,
};

use core::convert::TryInto;
use std::vec::Vec;

const MAGIC: &[u8; 4] = b"GAS1";
const VERSION: u16 = 1;
const MAX_ACCOUNTS: usize = 4096;
const MAX_USERNAME: usize = 32;
const MAX_USER_ID: usize = 128;
const MAX_DISPLAY_NAME: usize = 128;
const MAX_WALLET_ADDRESS: usize = 128;
const MAX_NETWORK: usize = 64;
const MAX_CREDENTIAL: usize = 1024;
const HEADER_LEN: usize = 10;

/// One account entry in the canonical encrypted repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountStateEntry {
    /// Canonical username.
    pub username: AccountUsername,
    /// Complete validated identity profile and existing credential verifier.
    pub identity: IdentityRecord,
}

/// Validated complete account state. It is a persistence object, not a second directory.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AccountState {
    accounts: Vec<AccountStateEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Errors returned while validating, decoding, or persisting the canonical account state.
pub enum AccountStateError {
    /// The account set violates the repository's structural or size constraints.
    InvalidState,
    /// The stored GAS1 schema version is not supported.
    UnsupportedVersion,
    /// The encoded state ends before a required field is complete.
    Truncated,
    /// The encoded state contains bytes after the final account record.
    TrailingData,
    /// Two entries share a username or stable UserId.
    DuplicateAccount,
    /// A field is empty, malformed, or outside its permitted bounds.
    InvalidField,
    /// The encrypted storage boundary failed or rejected the operation.
    Storage(SecureStoreError),
    /// The underlying blob persistence operation failed.
    Persistence(PersistenceError),
    /// The durable generation changed since the caller's state was loaded.
    GenerationConflict {
        /// Generation the caller originally loaded.
        expected: u64,
        /// Generation currently stored by the repository.
        actual: u64,
    },
    /// The durable generation cannot be incremented safely.
    GenerationOverflow,
    /// A PHC credential cannot be represented by the canonical ATID kernel record.
    CredentialFormatRejected,
}

/// Convert a PHC Argon2id verifier to the existing fixed-width ATID record.
/// The salt and output hash are copied unchanged; no password or re-hashing is involved.
fn phc_to_atid(phc: &str) -> Result<[u8; globus_identity_core::RECORD_LEN], AccountStateError> {
    let parsed = PasswordHash::new(phc).map_err(|_| AccountStateError::CredentialFormatRejected)?;
    if parsed.algorithm.as_str() != "argon2id"
        || parsed.version.map(|v| v.to_string()).as_deref() != Some("19")
    {
        return Err(AccountStateError::CredentialFormatRejected);
    }
    let read_param = |name: &str| {
        parsed
            .params
            .get_str(name)
            .and_then(|v| v.parse::<u32>().ok())
            .ok_or(AccountStateError::CredentialFormatRejected)
    };
    let memory_kib = read_param("m")?;
    let iterations = read_param("t")?;
    let parallelism = read_param("p")?;
    let params = globus_identity_core::KdfParams {
        memory_kib,
        iterations,
        parallelism,
    };
    params
        .validate()
        .map_err(|_| AccountStateError::CredentialFormatRejected)?;
    let salt = parsed
        .salt
        .as_ref()
        .ok_or(AccountStateError::CredentialFormatRejected)?;
    let mut salt_bytes = [0u8; globus_identity_core::SALT_LEN];
    let salt_len = salt
        .decode_b64(&mut salt_bytes)
        .map_err(|_| AccountStateError::CredentialFormatRejected)?
        .len();
    if salt_len != globus_identity_core::SALT_LEN
        || salt_bytes == [0; globus_identity_core::SALT_LEN]
    {
        return Err(AccountStateError::CredentialFormatRejected);
    }
    let hash = parsed
        .hash
        .as_ref()
        .ok_or(AccountStateError::CredentialFormatRejected)?;
    if hash.as_ref().len() != globus_identity_core::HASH_LEN
        || hash.as_ref().iter().all(|b| *b == 0)
        || memory_kib > u16::MAX as u32
    {
        return Err(AccountStateError::CredentialFormatRejected);
    }
    let mut record = [0u8; globus_identity_core::RECORD_LEN];
    record[0..4].copy_from_slice(&globus_identity_core::MAGIC);
    record[4] = globus_identity_core::FORMAT_VERSION;
    record[5] = globus_identity_core::KDF_ARGON2ID;
    record[6..8].copy_from_slice(&(memory_kib as u16).to_le_bytes());
    record[8..12].copy_from_slice(&iterations.to_le_bytes());
    record[12] = parallelism as u8;
    record[16..32].copy_from_slice(&salt_bytes);
    record[32..64].copy_from_slice(hash.as_ref());
    globus_identity_core::CredentialRecord::parse(&record)
        .map_err(|_| AccountStateError::CredentialFormatRejected)?;
    Ok(record)
}

impl From<SecureStoreError> for AccountStateError {
    fn from(value: SecureStoreError) -> Self {
        Self::Storage(value)
    }
}
impl From<PersistenceError> for AccountStateError {
    fn from(value: PersistenceError) -> Self {
        Self::Persistence(value)
    }
}

impl AccountState {
    /// Validate and canonicalize a complete account set.
    pub fn new(mut accounts: Vec<AccountStateEntry>) -> Result<Self, AccountStateError> {
        if accounts.len() > MAX_ACCOUNTS {
            return Err(AccountStateError::InvalidState);
        }
        accounts.sort_by(|left, right| left.username.as_str().cmp(right.username.as_str()));
        for (index, account) in accounts.iter().enumerate() {
            account.identity.validate()?;
            if account.identity.profile.user_id.as_str().len() > MAX_USER_ID
                || account.identity.profile.display_name.is_empty()
                || account.identity.profile.display_name.len() > MAX_DISPLAY_NAME
                || account.identity.profile.wallet_address.as_str().is_empty()
                || account.identity.profile.wallet_address.as_str().len() > MAX_WALLET_ADDRESS
                || account.identity.profile.binding.network.is_empty()
                || account.identity.profile.binding.network.len() > MAX_NETWORK
                || account.identity.credential.phc_hash.is_empty()
                || account.identity.credential.phc_hash.len() > MAX_CREDENTIAL
            {
                return Err(AccountStateError::InvalidField);
            }
            if account.username.as_str().len() > MAX_USERNAME
                || account
                    .identity
                    .profile
                    .user_id
                    .as_str()
                    .as_bytes()
                    .contains(&0)
                || accounts[..index].iter().any(|previous| {
                    previous.username == account.username
                        || previous.identity.profile.user_id == account.identity.profile.user_id
                })
            {
                return Err(AccountStateError::DuplicateAccount);
            }
        }
        Ok(Self { accounts })
    }

    /// Return all validated accounts in canonical username order.
    pub fn accounts(&self) -> &[AccountStateEntry] {
        &self.accounts
    }
    /// Return the number of accounts in this state.
    pub fn len(&self) -> usize {
        self.accounts.len()
    }
    /// Return whether the state contains no accounts.
    pub fn is_empty(&self) -> bool {
        self.accounts.is_empty()
    }

    /// Find an account by its canonical username.
    pub fn by_username(&self, username: &AccountUsername) -> Option<&AccountStateEntry> {
        self.accounts
            .binary_search_by(|entry| entry.username.as_str().cmp(username.as_str()))
            .ok()
            .map(|index| &self.accounts[index])
    }

    /// Find an account by its stable identity-layer UserId.
    pub fn by_user_id(&self, user_id: &UserId) -> Option<&AccountStateEntry> {
        self.accounts
            .iter()
            .find(|entry| &entry.identity.profile.user_id == user_id)
    }

    /// Export only the canonical GUA1 username-to-UserId projection for the boot image.
    /// Credentials are intentionally excluded and must be exported through the separate
    /// reviewed ATID/GCR1 credential contract.
    pub fn directory_snapshot(&self) -> Result<Vec<u8>, AccountStateError> {
        let entries: Vec<globus_identity_core::catalog::DirectoryEntry> = self
            .accounts
            .iter()
            .map(|entry| globus_identity_core::catalog::DirectoryEntry {
                username: entry.username.as_str().to_owned(),
                user_id: entry.identity.profile.user_id.as_str().to_owned(),
            })
            .collect();
        globus_identity_core::catalog::encode_directory(&entries)
            .map_err(|_| AccountStateError::InvalidField)
    }

    /// Export existing PHC verifiers as canonical ATID records in a GCR1 catalog.
    /// Conversion copies salt and hash unchanged; unsupported PHC records fail closed.
    pub fn credential_snapshot(&self) -> Result<Vec<u8>, AccountStateError> {
        let mut credentials = Vec::with_capacity(self.accounts.len());
        for entry in &self.accounts {
            if entry.identity.credential.scheme != "argon2id" {
                return Err(AccountStateError::CredentialFormatRejected);
            }
            let record = phc_to_atid(&entry.identity.credential.phc_hash)?;
            credentials.push((
                entry.identity.profile.user_id.as_str().to_owned(),
                record.to_vec(),
            ));
        }
        globus_identity_core::catalog::encode_credential_catalog(&credentials)
            .map_err(|_| AccountStateError::CredentialFormatRejected)
    }

    fn encode(&self) -> Result<Vec<u8>, AccountStateError> {
        let validated = Self::new(self.accounts.clone())?;
        let mut out = Vec::new();
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&VERSION.to_be_bytes());
        out.extend_from_slice(&(validated.accounts.len() as u32).to_be_bytes());
        for entry in &validated.accounts {
            let profile = &entry.identity.profile;
            put_u8_string(&mut out, entry.username.as_str(), MAX_USERNAME)?;
            put_u16_string(&mut out, profile.user_id.as_str(), MAX_USER_ID)?;
            put_u16_string(&mut out, &profile.display_name, MAX_DISPLAY_NAME)?;
            put_u16_string(
                &mut out,
                profile.wallet_address.as_str(),
                MAX_WALLET_ADDRESS,
            )?;
            out.extend_from_slice(&profile.binding.chain_id.to_be_bytes());
            put_u16_string(&mut out, &profile.binding.network, MAX_NETWORK)?;
            out.extend_from_slice(&profile.binding.key_version.to_be_bytes());
            out.extend_from_slice(&entry.identity.version.to_be_bytes());
            out.extend_from_slice(&entry.identity.credential.version.to_be_bytes());
            put_u16_string(
                &mut out,
                &entry.identity.credential.phc_hash,
                MAX_CREDENTIAL,
            )?;
        }
        Ok(out)
    }

    fn decode(bytes: &[u8]) -> Result<Self, AccountStateError> {
        if bytes.len() < HEADER_LEN {
            return Err(AccountStateError::Truncated);
        }
        if &bytes[..4] != MAGIC {
            return Err(AccountStateError::InvalidState);
        }
        if u16::from_be_bytes(
            bytes[4..6]
                .try_into()
                .map_err(|_| AccountStateError::Truncated)?,
        ) != VERSION
        {
            return Err(AccountStateError::UnsupportedVersion);
        }
        let count = u32::from_be_bytes(
            bytes[6..10]
                .try_into()
                .map_err(|_| AccountStateError::Truncated)?,
        ) as usize;
        if count > MAX_ACCOUNTS {
            return Err(AccountStateError::InvalidState);
        }
        let mut cursor = Cursor {
            bytes,
            offset: HEADER_LEN,
        };
        let mut accounts = Vec::with_capacity(count);
        for _ in 0..count {
            let username = AccountUsername::parse(cursor.u8_string(MAX_USERNAME)?)
                .map_err(|_| AccountStateError::InvalidField)?;
            let user_id = UserId::new(cursor.u16_string(MAX_USER_ID)?)
                .map_err(|_| AccountStateError::InvalidField)?;
            let display_name = cursor.u16_string(MAX_DISPLAY_NAME)?.to_owned();
            let wallet_address = WalletAddress(cursor.u16_string(MAX_WALLET_ADDRESS)?.to_owned());
            let chain_id = cursor.u64()?;
            let network = cursor.u16_string(MAX_NETWORK)?.to_owned();
            let key_version = cursor.u16()?;
            let record_version = cursor.u16()?;
            let credential_version = cursor.u16()?;
            let phc_hash = cursor.u16_string(MAX_CREDENTIAL)?.to_owned();
            let binding = IdentityBinding {
                user_id: user_id.clone(),
                wallet_address: wallet_address.clone(),
                chain_id,
                network,
                key_version,
            };
            let profile = AccountProfile::new(user_id, display_name, binding)
                .map_err(|_| AccountStateError::InvalidField)?;
            let identity = IdentityRecord {
                version: record_version,
                profile,
                credential: CredentialRecord {
                    scheme: "argon2id",
                    version: credential_version,
                    phc_hash,
                },
            };
            identity.validate()?;
            accounts.push(AccountStateEntry { username, identity });
        }
        if cursor.offset != bytes.len() {
            return Err(AccountStateError::TrailingData);
        }
        Self::new(accounts)
    }
}

struct Cursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}
impl<'a> Cursor<'a> {
    fn take(&mut self, len: usize) -> Result<&'a [u8], AccountStateError> {
        let end = self
            .offset
            .checked_add(len)
            .ok_or(AccountStateError::InvalidField)?;
        let value = self
            .bytes
            .get(self.offset..end)
            .ok_or(AccountStateError::Truncated)?;
        self.offset = end;
        Ok(value)
    }
    fn u8(&mut self) -> Result<u8, AccountStateError> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, AccountStateError> {
        Ok(u16::from_be_bytes(
            self.take(2)?
                .try_into()
                .map_err(|_| AccountStateError::Truncated)?,
        ))
    }
    fn u64(&mut self) -> Result<u64, AccountStateError> {
        Ok(u64::from_be_bytes(
            self.take(8)?
                .try_into()
                .map_err(|_| AccountStateError::Truncated)?,
        ))
    }
    fn u8_string(&mut self, max: usize) -> Result<&'a str, AccountStateError> {
        let len = self.u8()? as usize;
        if len == 0 || len > max {
            return Err(AccountStateError::InvalidField);
        }
        core::str::from_utf8(self.take(len)?).map_err(|_| AccountStateError::InvalidField)
    }
    fn u16_string(&mut self, max: usize) -> Result<&'a str, AccountStateError> {
        let len = self.u16()? as usize;
        if len == 0 || len > max {
            return Err(AccountStateError::InvalidField);
        }
        core::str::from_utf8(self.take(len)?).map_err(|_| AccountStateError::InvalidField)
    }
}
fn put_u8_string(out: &mut Vec<u8>, value: &str, max: usize) -> Result<(), AccountStateError> {
    if value.is_empty() || value.len() > max || value.len() > u8::MAX as usize {
        return Err(AccountStateError::InvalidField);
    }
    out.push(value.len() as u8);
    out.extend_from_slice(value.as_bytes());
    Ok(())
}
fn put_u16_string(out: &mut Vec<u8>, value: &str, max: usize) -> Result<(), AccountStateError> {
    if value.is_empty() || value.len() > max || value.len() > u16::MAX as usize {
        return Err(AccountStateError::InvalidField);
    }
    out.extend_from_slice(&(value.len() as u16).to_be_bytes());
    out.extend_from_slice(value.as_bytes());
    Ok(())
}

/// Repository over the existing encrypted-blob and atomic file-blob boundaries.
pub struct AccountStateRepository<B, K> {
    encrypted: EncryptedBlobStore<B, K>,
}

impl<B, K> AccountStateRepository<B, K> {
    /// Create a repository over the existing encrypted blob store.
    pub fn new(encrypted: EncryptedBlobStore<B, K>) -> Self {
        Self { encrypted }
    }
    /// Consume the repository and return its underlying encrypted store.
    pub fn into_inner(self) -> EncryptedBlobStore<B, K> {
        self.encrypted
    }
}

impl<B: BlobStore, K: IdentityKeyProvider> AccountDirectoryStore for AccountStateRepository<B, K> {
    fn lookup(&self, username: &AccountUsername) -> Result<UserId, AccountDirectoryError> {
        self.load_state()
            .map_err(|_| AccountDirectoryError::Corrupt)?
            .ok_or(AccountDirectoryError::NotFound)?
            .lookup(username)
            .map_err(|_| AccountDirectoryError::NotFound)
    }

    fn entries(&self) -> Result<Vec<AccountDirectoryEntry>, AccountDirectoryError> {
        self.load_state()
            .map_err(|_| AccountDirectoryError::Corrupt)?
            .ok_or(AccountDirectoryError::NotFound)?
            .entries()
            .map_err(|_| AccountDirectoryError::Corrupt)
    }
}

impl<B: BlobStore, K: IdentityKeyProvider> AccountStateRepository<B, K> {
    /// Load and validate the complete account state. No partial state is returned.
    pub fn load_state(&self) -> Result<Option<AccountState>, AccountStateError> {
        let Some(plaintext) = self.encrypted.load_plaintext()? else {
            return Ok(None);
        };
        AccountState::decode(&plaintext).map(Some)
    }

    /// Export the ephemeral GUA1 directory projection from the encrypted source of truth.
    /// No plaintext sidecar is persisted by this operation.
    pub fn export_directory_snapshot(&self) -> Result<Option<Vec<u8>>, AccountStateError> {
        self.load_state()?
            .map(|state| state.directory_snapshot())
            .transpose()
    }

    /// Export the ephemeral GCR1 credential projection from encrypted canonical state.
    /// No plaintext credential sidecar is persisted by this operation.
    pub fn export_credential_snapshot(&self) -> Result<Option<Vec<u8>>, AccountStateError> {
        self.load_state()?
            .map(|state| state.credential_snapshot())
            .transpose()
    }

    /// Commit the complete next state against an expected durable generation.
    /// A stale writer is rejected; callers must reload and reconcile rather than overwrite.
    pub fn commit_state(
        &mut self,
        expected_generation: u64,
        next_state: &AccountState,
    ) -> Result<u64, AccountStateError> {
        let actual = self.encrypted.generation()?;
        if actual != expected_generation {
            return Err(AccountStateError::GenerationConflict {
                expected: expected_generation,
                actual,
            });
        }
        let encoded = next_state.encode()?;
        self.encrypted
            .replace_plaintext_at_generation(expected_generation, &encoded)
            .map_err(AccountStateError::Storage)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{IdentityKeyProvider, InMemoryBlobStore, SecureStoreError};
    use zeroize::Zeroizing;

    struct TestKey;
    impl IdentityKeyProvider for TestKey {
        fn load_key(&self) -> Result<Zeroizing<[u8; 32]>, SecureStoreError> {
            Ok(Zeroizing::new([0x42; 32]))
        }
    }

    fn account(username: &str, id: &str, hash: &str) -> AccountStateEntry {
        let user_id = UserId::new(id).unwrap();
        let address = WalletAddress("ATC00000000000000000000000000000000AA".into());
        let binding = IdentityBinding {
            user_id: user_id.clone(),
            wallet_address: address,
            chain_id: 658467,
            network: "devnet".into(),
            key_version: 1,
        };
        let profile = AccountProfile::new(user_id, username, binding).unwrap();
        AccountStateEntry {
            username: AccountUsername::parse(username).unwrap(),
            identity: IdentityRecord::new(profile, CredentialRecord::argon2id(hash)),
        }
    }

    #[test]
    fn empty_repository_state_is_valid_before_first_account() {
        let state = AccountState::default();
        assert_eq!(
            AccountState::decode(&state.encode().unwrap()).unwrap(),
            state
        );
    }

    #[test]
    fn canonical_state_round_trips_multiple_accounts() {
        let state = AccountState::new(vec![
            account(
                "bob",
                "user-bob",
                "$argon2id$v=19$m=19456,t=2,p=1$salt$hash",
            ),
            account(
                "alice",
                "user-alice",
                "$argon2id$v=19$m=19456,t=2,p=1$salt$hash",
            ),
        ])
        .unwrap();
        let decoded = AccountState::decode(&state.encode().unwrap()).unwrap();
        assert_eq!(decoded.accounts()[0].username.as_str(), "alice");
        assert_eq!(decoded.accounts()[1].username.as_str(), "bob");
        assert_eq!(decoded, state);
    }

    #[test]
    fn phc_credentials_export_losslessly_to_kernel_atid_catalog() {
        let user_id = UserId::new("alice").unwrap();
        let mut service = crate::IdentityService::new();
        service
            .register(
                user_id,
                "Alice",
                b"correct horse battery",
                658467,
                "devnet",
                1,
            )
            .unwrap();
        let state = AccountState::new(vec![AccountStateEntry {
            username: AccountUsername::parse("alice").unwrap(),
            identity: service.snapshot().unwrap(),
        }])
        .unwrap();
        let bytes = state.credential_snapshot().unwrap();
        let catalog = globus_identity_core::catalog::parse_credential_catalog(&bytes).unwrap();
        let record = &catalog.by_user_id("alice").unwrap().credential;
        assert_eq!(record.verify(b"correct horse battery"), Ok(()));
        assert_eq!(
            record.verify(b"incorrect horse battery"),
            Err(globus_identity_core::VerifyError::WrongPassword)
        );
    }

    #[test]
    fn directory_snapshot_is_exported_from_encrypted_account_state() {
        let state = AccountState::new(vec![
            account(
                "bob",
                "user-bob",
                "$argon2id$v=19$m=19456,t=2,p=1$salt$hash",
            ),
            account(
                "alice",
                "user-alice",
                "$argon2id$v=19$m=19456,t=2,p=1$salt$hash",
            ),
        ])
        .unwrap();
        let bytes = state.directory_snapshot().unwrap();
        let entries = globus_identity_core::catalog::parse_directory(&bytes).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].username, "alice");
        assert_eq!(entries[0].user_id, "user-alice");
    }

    #[test]
    fn duplicate_username_or_user_id_is_rejected() {
        assert_eq!(
            AccountState::new(vec![
                account(
                    "alice",
                    "user-a",
                    "$argon2id$v=19$m=19456,t=2,p=1$salt$hash"
                ),
                account(
                    "alice",
                    "user-b",
                    "$argon2id$v=19$m=19456,t=2,p=1$salt$hash"
                ),
            ]),
            Err(AccountStateError::DuplicateAccount)
        );
    }

    #[test]
    fn trailing_bytes_are_rejected() {
        let state = AccountState::new(vec![account(
            "alice",
            "user-a",
            "$argon2id$v=19$m=19456,t=2,p=1$salt$hash",
        )])
        .unwrap();
        let mut bytes = state.encode().unwrap();
        bytes.push(0);
        assert_eq!(
            AccountState::decode(&bytes),
            Err(AccountStateError::TrailingData)
        );
    }

    #[test]
    fn encrypted_file_repository_reopens_with_complete_account_state() {
        use std::time::{SystemTime, UNIX_EPOCH};
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("globus-account-state-{unique}.blob"));
        let state = AccountState::new(vec![
            account(
                "alice",
                "user-a",
                "$argon2id$v=19$m=19456,t=2,p=1$salt$hash",
            ),
            account("bob", "user-b", "$argon2id$v=19$m=19456,t=2,p=1$salt$hash"),
        ])
        .unwrap();
        {
            let encrypted = EncryptedBlobStore::new(crate::FileBlobStore::open(&path), TestKey);
            let mut repository = AccountStateRepository::new(encrypted);
            assert_eq!(repository.commit_state(0, &state).unwrap(), 1);
        }
        {
            let encrypted = EncryptedBlobStore::new(crate::FileBlobStore::open(&path), TestKey);
            let repository = AccountStateRepository::new(encrypted);
            assert_eq!(repository.load_state().unwrap(), Some(state));
            let username = AccountUsername::parse("alice").unwrap();
            assert_eq!(
                AccountDirectoryStore::lookup(&repository, &username)
                    .unwrap()
                    .as_str(),
                "user-a"
            );
            let snapshot = repository.export_directory_snapshot().unwrap().unwrap();
            assert_eq!(
                globus_identity_core::catalog::parse_directory(&snapshot)
                    .unwrap()
                    .len(),
                2
            );
        }
        let _ = std::fs::remove_file(&path);
        let mut lock = path.as_os_str().to_os_string();
        lock.push(".lock");
        let _ = std::fs::remove_file(std::path::PathBuf::from(lock));
    }

    #[test]
    fn encrypted_repository_survives_reopen_and_rejects_stale_generation() {
        let encrypted = EncryptedBlobStore::new(InMemoryBlobStore::default(), TestKey);
        let mut repository = AccountStateRepository::new(encrypted);
        let state = AccountState::new(vec![
            account(
                "alice",
                "user-a",
                "$argon2id$v=19$m=19456,t=2,p=1$salt$hash",
            ),
            account("bob", "user-b", "$argon2id$v=19$m=19456,t=2,p=1$salt$hash"),
        ])
        .unwrap();
        assert_eq!(repository.commit_state(0, &state).unwrap(), 1);
        assert_eq!(repository.load_state().unwrap(), Some(state));
        assert_eq!(
            repository
                .lookup(&AccountUsername::parse("alice").unwrap())
                .unwrap()
                .as_str(),
            "user-a"
        );
        assert_eq!(repository.entries().unwrap().len(), 2);
        assert!(matches!(
            repository.commit_state(
                0,
                &AccountState::new(vec![account(
                    "alice",
                    "user-a",
                    "$argon2id$v=19$m=19456,t=2,p=1$salt$hash"
                ),])
                .unwrap()
            ),
            Err(AccountStateError::GenerationConflict {
                expected: 0,
                actual: 1
            })
        ));
    }
}
