//! Authenticated multi-profile lifecycle and in-memory session ownership.

use crate::application::ApplicationError;
use crate::application::contracts::{BackupDto, ProfileSummaryDto, SessionStatusDto};
use crate::application::use_cases::{
    BackupService, CreateProfileRequest, ProfileService, RestorePolicy, UnlockProfileRequest,
};
use crate::domain::{BackupId, ProfileId, ProfileScope};
use crate::infrastructure::backup;
use crate::infrastructure::database::Database;
use crate::infrastructure::profile_metadata::{ProfileMetadataStore, StoredProfileMetadata};
use crate::infrastructure::profile_registry::ProfileRegistry;
use crate::ports::security::{
    CredentialStorePort, DatabaseKey, EncryptionMetadata, KeyDerivationPort,
};
use crate::security::credential_store::credential_reference;
use crate::security::session_timeout::{DEFAULT_INACTIVITY_TIMEOUT, SessionTimeout};
use crate::security::unlock_throttle::UnlockThrottle;
use crate::security::{Argon2idKeyDeriver, PlatformCredentialStore};
use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Owns all sensitive material and the protected connection for one unlocked session.
struct UnlockedSession {
    profile_id: ProfileId,
    /// Retains non-secret derivation metadata so portable backups carry their unlock contract.
    encryption_metadata: EncryptionMetadata,
    _database_key: DatabaseKey,
    _database: Database,
}

/// Keeps the optional selected profile separate from the protected unlocked session.
struct SessionState {
    selected_profile_id: Option<ProfileId>,
    active_session: Option<UnlockedSession>,
    last_activity_at: Option<Instant>,
}

/// Coordinates the local profile lifecycle while keeping startup fully locked.
pub struct LocalProfileService {
    registry: ProfileRegistry,
    key_deriver: Argon2idKeyDeriver,
    credential_store: Arc<dyn CredentialStorePort>,
    inactivity_timeout: SessionTimeout,
    session: Mutex<SessionState>,
    unlock_throttle: Mutex<UnlockThrottle>,
}

impl LocalProfileService {
    /// Creates the service around an application-owned root without opening protected data.
    pub fn new(profile_root: PathBuf) -> Self {
        Self::with_inactivity_timeout(profile_root, DEFAULT_INACTIVITY_TIMEOUT)
    }

    /// Creates the service with an explicit idle policy for platform configuration or focused tests.
    pub fn with_inactivity_timeout(profile_root: PathBuf, inactivity_timeout: Duration) -> Self {
        Self::with_dependencies(profile_root, inactivity_timeout, Arc::new(PlatformCredentialStore))
    }

    /// Creates the service with an injected credential-store adapter for focused lifecycle tests.
    pub fn with_credential_store(
        profile_root: PathBuf,
        credential_store: Arc<dyn CredentialStorePort>,
    ) -> Self {
        Self::with_dependencies(profile_root, DEFAULT_INACTIVITY_TIMEOUT, credential_store)
    }

    /// Builds the service from the reviewed dependencies while preserving a locked startup state.
    fn with_dependencies(
        profile_root: PathBuf,
        inactivity_timeout: Duration,
        credential_store: Arc<dyn CredentialStorePort>,
    ) -> Self {
        Self {
            registry: ProfileRegistry::new(profile_root),
            key_deriver: Argon2idKeyDeriver,
            credential_store,
            inactivity_timeout: SessionTimeout::new(inactivity_timeout),
            session: Mutex::new(SessionState {
                selected_profile_id: None,
                active_session: None,
                last_activity_at: None,
            }),
            unlock_throttle: Mutex::new(UnlockThrottle::new()),
        }
    }

    /// Executes a protected operation only while one profile is active and scoped to that profile.
    pub fn with_active_profile<T>(
        &self,
        operation: impl FnOnce(&ProfileScope, &Database) -> Result<T, ApplicationError>,
    ) -> Result<T, ApplicationError> {
        let mut state = self.session.lock().map_err(|_| ApplicationError::internal())?;
        self.expire_inactive_session_at(&mut state, Instant::now());
        let active_session = state.active_session.as_ref().ok_or_else(ApplicationError::locked)?;
        let scope = ProfileScope::new(active_session.profile_id.clone());

        operation(&scope, &active_session._database)
    }

