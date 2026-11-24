//! Durable, non-secret profile metadata stored beside the protected database.

use crate::application::ApplicationError;
use crate::ports::security::EncryptionMetadata;
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

/// Records the public information required to identify and unlock one local profile.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct StoredProfileMetadata {
    /// Holds the validated profile identifier without any financial profile contents.
    pub profile_id: String,
    /// Holds the versioned salt and KDF settings needed to reproduce the database key.
    pub encryption: EncryptionMetadata,
    /// Identifies an optional OS credential-store entry and never contains a passphrase or key.
    #[serde(default)]
    pub credential_store_reference: Option<String>,
}

/// Owns the single approved metadata path and prevents accidental replacement.
pub struct ProfileMetadataStore {
    path: PathBuf,
}

impl ProfileMetadataStore {
    /// Creates a metadata adapter without reading or creating any profile files.
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// Loads public profile metadata, returning `None` only for a clean first run.
    pub fn load(&self) -> Result<Option<StoredProfileMetadata>, ApplicationError> {
        if !self.path.exists() {
            return Ok(None);
        }

        let serialized = fs::read(&self.path).map_err(|_| ApplicationError::conflict())?;
        let metadata =
            serde_json::from_slice(&serialized).map_err(|_| ApplicationError::conflict())?;

        Ok(Some(metadata))
    }

    /// Persists metadata once using a temporary file so partial JSON is never published.
    pub fn create(&self, metadata: &StoredProfileMetadata) -> Result<(), ApplicationError> {
        if self.path.exists() {
            return Err(ApplicationError::conflict());
        }
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|_| ApplicationError::internal())?;
        }

        let temporary_path = temporary_path(&self.path);
        let serialized =
            serde_json::to_vec_pretty(metadata).map_err(|_| ApplicationError::internal())?;
        let write_result = write_new_file(&temporary_path, &serialized);
        if let Err(error) = write_result {
            let _ignored_cleanup = fs::remove_file(&temporary_path);
            return Err(error);
        }

        if fs::rename(&temporary_path, &self.path).is_err() {
            let _ignored_cleanup = fs::remove_file(&temporary_path);
            return Err(ApplicationError::internal());
        }

        Ok(())
    }
}

/// Writes and flushes a newly created file without following an existing destination.
fn write_new_file(path: &Path, contents: &[u8]) -> Result<(), ApplicationError> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|_| ApplicationError::internal())?;
    file.write_all(contents).map_err(|_| ApplicationError::internal())?;
    file.sync_all().map_err(|_| ApplicationError::internal())
}

/// Derives a sibling staging path that contains no user-controlled path segments.
fn temporary_path(path: &Path) -> PathBuf {
    let mut temporary = path.as_os_str().to_owned();
    temporary.push(".tmp");

    PathBuf::from(temporary)
}

#[cfg(test)]
mod tests {
    use super::{ProfileMetadataStore, StoredProfileMetadata};
    use crate::ports::security::KeyDerivationPort;
    use crate::security::Argon2idKeyDeriver;

    /// Proves the sidecar round trip contains only reproducible, non-secret metadata.
    #[test]
    fn creates_and_loads_profile_metadata_without_replacing_it() {
        let directory = tempfile::tempdir().expect("temporary metadata directory is available");
        let store = ProfileMetadataStore::new(directory.path().join("profile.json"));
        let deriver = Argon2idKeyDeriver;
        let metadata = StoredProfileMetadata {
            profile_id: "primary-profile".to_owned(),
            encryption: deriver.create_metadata().expect("encryption metadata is generated"),
            credential_store_reference: Some(
                "FinanceTracker/v1/profile/primary-profile/database-key".to_owned(),
            ),
        };

        store.create(&metadata).expect("metadata is created");

        assert_eq!(store.load().expect("metadata loads"), Some(metadata.clone()));
        assert!(store.create(&metadata).is_err());
    }

    /// Proves pre-credential-store metadata remains readable after adding the optional reference.
    #[test]
    fn loads_legacy_metadata_without_a_credential_store_reference() {
        let directory = tempfile::tempdir().expect("temporary metadata directory is available");
        let store = ProfileMetadataStore::new(directory.path().join("profile.json"));
        let deriver = Argon2idKeyDeriver;
        let metadata = StoredProfileMetadata {
            profile_id: "primary-profile".to_owned(),
            encryption: deriver.create_metadata().expect("encryption metadata is generated"),
            credential_store_reference: None,
        };
        let mut serialized = serde_json::to_value(metadata).expect("metadata serializes");
        serialized
            .as_object_mut()
            .expect("metadata is an object")
            .remove("credential_store_reference");
        std::fs::write(
            directory.path().join("profile.json"),
            serde_json::to_vec(&serialized).expect("legacy metadata serializes"),
        )
        .expect("legacy metadata is written");

        assert!(
            store
                .load()
                .expect("legacy metadata loads")
                .expect("metadata exists")
                .credential_store_reference
                .is_none()
        );
    }
}
