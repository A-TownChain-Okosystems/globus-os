//! In-process coordinator for multiple independently authenticated identity accounts.
//!
//! This is an orchestration layer, not a durable account database. Callers must persist each
//! account through a separate durable IdentityStore and coordinate username-directory changes
//! with a journal/reconciliation protocol before claiming crash-consistent registration.
//! Wallet derivation remains delegated to the existing identity boundary and is not evidence of
//! conformance to the draft canonical atc-wallet BIP44 specification.

use std::collections::HashMap;

use super::{
    AccountLifecycleError, AccountStateEntry, AccountStateError, AccountStateRepository,
    AccountUsername, BlobStore, IdentityKeyProvider, IdentityService, IdentityServiceError,
    IdentitySession, LoginResult, RecoveryConfirmation, RegistrationResult, UserId,
};
use zeroize::Zeroizing;

#[derive(Debug, Clone, PartialEq, Eq)]
/// Fehler der Mehrkonten-Koordination.
pub enum MultiAccountError {
    /// Fuer diesen Benutzer existiert bereits ein Konto.
    DuplicateAccount,
    /// Fuer diesen Benutzer existiert kein Konto.
    AccountNotFound,
    /// Benutzername oder Zugangsdaten sind ungueltig; keine Kontoexistenz preisgeben.
    AuthenticationFailed,
    /// Die Recovery-Phrase wurde noch nicht bestaetigt.
    RecoveryConfirmationRequired,
    /// Die Recovery-Phrase oder die ausgewaehlten Bestaetigungswoerter sind ungueltig.
    RecoveryConfirmation(AccountLifecycleError),
    /// Der Identitaetsdienst hat einen Fehler gemeldet.
    Identity(IdentityServiceError),
    /// Das kanonische verschluesselte Account-Repository hat abgelehnt.
    AccountState(AccountStateError),
}

impl From<AccountLifecycleError> for MultiAccountError {
    fn from(value: AccountLifecycleError) -> Self {
        Self::RecoveryConfirmation(value)
    }
}

impl From<IdentityServiceError> for MultiAccountError {
    fn from(value: IdentityServiceError) -> Self {
        Self::Identity(value)
    }
}

impl From<AccountStateError> for MultiAccountError {
    fn from(value: AccountStateError) -> Self {
        Self::AccountState(value)
    }
}

/// A prepared account registration that is not active or persisted until recovery confirmation.
pub struct PendingAccountRegistration {
    username: AccountUsername,
    user_id: UserId,
    identity: super::IdentityRecord,
    account: IdentityService,
    registration: RegistrationResult,
    recovery_phrase: Zeroizing<String>,
    confirmation: RecoveryConfirmation,
    confirmed: bool,
    committed: bool,
}

impl PendingAccountRegistration {
    /// Recovery phrase that the trusted local UI must display before confirmation.
    pub fn recovery_phrase(&self) -> &str {
        self.recovery_phrase.as_str()
    }

    /// Three zero-based word positions; the UI should display one-based positions.
    pub fn confirmation_positions(&self) -> [usize; 3] {
        self.confirmation.positions()
    }

    /// Verify the selected recovery words. A failed attempt leaves this pending
    /// registration intact so the user can retry without generating a new wallet.
    pub fn confirm_recovery(&mut self, answers: [&str; 3]) -> Result<(), MultiAccountError> {
        self.confirmation.verify(answers)?;
        self.confirmed = true;
        Ok(())
    }
}

/// Keeps authentication state isolated by account identifier.
///
/// Each map entry owns its own credential hash and session. This coordinator is intentionally
/// in-memory: restart durability and atomic coordination with the username directory are separate
/// requirements and must not be inferred from this type.
#[derive(Default)]
pub struct MultiAccountIdentityService {
    accounts: HashMap<String, IdentityService>,
}

impl MultiAccountIdentityService {
    /// Leere Koordination ohne Konten.
    pub fn new() -> Self {
        Self::default()
    }

    /// Anzahl registrierter Konten.
    pub fn account_count(&self) -> usize {
        self.accounts.len()
    }

