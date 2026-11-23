//! GlobusOS identity and wallet boundary.

pub mod account_state;

mod account_lifecycle;
mod account_registry;
mod boot_catalog;
mod entropy;
mod file_store;
mod ipc_key_provider;
mod key_provider;
mod keystore;
mod multi_account;
mod persistence;
mod secure_store;
mod service;
/// Journal fuer mehrstufige Identitaets-Transaktionen.
pub mod transaction_journal;

pub use account_lifecycle::{AccountLifecycleError, AccountUsername, RecoveryConfirmation};
#[allow(deprecated)]
pub use account_registry::{
    AccountDirectoryEntry, AccountDirectoryError, AccountDirectoryStore, FileAccountDirectory,
    MutableAccountDirectoryStore,
};
pub use account_state::{
    AccountState, AccountStateEntry, AccountStateError, AccountStateRepository,
};
use bip39::{Language, Mnemonic};
pub use boot_catalog::{
    AccountCredential, AccountDirectorySnapshot, BootCatalogBuildError, BootCatalogImages,
    build_boot_catalog,
    build_boot_catalog_from_snapshots,
};
pub use entropy::{EntropyGate, OsEntropy};
pub use file_store::FileBlobStore;
use hmac::{Hmac, Mac};
pub use ipc_key_provider::{IdentityKeyIpcTransport, IpcIdentityKeyService};
pub use key_provider::{
    SecureKeyId, SecureKeyService, SecureKeyServiceError, ShivaCoreKeyProvider,
};
pub use keystore::{
    KeyId, KeyMetadata, KeyStore, KeystoreError, ProtectedKey, SignRequest, Signature,
};
pub use multi_account::{MultiAccountError, MultiAccountIdentityService};
pub use persistence::{
    CredentialRecord, IDENTITY_RECORD_VERSION, IdentityRecord, IdentityStore,
    InMemoryIdentityStore, PersistenceError,
};
pub use secure_store::{
    BlobStore, EncryptedBlobStore, IdentityKeyProvider, InMemoryBlobStore, SecureStoreError,
};
pub use service::{IdentityService, IdentityServiceError, LoginResult, RegistrationResult};
use sha2::{Digest, Sha256};
use zeroize::{Zeroize, Zeroizing};

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone, PartialEq, Eq)]
/// Stabiler Benutzerbezeichner mit 1 bis 128 Zeichen.
pub struct UserId(String);
impl UserId {
    /// Nimmt einen nicht leeren Bezeichner mit hoechstens 128 Zeichen an.
    pub fn new(value: impl Into<String>) -> Result<Self, IdentityError> {
        let value = value.into();
        if value.is_empty() || value.len() > 128 {
            return Err(IdentityError::InvalidUserId);
        }
        Ok(Self(value))
    }
    /// Bezeichner als Text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
/// Adresse des Wallets auf der Zielkette.
pub struct WalletAddress(String);
impl WalletAddress {
    /// Adresse als Text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
/// Bindung einer Wallet an Benutzer, Kette und Netzwerk.
pub struct IdentityBinding {
    /// Benutzer, dem die Wallet gehoert.
    pub user_id: UserId,
    /// Adresse der Wallet.
    pub wallet_address: WalletAddress,
    /// Kette, auf der die Wallet gilt.
    pub chain_id: u64,
    /// Netzwerkname, etwa mainnet oder devnet; darf nicht leer sein.
    pub network: String,
    /// Version des abgeleiteten Schluessels.
    pub key_version: u16,
}
#[derive(Debug, Clone, PartialEq, Eq)]
/// Profil eines Kontos, ohne jedes Geheimnis.
pub struct AccountProfile {
    /// Benutzer des Profils.
    pub user_id: UserId,
    /// Anzeigename mit 1 bis 128 Zeichen.
    pub display_name: String,
    /// Adresse aus der Bindung.
    pub wallet_address: WalletAddress,
    /// Bindung, aus der das Profil abgeleitet ist.
    pub binding: IdentityBinding,
}
impl AccountProfile {
    /// Prueft Anzeigename und verlangt, dass die Bindung zum Benutzer passt.
    pub fn new(
        user_id: UserId,
        display_name: impl Into<String>,
        binding: IdentityBinding,
    ) -> Result<Self, IdentityError> {
        let display_name = display_name.into();
        if display_name.is_empty() || display_name.len() > 128 {
            return Err(IdentityError::InvalidDisplayName);
        }
        if binding.user_id != user_id {
            return Err(IdentityError::BindingMismatch);
        }
        Ok(Self {
            user_id,
            display_name,
            wallet_address: binding.wallet_address.clone(),
            binding,
        })
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Anmeldezustand einer Sitzung.
pub enum LoginState {
    /// Keine Sitzung offen.
    LoggedOut,
    /// Anmeldung laeuft.
    Authenticating,
    /// Sitzung ist gueltig.
    Authenticated,
    /// Sitzung gesperrt, erneute Anmeldung noetig.
    Locked,
}
#[derive(Debug, Clone, PartialEq, Eq)]
/// Sitzung eines angemeldeten Benutzers.
pub struct IdentitySession {
    /// Angemeldeter Benutzer.
    pub user_id: UserId,
    /// Wallet der Sitzung.
    pub wallet_address: WalletAddress,
    /// Aktueller Anmeldezustand.
    pub state: LoginState,
    /// Zeitpunkt der Ausstellung in Unix-Sekunden.
    pub issued_at_unix: u64,
    /// Ablaufzeitpunkt in Unix-Sekunden.
    pub expires_at_unix: u64,
}
impl IdentitySession {
    /// True, solange die Sitzung angemeldet ist und `now_unix` vor dem Ablauf liegt.
    pub fn is_active(&self, now_unix: u64) -> bool {
        self.state == LoginState::Authenticated && now_unix < self.expires_at_unix
    }
    /// Sperrt die Sitzung.
    pub fn lock(&mut self) {
        self.state = LoginState::Locked;
    }
}
#[derive(Debug)]
/// Seed-Material fuer die Wiederherstellung; wird beim Drop genullt.
pub struct RecoveryMaterial {
    seed: Zeroizing<Vec<u8>>,
}
impl RecoveryMaterial {
    /// Nimmt einen Seed mit mindestens 32 Byte an.
    pub fn from_seed(seed: Vec<u8>) -> Result<Self, IdentityError> {
        if seed.len() < 32 {
            return Err(IdentityError::WeakSeed);
        }
        Ok(Self {
            seed: Zeroizing::new(seed),
        })
    }
    /// Laenge des Seeds in Byte.
    pub fn seed_len(&self) -> usize {
        self.seed.len()
    }
}
impl Drop for RecoveryMaterial {
    fn drop(&mut self) {
        self.seed.zeroize();
    }
}
/// 24-Wort-Wiederherstellungsphrase; der Text wird beim Drop genullt.
pub struct RecoveryPhrase(Zeroizing<String>);
impl RecoveryPhrase {
    /// Gibt die Phrase im Klartext zurueck, nur fuer die Anzeige an den Nutzer.
    pub fn expose(&self) -> &str {
        &self.0
    }
    /// Anzahl der Woerter in der Phrase.
    pub fn word_count(&self) -> usize {
        self.0.split_whitespace().count()
    }
}
/// Ergebnis der Wallet-Erzeugung.
pub struct WalletCreation {
    /// Phrase, die der Nutzer sichern muss.
    pub recovery_phrase: RecoveryPhrase,
    /// Abgeleitete Wallet-Adresse.
    pub wallet_address: WalletAddress,
    /// Bindung aus Benutzer, Kette und Netzwerk.
    pub binding: IdentityBinding,
}
impl WalletCreation {
    /// Phrase als Text.
    pub fn recovery_phrase(&self) -> &str {
        self.recovery_phrase.expose()
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
/// Fehler der Identitaets- und Wallet-Grenze.
pub enum IdentityError {
    /// Der Benutzerbezeichner ist leer oder zu lang.
    InvalidUserId,
    /// Der Anzeigename ist leer oder zu lang.
    InvalidDisplayName,
    /// Die Bindung gehoert zu einem anderen Benutzer.
    BindingMismatch,
    /// Der Seed hat weniger als 32 Byte.
    WeakSeed,
    /// Die Adresse ist unbrauchbar.
    InvalidWalletAddress,
    /// Der Netzwerkname ist leer.
    EmptyNetwork,
    /// Die Phrase ist keine gueltige BIP-39-Phrase.
    InvalidMnemonic,
    /// Die Ableitung von Schluessel oder Adresse ist fehlgeschlagen.
    WalletDerivationFailed,
    /// Die Zufallsquelle ist nicht bereit (REQ-REC-13, fail-closed).
    ///
    /// Kein Konto, keine Phrase: im Image darf ohne bestandenen Selbsttest
    /// keine Phrase entstehen.
    EntropyUnavailable,
}
/// Erzeugt eine 24-Wort-Phrase und leitet daraus die Wallet-Adresse ab.
pub fn create_wallet(
    user_id: UserId,
    chain_id: u64,
    network: impl Into<String>,
    key_version: u16,
) -> Result<WalletCreation, IdentityError> {
    let mut gate = EntropyGate::from_host();
    create_wallet_with_gate(&mut gate, user_id, chain_id, network, key_version)
}

/// Erzeugt eine 24-Wort-Phrase aus einem geprueften Entropietor.
///
/// Das ist der Durchsetzungspunkt von REQ-REC-13: der Zustand wird **vor** der
/// Erzeugung geprueft, bei allem ausser `Ready` endet der Aufruf mit
/// [`IdentityError::EntropyUnavailable`] und ohne Phrase. Die 256 Bit der
/// Phrase stammen aus dem Tor, nicht aus einer zweiten Zufallsquelle.
pub fn create_wallet_with_gate(
    gate: &mut EntropyGate,
    user_id: UserId,
    chain_id: u64,
    network: impl Into<String>,
    key_version: u16,
) -> Result<WalletCreation, IdentityError> {
    let network = network.into();
    if network.is_empty() {
        return Err(IdentityError::EmptyNetwork);
    }
    if gate.status() != globus_entropy_core::EntropyState::Ready {
        return Err(IdentityError::EntropyUnavailable);
    }
    let mut entropy = [0u8; 32];
    gate.fill(&mut entropy)?;
    let mnemonic = Mnemonic::from_entropy_in(Language::English, &entropy)
        .map_err(|_| IdentityError::WalletDerivationFailed);
    entropy.zeroize();
    let mnemonic = mnemonic?;
    let phrase = mnemonic.to_string();
    let seed = mnemonic.to_seed_normalized("");
    let private_key = derive_private_key(&seed)?;
    let public_key = derive_public_key(&private_key);
    let wallet_address = public_key_to_address(&public_key)?;
    let binding = IdentityBinding {
        user_id,
        wallet_address: wallet_address.clone(),
        chain_id,
        network,
        key_version,
    };
    Ok(WalletCreation {
        recovery_phrase: RecoveryPhrase(Zeroizing::new(phrase)),
        wallet_address,
        binding,
    })
}
/// Leitet aus einer vorhandenen Phrase dieselbe Bindung ab.
pub fn restore_wallet(
    user_id: UserId,
    phrase: &str,
    chain_id: u64,
    network: impl Into<String>,
    key_version: u16,
) -> Result<IdentityBinding, IdentityError> {
    let network = network.into();
    if network.is_empty() {
        return Err(IdentityError::EmptyNetwork);
    }
    let mnemonic = Mnemonic::parse_in_normalized(Language::English, phrase)
        .map_err(|_| IdentityError::InvalidMnemonic)?;
    if mnemonic.word_count() != 24 {
        return Err(IdentityError::InvalidMnemonic);
    }
    let seed = mnemonic.to_seed_normalized("");
    let private_key = derive_private_key(&seed)?;
    let public_key = derive_public_key(&private_key);
    let wallet_address = public_key_to_address(&public_key)?;
    Ok(IdentityBinding {
        user_id,
        wallet_address,
        chain_id,
        network,
        key_version,
    })
}
fn derive_private_key(seed: &[u8]) -> Result<[u8; 32], IdentityError> {
    let mut mac = HmacSha256::new_from_slice(b"A-TownChain")
        .map_err(|_| IdentityError::WalletDerivationFailed)?;
    mac.update(seed);
    let output = mac.finalize().into_bytes();
    let mut key = [0u8; 32];
    key.copy_from_slice(&output);
    Ok(key)
}
fn derive_public_key(private_key: &[u8; 32]) -> [u8; 32] {
    Sha256::digest(private_key).into()
}
fn public_key_to_address(public_key: &[u8; 32]) -> Result<WalletAddress, IdentityError> {
    let step1 = Sha256::digest(public_key);
    let step2 = Sha256::digest(step1);
    let checksum = Sha256::digest(step2);
    let address = format!(
        "ATC{}{}",
        hex_upper(&step2[..14]),
        hex_upper(&checksum[..2])
    );
    if address.len() != 35 {
        return Err(IdentityError::InvalidWalletAddress);
    }
    Ok(WalletAddress(address))
}
fn hex_upper(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut output = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn identity_rejects_empty_user_id() {
        assert_eq!(UserId::new("").unwrap_err(), IdentityError::InvalidUserId)
    }
    #[test]
    fn generated_wallet_has_24_words_and_atc_address() {
        let user = UserId::new("test-user").unwrap();
        let wallet = create_wallet(user, 600, "devnet", 1).unwrap();
        assert_eq!(wallet.recovery_phrase.word_count(), 24);
        assert_eq!(wallet.wallet_address.as_str().len(), 35);
        assert!(wallet.wallet_address.as_str().starts_with("ATC"))
    }
    #[test]
    fn uninitialisiertes_tor_verhindert_jede_phrase() {
        // REC-T-41: ohne bestandenen Selbsttest entsteht keine Phrase und kein
        // Konto. Geprueft wird der Zustand VOR der Erzeugung.
        let mut gate = EntropyGate::new();
        match create_wallet_with_gate(&mut gate, UserId::new("owner").unwrap(), 600, "devnet", 1) {
            Ok(_) => panic!("ohne Zufallsquelle darf keine Wallet entstehen"),
            Err(error) => assert_eq!(error, IdentityError::EntropyUnavailable),
        }
    }

    #[test]
    fn erkranktes_tor_verhindert_jede_phrase() {
        struct Flat;
        impl globus_entropy_core::EntropySource for Flat {
            fn fill(&mut self, out: &mut [u8]) -> Result<(), globus_entropy_core::SourceError> {
                for byte in out.iter_mut() {
                    *byte = 0x00;
                }
                Ok(())
            }
        }
        let mut gate = EntropyGate::new();
        assert_eq!(
            gate.initialize(&mut Flat, &mut Flat),
            globus_entropy_core::EntropyState::Failed
        );
        match create_wallet_with_gate(&mut gate, UserId::new("owner").unwrap(), 600, "devnet", 1) {
            Ok(_) => panic!("eine konstante Quelle ist keine Entropie"),
            Err(error) => assert_eq!(error, IdentityError::EntropyUnavailable),
        }
    }

    #[test]
    fn phrase_kommt_aus_dem_tor_und_adresse_bleibt_reproduzierbar() {
        let mut gate = EntropyGate::from_host();
        let wallet =
            create_wallet_with_gate(&mut gate, UserId::new("owner").unwrap(), 600, "devnet", 1)
                .expect("gesundes Tor");
        assert_eq!(wallet.recovery_phrase().split_whitespace().count(), 24);
        assert_eq!(wallet.wallet_address.as_str().len(), 35);
        let restored = restore_wallet(
            UserId::new("owner").unwrap(),
            wallet.recovery_phrase(),
            600,
            "devnet",
            1,
        )
        .expect("dieselbe Phrase ergibt dieselbe Bindung");
        assert_eq!(restored.wallet_address, wallet.wallet_address);
    }

    #[test]
    fn neuspeisung_haelt_die_erzeugung_an_und_gibt_sie_wieder_frei() {
        // REQ-REC-16 am Durchsetzungspunkt: nach dem Aufwachen aus S3/S4 ist
        // die Erzeugung gesperrt, bis eine Neuspeisung bestanden ist.
        let mut gate = EntropyGate::from_host();
        let user = UserId::new("owner").unwrap();
        let first = create_wallet_with_gate(&mut gate, user.clone(), 600, "devnet", 1)
            .expect("gesundes Tor");

        gate.invalidate();
        match create_wallet_with_gate(&mut gate, user.clone(), 600, "devnet", 1) {
            Ok(_) => panic!("nach dem Aufwachen darf keine Wallet entstehen"),
            Err(error) => assert_eq!(error, IdentityError::EntropyUnavailable),
        }

        gate.reseed(&mut OsEntropy, &mut OsEntropy);
        assert_eq!(gate.status(), globus_entropy_core::EntropyState::Ready);
        let second = create_wallet_with_gate(&mut gate, user, 600, "devnet", 1)
            .expect("nach bestandener Neuspeisung wieder bereit");
        assert_ne!(first.recovery_phrase(), second.recovery_phrase());
    }

    #[test]
    fn zwei_phrasen_aus_demselben_tor_sind_verschieden() {
        let mut gate = EntropyGate::from_host();
        let first =
            create_wallet_with_gate(&mut gate, UserId::new("owner").unwrap(), 600, "devnet", 1)
                .expect("gesundes Tor");
        let second =
            create_wallet_with_gate(&mut gate, UserId::new("owner").unwrap(), 600, "devnet", 1)
                .expect("gesundes Tor");
        assert_ne!(first.recovery_phrase(), second.recovery_phrase());
        assert_ne!(first.wallet_address, second.wallet_address);
    }

    #[test]
    fn wallet_restore_reproduces_address() {
        let user = UserId::new("test-user").unwrap();
        let wallet = create_wallet(user.clone(), 600, "devnet", 1).unwrap();
        let restored = restore_wallet(user, wallet.recovery_phrase(), 600, "devnet", 1).unwrap();
        assert_eq!(restored.wallet_address, wallet.wallet_address)
    }
    #[test]
    fn account_binding_is_checked() {
        let user = UserId::new("test-user").unwrap();
        let wallet = create_wallet(user.clone(), 600, "devnet", 1).unwrap();
        let profile = AccountProfile::new(user, "Test User", wallet.binding).unwrap();
        assert_eq!(profile.wallet_address, wallet.wallet_address)
    }
    #[test]
    fn session_expires_and_locks() {
        let user = UserId::new("test-user").unwrap();
        let wallet = create_wallet(user.clone(), 600, "devnet", 1).unwrap();
        let mut session = IdentitySession {
            user_id: user,
            wallet_address: wallet.wallet_address,
            state: LoginState::Authenticated,
            issued_at_unix: 100,
            expires_at_unix: 200,
        };
        assert!(session.is_active(199));
        assert!(!session.is_active(200));
        session.lock();
        assert!(!session.is_active(150))
    }
    #[test]
    fn weak_recovery_material_is_rejected() {
        assert_eq!(
            RecoveryMaterial::from_seed(vec![0; 16]).unwrap_err(),
            IdentityError::WeakSeed
        )
    }
}
