//! Durable identity persistence contracts and a deterministic in-memory backend.
//!
//! The in-memory backend is intentionally test-only infrastructure. Production storage must
//! provide confidentiality, integrity, atomic replacement, rollback protection, and secure
//! deletion through the OS security boundary.

use crate::AccountProfile;

/// Aktuelle Version des Identitaetsdatensatzes.
pub const IDENTITY_RECORD_VERSION: u16 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
/// Passwort-Verifier ohne Klartext.
pub struct CredentialRecord {
    /// Schema des Verifiers, heute nur argon2id.
    pub scheme: &'static str,
    /// Version des Schemas.
    /// Version des Datensatzes.
    pub version: u16,
    /// PHC-String des Hashes.
    pub phc_hash: String,
}

impl CredentialRecord {
    /// Baut einen argon2id-Verifier aus einem PHC-String.
    pub fn argon2id(phc_hash: impl Into<String>) -> Self {
        Self {
            scheme: "argon2id",
            version: 1,
            phc_hash: phc_hash.into(),
        }
    }

    /// True, wenn Schema, Version und Hash gesetzt und bekannt sind.
    pub fn is_supported(&self) -> bool {
        self.scheme == "argon2id" && self.version == 1 && !self.phc_hash.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Persistierter Identitaetsdatensatz aus Profil und Verifier.
pub struct IdentityRecord {
    /// Version des Datensatzes.
    pub version: u16,
    /// Profil des Kontos.
    pub profile: AccountProfile,
    /// Verifier fuer die Anmeldung.
    pub credential: CredentialRecord,
}

impl IdentityRecord {
    /// Legt einen Datensatz in der aktuellen Version an.
    /// Leerer Speicher.
    pub fn new(profile: AccountProfile, credential: CredentialRecord) -> Self {
        Self {
            version: IDENTITY_RECORD_VERSION,
            profile,
            credential,
        }
    }

    /// Prueft Version, Verifier und die Uebereinstimmung von Profil und Bindung.
    pub fn validate(&self) -> Result<(), PersistenceError> {
        if self.version != IDENTITY_RECORD_VERSION {
            return Err(PersistenceError::UnsupportedVersion(self.version));
        }
        if !self.credential.is_supported() {
            return Err(PersistenceError::UnsupportedCredentialScheme);
        }
        if self.profile.user_id != self.profile.binding.user_id
            || self.profile.wallet_address != self.profile.binding.wallet_address
        {
            return Err(PersistenceError::Corrupt);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Fehler der Identitaets-Ablage.
pub enum PersistenceError {
    /// Es ist kein Datensatz gespeichert.
    NotFound,
    /// Der Datensatz wurde zwischenzeitlich geaendert.
    Conflict,
    /// Der Datensatz ist inhaltlich inkonsistent.
    Corrupt,
    /// Die Datensatzversion wird nicht unterstuetzt.
    UnsupportedVersion(u16),
    /// Das Verifier-Schema wird nicht unterstuetzt.
    UnsupportedCredentialScheme,
    /// Das Speicher-Backend ist nicht erreichbar.
    BackendUnavailable,
    /// Die Integritaetspruefung ist fehlgeschlagen.
    IntegrityFailure,
    /// Dateisystemfehler.
    Io,
}

/// Durable identity storage. Implementations must provide atomic writes.
pub trait IdentityStore {
    /// Laedt den Datensatz, falls einer existiert.
    fn load(&self) -> Result<Option<IdentityRecord>, PersistenceError>;
    /// Schreibt den Datensatz atomar.
    fn save(&mut self, record: &IdentityRecord) -> Result<(), PersistenceError>;
    /// Loescht den Datensatz.
    fn delete(&mut self) -> Result<(), PersistenceError>;
}

/// Test-only storage implementation. It models atomic replacement and rejects invalid
/// records before changing the committed value. It must never be used as production storage.
#[derive(Debug, Default)]
pub struct InMemoryIdentityStore {
    committed: Option<IdentityRecord>,
}

impl InMemoryIdentityStore {
    /// Leerer Speicher.
    pub fn new() -> Self {
        Self::default()
    }

    /// True, solange nichts gespeichert ist.
    pub fn is_empty(&self) -> bool {
        self.committed.is_none()
    }
}

impl IdentityStore for InMemoryIdentityStore {
    fn load(&self) -> Result<Option<IdentityRecord>, PersistenceError> {
        Ok(self.committed.clone())
    }

    fn save(&mut self, record: &IdentityRecord) -> Result<(), PersistenceError> {
        record.validate()?;
        if self.committed.is_some() {
            return Err(PersistenceError::Conflict);
        }
        self.committed = Some(record.clone());
        Ok(())
    }

    fn delete(&mut self) -> Result<(), PersistenceError> {
        self.committed = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{IdentityBinding, UserId, WalletAddress};

    fn record() -> IdentityRecord {
        let user = UserId::new("test-user").unwrap();
        let address = WalletAddress("ATC00000000000000000000000000000000AA".into());
        let binding = IdentityBinding {
            user_id: user.clone(),
            wallet_address: address.clone(),
            chain_id: 600,
            network: "devnet".into(),
            key_version: 1,
        };
        let profile = AccountProfile::new(user, "Test", binding).unwrap();
        IdentityRecord::new(
            profile,
            CredentialRecord::argon2id("$argon2id$v=19$m=19456,t=2,p=1$test$hash"),
        )
    }

    #[test]
    fn record_validates_supported_schema() {
        assert!(record().validate().is_ok());
    }

    #[test]
    fn unknown_version_is_rejected() {
        let mut value = record();
        value.version = 99;
        assert_eq!(
            value.validate(),
            Err(PersistenceError::UnsupportedVersion(99))
        );
    }

    #[test]
    fn wrong_credential_scheme_is_rejected() {
        let mut value = record();
        value.credential.scheme = "sha256";
        assert_eq!(
            value.validate(),
            Err(PersistenceError::UnsupportedCredentialScheme)
        );
    }

    #[test]
    fn corrupt_binding_is_rejected_before_commit() {
        let mut value = record();
        value.profile.wallet_address = WalletAddress("ATCBAD".into());
        let mut store = InMemoryIdentityStore::new();
        assert_eq!(store.save(&value), Err(PersistenceError::Corrupt));
        assert!(store.is_empty());
    }

    #[test]
    fn save_load_delete_round_trip() {
        let value = record();
        let mut store = InMemoryIdentityStore::new();
        store.save(&value).unwrap();
        assert_eq!(store.load().unwrap(), Some(value));
        assert_eq!(store.save(&record()), Err(PersistenceError::Conflict));
        store.delete().unwrap();
        assert!(store.is_empty());
        assert_eq!(store.load().unwrap(), None);
    }
}