    /// Creates an encrypted package only for the currently authenticated profile.
    pub fn export_backup(&self) -> Result<BackupDto, ApplicationError> {
        let mut state = self.session.lock().map_err(|_| ApplicationError::internal())?;
        self.expire_inactive_session_at(&mut state, Instant::now());
        let active_session = state.active_session.as_ref().ok_or_else(ApplicationError::locked)?;
        let scope = ProfileScope::new(active_session.profile_id.clone());
        backup::export(
            &active_session._database,
            &active_session._database_key,
            &scope,
            &active_session.encryption_metadata,
            &self.registry.backup_directory(&active_session.profile_id),
        )
    }

    /// Restores one authenticated application-owned package after validating it off-line first.
    pub fn restore_backup(
        &self,
        backup_id: BackupId,
        policy: RestorePolicy,
    ) -> Result<(), ApplicationError> {
        let mut state = self.session.lock().map_err(|_| ApplicationError::internal())?;
        self.expire_inactive_session_at(&mut state, Instant::now());
        let active_session = state.active_session.as_ref().ok_or_else(ApplicationError::locked)?;
        if policy == RestorePolicy::Merge {
            // A file-level SQLCipher snapshot cannot safely merge immutable ledger records without
            // a separate conflict-aware logical package format. Reject rather than silently lose data.
            return Err(ApplicationError::conflict());
        }
        let scope = ProfileScope::new(active_session.profile_id.clone());
        let database_path = self.registry.database_path(&active_session.profile_id);
        let backup_directory = self.registry.backup_directory(&active_session.profile_id);
        let restored_bytes = backup::decrypt_for_restore(
            &backup_directory,
            backup_id.as_str(),
            &scope,
            &active_session._database_key,
            &active_session.encryption_metadata,
        )?;

        let staging_path = restore_staging_path(&database_path, "staging");
        write_restore_staging_file(&staging_path, &restored_bytes)?;
        // Opening the disposable copy proves SQLCipher authentication, schema compatibility, and
        // migrations before the active database is moved or overwritten.
        let staged_database = Database::open(&staging_path, &active_session._database_key)
            .map_err(|_| ApplicationError::backup())?;
        staged_database.checkpoint_wal().map_err(|_| ApplicationError::backup())?;
        drop(staged_database);

        // Take an authenticated recovery snapshot only after the requested backup is fully valid.
        backup::export(
            &active_session._database,
            &active_session._database_key,
            &scope,
            &active_session.encryption_metadata,
            &backup_directory,
        )?;

        let previous_session = state.active_session.take().ok_or_else(ApplicationError::locked)?;
        let UnlockedSession { profile_id, encryption_metadata, _database_key, _database } =
            previous_session;
        drop(_database);
        remove_database_sidecars(&database_path)?;
        remove_database_sidecars(&staging_path)?;
        let rollback_path = restore_staging_path(&database_path, "pre-restore");
        fs::rename(&database_path, &rollback_path).map_err(|_| ApplicationError::backup())?;
        if fs::rename(&staging_path, &database_path).is_err() {
            let _rollback = fs::rename(&rollback_path, &database_path);
            return Err(ApplicationError::backup());
        }

        let restored_database = match Database::open(&database_path, &_database_key) {
            Ok(database) => database,
            Err(_) => {
                let _ignored_current = fs::remove_file(&database_path);
                let _rollback = fs::rename(&rollback_path, &database_path);
                return Err(ApplicationError::backup());
            }
        };
        if fs::remove_file(&rollback_path).is_err() {
            drop(restored_database);
            let _ignored_current = fs::remove_file(&database_path);
            let _rollback = fs::rename(&rollback_path, &database_path);
            return Err(ApplicationError::backup());
        }
        state.active_session = Some(UnlockedSession {
            profile_id,
            encryption_metadata,
            _database_key,
            _database: restored_database,
        });
        state.last_activity_at = Some(Instant::now());

        Ok(())
    }

