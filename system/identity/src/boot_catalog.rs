//! Builds the two protected Ramdisk projections from the canonical account directory.
//!
//! GUA1 remains the username-to-UserId directory. GCR1 is a versioned collection
//! of existing ATID records keyed by full UserId. Neither is a persistent database.

use crate::{AccountDirectoryError, AccountDirectoryStore, UserId};

/// Existing ATID verifier bytes bound to one stable account identifier.
pub struct AccountCredential {
    /// Stable account identifier from the canonical directory.
    pub user_id: UserId,
    /// Exact existing 64-byte ATID record; never a plaintext password.
    pub record: Vec<u8>,
}

/// Ephemeral read-only GUA1 projection exported from AccountStateRepository.
/// This type is a build-time snapshot, not a persistent account store.
pub struct AccountDirectorySnapshot {
    entries: Vec<crate::AccountDirectoryEntry>,
}

impl AccountDirectorySnapshot {
    /// Parse a canonical GUA1 snapshot produced from the encrypted AccountStateRepository.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, BootCatalogBuildError> {
        let entries = globus_identity_core::catalog::parse_directory(bytes)
            .map_err(|_| BootCatalogBuildError::InvalidCatalog)?
            .into_iter()
            .map(|entry| {
                let username = crate::AccountUsername::parse(&entry.username)
                    .map_err(|_| BootCatalogBuildError::InvalidCatalog)?;
                let user_id = UserId::new(entry.user_id)
                    .map_err(|_| BootCatalogBuildError::InvalidCatalog)?;
                Ok(crate::AccountDirectoryEntry { username, user_id })
            })
            .collect::<Result<Vec<_>, BootCatalogBuildError>>()?;
        Ok(Self { entries })
    }
}

impl AccountDirectoryStore for AccountDirectorySnapshot {
    fn lookup(&self, username: &crate::AccountUsername) -> Result<UserId, AccountDirectoryError> {
        self.entries
            .iter()
            .find(|entry| &entry.username == username)
            .map(|entry| entry.user_id.clone())
            .ok_or(AccountDirectoryError::NotFound)
    }

    fn entries(&self) -> Result<Vec<crate::AccountDirectoryEntry>, AccountDirectoryError> {
        Ok(self.entries.clone())
    }
}

/// Two independent protected Ramdisk entries produced from one account snapshot.
pub struct BootCatalogImages {
    /// Existing GUA1 username-to-UserId directory bytes for `sys.accounts`.
    pub accounts: Vec<u8>,
    /// GCR1 credential records keyed by full UserId for `sys.credentials`.
    pub credentials: Vec<u8>,
}

/// Errors while projecting persistent identity state into the boot image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BootCatalogBuildError {
    /// The authoritative account directory could not be read.
    Directory(AccountDirectoryError),
    /// Directory and credential records could not be bound into a valid pair of catalogs.
    InvalidCatalog,
}

/// Validate and package already-exported GUA1 and GCR1 snapshots without rebuilding either catalog.
///
/// This is the direct path for ephemeral exports from AccountStateRepository.
pub fn build_boot_catalog_from_snapshots(
    accounts: &[u8],
    credentials: &[u8],
) -> Result<BootCatalogImages, BootCatalogBuildError> {
    globus_identity_core::catalog::parse_boot_catalog(accounts, credentials)
        .map_err(|_| BootCatalogBuildError::InvalidCatalog)?;
    Ok(BootCatalogImages {
        accounts: accounts.to_vec(),
        credentials: credentials.to_vec(),
    })
}

