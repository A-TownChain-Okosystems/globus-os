//! Persistent username-to-account index for the GlobusOS identity boundary.
//!
//! This index provides durable, fail-closed username uniqueness. It deliberately stores no
//! password, recovery phrase, wallet secret, or Aurora node key. Account-record persistence
//! must be coordinated by the caller; this index alone is not a transaction across stores.
//!
//! The lock file is acquired with create_new, so concurrent writers cannot both commit. A
//! process crash can leave a stale lock; this backend fails closed in that case and requires
//! explicit recovery rather than silently removing a lock that may belong to a live writer.

use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

use crate::{AccountUsername, UserId};

const MAX_FIELD: usize = 128;

#[derive(Debug, Clone, PartialEq, Eq)]
/// Fehler des Benutzernamen-Verzeichnisses.
pub enum AccountDirectoryError {
    /// Der Benutzername ist bereits vergeben.
    AlreadyExists,
    /// Der Benutzername ist nicht eingetragen.
    NotFound,
    /// Die Lock-Datei gehoert einem anderen Schreiber.
    Busy,
    /// Der Index ist beschaedigt.
    Corrupt,
    /// Benutzername oder Benutzer-ID ist unbrauchbar.
    InvalidAccountId,
    /// Dateisystemfehler.
    Io,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Ein Eintrag: Benutzername und zugehoerige Benutzer-ID.
pub struct AccountDirectoryEntry {
    /// Kanonischer Benutzername.
    pub username: AccountUsername,
    /// Benutzer-ID, die zu diesem Namen gehoert.
    pub user_id: UserId,
}

/// Read-only view of the canonical username-to-UserId mapping.
pub trait AccountDirectoryStore {
    /// Resolve a canonical username to its stable UserId.
    fn lookup(&self, username: &AccountUsername) -> Result<UserId, AccountDirectoryError>;
    /// Return the complete canonical directory projection.
    fn entries(&self) -> Result<Vec<AccountDirectoryEntry>, AccountDirectoryError>;
}

/// Legacy standalone directory writer. Production account mutations must commit
/// through AccountStateRepository so username and credential state remain atomic.
pub trait MutableAccountDirectoryStore: AccountDirectoryStore {
    /// Legacy atomic insertion into the standalone GUA1 file.
    fn insert_if_absent(
        &mut self,
        username: AccountUsername,
        user_id: UserId,
    ) -> Result<(), AccountDirectoryError>;
}

/// Legacy filesystem-backed GUA1 index. Production mutations must use AccountStateRepository.
#[deprecated(
    note = "legacy standalone directory; use AccountStateRepository as the canonical account store"
)]
/// Legacy GUA1 directory reader/writer retained for import and migration only.
pub struct FileAccountDirectory {
    path: PathBuf,
    lock_path: PathBuf,
}

impl FileAccountDirectory {
    /// Bindet das Verzeichnis an einen Pfad und leitet den Lock-Pfad daraus ab.
    pub fn open(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let mut lock_name = path.as_os_str().to_os_string();
        lock_name.push(".lock");
        Self {
            path,
            lock_path: PathBuf::from(lock_name),
        }
    }

    /// Pfad der Indexdatei.
    pub fn path(&self) -> &Path {
        &self.path
    }