    /// Builds a safe response from selected and active session state.
    fn status(&self, state: &SessionState) -> SessionStatusDto {
        let profile_id = state
            .active_session
            .as_ref()
            .map(|session| session.profile_id.as_str())
            .or_else(|| state.selected_profile_id.as_ref().map(ProfileId::as_str))
            .map(str::to_owned);
        SessionStatusDto {
            profile_id,
            is_locked: state.active_session.is_none(),
            inactivity_timeout_milliseconds: state
                .active_session
                .as_ref()
                .map(|_| self.inactivity_timeout.milliseconds()),
        }
    }

    /// Drops an idle unlocked session before any status or activity operation can use it.
    fn expire_inactive_session_at(&self, state: &mut SessionState, now: Instant) {
        let has_expired = state.active_session.is_some()
            && state.last_activity_at.is_some_and(|last_activity_at| {
                self.inactivity_timeout.has_expired_at(last_activity_at, now)
            });
        if has_expired {
            state.active_session = None;
            state.last_activity_at = None;
        }
    }

    /// Removes database artifacts created during a setup attempt whose registry publication failed.
    fn roll_back_database_creation(&self, profile_id: &ProfileId) -> Result<(), ApplicationError> {
        let database_path = self.registry.database_path(profile_id);
        remove_file_if_present(&database_path)?;
        for suffix in ["-wal", "-shm"] {
            let mut sidecar_path = database_path.as_os_str().to_owned();
            sidecar_path.push(suffix);
            remove_file_if_present(&PathBuf::from(sidecar_path))?;
        }
        remove_file_if_present(&self.registry.metadata_path(profile_id))?;

        Ok(())
    }

    /// Opens one selected profile while keeping every failure detail inside the Rust boundary.
    fn open_unlocked_session(
        &self,
        request: &UnlockProfileRequest,
    ) -> Result<UnlockedSession, ApplicationError> {
        let profile_id =
            ProfileId::parse(&request.profile_id).map_err(|_| ApplicationError::unauthorized())?;
        if !self.registry.contains(&profile_id)? {
            return Err(ApplicationError::unauthorized());
        }

        let metadata_store = ProfileMetadataStore::new(self.registry.metadata_path(&profile_id));
        let stored = metadata_store.load()?.ok_or_else(ApplicationError::unauthorized)?;
        if stored.profile_id != profile_id.as_str() {
            return Err(ApplicationError::unauthorized());
        }

        let database_key = self
            .key_deriver
            .derive_database_key(request.passphrase.as_slice(), &stored.encryption)?;
        let database = Database::open(&self.registry.database_path(&profile_id), &database_key)?;
        // Backfill the protected scope for profiles created before finance repositories were added.
        ensure_profile_scope(&database, &profile_id)?;
        self.refresh_credential_store(stored.credential_store_reference.as_deref(), &database_key);

        Ok(UnlockedSession {
            profile_id,
            encryption_metadata: stored.encryption.clone(),
            _database_key: database_key,
            _database: database,
        })
    }

    /// Best-effort credential storage never changes whether a passphrase-authorized session may open.
    fn refresh_credential_store(&self, reference: Option<&str>, key: &DatabaseKey) {
        let Some(reference) = reference else {
            return;
        };
        let is_current = self
            .credential_store
            .load_database_key(reference)
            .ok()
            .flatten()
            .is_some_and(|stored_key| stored_key.expose() == key.expose());
        if !is_current {
            let _ignored_unavailable_store =
                self.credential_store.store_database_key(reference, key);
        }
    }
}

/// Ensures every authenticated profile has a local ownership scope before finance operations run.
fn ensure_profile_scope(
    database: &Database,
    profile_id: &ProfileId,
) -> Result<(), ApplicationError> {
    database.in_transaction(|transaction| {
        transaction
            .execute(
                "INSERT OR IGNORE INTO profile_scopes (id, created_at, updated_at) VALUES (?1, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
                [profile_id.as_str()],
            )
            .map_err(|_| ApplicationError::internal())?;
        Ok(())
    })
}

