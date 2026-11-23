//! Shared no_std codecs for the canonical GUA1 directory and boot credential catalog.
//!
//! GUA1 is the username-to-UserId directory projection. GCR1 is a boot-only
//! collection of existing 64-byte ATID records keyed by full UserId. The kernel
//! combines the two entries and validates one-to-one membership before publish.

use crate::{CredentialRecord, RECORD_LEN};
use alloc::{string::String, vec::Vec};

const DIRECTORY_MAGIC: &[u8; 4] = b"GUA1";
const DIRECTORY_VERSION: u8 = 1;
const CREDENTIAL_MAGIC: &[u8; 4] = b"GCR1";
const CREDENTIAL_VERSION: u16 = 1;
const MAX_ACCOUNTS: usize = 4096;
const MAX_USERNAME: usize = 32;
const MAX_USER_ID: usize = 128;
const CREDENTIAL_HEADER_LEN: usize = 8;

/// Canonical username-to-stable-account-ID mapping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectoryEntry {
    /// Canonical lowercase username.
    pub username: String,
    /// Stable account identifier.
    pub user_id: String,
}

/// One existing ATID credential record keyed by the full stable UserId.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CredentialEntry {
    /// Stable account identifier.
    pub user_id: String,
    /// Parsed existing Argon2id credential verifier.
    pub credential: CredentialRecord,
}

/// Parsed GCR1 credential collection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CredentialCatalog {
    entries: Vec<CredentialEntry>,
}
impl CredentialCatalog {
    /// Number of credential records.
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    /// True if no credential records exist.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    /// Canonically sorted credential records.
    pub fn entries(&self) -> &[CredentialEntry] {
        &self.entries
    }
    /// Resolve by full stable UserId.
    pub fn by_user_id(&self, user_id: &str) -> Option<&CredentialEntry> {
        self.entries
            .binary_search_by(|entry| entry.user_id.as_str().cmp(user_id))
            .ok()
            .map(|index| &self.entries[index])
    }
}

/// One fully cross-validated account, created only after both Ramdisk entries pass validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootAccount {
    /// Canonical username.
    pub username: String,
    /// Stable account identifier.
    pub user_id: String,
    /// Matching ATID credential record.
    pub credential: CredentialRecord,
}

/// Complete read-only boot snapshot; it is not persistent account storage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootAccountCatalog {
    accounts: Vec<BootAccount>,
}
impl BootAccountCatalog {
    /// Number of validated accounts.
    pub fn len(&self) -> usize {
        self.accounts.len()
    }
    /// True when no accounts exist.
    pub fn is_empty(&self) -> bool {
        self.accounts.is_empty()
    }
    /// Accounts in canonical username order.
    pub fn accounts(&self) -> &[BootAccount] {
        &self.accounts
    }
    /// Resolve a canonical username.
    pub fn by_username(&self, username: &str) -> Option<&BootAccount> {
        self.accounts
            .binary_search_by(|entry| entry.username.as_str().cmp(username))
            .ok()
            .map(|index| &self.accounts[index])
    }
}

/// Fail-closed format and cross-reference errors.
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
    /// Directory and credential membership do not match exactly.
    AccountBindingMismatch,
    /// A directory account has no matching credential record.
    MissingCredential,
    /// Existing ATID credential record failed its canonical parser.
    InvalidCredential,
    /// Trailing bytes are not permitted.
    TrailingData,
}

