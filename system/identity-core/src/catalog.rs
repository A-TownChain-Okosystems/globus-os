//! Shared no_std codecs for the persistent account directory and boot catalog.
//!
//! GUA1 remains the sole username-to-UserId directory format. GAC1 is only a
//! boot-image envelope: it embeds the exact GUA1 directory snapshot and binds
//! one existing 64-byte ATID credential record to each UserId in directory order.
//! It is not a second account database or credential format.

use alloc::{string::String, vec::Vec};

use crate::{CredentialRecord, RECORD_LEN};

const DIRECTORY_MAGIC: &[u8; 4] = b"GUA1";
const DIRECTORY_VERSION: u8 = 1;
const CATALOG_MAGIC: &[u8; 4] = b"GAC1";
const CATALOG_VERSION: u16 = 1;
const MAX_ACCOUNTS: usize = 4096;
const MAX_USERNAME: usize = 32;
const MAX_USER_ID: usize = 128;
const CATALOG_HEADER_LEN: usize = 12;

/// A validated username-to-stable-account-ID mapping from the canonical GUA1 directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectoryEntry {
    /// Canonical lowercase username.
    pub username: String,
    /// Stable account identifier.
    pub user_id: String,
}

/// One account and its parsed existing ATID credential verifier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootAccount {
    /// Canonical username used by the login interface.
    pub username: String,
    /// Stable identity identifier resolved by the account directory.
    pub user_id: String,
    /// Existing no_std Argon2id credential record.
    pub credential: CredentialRecord,
}

/// Fully validated boot-time view; this is a read-only projection, not persistence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootAccountCatalog {
    accounts: Vec<BootAccount>,
}

impl BootAccountCatalog {
    /// Number of accounts in the validated catalog.
    pub fn len(&self) -> usize { self.accounts.len() }
    /// True when the catalog contains no accounts.
    pub fn is_empty(&self) -> bool { self.accounts.is_empty() }
    /// Read-only account entries in canonical username order.
    pub fn accounts(&self) -> &[BootAccount] { &self.accounts }
    /// Resolve a canonical username to its account entry.
    pub fn by_username(&self, username: &str) -> Option<&BootAccount> {
        self.accounts.binary_search_by(|entry| entry.username.as_str().cmp(username))
            .ok().map(|index| &self.accounts[index])
    }
    /// Resolve a stable account identifier to its account entry.
    pub fn by_user_id(&self, user_id: &str) -> Option<&BootAccount> {
        self.accounts.iter().find(|entry| entry.user_id == user_id)
    }
}

/// Errors are intentionally coarse at the boot boundary; malformed data never partially loads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogError {
    /// Truncated header, field, or credential record.
    Truncated,
    /// Unknown magic or version.
    UnsupportedFormat,
    /// Invalid count, field, or non-canonical username.
    InvalidField,
    /// Duplicate username or stable account ID.
    DuplicateAccount,
    /// Directory and credential records do not bind one-to-one.
    AccountBindingMismatch,
    /// Existing ATID credential record failed its canonical parser.
    InvalidCredential,
    /// Trailing bytes are not permitted.
    TrailingData,
}