/// Removes one exact rollback target while tolerating an already-cleaned SQLite sidecar.
fn remove_file_if_present(path: &PathBuf) -> Result<(), ApplicationError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(_) => Err(ApplicationError::internal()),
    }
}

/// Creates one sibling path so a restore never accepts a caller-controlled filesystem location.
fn restore_staging_path(database_path: &PathBuf, suffix: &str) -> PathBuf {
    let file_name = database_path.file_name().and_then(|name| name.to_str()).unwrap_or("database");
    database_path.with_file_name(format!(".{file_name}.{suffix}.{}", uuid::Uuid::new_v4()))
}

/// Writes a complete candidate database and synchronizes it before it is opened for validation.
fn write_restore_staging_file(path: &PathBuf, bytes: &[u8]) -> Result<(), ApplicationError> {
    use std::io::Write;

    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|_| ApplicationError::backup())?;
    file.write_all(bytes).map_err(|_| ApplicationError::backup())?;
    file.sync_all().map_err(|_| ApplicationError::backup())
}

/// Removes only known SQLite sidecars after their owning connection has been closed.
fn remove_database_sidecars(database_path: &PathBuf) -> Result<(), ApplicationError> {
    for suffix in ["-wal", "-shm"] {
        let mut sidecar_path = database_path.as_os_str().to_owned();
        sidecar_path.push(suffix);
        remove_file_if_present(&PathBuf::from(sidecar_path))
            .map_err(|_| ApplicationError::backup())?;
    }
    Ok(())
}

impl ProfileService for LocalProfileService {
    /// Lists profiles without opening their metadata or encrypted databases.
    fn list_profiles(&self) -> Result<Vec<ProfileSummaryDto>, ApplicationError> {
        let profiles = self
            .registry
            .list()?
            .into_iter()
            .map(|profile_id| ProfileSummaryDto { id: profile_id.as_str().to_owned() })
            .collect();
        Ok(profiles)
    }

    /// Reports selected and active status without opening any protected database.
    fn session_status(&self) -> Result<SessionStatusDto, ApplicationError> {
        let mut state = self.session.lock().map_err(|_| ApplicationError::internal())?;
        self.expire_inactive_session_at(&mut state, Instant::now());
        Ok(self.status(&state))
    }

    /// Creates one encrypted profile, then registers it after all protected files exist.
    fn create_profile(
        &self,
        request: CreateProfileRequest,
    ) -> Result<SessionStatusDto, ApplicationError> {
        let mut state = self.session.lock().map_err(|_| ApplicationError::internal())?;
        self.expire_inactive_session_at(&mut state, Instant::now());
        if state.active_session.is_some() {
            return Err(ApplicationError::conflict());
        }

        let profile_id = ProfileId::parse(&request.profile_id)?;
        if self.registry.contains(&profile_id)? {
            return Err(ApplicationError::conflict());
        }

        let encryption = self.key_deriver.create_metadata()?;
        let database_key =
            self.key_deriver.derive_database_key(request.passphrase.as_slice(), &encryption)?;
        let database = Database::create(&self.registry.database_path(&profile_id), &database_key)?;
        // Seed the protected profile scope before any finance repository can write scoped records.
        database.in_transaction(|transaction| {
            transaction
                .execute(
                    "INSERT INTO profile_scopes (id, created_at, updated_at) VALUES (?1, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
                    [profile_id.as_str()],
                )
                .map_err(|_| ApplicationError::internal())?;
            Ok(())
        })?;
        let metadata_store = ProfileMetadataStore::new(self.registry.metadata_path(&profile_id));
        let stored_metadata = StoredProfileMetadata {
            profile_id: profile_id.as_str().to_owned(),
            encryption,
            credential_store_reference: Some(credential_reference(profile_id.as_str())),
        };

        if let Err(error) = metadata_store.create(&stored_metadata) {
            drop(database);
            self.roll_back_database_creation(&profile_id)?;
            return Err(error);
        }
        if let Err(error) = self.registry.add(&profile_id) {
            drop(database);
            self.roll_back_database_creation(&profile_id)?;
            return Err(error);
        }

        state.selected_profile_id = Some(profile_id.clone());
        self.refresh_credential_store(
            stored_metadata.credential_store_reference.as_deref(),
            &database_key,
        );
        state.active_session = Some(UnlockedSession {
            profile_id,
            encryption_metadata: stored_metadata.encryption.clone(),
            _database_key: database_key,
            _database: database,
        });
        state.last_activity_at = Some(Instant::now());

        Ok(self.status(&state))
    }