/// Parse the existing GUA1 directory encoding.
pub fn parse_directory(bytes: &[u8]) -> Result<Vec<DirectoryEntry>, CatalogError> {
    if bytes.len() < 9 {
        return Err(CatalogError::Truncated);
    }
    if &bytes[..4] != DIRECTORY_MAGIC || bytes[4] != DIRECTORY_VERSION {
        return Err(CatalogError::UnsupportedFormat);
    }
    let count = u32::from_be_bytes(
        bytes[5..9]
            .try_into()
            .map_err(|_| CatalogError::Truncated)?,
    ) as usize;
    if count > MAX_ACCOUNTS {
        return Err(CatalogError::InvalidField);
    }
    let mut cursor = 9usize;
    let mut entries: Vec<DirectoryEntry> = Vec::with_capacity(count);
    for _ in 0..count {
        let name_len = *bytes.get(cursor).ok_or(CatalogError::Truncated)? as usize;
        cursor = cursor.checked_add(1).ok_or(CatalogError::InvalidField)?;
        if !(3..=MAX_USERNAME).contains(&name_len) {
            return Err(CatalogError::InvalidField);
        }
        let name_end = cursor
            .checked_add(name_len)
            .ok_or(CatalogError::InvalidField)?;
        let name =
            core::str::from_utf8(bytes.get(cursor..name_end).ok_or(CatalogError::Truncated)?)
                .map_err(|_| CatalogError::InvalidField)?;
        cursor = name_end;
        if !canonical_username(name) {
            return Err(CatalogError::InvalidField);
        }
        let id_len_end = cursor.checked_add(2).ok_or(CatalogError::InvalidField)?;
        let id_len = u16::from_be_bytes(
            bytes
                .get(cursor..id_len_end)
                .ok_or(CatalogError::Truncated)?
                .try_into()
                .map_err(|_| CatalogError::Truncated)?,
        ) as usize;
        cursor = id_len_end;
        if id_len == 0 || id_len > MAX_USER_ID {
            return Err(CatalogError::InvalidField);
        }
        let id_end = cursor
            .checked_add(id_len)
            .ok_or(CatalogError::InvalidField)?;
        let id_bytes = bytes.get(cursor..id_end).ok_or(CatalogError::Truncated)?;
        let user_id = core::str::from_utf8(id_bytes).map_err(|_| CatalogError::InvalidField)?;
        if user_id.as_bytes().contains(&0) {
            return Err(CatalogError::InvalidField);
        }
        cursor = id_end;
        if entries
            .iter()
            .any(|entry| entry.username == name || entry.user_id == user_id)
        {
            return Err(CatalogError::DuplicateAccount);
        }
        entries.push(DirectoryEntry {
            username: String::from(name),
            user_id: String::from(user_id),
        });
    }
    if cursor != bytes.len() {
        return Err(CatalogError::TrailingData);
    }
    if entries
        .windows(2)
        .any(|pair| pair[0].username >= pair[1].username)
    {
        return Err(CatalogError::InvalidField);
    }
    Ok(entries)
}

/// Encode canonical GUA1 bytes; entries are sorted by username.
pub fn encode_directory(entries: &[DirectoryEntry]) -> Result<Vec<u8>, CatalogError> {
    if entries.len() > MAX_ACCOUNTS {
        return Err(CatalogError::InvalidField);
    }
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
        {
            return Err(CatalogError::InvalidField);
        }
        if sorted
            .iter()
            .filter(|other| other.username == entry.username)
            .count()
            != 1
            || sorted
                .iter()
                .filter(|other| other.user_id == entry.user_id)
                .count()
                != 1
        {
            return Err(CatalogError::DuplicateAccount);
        }
        out.push(entry.username.len() as u8);
        out.extend_from_slice(entry.username.as_bytes());
        out.extend_from_slice(&(entry.user_id.len() as u16).to_be_bytes());
        out.extend_from_slice(entry.user_id.as_bytes());
    }
    Ok(out)
}