/// Build GUA1 + GCR1 from the canonical AccountDirectoryStore view.
///
/// Every directory entry must have exactly one ATID record keyed by its full UserId;
/// missing, duplicate, orphaned, malformed, or unsupported records are rejected.
pub fn build_boot_catalog<D: AccountDirectoryStore>(
    directory: &D,
    credentials: &[AccountCredential],
) -> Result<BootCatalogImages, BootCatalogBuildError> {
    let entries = directory
        .entries()
        .map_err(BootCatalogBuildError::Directory)?;
    let raw_entries: Vec<globus_identity_core::catalog::DirectoryEntry> = entries
        .into_iter()
        .map(|entry| globus_identity_core::catalog::DirectoryEntry {
            username: entry.username.as_str().to_owned(),
            user_id: entry.user_id.as_str().to_owned(),
        })
        .collect();
    let accounts = globus_identity_core::catalog::encode_directory(&raw_entries)
        .map_err(|_| BootCatalogBuildError::InvalidCatalog)?;
    let raw_credentials: Vec<(String, Vec<u8>)> = credentials
        .iter()
        .map(|credential| {
            (
                credential.user_id.as_str().to_owned(),
                credential.record.clone(),
            )
        })
        .collect();
    let credential_bytes =
        globus_identity_core::catalog::encode_credential_catalog(&raw_credentials)
            .map_err(|_| BootCatalogBuildError::InvalidCatalog)?;
    globus_identity_core::catalog::parse_boot_catalog(&accounts, &credential_bytes)
        .map_err(|_| BootCatalogBuildError::InvalidCatalog)?;
    Ok(BootCatalogImages {
        accounts,
        credentials: credential_bytes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use globus_identity_core::{HASH_LEN, KdfParams, RECORD_LEN, SALT_LEN, hash_into};

    fn atid(password: &[u8], salt_byte: u8) -> Vec<u8> {
        let params = KdfParams {
            memory_kib: 8 * 1024,
            iterations: 1,
            parallelism: 1,
        };
        let salt = [salt_byte; SALT_LEN];
        let mut hash = [0; HASH_LEN];
        hash_into(params, password, &salt, &mut hash).unwrap();
        let mut record = Vec::from(*b"ATID");
        record.push(1);
        record.push(1);
        record.extend_from_slice(&(params.memory_kib as u16).to_le_bytes());
        record.extend_from_slice(&params.iterations.to_le_bytes());
        record.push(params.parallelism as u8);
        record.extend_from_slice(&[0, 0, 0]);
        record.extend_from_slice(&salt);
        record.extend_from_slice(&hash);
        assert_eq!(record.len(), RECORD_LEN);
        record
    }

    #[test]
    fn account_state_directory_view_packages_two_separate_ramdisk_entries() {
        let directory_bytes = globus_identity_core::catalog::encode_directory(&[
            globus_identity_core::catalog::DirectoryEntry {
                username: "alice".into(),
                user_id: "id-alice".into(),
            },
            globus_identity_core::catalog::DirectoryEntry {
                username: "bob".into(),
                user_id: "id-bob".into(),
            },
        ])
        .unwrap();
        let directory = AccountDirectorySnapshot::from_bytes(&directory_bytes).unwrap();
        let credentials = vec![
            AccountCredential {
                user_id: UserId::new("id-bob").unwrap(),
                record: atid(b"bob-secret", 0x22),
            },
            AccountCredential {
                user_id: UserId::new("id-alice").unwrap(),
                record: atid(b"alice-secret", 0x11),
            },
        ];
        let images = build_boot_catalog(&directory, &credentials).unwrap();
        let parsed = globus_identity_core::catalog::parse_boot_catalog(
            &images.accounts,
            &images.credentials,
        )
        .unwrap();
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
    }

    #[test]
    fn repository_snapshots_are_validated_without_rebuilding_gcr1() {
        let directory_bytes = globus_identity_core::catalog::encode_directory(&[
            globus_identity_core::catalog::DirectoryEntry {
                username: "alice".into(),
                user_id: "id-alice".into(),
            },
        ]).unwrap();
        let credentials = vec![AccountCredential {
            user_id: UserId::new("id-alice").unwrap(),
            record: atid(b"alice-secret", 0x11),
        }];
        let generated = build_boot_catalog(
            &AccountDirectorySnapshot::from_bytes(&directory_bytes).unwrap(),
            &credentials,
        ).unwrap();
        let packaged = build_boot_catalog_from_snapshots(&generated.accounts, &generated.credentials).unwrap();
        assert_eq!(packaged.accounts, generated.accounts);
        assert_eq!(packaged.credentials, generated.credentials);
    }

    #[test]
    fn boot_packaging_rejects_missing_or_orphaned_user_credentials() {
        let directory_bytes = globus_identity_core::catalog::encode_directory(&[
            globus_identity_core::catalog::DirectoryEntry {
                username: "alice".into(),
                user_id: "id-alice".into(),
            },
        ])
        .unwrap();
        let directory = AccountDirectorySnapshot::from_bytes(&directory_bytes).unwrap();
        let credentials = vec![AccountCredential {
            user_id: UserId::new("id-bob").unwrap(),
            record: atid(b"bob-secret", 0x22),
        }];
        assert!(matches!(
            build_boot_catalog(&directory, &credentials),
            Err(BootCatalogBuildError::InvalidCatalog)
        ));
    }
}