    /// Reproduces the selected database key and opens data only after key validation.
    fn unlock_profile(
        &self,
        request: UnlockProfileRequest,
    ) -> Result<SessionStatusDto, ApplicationError> {
        let mut state = self.session.lock().map_err(|_| ApplicationError::internal())?;
        let now = Instant::now();
        self.expire_inactive_session_at(&mut state, now);
        if let Some(active_session) = state.active_session.as_ref() {
            if active_session.profile_id.as_str() == request.profile_id {
                return Ok(self.status(&state));
            }
            return Err(ApplicationError::conflict());
        }
        if state.selected_profile_id.as_ref().map(ProfileId::as_str)
            != Some(request.profile_id.as_str())
        {
            return Err(ApplicationError::unauthorized());
        }

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
        throttle.reset();
        state.active_session = Some(active_session);
        state.last_activity_at = Some(now);

        Ok(self.status(&state))
    }

    /// Drops the connection and zeroizing key owner while retaining only a safe selection.
    fn lock(&self) -> Result<(), ApplicationError> {
        let mut state = self.session.lock().map_err(|_| ApplicationError::internal())?;
        state.active_session = None;
        state.last_activity_at = None;

        Ok(())
    }

    /// Refreshes the idle deadline only for an active session that has not already expired.
    fn record_activity(&self) -> Result<SessionStatusDto, ApplicationError> {
        let mut state = self.session.lock().map_err(|_| ApplicationError::internal())?;
        let now = Instant::now();
        self.expire_inactive_session_at(&mut state, now);
        if state.active_session.is_some() {
            state.last_activity_at = Some(now);
        }

        Ok(self.status(&state))
    }

    /// Locks a current profile before selecting another known profile for unlock.
    fn switch_profile(&self, profile_id: String) -> Result<SessionStatusDto, ApplicationError> {
        let profile_id = ProfileId::parse(&profile_id)?;
        if !self.registry.contains(&profile_id)? {
            return Err(ApplicationError::not_found());
        }

        let mut state = self.session.lock().map_err(|_| ApplicationError::internal())?;
        state.active_session = None;
        state.last_activity_at = None;
        state.selected_profile_id = Some(profile_id);

        Ok(self.status(&state))
    }
}

impl BackupService for LocalProfileService {
    /// Exports only through the active session and never exposes package bytes to the caller.
    fn export(&self, _scope: &ProfileScope) -> Result<BackupDto, ApplicationError> {
        self.export_backup()
    }

    /// Restores through the active session so package decryption never occurs while locked.
    fn restore(
        &self,
        scope: &ProfileScope,
        backup_id: &crate::domain::BackupId,
        policy: RestorePolicy,
    ) -> Result<(), ApplicationError> {
        self.with_active_profile(|active_scope, _| {
            if active_scope != scope {
                return Err(ApplicationError::unauthorized());
            }
            Ok(())
        })?;
        self.restore_backup(backup_id.clone(), policy)
    }
}

#[cfg(test)]
mod tests {
    use super::LocalProfileService;
    use crate::application::use_cases::{
        CreateProfileRequest, ProfileService, RestorePolicy, UnlockProfileRequest,
    };
    use crate::application::{ApplicationError, ErrorCode};
    use crate::domain::BackupId;
    use crate::ports::database::DatabasePort;
    use crate::ports::security::{CredentialStorePort, DATABASE_KEY_LENGTH, DatabaseKey};
    use std::collections::HashMap;
    use std::fs;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    use zeroize::Zeroizing;

    /// Stores test-only credential values so lifecycle tests can observe the optional adapter boundary.
    struct TestCredentialStore {
        is_available: bool,
        entries: Mutex<HashMap<String, [u8; DATABASE_KEY_LENGTH]>>,
    }