/// Parse GCR1, a collection of existing ATID records keyed by full UserId.
pub fn parse_credential_catalog(bytes: &[u8]) -> Result<CredentialCatalog, CatalogError> {
    if bytes.len() < CREDENTIAL_HEADER_LEN {
        return Err(CatalogError::Truncated);
    }
    if &bytes[..4] != CREDENTIAL_MAGIC
        || u16::from_be_bytes([bytes[4], bytes[5]]) != CREDENTIAL_VERSION
    {
        return Err(CatalogError::UnsupportedFormat);
    }
    let count = u16::from_be_bytes([bytes[6], bytes[7]]) as usize;
    if count == 0 || count > MAX_ACCOUNTS {
        return Err(CatalogError::InvalidField);
    }
    let mut cursor = CREDENTIAL_HEADER_LEN;
    let mut entries = Vec::with_capacity(count);
    for _ in 0..count {
        let len_end = cursor.checked_add(2).ok_or(CatalogError::InvalidField)?;
        let id_len = u16::from_be_bytes(
            bytes
                .get(cursor..len_end)
                .ok_or(CatalogError::Truncated)?
                .try_into()
                .map_err(|_| CatalogError::Truncated)?,
        ) as usize;
        cursor = len_end;
        if id_len == 0 || id_len > MAX_USER_ID {
            return Err(CatalogError::InvalidField);
        }
        let id_end = cursor
            .checked_add(id_len)
            .ok_or(CatalogError::InvalidField)?;
        let user_id =
            core::str::from_utf8(bytes.get(cursor..id_end).ok_or(CatalogError::Truncated)?)
                .map_err(|_| CatalogError::InvalidField)?;
        if user_id.as_bytes().contains(&0) {
            return Err(CatalogError::InvalidField);
        }
        cursor = id_end;
        let record_end = cursor
            .checked_add(RECORD_LEN)
            .ok_or(CatalogError::InvalidField)?;
        let credential = CredentialRecord::parse(
            bytes
                .get(cursor..record_end)
                .ok_or(CatalogError::Truncated)?,
        )
        .map_err(|_| CatalogError::InvalidCredential)?;
        cursor = record_end;
        if entries
            .last()
            .is_some_and(|previous: &CredentialEntry| previous.user_id.as_str() >= user_id)
        {
            return Err(CatalogError::DuplicateAccount);
        }
        entries.push(CredentialEntry {
            user_id: String::from(user_id),
            credential,
        });
    }
    if cursor != bytes.len() {
        return Err(CatalogError::TrailingData);
    }
    Ok(CredentialCatalog { entries })
}

/// Encode GCR1 records in canonical UserId order.
pub fn encode_credential_catalog(
    credentials: &[(String, Vec<u8>)],
) -> Result<Vec<u8>, CatalogError> {
    if credentials.is_empty() || credentials.len() > MAX_ACCOUNTS {
        return Err(CatalogError::InvalidField);
    }
    let mut sorted = credentials.to_vec();
    sorted.sort_by(|left, right| left.0.cmp(&right.0));
    let mut out = Vec::new();
    out.extend_from_slice(CREDENTIAL_MAGIC);
    out.extend_from_slice(&CREDENTIAL_VERSION.to_be_bytes());
    out.extend_from_slice(&(sorted.len() as u16).to_be_bytes());
    for (index, (user_id, record)) in sorted.iter().enumerate() {
        if user_id.is_empty() || user_id.len() > MAX_USER_ID || user_id.as_bytes().contains(&0) {
            return Err(CatalogError::InvalidField);
        }
        if index > 0 && sorted[index - 1].0 == *user_id {
            return Err(CatalogError::DuplicateAccount);
        }
        if record.len() != RECORD_LEN {
            return Err(CatalogError::InvalidCredential);
        }
        CredentialRecord::parse(record).map_err(|_| CatalogError::InvalidCredential)?;
        out.extend_from_slice(&(user_id.len() as u16).to_be_bytes());
        out.extend_from_slice(user_id.as_bytes());
        out.extend_from_slice(record);
    }
    Ok(out)
}

/// Validate the GUA1 and GCR1 Ramdisk entries as one complete one-to-one account set.
pub fn parse_boot_catalog(
    directory_bytes: &[u8],
    credential_bytes: &[u8],
) -> Result<BootAccountCatalog, CatalogError> {
    let directory = parse_directory(directory_bytes)?;
    let credentials = parse_credential_catalog(credential_bytes)?;
    if directory.is_empty() || directory.len() != credentials.len() {
        return Err(CatalogError::AccountBindingMismatch);
    }
    let mut accounts = Vec::with_capacity(directory.len());
    for entry in directory {
        let credential = credentials
            .by_user_id(&entry.user_id)
            .ok_or(CatalogError::MissingCredential)?;
        accounts.push(BootAccount {
            username: entry.username,
            user_id: entry.user_id,
            credential: credential.credential.clone(),
        });
    }
    if credentials.entries().iter().any(|credential| {
        !accounts
            .iter()
            .any(|account| account.user_id == credential.user_id)
    }) {
        return Err(CatalogError::AccountBindingMismatch);
    }
    Ok(BootAccountCatalog { accounts })
}

