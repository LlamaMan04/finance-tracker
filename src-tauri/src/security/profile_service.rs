//! Authenticated profile creation, unlock, and in-memory session ownership.

use crate::application::ApplicationError;
use crate::application::contracts::SessionStatusDto;
use crate::application::use_cases::{CreateProfileRequest, ProfileService, UnlockProfileRequest};
use crate::domain::ProfileId;
use crate::infrastructure::database::Database;
use crate::infrastructure::profile_metadata::{ProfileMetadataStore, StoredProfileMetadata};
use crate::ports::security::{DatabaseKey, KeyDerivationPort};
use crate::security::Argon2idKeyDeriver;
use crate::security::unlock_throttle::UnlockThrottle;
use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Instant;

/// Owns all sensitive material and the protected connection for one unlocked session.
struct UnlockedSession {
    profile_id: ProfileId,
    _database_key: DatabaseKey,
    _database: Database,
}

/// Coordinates the local profile lifecycle while keeping startup fully locked.
pub struct LocalProfileService {
    database_path: PathBuf,
    metadata_store: ProfileMetadataStore,
    key_deriver: Argon2idKeyDeriver,
    session: Mutex<Option<UnlockedSession>>,
    unlock_throttle: Mutex<UnlockThrottle>,
}

impl LocalProfileService {
    /// Creates the service around approved paths without opening protected data.
    pub fn new(database_path: PathBuf, metadata_path: PathBuf) -> Self {
        Self {
            database_path,
            metadata_store: ProfileMetadataStore::new(metadata_path),
            key_deriver: Argon2idKeyDeriver,
            session: Mutex::new(None),
            unlock_throttle: Mutex::new(UnlockThrottle::new()),
        }
    }

    /// Builds a safe response from an authenticated session.
    fn unlocked_status(session: &UnlockedSession) -> SessionStatusDto {
        SessionStatusDto {
            profile_id: Some(session.profile_id.as_str().to_owned()),
            is_locked: false,
        }
    }

    /// Removes a database created during a setup attempt whose metadata could not be committed.
    fn roll_back_database_creation(&self) -> Result<(), ApplicationError> {
        remove_file_if_present(&self.database_path)?;
        for suffix in ["-wal", "-shm"] {
            let mut sidecar_path = self.database_path.as_os_str().to_owned();
            sidecar_path.push(suffix);
            remove_file_if_present(&PathBuf::from(sidecar_path))?;
        }

        Ok(())
    }

    /// Opens one authenticated session while keeping every failure detail inside the Rust boundary.
    fn open_unlocked_session(
        &self,
        request: &UnlockProfileRequest,
    ) -> Result<UnlockedSession, ApplicationError> {
        let stored = self.metadata_store.load()?.ok_or_else(ApplicationError::unauthorized)?;
        if stored.profile_id != request.profile_id {
            return Err(ApplicationError::unauthorized());
        }

        let profile_id =
            ProfileId::parse(&stored.profile_id).map_err(|_| ApplicationError::unauthorized())?;
        let database_key = self
            .key_deriver
            .derive_database_key(request.passphrase.as_slice(), &stored.encryption)?;
        let database = Database::open(&self.database_path, &database_key)?;

        Ok(UnlockedSession { profile_id, _database_key: database_key, _database: database })
    }
}

/// Removes one exact rollback target while tolerating a sidecar SQLite already cleaned up.
fn remove_file_if_present(path: &PathBuf) -> Result<(), ApplicationError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(_) => Err(ApplicationError::internal()),
    }
}

impl ProfileService for LocalProfileService {
    /// Reports whether setup is required or an existing profile is locked/unlocked.
    fn session_status(&self) -> Result<SessionStatusDto, ApplicationError> {
        let session = self.session.lock().map_err(|_| ApplicationError::internal())?;
        if let Some(active_session) = session.as_ref() {
            return Ok(Self::unlocked_status(active_session));
        }
        drop(session);

        let metadata = self.metadata_store.load()?;
        match (metadata, self.database_path.is_file()) {
            (None, false) => Ok(SessionStatusDto { profile_id: None, is_locked: true }),
            (Some(stored), true) => {
                let profile_id = ProfileId::parse(&stored.profile_id)
                    .map_err(|_| ApplicationError::conflict())?;
                Ok(SessionStatusDto {
                    profile_id: Some(profile_id.as_str().to_owned()),
                    is_locked: true,
                })
            }
            _ => Err(ApplicationError::conflict()),
        }
    }

    /// Creates the encrypted database and publishes its non-secret profile envelope.
    fn create_profile(
        &self,
        request: CreateProfileRequest,
    ) -> Result<SessionStatusDto, ApplicationError> {
        let mut session = self.session.lock().map_err(|_| ApplicationError::internal())?;
        if session.is_some() || self.metadata_store.load()?.is_some() || self.database_path.exists()
        {
            return Err(ApplicationError::conflict());
        }

        let profile_id = ProfileId::parse(&request.profile_id)?;
        let encryption = self.key_deriver.create_metadata()?;
        let database_key =
            self.key_deriver.derive_database_key(request.passphrase.as_slice(), &encryption)?;
        let database = Database::create(&self.database_path, &database_key)?;
        let stored_metadata =
            StoredProfileMetadata { profile_id: profile_id.as_str().to_owned(), encryption };

        if let Err(error) = self.metadata_store.create(&stored_metadata) {
            drop(database);
            self.roll_back_database_creation()?;
            return Err(error);
        }

        let active_session =
            UnlockedSession { profile_id, _database_key: database_key, _database: database };
        let status = Self::unlocked_status(&active_session);
        *session = Some(active_session);

        Ok(status)
    }

