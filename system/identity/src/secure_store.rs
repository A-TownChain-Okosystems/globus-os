//! Authenticated encryption boundary for durable identity blobs.
//!
//! This module deliberately does not implement a filesystem or hardware key store. The key
//! provider is an explicit dependency so production wiring can bind it to ShivaCore/TPM/TEE.

use chacha20poly1305::{aead::{Aead, OsRng}, AeadCore, ChaCha20Poly1305, KeyInit};
use zeroize::Zeroizing;

const MAGIC: &[u8; 4] = b"GID1";
const FORMAT_VERSION: u8 = 1;
const NONCE_LEN: usize = 12;

pub trait IdentityKeyProvider {
    fn load_key(&self) -> Result<Zeroizing<[u8; 32]>, SecureStoreError>;
}

pub trait BlobStore {
    fn load(&self) -> Result<Option<Vec<u8>>, SecureStoreError>;
    fn replace(&mut self, blob: &[u8]) -> Result<(), SecureStoreError>;
    fn delete(&mut self) -> Result<(), SecureStoreError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecureStoreError {
    KeyUnavailable,
    EncryptionFailed,
    DecryptionFailed,
    InvalidEnvelope,
    IntegrityFailure,
    Conflict,
    BackendUnavailable,
}

/// Encrypts an opaque identity record using XChaCha20-Poly1305.
///
/// The encrypted blob contains only a format marker, version, random nonce and ciphertext.
/// Key material is never serialized into the blob.
pub struct EncryptedBlobStore<B, K> {
    backend: B,
    keys: K,
}

impl<B, K> EncryptedBlobStore<B, K> {
    pub fn new(backend: B, keys: K) -> Self { Self { backend, keys } }

    pub fn into_parts(self) -> (B, K) { (self.backend, self.keys) }
}

impl<B: BlobStore, K: IdentityKeyProvider> EncryptedBlobStore<B, K> {
    pub fn load_plaintext(&self) -> Result<Option<Zeroizing<Vec<u8>>>, SecureStoreError> {
        let Some(blob) = self.backend.load()? else { return Ok(None); };
        decrypt(&blob, &self.keys)
    }

    pub fn replace_plaintext(&mut self, plaintext: &[u8]) -> Result<(), SecureStoreError> {
        let blob = encrypt(plaintext, &self.keys)?;
        self.backend.replace(&blob)
    }

    pub fn delete(&mut self) -> Result<(), SecureStoreError> {
        self.backend.delete()
    }
}

fn encrypt<K: IdentityKeyProvider>(plaintext: &[u8], keys: &K) -> Result<Vec<u8>, SecureStoreError> {
    let key = keys.load_key()?;
    let cipher = ChaCha20Poly1305::new_from_slice(&*key).map_err(|_| SecureStoreError::KeyUnavailable)?;
    let nonce = ChaCha20Poly1305::generate_nonce(&mut OsRng);
    let ciphertext = cipher.encrypt(&nonce, plaintext).map_err(|_| SecureStoreError::EncryptionFailed)?;
    let mut envelope = Vec::with_capacity(MAGIC.len() + 1 + NONCE_LEN + ciphertext.len());
    envelope.extend_from_slice(MAGIC);
    envelope.push(FORMAT_VERSION);
    envelope.extend_from_slice(&nonce);
    envelope.extend_from_slice(&ciphertext);
    Ok(envelope)
}

fn decrypt<K: IdentityKeyProvider>(blob: &[u8], keys: &K) -> Result<Option<Zeroizing<Vec<u8>>>, SecureStoreError> {
    if blob.len() < MAGIC.len() + 1 + NONCE_LEN + 16 { return Err(SecureStoreError::InvalidEnvelope); }
    if &blob[..4] != MAGIC || blob[4] != FORMAT_VERSION { return Err(SecureStoreError::InvalidEnvelope); }
    let nonce = &blob[5..5 + NONCE_LEN];
    let ciphertext = &blob[5 + NONCE_LEN..];
    let key = keys.load_key()?;
    let cipher = ChaCha20Poly1305::new_from_slice(&*key).map_err(|_| SecureStoreError::KeyUnavailable)?;
    let plaintext = cipher.decrypt(nonce.into(), ciphertext).map_err(|_| SecureStoreError::DecryptionFailed)?;
    Ok(Some(Zeroizing::new(plaintext)))
}

#[derive(Debug, Default)]
pub struct InMemoryBlobStore { blob: Option<Vec<u8>> }

impl BlobStore for InMemoryBlobStore {
    fn load(&self) -> Result<Option<Vec<u8>>, SecureStoreError> { Ok(self.blob.clone()) }

    fn replace(&mut self, blob: &[u8]) -> Result<(), SecureStoreError> {
        self.blob = Some(blob.to_vec());
        Ok(())
    }

    fn delete(&mut self) -> Result<(), SecureStoreError> {
        if let Some(mut blob) = self.blob.take() { zeroize_bytes(&mut blob); }
        Ok(())
    }
}

fn zeroize_bytes(bytes: &mut [u8]) {
    for byte in bytes { *byte = 0; }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestKey(Zeroizing<[u8; 32]>);
    impl IdentityKeyProvider for TestKey {
        fn load_key(&self) -> Result<Zeroizing<[u8; 32]>, SecureStoreError> { Ok(self.0.clone()) }
    }

    fn store() -> EncryptedBlobStore<InMemoryBlobStore, TestKey> {
        EncryptedBlobStore::new(InMemoryBlobStore::default(), TestKey(Zeroizing::new([7u8; 32])))
    }

    #[test]
    fn round_trip_decrypts() {
        let mut store = store();
        store.replace_plaintext(b"identity-record").unwrap();
        assert_eq!(&*store.load_plaintext().unwrap().unwrap(), b"identity-record");
    }

    #[test]
    fn random_nonce_produces_distinct_ciphertexts() {
        let mut first = store();
        let mut second = store();
        first.replace_plaintext(b"same").unwrap();
        second.replace_plaintext(b"same").unwrap();
        let (backend_a, _) = first.into_parts();
        let (backend_b, _) = second.into_parts();
        assert_ne!(backend_a.load().unwrap(), backend_b.load().unwrap());
    }

    #[test]
    fn tampering_is_rejected() {
        let mut store = store();
        store.replace_plaintext(b"identity-record").unwrap();
        let (mut backend, keys) = store.into_parts();
        let mut blob = backend.load().unwrap().unwrap();
        *blob.last_mut().unwrap() ^= 1;
        backend.replace(&blob).unwrap();
        let store = EncryptedBlobStore::new(backend, keys);
        assert_eq!(store.load_plaintext().unwrap_err(), SecureStoreError::DecryptionFailed);
    }

    #[test]
    fn wrong_key_is_rejected() {
        let mut store = store();
        store.replace_plaintext(b"identity-record").unwrap();
        let (backend, _) = store.into_parts();
        let wrong = TestKey(Zeroizing::new([8u8; 32]));
        let store = EncryptedBlobStore::new(backend, wrong);
        assert_eq!(store.load_plaintext().unwrap_err(), SecureStoreError::DecryptionFailed);
    }

    #[test]
    fn delete_removes_blob() {
        let mut store = store();
        store.replace_plaintext(b"identity-record").unwrap();
        store.delete().unwrap();
        assert_eq!(store.load_plaintext().unwrap(), None);
    }
}