fn canonical_username(value: &str) -> bool {
    let bytes = value.as_bytes();
    (3..=MAX_USERNAME).contains(&bytes.len())
        && bytes.iter().all(|byte| {
            byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || matches!(*byte, b'_' | b'.' | b'-')
        })
        && bytes[0].is_ascii_alphanumeric()
        && bytes[bytes.len() - 1].is_ascii_alphanumeric()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{HASH_LEN, KdfParams, SALT_LEN, hash_into};

    fn credential(password: &[u8], salt_byte: u8) -> Vec<u8> {
        let params = KdfParams {
            memory_kib: 8 * 1024,
            iterations: 1,
            parallelism: 1,
        };
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
    fn existing_gua1_directory_codec_is_shared_and_canonical() {
        let entries = vec![
            DirectoryEntry {
                username: String::from("bob"),
                user_id: String::from("user-b"),
            },
            DirectoryEntry {
                username: String::from("alice"),
                user_id: String::from("user-a"),
            },
        ];
        let bytes = encode_directory(&entries).unwrap();
        let decoded = parse_directory(&bytes).unwrap();
        assert_eq!(decoded[0].username, "alice");
        assert_eq!(decoded[1].username, "bob");
    }

    #[test]
    fn separate_gua1_and_gcr1_entries_bind_credentials_by_full_user_id() {
        let directory = encode_directory(&[
            DirectoryEntry {
                username: String::from("alice"),
                user_id: String::from("id-alice"),
            },
            DirectoryEntry {
                username: String::from("bob"),
                user_id: String::from("id-bob"),
            },
        ])
        .unwrap();
        let credentials = encode_credential_catalog(&[
            (String::from("id-bob"), credential(b"bob-secret", 0x22)),
            (String::from("id-alice"), credential(b"alice-secret", 0x11)),
        ])
        .unwrap();
        let parsed = parse_boot_catalog(&directory, &credentials).unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(
            parsed
                .by_username("alice")
                .unwrap()
                .credential
                .verify(b"alice-secret"),
            Ok(())
        );
        assert_eq!(
            parsed
                .by_username("bob")
                .unwrap()
                .credential
                .verify(b"bob-secret"),
            Ok(())
        );
        assert!(
            parsed
                .by_username("alice")
                .unwrap()
                .credential
                .verify(b"bob-secret")
                .is_err()
        );
    }

    #[test]
    fn missing_or_orphaned_credential_is_rejected() {
        let directory = encode_directory(&[DirectoryEntry {
            username: String::from("alice"),
            user_id: String::from("id-alice"),
        }])
        .unwrap();
        let credentials =
            encode_credential_catalog(&[(String::from("id-bob"), credential(b"bob-secret", 0x22))])
                .unwrap();
        assert_eq!(
            parse_boot_catalog(&directory, &credentials),
            Err(CatalogError::MissingCredential)
        );
    }

    #[test]
    fn malformed_duplicate_and_trailing_credential_data_fail_closed() {
        let valid =
            encode_credential_catalog(&[(String::from("id-alice"), credential(b"secret", 0x33))])
                .unwrap();
        assert_eq!(
            parse_credential_catalog(&valid[..valid.len() - 1]),
            Err(CatalogError::Truncated)
        );
        let mut trailing = valid.clone();
        trailing.push(0);
        assert_eq!(
            parse_credential_catalog(&trailing),
            Err(CatalogError::TrailingData)
        );
        assert_eq!(
            encode_credential_catalog(&[
                (String::from("same"), credential(b"a", 0x11)),
                (String::from("same"), credential(b"b", 0x22)),
            ]),
            Err(CatalogError::DuplicateAccount)
        );
    }
}
