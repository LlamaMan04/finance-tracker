//! Authenticated multi-profile lifecycle and in-memory session ownership.

use crate::application::ApplicationError;
use crate::application::contracts::{ProfileSummaryDto, SessionStatusDto};
use crate::application::use_cases::{CreateProfileRequest, ProfileService, UnlockProfileRequest};
use crate::domain::ProfileId;
use crate::infrastructure::database::Database;
use crate::infrastructure::profile_metadata::{ProfileMetadataStore, StoredProfileMetadata};
use crate::infrastructure::profile_registry::ProfileRegistry;
use crate::ports::security::{DatabaseKey, KeyDerivationPort};
use crate::security::Argon2idKeyDeriver;
use crate::security::session_timeout::{DEFAULT_INACTIVITY_TIMEOUT, SessionTimeout};
use crate::security::unlock_throttle::UnlockThrottle;
use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Owns all sensitive material and the protected connection for one unlocked session.
struct UnlockedSession {
    profile_id: ProfileId,
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
        Self {
            registry: ProfileRegistry::new(profile_root),
            key_deriver: Argon2idKeyDeriver,
            inactivity_timeout: SessionTimeout::new(inactivity_timeout),
            session: Mutex::new(SessionState {
                selected_profile_id: None,
                active_session: None,
                last_activity_at: None,
            }),
            unlock_throttle: Mutex::new(UnlockThrottle::new()),
        }
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

        Ok(UnlockedSession { profile_id, _database_key: database_key, _database: database })
    }
}

/// Removes one exact rollback target while tolerating an already-cleaned SQLite sidecar.
fn remove_file_if_present(path: &PathBuf) -> Result<(), ApplicationError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(_) => Err(ApplicationError::internal()),
    }
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
        let metadata_store = ProfileMetadataStore::new(self.registry.metadata_path(&profile_id));
        let stored_metadata =
            StoredProfileMetadata { profile_id: profile_id.as_str().to_owned(), encryption };

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
        state.active_session =
            Some(UnlockedSession { profile_id, _database_key: database_key, _database: database });
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

#[cfg(test)]
mod tests {
    use super::LocalProfileService;
    use crate::application::ErrorCode;
    use crate::application::use_cases::{
        CreateProfileRequest, ProfileService, UnlockProfileRequest,
    };
    use std::fs;
    use std::time::Duration;
    use zeroize::Zeroizing;

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
}
