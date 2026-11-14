//! Local wallet keystore boundary.
//!
//! This module intentionally defines the contract before selecting a platform-specific
//! encrypted storage implementation. Secret bytes must never be persisted through the
//! generic account/profile layer.

use zeroize::Zeroizing;

/// Stable identifier for a locally protected wallet record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyId(String);

impl KeyId {
    /// Akzeptiert eine nicht leere Kennung mit hoechstens 128 Zeichen.
    pub fn new(value: impl Into<String>) -> Result<Self, KeystoreError> {
        let value = value.into();
        if value.is_empty() || value.len() > 128 {
            return Err(KeystoreError::InvalidKeyId);
        }
        Ok(Self(value))
    }

    /// Kennung als Text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Metadata that may be persisted without secret material.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyMetadata {
    /// Kennung des Schluessels.
    pub key_id: KeyId,
    /// Algorithmus, an den der Schluessel gebunden ist.
    pub algorithm: String,
    /// Version der Metadaten fuer Migrationen.
    pub version: u16,
    /// True, wenn der Schluessel in Hardware liegt.
    pub hardware_backed: bool,
}

/// A protected private-key handle. The actual key bytes remain inside the keystore.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtectedKey {
    /// Oeffentliche Metadaten des Schluessels.
    pub metadata: KeyMetadata,
}

/// One-shot signing request. The keystore owns the private key and returns only a signature.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignRequest {
    /// Kennung des Schluessels fuer diesen Auftrag.
    pub key_id: KeyId,
    /// Zweck, an den die Signatur gebunden ist.
    pub domain: String,
    /// Zu signierende Bytes.
    pub message: Vec<u8>,
}

/// Signature result returned from a protected signing backend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signature {
    /// Algorithmus der Signatur.
    pub algorithm: String,
    /// Signaturbytes.
    pub bytes: Vec<u8>,
}

/// Platform keystore interface.
///
/// Production implementations should map this to TPM/TEE/OS credential facilities where
/// available. A backend must reject unauthorized access and must never export private keys.
pub trait KeyStore {
    /// Importiert ein Geheimnis und gibt das geschuetzte Handle zurueck.
    fn provision(
        &mut self,
        key_id: KeyId,
        secret: Zeroizing<Vec<u8>>,
        algorithm: String,
    ) -> Result<ProtectedKey, KeystoreError>;
    /// Signiert mit dem gespeicherten Schluessel; der Schluessel verlaesst das Backend nie.
    fn sign(&self, request: SignRequest) -> Result<Signature, KeystoreError>;
    /// Entfernt den Schluessel.
    fn delete(&mut self, key_id: &KeyId) -> Result<(), KeystoreError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Fehler der Keystore-Grenze.
pub enum KeystoreError {
    /// Die Kennung ist leer oder zu lang.
    InvalidKeyId,
    /// Der Aufrufer ist nicht berechtigt.
    Unauthorized,
    /// Der Schluessel existiert nicht.
    NotFound,
    /// Der Auftrag ist unbrauchbar.
    InvalidRequest,
    /// Das Backend kann diese Operation nicht.
    Unsupported,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_id_is_validated() {
        assert_eq!(KeyId::new("").unwrap_err(), KeystoreError::InvalidKeyId);
        assert_eq!(
            KeyId::new("wallet-primary").unwrap().as_str(),
            "wallet-primary"
        );
    }
}