/// Parse the existing GUA1 username directory format.
pub fn parse_directory(bytes: &[u8]) -> Result<Vec<DirectoryEntry>, CatalogError> {
    if bytes.len() < 9 { return Err(CatalogError::Truncated); }
    if &bytes[..4] != DIRECTORY_MAGIC || bytes[4] != DIRECTORY_VERSION {
        return Err(CatalogError::UnsupportedFormat);
    }
    let count = u32::from_be_bytes(bytes[5..9].try_into().map_err(|_| CatalogError::Truncated)?) as usize;
    if count > MAX_ACCOUNTS { return Err(CatalogError::InvalidField); }
    let mut cursor = 9usize;
    let mut entries: Vec<DirectoryEntry> = Vec::with_capacity(count);
    for _ in 0..count {
        let name_len = *bytes.get(cursor).ok_or(CatalogError::Truncated)? as usize;
        cursor = cursor.checked_add(1).ok_or(CatalogError::InvalidField)?;
        if !(3..=MAX_USERNAME).contains(&name_len) { return Err(CatalogError::InvalidField); }
        let name_end = cursor.checked_add(name_len).ok_or(CatalogError::InvalidField)?;
        let name = core::str::from_utf8(bytes.get(cursor..name_end).ok_or(CatalogError::Truncated)?)
            .map_err(|_| CatalogError::InvalidField)?;
        cursor = name_end;
        if !canonical_username(name) { return Err(CatalogError::InvalidField); }
        let id_len_end = cursor.checked_add(2).ok_or(CatalogError::InvalidField)?;
        let id_len = u16::from_be_bytes(bytes.get(cursor..id_len_end).ok_or(CatalogError::Truncated)?
            .try_into().map_err(|_| CatalogError::Truncated)?) as usize;
        cursor = id_len_end;
        if id_len == 0 || id_len > MAX_USER_ID { return Err(CatalogError::InvalidField); }
        let id_end = cursor.checked_add(id_len).ok_or(CatalogError::InvalidField)?;
        let id_bytes = bytes.get(cursor..id_end).ok_or(CatalogError::Truncated)?;
        let user_id = core::str::from_utf8(id_bytes).map_err(|_| CatalogError::InvalidField)?;
        if user_id.as_bytes().contains(&0) { return Err(CatalogError::InvalidField); }
        cursor = id_end;
        if entries.iter().any(|entry| entry.username == name || entry.user_id == user_id) {
            return Err(CatalogError::DuplicateAccount);
        }
        entries.push(DirectoryEntry { username: String::from(name), user_id: String::from(user_id) });
    }
    if cursor != bytes.len() { return Err(CatalogError::TrailingData); }
    if entries.windows(2).any(|pair| pair[0].username >= pair[1].username) {
        return Err(CatalogError::InvalidField);
    }
    Ok(entries)
}

/// Encode canonical GUA1 bytes. Entries are sorted by username before encoding.
pub fn encode_directory(entries: &[DirectoryEntry]) -> Result<Vec<u8>, CatalogError> {
    if entries.len() > MAX_ACCOUNTS { return Err(CatalogError::InvalidField); }
    let mut sorted = entries.to_vec();
    sorted.sort_by(|left, right| left.username.cmp(&right.username));
    let mut out = Vec::new();
    out.extend_from_slice(DIRECTORY_MAGIC);
    out.push(DIRECTORY_VERSION);
    out.extend_from_slice(&(sorted.len() as u32).to_be_bytes());
    for entry in &sorted {
        if !canonical_username(&entry.username)
            || entry.user_id.is_empty()
            || entry.user_id.len() > MAX_USER_ID
            || entry.user_id.as_bytes().contains(&0)
            || sorted.iter().filter(|other| other.username == entry.username).count() != 1
            || sorted.iter().filter(|other| other.user_id == entry.user_id).count() != 1
        { return Err(CatalogError::InvalidField); }
        out.push(entry.username.len() as u8);
        out.extend_from_slice(entry.username.as_bytes());
        out.extend_from_slice(&(entry.user_id.len() as u16).to_be_bytes());
        out.extend_from_slice(entry.user_id.as_bytes());
    }
    Ok(out)
}

