//! Durable registry of the non-secret local profiles owned by this application.

use crate::application::ApplicationError;
use crate::domain::ProfileId;
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

/// Records only the identifiers that may be selected before authentication.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct ProfileRegistryDocument {
    /// Preserves a format boundary for future registry migrations.
    version: u32,
    /// Stores no passphrases, keys, database paths, or finance content.
    profile_ids: Vec<String>,
}

/// Resolves registry entries to paths wholly owned by the application.
pub struct ProfileRegistry {
    root: PathBuf,
    registry_path: PathBuf,
}

impl ProfileRegistry {
    /// Creates a registry rooted at an application-controlled directory.
    pub fn new(root: PathBuf) -> Self {
        let registry_path = root.join("profiles.json");

        Self { root, registry_path }
    }

    /// Lists validated profile identifiers without opening protected profile data.
    pub fn list(&self) -> Result<Vec<ProfileId>, ApplicationError> {
        let document = self.load()?;
        document
            .profile_ids
            .iter()
            .map(|value| ProfileId::parse(value).map_err(|_| ApplicationError::conflict()))
            .collect()
    }

    /// Registers an identifier exactly once after its protected files are created.
    pub fn add(&self, profile_id: &ProfileId) -> Result<(), ApplicationError> {
        let mut document = self.load()?;
        if document.profile_ids.iter().any(|value| value == profile_id.as_str()) {
            return Err(ApplicationError::conflict());
        }

        document.profile_ids.push(profile_id.as_str().to_owned());
        document.profile_ids.sort();
        self.write(&document)
    }

    /// Verifies that an identifier is known without revealing protected file state.
    pub fn contains(&self, profile_id: &ProfileId) -> Result<bool, ApplicationError> {
        Ok(self.list()?.iter().any(|known| known == profile_id))
    }

    /// Returns the fixed directory for one validated opaque identifier.
    pub fn profile_directory(&self, profile_id: &ProfileId) -> PathBuf {
        self.root.join("profiles").join(profile_id.as_str())
    }

    /// Returns the metadata path below the validated profile directory.
    pub fn metadata_path(&self, profile_id: &ProfileId) -> PathBuf {
        self.profile_directory(profile_id).join("profile.json")
    }

    /// Returns the encrypted database path below the validated profile directory.
    pub fn database_path(&self, profile_id: &ProfileId) -> PathBuf {
        self.profile_directory(profile_id).join("finance-tracker.sqlite3")
    }

    /// Returns the application-owned directory for encrypted backup packages.
    pub fn backup_directory(&self, profile_id: &ProfileId) -> PathBuf {
        self.profile_directory(profile_id).join("backups")
    }

    /// Loads the registry or supplies an empty first-run document.
    fn load(&self) -> Result<ProfileRegistryDocument, ApplicationError> {
        if !self.registry_path.exists() {
            return Ok(ProfileRegistryDocument { version: 1, profile_ids: Vec::new() });
        }

        let serialized = fs::read(&self.registry_path).map_err(|_| ApplicationError::conflict())?;
        let document: ProfileRegistryDocument =
            serde_json::from_slice(&serialized).map_err(|_| ApplicationError::conflict())?;
        if document.version != 1 {
            return Err(ApplicationError::conflict());
        }

        Ok(document)
    }

    /// Publishes a complete registry document so a partial JSON file is never visible.
    fn write(&self, document: &ProfileRegistryDocument) -> Result<(), ApplicationError> {
        fs::create_dir_all(&self.root).map_err(|_| ApplicationError::internal())?;
        let temporary_path = self.registry_path.with_extension("json.tmp");
        let serialized =
            serde_json::to_vec_pretty(document).map_err(|_| ApplicationError::internal())?;
        let result = write_new_file(&temporary_path, &serialized);
        if let Err(error) = result {
            let _ignored_cleanup = fs::remove_file(&temporary_path);
            return Err(error);
        }
        if fs::rename(&temporary_path, &self.registry_path).is_err() {
            let _ignored_cleanup = fs::remove_file(&temporary_path);
            return Err(ApplicationError::internal());
        }

        Ok(())
    }
}

/// Writes a new exact staging target without following an existing registry file.
fn write_new_file(path: &Path, contents: &[u8]) -> Result<(), ApplicationError> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|_| ApplicationError::internal())?;
    file.write_all(contents).map_err(|_| ApplicationError::internal())?;
    file.sync_all().map_err(|_| ApplicationError::internal())
}

#[cfg(test)]
mod tests {
    use super::ProfileRegistry;
    use crate::application::ErrorCode;
    use crate::domain::ProfileId;
    use std::fs;

    #[test]
    fn stores_only_validated_ids_and_derives_application_owned_paths() {
        let directory = tempfile::tempdir().expect("temporary registry directory is available");
        let registry = ProfileRegistry::new(directory.path().to_path_buf());
        let profile_id = ProfileId::parse("household-01").expect("fixture ID is valid");

        registry.add(&profile_id).expect("profile is registered");

        assert_eq!(registry.list().expect("profiles list"), vec![profile_id.clone()]);
        assert_eq!(
            registry.database_path(&profile_id),
            directory.path().join("profiles").join("household-01").join("finance-tracker.sqlite3")
        );
    }

    #[test]
    fn rejects_duplicate_and_corrupt_registries() {
        let directory = tempfile::tempdir().expect("temporary registry directory is available");
        let registry = ProfileRegistry::new(directory.path().to_path_buf());
        let profile_id = ProfileId::parse("household-01").expect("fixture ID is valid");
        registry.add(&profile_id).expect("profile is registered");
        assert_eq!(
            registry.add(&profile_id).expect_err("duplicates are rejected").code,
            ErrorCode::Conflict
        );

        fs::write(directory.path().join("profiles.json"), b"not-json")
            .expect("registry fixture is corrupted");
        assert_eq!(registry.list().expect_err("corruption is rejected").code, ErrorCode::Conflict);
    }
}