    impl TestCredentialStore {
        /// Creates an adapter whose availability is explicit in each test fixture.
        fn new(is_available: bool) -> Self {
            Self { is_available, entries: Mutex::new(HashMap::new()) }
        }
    }

    impl CredentialStorePort for TestCredentialStore {
        /// Returns a fresh zeroizing key copy so tests never share secret ownership with the adapter.
        fn load_database_key(
            &self,
            reference: &str,
        ) -> Result<Option<DatabaseKey>, ApplicationError> {
            if !self.is_available {
                return Err(ApplicationError::internal());
            }
            let entries = self.entries.lock().map_err(|_| ApplicationError::internal())?;

            Ok(entries.get(reference).copied().map(DatabaseKey::new))
        }

        /// Stores one copied test key under the supplied non-secret reference.
        fn store_database_key(
            &self,
            reference: &str,
            key: &DatabaseKey,
        ) -> Result<(), ApplicationError> {
            if !self.is_available {
                return Err(ApplicationError::internal());
            }
            let mut entries = self.entries.lock().map_err(|_| ApplicationError::internal())?;
            entries.insert(reference.to_owned(), *key.expose());

            Ok(())
        }
    }

    fn service_for(directory: &tempfile::TempDir) -> LocalProfileService {
        LocalProfileService::new(directory.path().join("profile-store"))
    }

    fn create_request(profile_id: &str, passphrase: &[u8]) -> CreateProfileRequest {
        CreateProfileRequest {
            profile_id: profile_id.to_owned(),
            passphrase: Zeroizing::new(passphrase.to_vec()),
        }
    }

    fn unlock_request(profile_id: &str, passphrase: &[u8]) -> UnlockProfileRequest {
        UnlockProfileRequest {
            profile_id: profile_id.to_owned(),
            passphrase: Zeroizing::new(passphrase.to_vec()),
        }
    }

    #[test]
    fn creates_switches_and_reopens_independently_protected_profiles() {
        let directory = tempfile::tempdir().expect("temporary profile directory is available");
        let service = service_for(&directory);
        service
            .create_profile(create_request("home", b"home passphrase"))
            .expect("home profile is created");
        service.lock().expect("home profile locks");
        service
            .create_profile(create_request("work", b"work passphrase"))
            .expect("work profile is created");

        assert_eq!(
            service
                .list_profiles()
                .expect("profiles list")
                .into_iter()
                .map(|profile| profile.id)
                .collect::<Vec<_>>(),
            vec!["home", "work"]
        );
        service.switch_profile("home".to_owned()).expect("switch locks work");
        let wrong_key = service
            .unlock_profile(unlock_request("home", b"work passphrase"))
            .expect_err("keys do not cross profile boundaries");
        assert_eq!(wrong_key.code, ErrorCode::Unauthorized);
        assert!(service.unlock_profile(unlock_request("home", b"home passphrase")).is_ok());
    }

    #[test]
    fn rejects_a_wrong_passphrase_without_unlocking_the_selected_profile() {
        let directory = tempfile::tempdir().expect("temporary profile directory is available");
        let service = service_for(&directory);
        service
            .create_profile(create_request("home", b"home passphrase"))
            .expect("profile is created");
        service.lock().expect("profile locks");

        let error = service
            .unlock_profile(unlock_request("home", b"incorrect passphrase"))
            .expect_err("wrong passphrase is rejected");

        assert_eq!(error.code, ErrorCode::Unauthorized);
        assert!(service.session_status().expect("safe status is available").is_locked);
    }

    /// Proves an expired active session closes its database owner and cannot be revived by activity.
    #[test]
    fn expires_an_idle_session_before_recording_new_activity() {
        let directory = tempfile::tempdir().expect("temporary profile directory is available");
        let service = LocalProfileService::with_inactivity_timeout(
            directory.path().join("profile-store"),
            Duration::ZERO,
        );
        service
            .create_profile(create_request("home", b"home passphrase"))
            .expect("profile is created");

        let status = service.record_activity().expect("activity status is safe");

        assert!(status.is_locked);
        assert_eq!(status.profile_id.as_deref(), Some("home"));
        assert_eq!(status.inactivity_timeout_milliseconds, None);
        assert!(service.session_status().expect("safe status is available").is_locked);
    }

