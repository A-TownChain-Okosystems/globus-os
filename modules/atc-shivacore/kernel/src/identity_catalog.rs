//! Kernel-owned, read-only projection of the multi-account boot catalog.
//!
//! The catalog is parsed by the same no_std identity-core codec as the host
//! packer. No account directory or credential database is created in kernel
//! memory; this is a validated boot-time snapshot only.

use globus_identity_core::catalog::{BootAccountCatalog, CatalogError};
use spin::Mutex;

static CATALOG: Mutex<Option<BootAccountCatalog>> = Mutex::new(None);

/// Parse and publish the complete catalog atomically. A failed parse leaves it unavailable.
pub fn publish(bytes: &[u8]) -> Result<usize, CatalogError> {
    let parsed = match globus_identity_core::catalog::parse_boot_catalog(bytes) {
        Ok(parsed) => parsed,
        Err(error) => {
            *CATALOG.lock() = None;
            return Err(error);
        }
    };
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

/// Execute a read-only callback while holding the catalog lock.
pub fn with_catalog<R>(callback: impl FnOnce(Option<&BootAccountCatalog>) -> R) -> R {
    let guard = CATALOG.lock();
    callback(guard.as_ref())
}

#[cfg(test)]
mod tests {
    use super::*;
    use globus_identity_core::catalog::{encode_boot_catalog, encode_directory, DirectoryEntry};
    use globus_identity_core::{hash_into, KdfParams, HASH_LEN, RECORD_LEN, SALT_LEN};
    use alloc::{string::String, vec, vec::Vec};

    fn record(password: &[u8]) -> [u8; RECORD_LEN] {
        let params = KdfParams { memory_kib: 8 * 1024, iterations: 1, parallelism: 1 };
        let salt = [0x51; SALT_LEN];
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
    fn kernel_publishes_only_a_complete_validated_multi_account_catalog() {
        let directory = encode_directory(&[
            DirectoryEntry { username: String::from("alice"), user_id: String::from("id-alice") },
            DirectoryEntry { username: String::from("bob"), user_id: String::from("id-bob") },
        ]).unwrap();
        let catalog = encode_boot_catalog(&directory, &[
            (String::from("id-alice"), Vec::from(record(b"alice-secret"))),
            (String::from("id-bob"), Vec::from(record(b"bob-secret"))),
        ]).unwrap();
        assert_eq!(publish(&catalog), Ok(2));
        assert_eq!(account_count(), 2);
        assert!(is_loaded());
        with_catalog(|catalog| {
            let catalog = catalog.unwrap();
            assert_eq!(catalog.by_username("alice").unwrap().user_id, "id-alice");
            assert_eq!(catalog.by_username("bob").unwrap().credential.verify(b"bob-secret"), Ok(()));
            assert!(catalog.by_username("alice").unwrap().credential.verify(b"bob-secret").is_err());
        });
    }

    #[test]
    fn invalid_catalog_is_rejected_without_publishing_partial_state() {
        let invalid = b"GAC1";
        assert_eq!(publish(invalid), Err(CatalogError::Truncated));
    }
}