    /// Registriert ein Konto; ein zweites Konto zum selben Benutzer wird abgelehnt.
    pub fn register(
        &mut self,
        user_id: UserId,
        display_name: impl Into<String>,
        credential: &[u8],
        chain_id: u64,
        network: impl Into<String>,
        key_version: u16,
    ) -> Result<(RegistrationResult, WalletCreation), MultiAccountError> {
        let key = user_id.as_str().to_owned();
        if self.accounts.contains_key(&key) {
            return Err(MultiAccountError::DuplicateAccount);
        }

        let mut account = IdentityService::new();
        let result = account.register(
            user_id,
            display_name,
            credential,
            chain_id,
            network,
            key_version,
        )?;
        // Publish the account only after registration succeeds.
        self.accounts.insert(key, account);
        Ok(result)
    }

    #[allow(clippy::too_many_arguments)]
    /// Stellt ein Konto aus einer Wiederherstellungsphrase wieder her.
    pub fn restore(
        &mut self,
        user_id: UserId,
        display_name: impl Into<String>,
        phrase: &str,
        credential: &[u8],
        chain_id: u64,
        network: impl Into<String>,
        key_version: u16,
    ) -> Result<RegistrationResult, MultiAccountError> {
        let key = user_id.as_str().to_owned();
        if self.accounts.contains_key(&key) {
            return Err(MultiAccountError::DuplicateAccount);
        }

        let mut account = IdentityService::new();
        let result = account.restore(
            user_id,
            display_name,
            phrase,
            credential,
            chain_id,
            network,
            key_version,
        )?;
        self.accounts.insert(key, account);
        Ok(result)
    }

    /// Prepare registration without activating or persisting the account.
    ///
    /// The caller must display `pending.recovery_phrase()`, collect the selected words,
    /// and call `confirm_recovery` before committing. Dropping the pending value aborts
    /// registration; no durable account record or username mapping is written.
    #[allow(clippy::too_many_arguments)]
    pub fn prepare_registration_persisted(
        &self,
        username: AccountUsername,
        user_id: UserId,
        display_name: impl Into<String>,
        credential: &[u8],
        chain_id: u64,
        network: impl Into<String>,
        key_version: u16,
    ) -> Result<PendingAccountRegistration, MultiAccountError> {
        if self.accounts.contains_key(user_id.as_str()) {
            return Err(MultiAccountError::DuplicateAccount);
        }
        let mut account = IdentityService::new();
        let (registration, wallet) = account.register(
            user_id.clone(),
            display_name,
            credential,
            chain_id,
            network,
            key_version,
        )?;
        let recovery_phrase = Zeroizing::new(wallet.recovery_phrase().to_owned());
        let confirmation = RecoveryConfirmation::new(&recovery_phrase)?;
        let identity = account.snapshot()?;
        Ok(PendingAccountRegistration {
            username,
            user_id,
            identity,
            account,
            registration,
            recovery_phrase,
            confirmation,
            confirmed: false,
            committed: false,
        })
    }

    /// Prepare recovery from an existing 24-word phrase without activating the account.
    #[allow(clippy::too_many_arguments)]
    pub fn prepare_restore_persisted(
        &self,
        username: AccountUsername,
        user_id: UserId,
        display_name: impl Into<String>,
        phrase: &str,
        credential: &[u8],
        chain_id: u64,
        network: impl Into<String>,
        key_version: u16,
    ) -> Result<PendingAccountRegistration, MultiAccountError> {
        if self.accounts.contains_key(user_id.as_str()) {
            return Err(MultiAccountError::DuplicateAccount);
        }
        let mut account = IdentityService::new();
        let registration = account.restore(
            user_id.clone(),
            display_name,
            phrase,
            credential,
            chain_id,
            network,
            key_version,
        )?;
        let recovery_phrase = Zeroizing::new(phrase.to_owned());
        let confirmation = RecoveryConfirmation::new(&recovery_phrase)?;
        let identity = account.snapshot()?;
        Ok(PendingAccountRegistration {
            username,
            user_id,
            identity,
            account,
            registration,
            recovery_phrase,
            confirmation,
            confirmed: false,
            committed: false,
        })
    }

    /// Commit a prepared account only after the recovery phrase challenge succeeded.
    /// The complete account record and username mapping enter the canonical encrypted
    /// repository in one generation-bound transaction.
    pub fn commit_registration_persisted<B, K>(
        &mut self,
        repository: &mut AccountStateRepository<B, K>,
        pending: &mut PendingAccountRegistration,
    ) -> Result<RegistrationResult, MultiAccountError>
    where
        B: BlobStore,
        K: IdentityKeyProvider,
    {
        if pending.committed {
            return Err(MultiAccountError::DuplicateAccount);
        }
        if !pending.confirmed {
            return Err(MultiAccountError::RecoveryConfirmationRequired);
        }
        if self.accounts.contains_key(pending.user_id.as_str()) {
            return Err(MultiAccountError::DuplicateAccount);
        }
        let expected_generation = repository.generation()?;
        repository.insert_account(
            expected_generation,
            AccountStateEntry {
                username: pending.username.clone(),
                identity: pending.identity.clone(),
            },
        )?;
        let account = core::mem::replace(&mut pending.account, IdentityService::new());
        self.accounts.insert(pending.user_id.as_str().to_owned(), account);
        pending.committed = true;
        Ok(pending.registration.clone())
    }

