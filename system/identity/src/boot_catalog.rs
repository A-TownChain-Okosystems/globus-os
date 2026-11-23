//! Builds the boot-only catalog from the canonical persistent account directory.
//!
//! The directory remains the only username/UserId authority. Credential files
//! are supplied by the caller in the existing 64-byte ATID format; this module
//! validates and packages them but does not create another credential store.

use crate::{AccountDirectoryError, AccountDirectoryStore, UserId};

/// Existing ATID verifier bytes bound to one stable account identifier.
pub struct AccountCredential {
    /// Stable account identifier from the canonical directory.
    pub user_id: UserId,
    /// Exact existing 64-byte ATID record; never a plaintext password.
    pub record: Vec<u8>,
}

/// Errors while projecting persistent identity state into a boot image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BootCatalogBuildError {
    /// The authoritative username directory could not be read.
    Directory(AccountDirectoryError),
    /// Directory and credential records could not be bound into a valid catalog.
    InvalidCatalog,
}

/// Build a deterministic GAC1 image envelope from the canonical AccountDirectoryStore.
///
/// Every directory entry must have exactly one ATID record keyed by its UserId;
/// missing, duplicate, orphaned, malformed, or unsupported records are rejected.
pub fn build_boot_catalog<D: AccountDirectoryStore>(
    directory: &D,
    credentials: &[AccountCredential],
) -> Result<Vec<u8>, BootCatalogBuildError> {
    let entries = directory.entries().map_err(BootCatalogBuildError::Directory)?;
    let raw_entries: Vec<globus_identity_core::catalog::DirectoryEntry> = entries
        .into_iter()
        .map(|entry| globus_identity_core::catalog::DirectoryEntry {
            username: entry.username.as_str().to_owned(),
            user_id: entry.user_id.as_str().to_owned(),
        })
        .collect();
    let directory_bytes = globus_identity_core::catalog::encode_directory(&raw_entries)
        .map_err(|_| BootCatalogBuildError::InvalidCatalog)?;
    let raw_credentials: Vec<(String, Vec<u8>)> = credentials
        .iter()
        .map(|credential| (credential.user_id.as_str().to_owned(), credential.record.clone()))
        .collect();
    globus_identity_core::catalog::encode_boot_catalog(&directory_bytes, &raw_credentials)
        .map_err(|_| BootCatalogBuildError::InvalidCatalog)
}
