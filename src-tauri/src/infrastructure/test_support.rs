//! Disposable file-backed database fixtures shared by infrastructure tests.

use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// Owns a temporary directory and the SQLite path inside it for one isolated test.
pub(crate) struct TemporaryDatabase {
    _directory: TempDir,
    path: PathBuf,
}

impl TemporaryDatabase {
    /// Creates an isolated file location that behaves like the production file-backed database.
    pub(crate) fn new() -> Self {
        let directory =
            tempfile::tempdir().expect("temporary directory is available for database test");
        let path = directory.path().join("finance-tracker.sqlite3");

        Self { _directory: directory, path }
    }

    /// Returns the database file path while retaining ownership of its temporary parent directory.
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}