    /// Authenticate by canonical username using the durable account repository.
    /// Unknown usernames and invalid credentials must be normalized to one UI response.
    pub fn login_persisted<B, K>(
        &mut self,
        repository: &AccountStateRepository<B, K>,
        username: &AccountUsername,
        credential: &[u8],
        now_unix: u64,
        ttl_seconds: u64,
    ) -> Result<LoginResult, MultiAccountError>
    where
        B: BlobStore,
        K: IdentityKeyProvider,
    {
        let state = repository.load_state()?;
        let Some(state) = state else {
            return Err(MultiAccountError::AuthenticationFailed);
        };
        let Some(entry) = state.by_username(username) else {
            return Err(MultiAccountError::AuthenticationFailed);
        };
        let user_id = entry.identity.profile.user_id.clone();
        let mut account = IdentityService::new();
        account.load_record(entry.identity.clone())?;
        let result =
            account
                .login(credential, now_unix, ttl_seconds)
                .map_err(|error| match error {
                    IdentityServiceError::InvalidCredentials
                    | IdentityServiceError::NotRegistered => {
                        MultiAccountError::AuthenticationFailed
                    }
                    other => MultiAccountError::Identity(other),
                })?;
        self.accounts.insert(user_id.as_str().to_owned(), account);
        Ok(result)
    }

    /// Reauthenticate, then change a credential in one atomic commit.
    /// The generation is captured before the read so a concurrent writer cannot be overwritten.
    /// Invalid current credentials and unknown usernames return the same public error.
    pub fn change_credential_persisted<B, K>(
        &mut self,
        repository: &mut AccountStateRepository<B, K>,
        username: &AccountUsername,
        current_credential: &[u8],
        new_credential: &[u8],
    ) -> Result<u64, MultiAccountError>
    where
        B: BlobStore,
        K: IdentityKeyProvider,
    {
        let expected_generation = repository.generation()?;
        let state = repository
            .load_state()?
            .ok_or(MultiAccountError::AccountNotFound)?;
        let entry = state
            .by_username(username)
            .ok_or(MultiAccountError::AccountNotFound)?;
        let user_id = entry.identity.profile.user_id.clone();
        let mut account = IdentityService::new();
        account.load_record(entry.identity.clone())?;
        account
            .verify_current_credential(current_credential)
            .map_err(|error| match error {
                IdentityServiceError::InvalidCredentials | IdentityServiceError::NotRegistered => {
                    MultiAccountError::AuthenticationFailed
                }
                other => MultiAccountError::Identity(other),
            })?;
        account.change_credential(new_credential)?;
        let replacement = account.snapshot()?;
        let generation = repository.replace_identity(expected_generation, username, replacement)?;
        self.accounts.insert(user_id.as_str().to_owned(), account);
        Ok(generation)
    }

    /// Reauthenticate, remove an account durably, then drop its in-memory session coordinator entry.
    /// Invalid current credentials and unknown usernames return the same public error.
    pub fn remove_persisted<B, K>(
        &mut self,
        repository: &mut AccountStateRepository<B, K>,
        username: &AccountUsername,
        current_credential: &[u8],
    ) -> Result<u64, MultiAccountError>
    where
        B: BlobStore,
        K: IdentityKeyProvider,
    {
        let expected_generation = repository.generation()?;
        let state = repository
            .load_state()?
            .ok_or(MultiAccountError::AuthenticationFailed)?;
        let entry = state
            .by_username(username)
            .ok_or(MultiAccountError::AuthenticationFailed)?;
        let user_id = entry.identity.profile.user_id.clone();
        let mut account = IdentityService::new();
        account.load_record(entry.identity.clone())?;
        account
            .verify_current_credential(current_credential)
            .map_err(|error| match error {
                IdentityServiceError::InvalidCredentials | IdentityServiceError::NotRegistered => {
                    MultiAccountError::AuthenticationFailed
                }
                other => MultiAccountError::Identity(other),
            })?;
        let generation = repository.remove_account(expected_generation, username)?;
        self.accounts.remove(user_id.as_str());
        Ok(generation)
    }