    #[test]
    fn rejects_duplicate_ids_and_corrupt_profile_metadata_without_unlocking() {
        let directory = tempfile::tempdir().expect("temporary profile directory is available");
        let service = service_for(&directory);
        service
            .create_profile(create_request("home", b"home passphrase"))
            .expect("profile is created");
        service.lock().expect("profile locks");
        assert_eq!(
            service
                .create_profile(create_request("home", b"replacement"))
                .expect_err("duplicate is rejected")
                .code,
            ErrorCode::Conflict
        );
        fs::write(
            directory
                .path()
                .join("profile-store")
                .join("profiles")
                .join("home")
                .join("profile.json"),
            b"not-json",
        )
        .expect("metadata fixture is corrupted");
        let error = service
            .unlock_profile(unlock_request("home", b"home passphrase"))
            .expect_err("corruption remains private");
        assert_eq!(error.code, ErrorCode::Unauthorized);
        assert!(service.session_status().expect("safe status is available").is_locked);
    }

    /// Proves every future protected operation receives the active profile scope and rejects locks.
    #[test]
    fn scopes_protected_operations_to_the_active_profile_and_rejects_locked_access() {
        let directory = tempfile::tempdir().expect("temporary profile directory is available");
        let service = service_for(&directory);

        let locked_error = service
            .with_active_profile(|_, _| Ok(()))
            .expect_err("locked sessions cannot run protected operations");
        assert_eq!(locked_error.code, ErrorCode::Locked);

        service
            .create_profile(create_request("home", b"home passphrase"))
            .expect("profile is created");
        let schema_version = service
            .with_active_profile(|scope, database| {
                assert_eq!(scope.profile_id().as_str(), "home");
                database.schema_version()
            })
            .expect("active profile authorizes its own database");
        assert_eq!(schema_version, 3);

        service.lock().expect("profile locks");
        let relocked_error = service
            .with_active_profile(|_, _| Ok(()))
            .expect_err("explicit lock revokes protected access");
        assert_eq!(relocked_error.code, ErrorCode::Locked);
    }

    /// Proves backup export requires the same authenticated session boundary as finance queries.
    #[test]
    fn rejects_backup_export_while_locked() {
        let directory = tempfile::tempdir().expect("temporary profile directory is available");
        let service = service_for(&directory);

        let error = service.export_backup().expect_err("locked export is rejected");

        assert_eq!(error.code, ErrorCode::Locked);
    }

    /// Proves a replace restore returns the active profile to the exported protected snapshot.
    #[test]
    fn restores_a_validated_backup_without_exposing_database_contents() {
        let directory = tempfile::tempdir().expect("temporary profile directory is available");
        let service = service_for(&directory);
        service
            .create_profile(create_request("home", b"home passphrase"))
            .expect("profile is created");
        service
            .with_active_profile(|_, database| {
                database.in_transaction(|transaction| {
                    transaction
                        .execute(
                            "INSERT INTO application_metadata (key, value) VALUES ('restore-probe', 'before')",
                            [],
                        )
                        .map_err(|_| ApplicationError::internal())?;
                    Ok(())
                })
            })
            .expect("pre-backup data writes");
        let exported = service.export_backup().expect("backup exports");
        service
            .with_active_profile(|_, database| {
                database.in_transaction(|transaction| {
                    transaction
                        .execute(
                            "UPDATE application_metadata SET value = 'after' WHERE key = 'restore-probe'",
                            [],
                        )
                        .map_err(|_| ApplicationError::internal())?;
                    Ok(())
                })
            })
            .expect("post-backup data writes");

        service
            .restore_backup(
                BackupId::parse(&exported.id).expect("export ID is valid"),
                RestorePolicy::Replace,
            )
            .expect("backup restores");
        let restored_value = service
            .with_active_profile(|_, database| {
                database.in_transaction(|transaction| {
                    transaction
                        .query_row(
                            "SELECT value FROM application_metadata WHERE key = 'restore-probe'",
                            [],
                            |row| row.get::<_, String>(0),
                        )
                        .map_err(|_| ApplicationError::internal())
                })
            })
            .expect("restored protected data reads");
        assert_eq!(restored_value, "before");
    }

