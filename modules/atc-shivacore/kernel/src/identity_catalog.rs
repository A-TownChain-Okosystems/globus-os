//! Kernel-owned read-only projection of the two protected multi-account Ramdisk entries.
//!
//! The GUA1 directory and GCR1 credential catalog are parsed by the same no_std
//! identity-core codec used by the host packer. Both must validate and cross-bind
//! completely before the catalog becomes visible to any kernel consumer.

use globus_identity_core::catalog::{BootAccount, BootAccountCatalog, CatalogError};
use spin::Mutex;

static CATALOG: Mutex<Option<BootAccountCatalog>> = Mutex::new(None);

/// Parse and publish both Ramdisk entries atomically. Any error clears the published snapshot.
pub fn publish(directory_bytes: &[u8], credential_bytes: &[u8]) -> Result<usize, CatalogError> {
    let parsed = match globus_identity_core::catalog::parse_boot_catalog(
        directory_bytes,
        credential_bytes,
    ) {
        Ok(parsed) => parsed,
        Err(error) => {
            *CATALOG.lock() = None;
            return Err(error);
        }
    };
    // A single KDF profile keeps real-account and dummy unknown-user work
    // comparable and prevents account-specific verifier-cost fingerprinting.
    if parsed
        .accounts()
        .iter()
        .any(|account| account.credential.params != globus_identity_core::KdfParams::OWNER_DEFAULT)
    {
        *CATALOG.lock() = None;
        return Err(CatalogError::InvalidCredential);
    }
    let count = parsed.len();
    *CATALOG.lock() = Some(parsed);
    Ok(count)
}

/// Number of accounts in the validated boot catalog, or zero when unavailable.
pub fn account_count() -> usize {
    CATALOG.lock().as_ref().map_or(0, BootAccountCatalog::len)
}

/// Whether a complete catalog was published successfully.
pub fn is_loaded() -> bool {
    CATALOG.lock().is_some()
}

/// Clone the immutable validated snapshot and release the spin lock immediately.
/// Callers must not hold the kernel catalog lock while running Argon2id or other slow work.
pub fn snapshot() -> Option<BootAccountCatalog> {
    CATALOG.lock().clone()
}

/// Clone one account entry so credential verification runs outside the spin lock.
pub fn account_for_username(username: &str) -> Option<BootAccount> {
    CATALOG.lock().as_ref()?.by_username(username).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::{string::String, vec::Vec};
    use globus_identity_core::catalog::{
        DirectoryEntry, encode_credential_catalog, encode_directory,
    };
    use globus_identity_core::{HASH_LEN, KdfParams, RECORD_LEN, SALT_LEN, hash_into};

    fn record(password: &[u8], salt_byte: u8) -> [u8; RECORD_LEN] {
        let params = KdfParams::OWNER_DEFAULT;
        let salt = [salt_byte; SALT_LEN];
        let mut hash = [0; HASH_LEN];
        hash_into(params, password, &salt, &mut hash).unwrap();
        let mut bytes = [0; RECORD_LEN];
        bytes[..4].copy_from_slice(b"ATID");
        bytes[4] = 1;
        bytes[5] = 1;
        bytes[6..8].copy_from_slice(&(params.memory_kib as u16).to_le_bytes());
        bytes[8..12].copy_from_slice(&params.iterations.to_le_bytes());
        bytes[12] = params.parallelism as u8;
        bytes[16..32].copy_from_slice(&salt);
        bytes[32..64].copy_from_slice(&hash);
        bytes
    }

    #[test]
    fn kernel_publishes_only_a_complete_two_entry_catalog() {
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
            (
                String::from("id-alice"),
                Vec::from(record(b"alice-secret", 0x11)),
            ),
            (
                String::from("id-bob"),
                Vec::from(record(b"bob-secret", 0x22)),
            ),
        ])
        .unwrap();
        assert_eq!(publish(&directory, &credentials), Ok(2));
        assert_eq!(account_count(), 2);
        assert!(is_loaded());
        let alice = account_for_username("alice").unwrap();
        let bob = account_for_username("bob").unwrap();
        assert_eq!(alice.user_id, "id-alice");
        assert_eq!(bob.credential.verify(b"bob-secret"), Ok(()));
        assert!(alice.credential.verify(b"bob-secret").is_err());
        assert_eq!(
            crate::identity_check::authenticate_catalog("alice", b"alice-secret"),
            Ok(String::from("id-alice"))
        );
        assert_eq!(
            crate::identity_check::check_catalog("alice", b"alice-secret"),
            crate::identity_check::CheckOutcome::Ok
        );
        assert_eq!(
            crate::identity_check::check_catalog("alice", b"bob-secret"),
            crate::identity_check::CheckOutcome::WrongPassword
        );
        assert_eq!(
            crate::identity_check::check_catalog("bob", b"bob-secret"),
            crate::identity_check::CheckOutcome::Ok
        );
        assert_eq!(
            crate::identity_check::check_catalog("unknown", b"alice-secret"),
            crate::identity_check::CheckOutcome::WrongPassword
        );
        assert_eq!(snapshot().unwrap().len(), 2);
    }

    #[test]
    fn invalid_cross_reference_is_rejected_by_shared_codec() {
        let directory = encode_directory(&[DirectoryEntry {
            username: String::from("alice"),
            user_id: String::from("id-alice"),
        }])
        .unwrap();
        let credentials = encode_credential_catalog(&[(
            String::from("id-bob"),
            Vec::from(record(b"bob-secret", 0x22)),
        )])
        .unwrap();
        assert_eq!(
            globus_identity_core::catalog::parse_boot_catalog(&directory, &credentials),
            Err(CatalogError::MissingCredential)
        );
    }
}
