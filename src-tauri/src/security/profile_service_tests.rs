#[cfg(test)]
mod tests {
    use crate::application::use_cases::{
        CreateProfileRequest, ProfileService, RestorePolicy, UnlockProfileRequest,
    };
    use crate::application::{ApplicationError, ErrorCode};
    use crate::domain::BackupId;
    use crate::ports::database::DatabasePort;
    use crate::ports::security::{CredentialStorePort, DATABASE_KEY_LENGTH, DatabaseKey};
    use crate::security::profile_service::LocalProfileService;
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

    /// Proves restore authorization is enforced before backup lookup while the session is locked.
    #[test]
    fn rejects_backup_restore_while_locked() {
        let directory = tempfile::tempdir().expect("temporary profile directory is available");
        let service = service_for(&directory);

        let error = service
            .restore_backup(
                BackupId::parse("backup-01").expect("fixture ID is valid"),
                RestorePolicy::Replace,
            )
            .expect_err("locked restore is rejected");

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

    /// Proves malformed and incompatible packages fail before the active database is touched.
    #[test]
    fn rejects_malformed_and_incompatible_restore_without_mutation() {
        for incompatible in [false, true] {
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
                                "INSERT INTO application_metadata (key, value) VALUES ('restore-probe', 'active')",
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
            if incompatible {
                package[4..8].copy_from_slice(
                    &(crate::infrastructure::backup::FORMAT_VERSION + 1).to_le_bytes(),
                );
            } else {
                package.truncate(package.len() - 1);
            }
            fs::write(package_path, package).expect("invalid package writes");

            assert_eq!(
                service
                    .restore_backup(
                        BackupId::parse(&exported.id).expect("export ID is valid"),
                        RestorePolicy::Replace,
                    )
                    .expect_err("invalid package is rejected")
                    .code,
                ErrorCode::Backup
            );
            let active_value = service
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
                .expect("active protected data remains available");
            assert_eq!(active_value, "active");
        }
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