/// Parse GAC1: embedded GUA1 directory followed by one user-ID-bound ATID record per account.
pub fn parse_boot_catalog(bytes: &[u8]) -> Result<BootAccountCatalog, CatalogError> {
    if bytes.len() < CATALOG_HEADER_LEN { return Err(CatalogError::Truncated); }
    if &bytes[..4] != CATALOG_MAGIC
        || u16::from_be_bytes([bytes[4], bytes[5]]) != CATALOG_VERSION
    { return Err(CatalogError::UnsupportedFormat); }
    let count = u16::from_be_bytes([bytes[6], bytes[7]]) as usize;
    if count == 0 || count > MAX_ACCOUNTS { return Err(CatalogError::InvalidField); }
    let directory_len = u32::from_be_bytes(bytes[8..12].try_into().map_err(|_| CatalogError::Truncated)?) as usize;
    let directory_end = CATALOG_HEADER_LEN.checked_add(directory_len).ok_or(CatalogError::InvalidField)?;
    let directory_bytes = bytes.get(CATALOG_HEADER_LEN..directory_end).ok_or(CatalogError::Truncated)?;
    let directory = parse_directory(directory_bytes)?;
    if directory.len() != count { return Err(CatalogError::AccountBindingMismatch); }
    let mut cursor = directory_end;
    let mut accounts = Vec::with_capacity(count);
    for directory_entry in directory {
        let id_len_end = cursor.checked_add(2).ok_or(CatalogError::InvalidField)?;
        let id_len = u16::from_be_bytes(bytes.get(cursor..id_len_end).ok_or(CatalogError::Truncated)?
            .try_into().map_err(|_| CatalogError::Truncated)?) as usize;
        cursor = id_len_end;
        if id_len == 0 || id_len > MAX_USER_ID { return Err(CatalogError::InvalidField); }
        let id_end = cursor.checked_add(id_len).ok_or(CatalogError::InvalidField)?;
        let user_id = core::str::from_utf8(bytes.get(cursor..id_end).ok_or(CatalogError::Truncated)?)
            .map_err(|_| CatalogError::InvalidField)?;
        cursor = id_end;
        if user_id != directory_entry.user_id { return Err(CatalogError::AccountBindingMismatch); }
        let record_end = cursor.checked_add(RECORD_LEN).ok_or(CatalogError::InvalidField)?;
        let credential = CredentialRecord::parse(bytes.get(cursor..record_end).ok_or(CatalogError::Truncated)?)
            .map_err(|_| CatalogError::InvalidCredential)?;
        cursor = record_end;
        accounts.push(BootAccount { username: directory_entry.username, user_id: directory_entry.user_id, credential });
    }
    if cursor != bytes.len() { return Err(CatalogError::TrailingData); }
    Ok(BootAccountCatalog { accounts })
}

/// Build GAC1 from the canonical GUA1 snapshot and credentials keyed by stable UserId.
pub fn encode_boot_catalog(
    directory_bytes: &[u8],
    credentials: &[(String, Vec<u8>)],
) -> Result<Vec<u8>, CatalogError> {
    let directory = parse_directory(directory_bytes)?;
    if directory.is_empty() || directory.len() != credentials.len() || directory.len() > MAX_ACCOUNTS {
        return Err(CatalogError::AccountBindingMismatch);
    }
    let mut out = Vec::new();
    out.extend_from_slice(CATALOG_MAGIC);
    out.extend_from_slice(&CATALOG_VERSION.to_be_bytes());
    out.extend_from_slice(&(directory.len() as u16).to_be_bytes());
    out.extend_from_slice(&(directory_bytes.len() as u32).to_be_bytes());
    out.extend_from_slice(directory_bytes);
    for entry in &directory {
        let mut matches = credentials.iter().filter(|(id, _)| id == &entry.user_id);
        let (_, record) = matches.next().ok_or(CatalogError::AccountBindingMismatch)?;
        if matches.next().is_some() || record.len() != RECORD_LEN {
            return Err(CatalogError::AccountBindingMismatch);
        }
        CredentialRecord::parse(record).map_err(|_| CatalogError::InvalidCredential)?;
        out.extend_from_slice(&(entry.user_id.len() as u16).to_be_bytes());
        out.extend_from_slice(entry.user_id.as_bytes());
        out.extend_from_slice(record);
    }
    Ok(out)
}