    fn acquire_lock(&self) -> Result<DirectoryLock, AccountDirectoryError> {
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&self.lock_path)
        {
            Ok(mut file) => {
                // A diagnostic marker only; lock ownership is established by create_new.
                let _ = writeln!(file, "pid={}", std::process::id());
                let _ = file.sync_all();
                Ok(DirectoryLock {
                    path: self.lock_path.clone(),
                })
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                Err(AccountDirectoryError::Busy)
            }
            Err(_) => Err(AccountDirectoryError::Io),
        }
    }

    fn read_entries(&self) -> Result<Vec<AccountDirectoryEntry>, AccountDirectoryError> {
        let mut file = match File::open(&self.path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(_) => return Err(AccountDirectoryError::Io),
        };
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)
            .map_err(|_| AccountDirectoryError::Io)?;
        decode_entries(&bytes)
    }

    fn write_entries(
        &self,
        entries: &[AccountDirectoryEntry],
    ) -> Result<(), AccountDirectoryError> {
        let parent = self
            .path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        fs::create_dir_all(parent).map_err(|_| AccountDirectoryError::Io)?;
        let bytes = encode_entries(entries)?;
        let mut tmp_name = self.path.as_os_str().to_os_string();
        tmp_name.push(format!(".tmp.{}", std::process::id()));
        let tmp_path = PathBuf::from(tmp_name);

        // Never follow or overwrite a pre-existing temporary path.
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp_path)
            .map_err(|_| AccountDirectoryError::Io)?;
        let write_result = (|| {
            file.write_all(&bytes)
                .map_err(|_| AccountDirectoryError::Io)?;
            file.sync_all().map_err(|_| AccountDirectoryError::Io)?;
            fs::rename(&tmp_path, &self.path).map_err(|_| AccountDirectoryError::Io)?;
            File::open(parent)
                .and_then(|dir| dir.sync_all())
                .map_err(|_| AccountDirectoryError::Io)?;
            Ok(())
        })();
        if write_result.is_err() {
            let _ = fs::remove_file(&tmp_path);
        }
        write_result
    }
}

struct DirectoryLock {
    path: PathBuf,
}
impl Drop for DirectoryLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

#[allow(deprecated)]
impl AccountDirectoryStore for FileAccountDirectory {
    fn lookup(&self, username: &AccountUsername) -> Result<UserId, AccountDirectoryError> {
        self.read_entries()?
            .into_iter()
            .find(|entry| &entry.username == username)
            .map(|entry| entry.user_id)
            .ok_or(AccountDirectoryError::NotFound)
    }

    fn entries(&self) -> Result<Vec<AccountDirectoryEntry>, AccountDirectoryError> {
        self.read_entries()
    }
}

#[allow(deprecated)]
impl MutableAccountDirectoryStore for FileAccountDirectory {
    fn insert_if_absent(
        &mut self,
        username: AccountUsername,
        user_id: UserId,
    ) -> Result<(), AccountDirectoryError> {
        if user_id.as_str().is_empty()
            || user_id.as_str().len() > MAX_FIELD
            || user_id.as_str().as_bytes().contains(&0)
        {
            return Err(AccountDirectoryError::InvalidAccountId);
        }
        let _lock = self.acquire_lock()?;
        let mut entries = self.read_entries()?;
        if entries
            .iter()
            .any(|entry| entry.username == username || entry.user_id == user_id)
        {
            return Err(AccountDirectoryError::AlreadyExists);
        }
        entries.push(AccountDirectoryEntry { username, user_id });
        entries.sort_by(|a, b| a.username.as_str().cmp(b.username.as_str()));
        self.write_entries(&entries)
    }
}

/// AccountState is the canonical production source; this implementation exposes
/// its username index as a read-only view and cannot commit a detached directory entry.
impl AccountDirectoryStore for crate::AccountState {
    fn lookup(&self, username: &AccountUsername) -> Result<UserId, AccountDirectoryError> {
        self.by_username(username)
            .map(|entry| entry.identity.profile.user_id.clone())
            .ok_or(AccountDirectoryError::NotFound)
    }

    fn entries(&self) -> Result<Vec<AccountDirectoryEntry>, AccountDirectoryError> {
        Ok(self
            .accounts()
            .iter()
            .map(|entry| AccountDirectoryEntry {
                username: entry.username.clone(),
                user_id: entry.identity.profile.user_id.clone(),
            })
            .collect())
    }
}

fn encode_entries(entries: &[AccountDirectoryEntry]) -> Result<Vec<u8>, AccountDirectoryError> {
    let raw: Vec<globus_identity_core::catalog::DirectoryEntry> = entries
        .iter()
        .map(|entry| globus_identity_core::catalog::DirectoryEntry {
            username: entry.username.as_str().to_owned(),
            user_id: entry.user_id.as_str().to_owned(),
        })
        .collect();
    globus_identity_core::catalog::encode_directory(&raw)
        .map_err(|_| AccountDirectoryError::Corrupt)
}