    /// Proves authentication failure occurs before a replace restore can change active data.
    #[test]
    fn rejects_a_tampered_restore_without_mutating_the_active_database() {
        let directory = tempfile::tempdir().expect("temporary profile directory is available");
        let service = service_for(&directory);
        service
            .create_profile(create_request("home", b"home passphrase"))
            .expect("profile is created");
        service
            .with_active_profile(|_, database| {
                database.in_transaction(|transaction| {
                    transaction
                        .execute(
                            "INSERT INTO application_metadata (key, value) VALUES ('tamper-probe', 'active')",
                            [],
                        )
                        .map_err(|_| ApplicationError::internal())?;
                    Ok(())
                })
            })
            .expect("active data writes");
        let exported = service.export_backup().expect("backup exports");
        let package_path = directory
            .path()
            .join("profile-store")
            .join("profiles")
            .join("home")
            .join("backups")
            .join(format!("{}.ftbackup", exported.id));
        let mut package = fs::read(&package_path).expect("package reads");
        let last_byte = package.len() - 1;
        package[last_byte] ^= 1;
        fs::write(package_path, package).expect("tampered package writes");

        assert_eq!(
            service
                .restore_backup(
                    BackupId::parse(&exported.id).expect("export ID is valid"),
                    RestorePolicy::Replace,
                )
                .expect_err("tampered package is rejected")
                .code,
            ErrorCode::Backup
        );
        let active_value = service
            .with_active_profile(|_, database| {
                database.in_transaction(|transaction| {
                    transaction
                        .query_row(
                            "SELECT value FROM application_metadata WHERE key = 'tamper-probe'",
                            [],
                            |row| row.get::<_, String>(0),
                        )
                        .map_err(|_| ApplicationError::internal())
                })
            })
            .expect("active protected data remains available");
        assert_eq!(active_value, "active");
    }

    /// Proves the unsupported merge selection fails without mutating the active protected data.
    #[test]
    fn rejects_merge_restore_policy_without_touching_the_active_database() {
        let directory = tempfile::tempdir().expect("temporary profile directory is available");
        let service = service_for(&directory);
        service
            .create_profile(create_request("home", b"home passphrase"))
            .expect("profile is created");
        let error = service
            .restore_backup(
                BackupId::parse("backup-01").expect("fixture ID is valid"),
                RestorePolicy::Merge,
            )
            .expect_err("merge is not silently approximated");

        assert_eq!(error.code, ErrorCode::Conflict);
        assert!(!service.session_status().expect("session is readable").is_locked);
    }

    /// Proves an unavailable optional credential store never replaces the mandatory passphrase path.
    #[test]
    fn falls_back_to_passphrase_when_credential_storage_is_unavailable() {
        let directory = tempfile::tempdir().expect("temporary profile directory is available");
        let credential_store = Arc::new(TestCredentialStore::new(false));
        let service = LocalProfileService::with_credential_store(
            directory.path().join("profile-store"),
            credential_store,
        );
        service
            .create_profile(create_request("home", b"home passphrase"))
            .expect("profile creation ignores unavailable optional storage");
        service.lock().expect("profile locks");

        assert!(service.unlock_profile(unlock_request("home", b"home passphrase")).is_ok());
    }

    /// Proves a successful profile setup records only its derived key in the optional credential store.
    #[test]
    fn stores_profile_keys_under_the_expected_non_secret_credential_reference() {
        let directory = tempfile::tempdir().expect("temporary profile directory is available");
        let credential_store = Arc::new(TestCredentialStore::new(true));
        let service = LocalProfileService::with_credential_store(
            directory.path().join("profile-store"),
            credential_store.clone(),
        );
        service
            .create_profile(create_request("home", b"home passphrase"))
            .expect("profile is created");

        let entries = credential_store.entries.lock().expect("test credential store is available");
        assert!(entries.contains_key("FinanceTracker/v1/profile/home/database-key"));
    }
}
