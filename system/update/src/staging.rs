//! Fail-closed update staging boundary for verified package bytes.
//!
//! This module deliberately defines the storage contract, not a fake production
//! disk driver. A concrete platform adapter must implement this trait and bind
//! it to the real inactive-slot write/readback/atomic boot-selection path.
//! Callers cannot pass raw download bytes: staging accepts only VerifiedArtifact.

use crate::Slot;
use globus_package::VerifiedArtifact;

/// Platform storage contract. Implementations must write only the requested
/// inactive slot and make activation an atomic boot-selection change.
pub trait UpdateSlotStorage {
    type Error;

    /// Return the currently selected boot slot.
    fn active_slot(&self) -> Slot;

    /// Write the supplied bytes to the target slot without activating it.
    fn write_inactive_slot(&mut self, slot: Slot, bytes: &[u8]) -> Result<(), Self::Error>;

    /// Read back the complete slot contents after the write has completed.
    fn read_slot(&mut self, slot: Slot) -> Result<Vec<u8>, Self::Error>;

    /// Atomically select the already-written slot for the next boot.
    fn activate_slot(&mut self, slot: Slot) -> Result<(), Self::Error>;
}

#[derive(Debug, PartialEq, Eq)]
pub enum StagingError<E> {
    TargetIsActive,
    Write(E),
    ReadBack(E),
    ReadBackMismatch,
    Activate(E),
}

/// Write, read back, compare byte-for-byte, and only then activate a verified
/// update. The same owned byte buffer is used for verification result, write,
/// and comparison; the untrusted download source is never reopened here.
///
/// This function is a safe integration boundary. Release readiness still
/// requires a concrete platform implementation of UpdateSlotStorage and tests
/// against its real storage and boot-selection mechanisms.
pub fn stage_and_activate<S: UpdateSlotStorage>(
    storage: &mut S,
    target: Slot,
    artifact: &VerifiedArtifact,
) -> Result<(), StagingError<S::Error>> {
    if target == storage.active_slot() {
        return Err(StagingError::TargetIsActive);
    }

    let verified_bytes = artifact.bytes();
    storage
        .write_inactive_slot(target, verified_bytes)
        .map_err(StagingError::Write)?;

    let written_bytes = storage.read_slot(target).map_err(StagingError::ReadBack)?;
    if written_bytes.as_slice() != verified_bytes {
        return Err(StagingError::ReadBackMismatch);
    }

    storage
        .activate_slot(target)
        .map_err(StagingError::Activate)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use sha2::{Digest, Sha256};
    use std::collections::HashMap;

    #[derive(Default)]
    struct MemorySlots {
        active: Slot,
        slots: HashMap<SlotKey, Vec<u8>>,
        tamper_after_write: bool,
        fail_write: bool,
        activations: usize,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    struct SlotKey(u8);

    impl From<Slot> for SlotKey {
        fn from(slot: Slot) -> Self {
            Self(match slot { Slot::A => 0, Slot::B => 1 })
        }
    }

    impl MemorySlots {
        fn new() -> Self {
            Self { active: Slot::A, ..Self::default() }
        }
    }

    impl UpdateSlotStorage for MemorySlots {
        type Error = &'static str;

        fn active_slot(&self) -> Slot { self.active }

        fn write_inactive_slot(&mut self, slot: Slot, bytes: &[u8]) -> Result<(), Self::Error> {
            if self.fail_write {
                return Err("write failed");
            }
            self.slots.insert(slot.into(), bytes.to_vec());
            if self.tamper_after_write {
                self.slots.get_mut(&slot.into()).unwrap().push(0xff);
            }
            Ok(())
        }

        fn read_slot(&mut self, slot: Slot) -> Result<Vec<u8>, Self::Error> {
            self.slots.get(&slot.into()).cloned().ok_or("slot empty")
        }

        fn activate_slot(&mut self, slot: Slot) -> Result<(), Self::Error> {
            self.active = slot;
            self.activations += 1;
            Ok(())
        }
    }

    fn verified_artifact(bytes: &[u8]) -> VerifiedArtifact {
        let key = SigningKey::from_bytes(&[31_u8; 32]);
        let mut package = globus_package::Package {
            name: "system-update".into(),
            version: "2.0.0".into(),
            digest: format!("sha256:{}", hex::encode(Sha256::digest(bytes))),
            signature: String::new(),
        };
        let mut message = b"GLOBUSOS-PACKAGE-SIGNATURE-V1\0".to_vec();
        message.extend_from_slice(package.name.as_bytes());
        message.push(0);
        message.extend_from_slice(package.version.as_bytes());
        message.push(0);
        message.extend_from_slice(package.digest.as_bytes());
        package.signature = hex::encode(key.sign(&message).to_bytes());

        let mut keyring = globus_package::TrustedKeyring::new();
        keyring.trust_key(key.verifying_key().to_bytes());
        globus_package::verify_for_update(&package, bytes, &keyring).unwrap()
    }

    #[test]
    fn activates_only_after_exact_readback_of_verified_bytes() {
        let bytes = b"exact verified image";
        let artifact = verified_artifact(bytes);
        let mut storage = MemorySlots::new();

        assert_eq!(stage_and_activate(&mut storage, Slot::B, &artifact), Ok(()));
        assert_eq!(storage.slots.get(&SlotKey(1)).unwrap().as_slice(), bytes);
        assert_eq!(storage.active, Slot::B);
        assert_eq!(storage.activations, 1);
    }

    #[test]
    fn does_not_activate_when_readback_differs() {
        let artifact = verified_artifact(b"verified image");
        let mut storage = MemorySlots { tamper_after_write: true, ..MemorySlots::new() };

        assert_eq!(
            stage_and_activate(&mut storage, Slot::B, &artifact),
            Err(StagingError::ReadBackMismatch)
        );
        assert_eq!(storage.active, Slot::A);
        assert_eq!(storage.activations, 0);
    }

    #[test]
    fn does_not_activate_when_write_fails() {
        let artifact = verified_artifact(b"verified image");
        let mut storage = MemorySlots { fail_write: true, ..MemorySlots::new() };

        assert_eq!(
            stage_and_activate(&mut storage, Slot::B, &artifact),
            Err(StagingError::Write("write failed"))
        );
        assert_eq!(storage.active, Slot::A);
        assert_eq!(storage.activations, 0);
    }

    #[test]
    fn refuses_to_overwrite_the_active_slot() {
        let artifact = verified_artifact(b"verified image");
        let mut storage = MemorySlots::new();

        assert_eq!(
            stage_and_activate(&mut storage, Slot::A, &artifact),
            Err(StagingError::TargetIsActive)
        );
        assert_eq!(storage.activations, 0);
    }
}