fn canonical_username(value: &str) -> bool {
    let bytes = value.as_bytes();
    (3..=MAX_USERNAME).contains(&bytes.len())
        && bytes.iter().all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(*byte, b'_' | b'.' | b'-'))
        && bytes[0].is_ascii_alphanumeric()
        && bytes[bytes.len() - 1].is_ascii_alphanumeric()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{hash_into, KdfParams, HASH_LEN, SALT_LEN};

    fn credential(password: &[u8], salt_byte: u8) -> Vec<u8> {
        let params = KdfParams { memory_kib: 8 * 1024, iterations: 1, parallelism: 1 };
        let salt = [salt_byte; SALT_LEN];
        let mut hash = [0u8; HASH_LEN];
        hash_into(params, password, &salt, &mut hash).unwrap();
        let mut bytes = Vec::from(*b"ATID");
        bytes.push(1);
        bytes.push(1);
        bytes.extend_from_slice(&(params.memory_kib as u16).to_le_bytes());
        bytes.extend_from_slice(&params.iterations.to_le_bytes());
        bytes.push(params.parallelism as u8);
        bytes.extend_from_slice(&[0, 0, 0]);
        bytes.extend_from_slice(&salt);
        bytes.extend_from_slice(&hash);
        bytes
    }

    #[test]
    fn directory_codec_preserves_the_existing_gua1_contract() {
        let entries = vec![
            DirectoryEntry { username: String::from("bob"), user_id: String::from("user-b") },
            DirectoryEntry { username: String::from("alice"), user_id: String::from("user-a") },
        ];
        let bytes = encode_directory(&entries).unwrap();
        let decoded = parse_directory(&bytes).unwrap();
        assert_eq!(decoded[0].username, "alice");
        assert_eq!(decoded[1].username, "bob");
    }

    #[test]
    fn boot_catalog_binds_each_existing_atid_record_to_the_directory_user_id() {
        let directory = encode_directory(&[
            DirectoryEntry { username: String::from("alice"), user_id: String::from("user-a") },
            DirectoryEntry { username: String::from("bob"), user_id: String::from("user-b") },
        ]).unwrap();
        let credentials = vec![
            (String::from("user-b"), credential(b"bob-secret", 0x22)),
            (String::from("user-a"), credential(b"alice-secret", 0x11)),
        ];
        let catalog = encode_boot_catalog(&directory, &credentials).unwrap();
        let parsed = parse_boot_catalog(&catalog).unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed.by_username("alice").unwrap().credential.verify(b"alice-secret"), Ok(()));
        assert_eq!(parsed.by_username("bob").unwrap().credential.verify(b"bob-secret"), Ok(()));
        assert!(parsed.by_username("alice").unwrap().credential.verify(b"bob-secret").is_err());
    }

    #[test]
    fn mismatched_user_id_and_credential_records_fail_closed() {
        let directory = encode_directory(&[
            DirectoryEntry { username: String::from("alice"), user_id: String::from("user-a") },
        ]).unwrap();
        let catalog = encode_boot_catalog(&directory, &[(String::from("user-b"), credential(b"secret", 0x33))]);
        assert_eq!(catalog, Err(CatalogError::AccountBindingMismatch));
    }

    #[test]
    fn truncated_or_trailing_catalog_data_is_rejected() {
        let directory = encode_directory(&[
            DirectoryEntry { username: String::from("alice"), user_id: String::from("user-a") },
        ]).unwrap();
        let catalog = encode_boot_catalog(&directory, &[(String::from("user-a"), credential(b"secret", 0x33))]).unwrap();
        assert_eq!(parse_boot_catalog(&catalog[..catalog.len() - 1]), Err(CatalogError::Truncated));
        let mut trailing = catalog;
        trailing.push(0);
        assert_eq!(parse_boot_catalog(&trailing), Err(CatalogError::TrailingData));
    }

    #[test]
    fn duplicate_directory_entries_are_rejected() {
        let entries = [
            DirectoryEntry { username: String::from("alice"), user_id: String::from("user-a") },
            DirectoryEntry { username: String::from("alice"), user_id: String::from("user-b") },
        ];
        assert_eq!(encode_directory(&entries), Err(CatalogError::InvalidField));
    }
}
