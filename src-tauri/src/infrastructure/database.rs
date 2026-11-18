//! SQLite connection lifecycle for the local bootstrap database.

use crate::application::ApplicationError;
use crate::infrastructure::migrations;
use crate::ports::database::{DatabasePort, DatabaseResult};
use rusqlite::{Connection, Transaction};
use std::fs;
use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

/// Owns the single synchronized SQLite connection used by the local application.
pub struct Database {
    connection: Mutex<Connection>,
}

impl Database {
    /// Opens, configures, and transactionally migrates a database at the approved application path.
    pub fn open(path: &Path) -> DatabaseResult<Self> {
        create_parent_directory(path)?;

        let mut connection = Connection::open(path).map_err(|_| ApplicationError::internal())?;
        configure_connection(&mut connection)?;
        migrations::run(&mut connection)?;

        Ok(Self { connection: Mutex::new(connection) })
    }

    /// Runs a repository operation atomically, committing only when the closure succeeds.
    pub fn in_transaction<T>(
        &self,
        operation: impl FnOnce(&Transaction<'_>) -> DatabaseResult<T>,
    ) -> DatabaseResult<T> {
        let mut connection = self.connection.lock().map_err(|_| ApplicationError::internal())?;
        let transaction = connection.transaction().map_err(|_| ApplicationError::internal())?;
        let result = operation(&transaction)?;

        transaction.commit().map_err(|_| ApplicationError::internal())?;

        Ok(result)
    }

    /// Executes a read-only query while keeping the connection and SQL implementation private.
    #[cfg(test)]
    fn with_connection<T>(&self, operation: impl FnOnce(&Connection) -> T) -> T {
        let connection = self.connection.lock().expect("test database mutex is not poisoned");

        operation(&connection)
    }
}

impl DatabasePort for Database {
    /// Returns the current schema version without exposing raw SQLite state outside infrastructure.
    fn schema_version(&self) -> DatabaseResult<u32> {
        let connection = self.connection.lock().map_err(|_| ApplicationError::internal())?;

        connection
            .query_row("SELECT COALESCE(MAX(version), 0) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .map_err(|_| ApplicationError::internal())
    }
}

/// Creates the approved database parent directory when the platform has not created it yet.
fn create_parent_directory(path: &Path) -> DatabaseResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|_| ApplicationError::internal())?;
    }

    Ok(())
}

/// Applies the SQLite settings required for integrity, foreign keys, and short local contention.
fn configure_connection(connection: &mut Connection) -> DatabaseResult<()> {
    connection
        .pragma_update(None, "foreign_keys", "ON")
        .map_err(|_| ApplicationError::internal())?;
    connection.busy_timeout(Duration::from_secs(5)).map_err(|_| ApplicationError::internal())?;
    connection
        .pragma_update(None, "journal_mode", "WAL")
        .map_err(|_| ApplicationError::internal())?;
    connection
        .pragma_update(None, "synchronous", "NORMAL")
        .map_err(|_| ApplicationError::internal())?;

    let foreign_keys_enabled: u8 = connection
        .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
        .map_err(|_| ApplicationError::internal())?;
    if foreign_keys_enabled != 1 {
        return Err(ApplicationError::internal());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::Database;
    use crate::application::ApplicationError;
    use crate::infrastructure::test_support::TemporaryDatabase;
    use crate::ports::database::DatabasePort;

    /// Proves a fresh file receives the embedded bootstrap schema and migration record.
    #[test]
    fn creates_and_migrates_a_fresh_database() {
        let fixture = TemporaryDatabase::new();
        let database = Database::open(fixture.path()).expect("database initializes");

        assert_eq!(database.schema_version().expect("schema version is readable"), 1);
        database.with_connection(|connection| {
            let metadata_table_count: u32 = connection
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'application_metadata'",
                    [],
                    |row| row.get(0),
                )
                .expect("SQLite metadata query succeeds");
            assert_eq!(metadata_table_count, 1);
        });
    }

    /// Proves reopening a migrated database is idempotent and preserves a single migration record.
    #[test]
    fn reruns_migrations_idempotently_when_database_is_reopened() {
        let fixture = TemporaryDatabase::new();
        let first = Database::open(fixture.path()).expect("first database initialization succeeds");
        drop(first);
        let reopened = Database::open(fixture.path()).expect("reopening database succeeds");

        assert_eq!(reopened.schema_version().expect("schema version is readable"), 1);
        reopened.with_connection(|connection| {
            let migration_count: u32 = connection
                .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| row.get(0))
                .expect("migration count query succeeds");
            assert_eq!(migration_count, 1);
        });
    }

    /// Proves the connection setup enables foreign-key enforcement for all future repositories.
    #[test]
    fn enables_foreign_key_enforcement() {
        let fixture = TemporaryDatabase::new();
        let database = Database::open(fixture.path()).expect("database initializes");

        database.with_connection(|connection| {
            let foreign_keys_enabled: u8 = connection
                .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
                .expect("foreign key pragma query succeeds");
            assert_eq!(foreign_keys_enabled, 1);
        });
    }

    /// Proves a repository error rolls back all writes made inside the shared transaction boundary.
    #[test]
    fn rolls_back_repository_writes_when_the_operation_returns_an_error() {
        let fixture = TemporaryDatabase::new();
        let database = Database::open(fixture.path()).expect("database initializes");
        let result: Result<(), ApplicationError> = database.in_transaction(|transaction| {
            transaction
                .execute(
                    "INSERT INTO application_metadata (key, value) VALUES ('rollback-test', 'pending')",
                    [],
                )
                .map_err(|_| ApplicationError::internal())?;

            Err(ApplicationError::conflict())
        });

        assert!(result.is_err());
        database.with_connection(|connection| {
            let metadata_count: u32 = connection
                .query_row(
                    "SELECT COUNT(*) FROM application_metadata WHERE key = 'rollback-test'",
                    [],
                    |row| row.get(0),
                )
                .expect("metadata count query succeeds");
            assert_eq!(metadata_count, 0);
        });
    }
}