    /// Meldet ein Konto an und liefert die Sitzung.
    pub fn login(
        &mut self,
        user_id: &UserId,
        credential: &[u8],
        now_unix: u64,
        ttl_seconds: u64,
    ) -> Result<LoginResult, MultiAccountError> {
        self.accounts
            .get_mut(user_id.as_str())
            .ok_or(MultiAccountError::AccountNotFound)?
            .login(credential, now_unix, ttl_seconds)
            .map_err(MultiAccountError::Identity)
    }

    /// Meldet ein Konto ab.
    pub fn logout(&mut self, user_id: &UserId) -> Result<(), MultiAccountError> {
        self.accounts
            .get_mut(user_id.as_str())
            .ok_or(MultiAccountError::AccountNotFound)?
            .logout();
        Ok(())
    }

    /// Sitzung eines Kontos, falls sie noch gueltig ist.
    pub fn session(&self, user_id: &UserId, now_unix: u64) -> Option<&IdentitySession> {
        self.accounts
            .get(user_id.as_str())
            .and_then(|account| account.session(now_unix))
    }

    /// Remove an account from this in-memory coordinator and thereby revoke its local session.
    ///
    /// This does not delete durable identity records or the persistent username index. A caller
    /// must complete those operations transactionally before treating account deletion as durable.
    pub fn remove_account(&mut self, user_id: &UserId) -> Result<(), MultiAccountError> {
        self.accounts
            .remove(user_id.as_str())
            .map(|_| ())
            .ok_or(MultiAccountError::AccountNotFound)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestKey;
    impl IdentityKeyProvider for TestKey {
        fn load_key(&self) -> Result<zeroize::Zeroizing<[u8; 32]>, super::super::SecureStoreError> {
            Ok(zeroize::Zeroizing::new([0x42; 32]))
        }
    }

    fn correct_recovery_answers(pending: &PendingAccountRegistration) -> [String; 3] {
        let words: Vec<&str> = pending.recovery_phrase().split_whitespace().collect();
        let positions = pending.confirmation_positions();
        [
            words[positions[0]].to_owned(),
            words[positions[1]].to_owned(),
            words[positions[2]].to_owned(),
        ]
    }

    #[test]
    fn persisted_lifecycle_uses_one_encrypted_repository() {
        let encrypted = super::super::EncryptedBlobStore::new(
            super::super::InMemoryBlobStore::default(),
            TestKey,
        );
        let mut repository = AccountStateRepository::new(encrypted);
        let mut service = MultiAccountIdentityService::new();
        let alice_id = UserId::new("user-alice").unwrap();
        let bob_id = UserId::new("user-bob").unwrap();
        let mut pending_alice = service
            .prepare_registration_persisted(
                AccountUsername::parse("alice").unwrap(),
                alice_id.clone(),
                "Alice",
                b"alice-secret",
                658467,
                "devnet",
                1,
            )
            .unwrap();
        assert!(repository.load_state().unwrap().is_none());
        assert_eq!(
            service.commit_registration_persisted(&mut repository, &mut pending_alice),
            Err(MultiAccountError::RecoveryConfirmationRequired)
        );
        // An unconfirmed pending account cannot be committed; it remains available for retry.
        assert!(repository.load_state().unwrap().is_none());
        assert_eq!(
            pending_alice.confirm_recovery(["wrong", "wrong", "wrong"]),
            Err(MultiAccountError::RecoveryConfirmation(
                AccountLifecycleError::InvalidConfirmation
            ))
        );
        let alice_answers = correct_recovery_answers(&pending_alice);
        pending_alice
            .confirm_recovery([&alice_answers[0], &alice_answers[1], &alice_answers[2]])
            .unwrap();
        service
            .commit_registration_persisted(&mut repository, &mut pending_alice)
            .unwrap();

        let mut pending_bob = service
            .prepare_registration_persisted(
                AccountUsername::parse("bob").unwrap(),
                bob_id.clone(),
                "Bob",
                b"bob-secret",
                658467,
                "devnet",
                1,
            )
            .unwrap();
        let bob_answers = correct_recovery_answers(&pending_bob);
        pending_bob
            .confirm_recovery([&bob_answers[0], &bob_answers[1], &bob_answers[2]])
            .unwrap();
        service
            .commit_registration_persisted(&mut repository, &mut pending_bob)
            .unwrap();
        assert_eq!(repository.load_state().unwrap().unwrap().len(), 2);

        let mut after_restart = MultiAccountIdentityService::new();
        assert!(
            after_restart
                .login_persisted(
                    &repository,
                    &AccountUsername::parse("alice").unwrap(),
                    b"alice-secret",
                    100,
                    300,
                )
                .is_ok()
        );
        assert_eq!(
            after_restart.login_persisted(
                &repository,
                &AccountUsername::parse("bob").unwrap(),
                b"alice-secret",
                100,
                300,
            ),
            Err(MultiAccountError::AuthenticationFailed)
        );
        assert_eq!(
            after_restart.login_persisted(
                &repository,
                &AccountUsername::parse("unknown").unwrap(),
                b"alice-secret",
                100,
                300,
            ),
            Err(MultiAccountError::AuthenticationFailed)
        );

        let alice = AccountUsername::parse("alice").unwrap();
        assert_eq!(
            after_restart.change_credential_persisted(
                &mut repository,
                &alice,
                b"wrong-secret",
                b"alice-new-secret",
            ),
            Err(MultiAccountError::AuthenticationFailed)
        );
        after_restart
            .change_credential_persisted(
                &mut repository,
                &alice,
                b"alice-secret",
                b"alice-new-secret",
            )
            .unwrap();
        assert!(
            after_restart
                .login_persisted(&repository, &alice, b"alice-secret", 101, 300)
                .is_err()
        );
        assert!(
            after_restart
                .login_persisted(&repository, &alice, b"alice-new-secret", 101, 300)
                .is_ok()
        );

        assert_eq!(
            after_restart.remove_persisted(&mut repository, &alice, b"wrong-secret"),
            Err(MultiAccountError::AuthenticationFailed)
        );
        after_restart
            .remove_persisted(&mut repository, &alice, b"alice-new-secret")
            .unwrap();
        assert_eq!(repository.load_state().unwrap().unwrap().len(), 1);
        assert!(
            after_restart
                .login_persisted(&repository, &alice, b"alice-new-secret", 102, 300)
                .is_err()
        );
    }

    #[test]
    fn accounts_have_independent_credentials_and_sessions() {
        let alice = UserId::new("alice").unwrap();
        let bob = UserId::new("bob").unwrap();
        let mut service = MultiAccountIdentityService::new();

        service
            .register(alice.clone(), "Alice", b"alice-secret", 658467, "devnet", 1)
            .unwrap();
        service
            .register(bob.clone(), "Bob", b"bob-secret", 658467, "devnet", 1)
            .unwrap();

        assert_eq!(service.account_count(), 2);
        assert!(service.login(&alice, b"alice-secret", 100, 60).is_ok());
        assert!(service.login(&bob, b"bob-secret", 100, 60).is_ok());
        assert!(service.login(&alice, b"bob-secret", 100, 60).is_err());

        service.logout(&alice).unwrap();
        assert!(service.session(&alice, 101).is_none());
        assert!(service.session(&bob, 101).is_some());
    }

    #[test]
    fn duplicate_account_is_rejected_without_replacing_existing_account() {
        let alice = UserId::new("alice").unwrap();
        let mut service = MultiAccountIdentityService::new();
        service
            .register(alice.clone(), "Alice", b"alice-secret", 658467, "devnet", 1)
            .unwrap();

        assert_eq!(
            service
                .register(
                    alice.clone(),
                    "Imposter",
                    b"other-secret",
                    658467,
                    "devnet",
                    1
                )
                .err()
                .unwrap(),
            MultiAccountError::DuplicateAccount
        );
        assert!(service.login(&alice, b"alice-secret", 100, 60).is_ok());
        assert_eq!(service.account_count(), 1);
    }

    #[test]
    fn removing_account_revokes_its_local_session() {
        let alice = UserId::new("alice").unwrap();
        let mut service = MultiAccountIdentityService::new();
        service
            .register(alice.clone(), "Alice", b"alice-secret", 658467, "devnet", 1)
            .unwrap();
        service.login(&alice, b"alice-secret", 100, 60).unwrap();

        service.remove_account(&alice).unwrap();
        assert_eq!(service.account_count(), 0);
        assert!(service.session(&alice, 101).is_none());
        assert_eq!(
            service.login(&alice, b"alice-secret", 101, 60).unwrap_err(),
            MultiAccountError::AccountNotFound
        );
    }
}