fn decode_entries(bytes: &[u8]) -> Result<Vec<AccountDirectoryEntry>, AccountDirectoryError> {
    globus_identity_core::catalog::parse_directory(bytes)
        .map_err(|_| AccountDirectoryError::Corrupt)?
        .into_iter()
        .map(|entry| {
            let username = AccountUsername::parse(&entry.username)
                .map_err(|_| AccountDirectoryError::Corrupt)?;
            let user_id = UserId::new(entry.user_id).map_err(|_| AccountDirectoryError::Corrupt)?;
            Ok(AccountDirectoryEntry { username, user_id })
        })
        .collect()
}

#[cfg(test)]
#[allow(deprecated)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(0);

    fn temp_path() -> PathBuf {
        std::env::temp_dir().join(format!(
            "globus-account-directory-{}-{}.bin",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
    }

    #[test]
    fn registration_is_unique_and_survives_reopen() {
        let path = temp_path();
        let username = AccountUsername::parse("Alice").unwrap();
        let user_id = UserId::new("user-alice").unwrap();
        {
            let mut directory = FileAccountDirectory::open(&path);
            directory
                .insert_if_absent(username.clone(), user_id.clone())
                .unwrap();
            assert_eq!(
                directory.insert_if_absent(username.clone(), UserId::new("other").unwrap()),
                Err(AccountDirectoryError::AlreadyExists)
            );
        }
        let reopened = FileAccountDirectory::open(&path);
        assert_eq!(reopened.lookup(&username).unwrap(), user_id);
        let _ = fs::remove_file(&path);
        let mut lock = path.as_os_str().to_os_string();
        lock.push(".lock");
        let _ = fs::remove_file(PathBuf::from(lock));
    }

    #[test]
    fn concurrent_registration_cannot_create_duplicate_username() {
        use std::sync::{Arc, Barrier};
        use std::{
            thread,
            time::{Duration, Instant},
        };

        let path = temp_path();
        let barrier = Arc::new(Barrier::new(2));
        let mut workers = Vec::new();
        for suffix in ["one", "two"] {
            let worker_path = path.clone();
            let worker_barrier = Arc::clone(&barrier);
            workers.push(thread::spawn(move || {
                let username = AccountUsername::parse("ConcurrentUser").unwrap();
                let user_id = UserId::new(format!("user-{suffix}")).unwrap();
                let mut directory = FileAccountDirectory::open(worker_path);
                worker_barrier.wait();
                // Filesystem sync/rename latency varies substantially across CI runners.
                // Retry for a bounded wall-clock interval instead of a brittle 200 ms budget.
                let deadline = Instant::now() + Duration::from_secs(10);
                loop {
                    match directory.insert_if_absent(username.clone(), user_id.clone()) {
                        Ok(()) => return true,
                        Err(AccountDirectoryError::Busy) if Instant::now() < deadline => {
                            thread::sleep(Duration::from_millis(1));
                        }
                        Err(AccountDirectoryError::Busy) => {
                            panic!("directory lock did not become available within 10 seconds");
                        }
                        Err(AccountDirectoryError::AlreadyExists) => return false,
                        Err(error) => panic!("unexpected directory error: {error:?}"),
                    }
                }
            }));
        }
        let winners = workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .filter(|won| *won)
            .count();
        assert_eq!(winners, 1);
        let directory = FileAccountDirectory::open(&path);
        assert_eq!(directory.entries().unwrap().len(), 1);
        let _ = fs::remove_file(&path);
        let mut lock = path.as_os_str().to_os_string();
        lock.push(".lock");
        let _ = fs::remove_file(PathBuf::from(lock));
    }

    #[test]
    fn corrupt_file_fails_closed() {
        let path = temp_path();
        fs::write(&path, b"not-an-account-index").unwrap();
        let directory = FileAccountDirectory::open(&path);
        assert_eq!(directory.entries(), Err(AccountDirectoryError::Corrupt));
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn malformed_and_duplicate_entries_are_rejected() {
        assert_eq!(decode_entries(b"GUA1\x01\0\0\0\0"), Ok(Vec::new()));
        assert_eq!(
            decode_entries(b"GUA1\x02\0\0\0\0"),
            Err(AccountDirectoryError::Corrupt)
        );
    }
}