    /// Reproduces the database key and opens protected data only after key validation.
    fn unlock_profile(
        &self,
        request: UnlockProfileRequest,
    ) -> Result<SessionStatusDto, ApplicationError> {
        let mut session = self.session.lock().map_err(|_| ApplicationError::internal())?;
        if let Some(active_session) = session.as_ref() {
            return Ok(Self::unlocked_status(active_session));
        }

        let now = Instant::now();
        let mut throttle = self.unlock_throttle.lock().map_err(|_| ApplicationError::internal())?;
        if throttle.is_blocked_at(now) {
            return Err(ApplicationError::unauthorized());
        }

        let active_session = match self.open_unlocked_session(&request) {
            Ok(active_session) => active_session,
            Err(_) => {
                throttle.record_failure_at(now);
                return Err(ApplicationError::unauthorized());
            }
        };
        let status = Self::unlocked_status(&active_session);
        throttle.reset();
        *session = Some(active_session);

        Ok(status)
    }

    /// Drops the connection and zeroizing key owner to restore the locked boundary.
    fn lock(&self) -> Result<(), ApplicationError> {
        let mut session = self.session.lock().map_err(|_| ApplicationError::internal())?;
        *session = None;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::LocalProfileService;
    use crate::application::ErrorCode;
    use crate::application::use_cases::{
        CreateProfileRequest, ProfileService, UnlockProfileRequest,
    };
    use std::fs;
    use zeroize::Zeroizing;

    /// Builds a temporary directory whose profile files cannot affect another test.
    fn test_directory() -> tempfile::TempDir {
        tempfile::tempdir().expect("temporary profile directory is available")
    }

    /// Builds a new application-lifetime service over one test profile directory.
    fn service_for(directory: &tempfile::TempDir) -> LocalProfileService {
        LocalProfileService::new(
            directory.path().join("finance-tracker.sqlite3"),
            directory.path().join("finance-tracker.profile.json"),
        )
    }

    /// Produces a wiping setup request without retaining the passphrase in the service fixture.
    fn create_request() -> CreateProfileRequest {
        CreateProfileRequest {
            profile_id: "primary-profile".to_owned(),
            passphrase: Zeroizing::new(b"correct horse battery staple".to_vec()),
        }
    }

    /// Proves first-run creation unlocks the session and can later reopen the same profile.
    #[test]
    fn creates_locks_and_reopens_a_protected_profile() {
        let directory = test_directory();
        let service = service_for(&directory);

        assert_eq!(service.session_status().expect("initial status").profile_id, None);
        let created = service.create_profile(create_request()).expect("profile is created");
        assert!(!created.is_locked);

        service.lock().expect("profile locks");
        assert!(service.session_status().expect("locked status").is_locked);
        drop(service);

        let restarted_service = service_for(&directory);
        let reopened = restarted_service
            .unlock_profile(UnlockProfileRequest {
                profile_id: "primary-profile".to_owned(),
                passphrase: Zeroizing::new(b"correct horse battery staple".to_vec()),
            })
            .expect("profile reopens");
        assert!(!reopened.is_locked);
    }

    /// Proves a wrong passphrase receives only the generic authorization result and stays locked.
    #[test]
    fn rejects_a_wrong_passphrase_without_unlocking() {
        let directory = test_directory();
        let service = service_for(&directory);
        service.create_profile(create_request()).expect("profile is created");
        service.lock().expect("profile locks");

        let error = service
            .unlock_profile(UnlockProfileRequest {
                profile_id: "primary-profile".to_owned(),
                passphrase: Zeroizing::new(b"incorrect passphrase".to_vec()),
            })
            .expect_err("wrong passphrase is rejected");

        assert_eq!(error.code, ErrorCode::Unauthorized);
        assert!(service.session_status().expect("status remains available").is_locked);
    }

    /// Proves malformed public metadata reveals no more than a wrong passphrase would.
    #[test]
    fn normalizes_corrupt_metadata_to_a_generic_unlock_failure() {
        let directory = test_directory();
        let service = service_for(&directory);
        service.create_profile(create_request()).expect("profile is created");
        service.lock().expect("profile locks");
        fs::write(directory.path().join("finance-tracker.profile.json"), b"not-json")
            .expect("metadata fixture is corrupted");

        let error = service
            .unlock_profile(UnlockProfileRequest {
                profile_id: "primary-profile".to_owned(),
                passphrase: Zeroizing::new(b"correct horse battery staple".to_vec()),
            })
            .expect_err("corrupt metadata is not disclosed");

        assert_eq!(error.code, ErrorCode::Unauthorized);
    }

    /// Proves a missing protected database has the same safe response as a wrong passphrase.
    #[test]
    fn normalizes_missing_database_to_a_generic_unlock_failure() {
        let directory = test_directory();
        let service = service_for(&directory);
        service.create_profile(create_request()).expect("profile is created");
        service.lock().expect("profile locks");
        fs::remove_file(directory.path().join("finance-tracker.sqlite3"))
            .expect("database fixture is removed");

        let error = service
            .unlock_profile(UnlockProfileRequest {
                profile_id: "primary-profile".to_owned(),
                passphrase: Zeroizing::new(b"correct horse battery staple".to_vec()),
            })
            .expect_err("missing database is not disclosed");

        assert_eq!(error.code, ErrorCode::Unauthorized);
    }

    /// Proves a second setup cannot replace an existing encrypted profile.
    #[test]
    fn refuses_to_replace_an_existing_profile() {
        let directory = test_directory();
        let service = service_for(&directory);
        service.create_profile(create_request()).expect("profile is created");

        let error =
            service.create_profile(create_request()).expect_err("duplicate setup is rejected");

        assert_eq!(error.code, ErrorCode::Conflict);
    }
}
