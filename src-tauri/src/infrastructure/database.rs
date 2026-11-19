//! SQLite connection lifecycle for the local bootstrap database.

use crate::application::ApplicationError;
use crate::infrastructure::migrations;
use crate::ports::database::{DatabasePort, DatabaseResult};
use crate::ports::security::DatabaseKey;
use rusqlite::{Connection, OpenFlags, Transaction};
use std::fs;
use std::os::raw::c_int;
use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;
use zeroize::Zeroizing;

/// SQLCipher raw keys use the byte form `x'` + 64 hexadecimal digits + `'`.
const SQLCIPHER_RAW_KEY_LENGTH: usize = 67;

/// Owns the single synchronized SQLite connection used by the local application.
pub struct Database {
    connection: Mutex<Connection>,
}

impl Database {
    /// Creates a new protected database without replacing an existing profile.
    pub fn create(path: &Path, key: &DatabaseKey) -> DatabaseResult<Self> {
        if path.exists() {
            return Err(ApplicationError::conflict());
        }
        create_parent_directory(path)?;

        let flags = OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE;
        let connection =
            Connection::open_with_flags(path, flags).map_err(|_| ApplicationError::internal())?;
        Self::initialize(connection, key)
    }

    /// Opens an existing protected database without creating a replacement during unlock.
    pub fn open(path: &Path, key: &DatabaseKey) -> DatabaseResult<Self> {
        if !path.is_file() {
            return Err(ApplicationError::not_found());
        }

        let flags = OpenFlags::SQLITE_OPEN_READ_WRITE;
        let connection =
            Connection::open_with_flags(path, flags).map_err(|_| ApplicationError::internal())?;
        Self::initialize(connection, key)
    }

    /// Applies the key boundary before normal connection setup and migrations.
    fn initialize(mut connection: Connection, key: &DatabaseKey) -> DatabaseResult<Self> {
        apply_database_key(&connection, key)?;
        validate_database_key(&connection)?;
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

/// Supplies raw key bytes through SQLCipher's C API before the first database page is read.
fn apply_database_key(connection: &Connection, key: &DatabaseKey) -> DatabaseResult<()> {
    let encoded_key = encode_sqlcipher_raw_key(key);
    let key_length =
        c_int::try_from(encoded_key.len()).map_err(|_| ApplicationError::internal())?;
    // SAFETY: rusqlite owns a live handle for this call, and the borrowed key remains valid for its
    // exact byte length. SQLCipher copies its internal key state during this invocation.
    let result = unsafe {
        rusqlite::ffi::sqlite3_key(connection.handle(), encoded_key.as_ptr().cast(), key_length)
    };
    if result != rusqlite::ffi::SQLITE_OK {
        return Err(ApplicationError::unauthorized());
    }

    // Disable native SQLCipher diagnostics before key validation so wrong-key details never reach
    // normal application stderr or packaged logs.
    connection
        .pragma_update(None, "cipher_log_level", "NONE")
        .map_err(|_| ApplicationError::internal())?;
    connection
        .pragma_update(None, "cipher_memory_security", "ON")
        .map_err(|_| ApplicationError::unauthorized())?;

    Ok(())
}

/// Encodes the application-derived bytes in SQLCipher's no-secondary-KDF raw-key format.
fn encode_sqlcipher_raw_key(key: &DatabaseKey) -> Zeroizing<[u8; SQLCIPHER_RAW_KEY_LENGTH]> {
    const HEX: &[u8; 16] = b"0123456789abcdef";

    let mut encoded = Zeroizing::new([0_u8; SQLCIPHER_RAW_KEY_LENGTH]);
    encoded[0] = b'x';
    encoded[1] = b'\'';
    for (index, byte) in key.expose().iter().enumerate() {
        encoded[2 + index * 2] = HEX[usize::from(byte >> 4)];
        encoded[3 + index * 2] = HEX[usize::from(byte & 0x0f)];
    }
    encoded[SQLCIPHER_RAW_KEY_LENGTH - 1] = b'\'';

    encoded
}

/// Forces the first encrypted page read so a wrong key fails before configuration or migration.
fn validate_database_key(connection: &Connection) -> DatabaseResult<()> {
    connection
        .query_row("SELECT COUNT(*) FROM sqlite_master", [], |row| row.get::<_, u32>(0))
        .map(|_| ())
        .map_err(|_| ApplicationError::unauthorized())
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
    use crate::application::{ApplicationError, ErrorCode};
    use crate::infrastructure::test_support::TemporaryDatabase;
    use crate::ports::database::DatabasePort;
    use crate::ports::security::KeyDerivationPort;
    use crate::security::Argon2idKeyDeriver;
    use rusqlite::Connection;
    use std::fs;

    /// Derives a stable test-only key through the same production KDF boundary.
    fn test_key() -> crate::ports::security::DatabaseKey {
        let deriver = Argon2idKeyDeriver;
        let metadata = deriver.create_metadata().expect("test metadata is generated");

        deriver.derive_database_key(b"test-only passphrase", &metadata).expect("test key derives")
    }

    /// Proves a fresh file receives the embedded bootstrap schema and migration record.
    #[test]
    fn creates_and_migrates_a_fresh_database() {
        let fixture = TemporaryDatabase::new();
        let key = test_key();
        let database = Database::create(fixture.path(), &key).expect("database initializes");

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

    /// Proves an unlock path cannot silently create a replacement for a missing profile database.
    #[test]
    fn refuses_to_open_a_missing_database() {
        let fixture = TemporaryDatabase::new();
        let key = test_key();

        let error =
            Database::open(fixture.path(), &key).err().expect("missing database is rejected");

        assert_eq!(error.code, ErrorCode::NotFound);
        assert!(!fixture.path().exists());
    }

    /// Proves profile creation cannot overwrite or reinterpret an existing protected database.
    #[test]
    fn refuses_to_create_over_an_existing_database() {
        let fixture = TemporaryDatabase::new();
        let key = test_key();
        let database = Database::create(fixture.path(), &key).expect("database initializes");
        drop(database);

        let error =
            Database::create(fixture.path(), &key).err().expect("duplicate create is rejected");

        assert_eq!(error.code, ErrorCode::Conflict);
    }

    /// Proves reopening a migrated database is idempotent and preserves a single migration record.
    #[test]
    fn reruns_migrations_idempotently_when_database_is_reopened() {
        let fixture = TemporaryDatabase::new();
        let key = test_key();
        let first =
            Database::create(fixture.path(), &key).expect("first database initialization succeeds");
        drop(first);
        let reopened = Database::open(fixture.path(), &key).expect("reopening database succeeds");

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
        let key = test_key();
        let database = Database::create(fixture.path(), &key).expect("database initializes");

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
        let key = test_key();
        let database = Database::create(fixture.path(), &key).expect("database initializes");
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

    /// Proves a normal SQLite connection cannot inspect the SQLCipher-protected schema.
    #[test]
    fn hides_schema_and_values_from_plaintext_sqlite() {
        let fixture = TemporaryDatabase::new();
        let key = test_key();
        let database = Database::create(fixture.path(), &key).expect("database initializes");
        database
            .in_transaction(|transaction| {
                transaction
                    .execute(
                        "INSERT INTO application_metadata (key, value) VALUES (?1, ?2)",
                        ("encryption-probe", "sensitive-fixture-value"),
                    )
                    .map_err(|_| ApplicationError::internal())?;
                Ok(())
            })
            .expect("encrypted fixture is written");
        drop(database);

        let bytes = fs::read(fixture.path()).expect("encrypted database file is readable");
        assert!(!bytes.windows(b"SQLite format 3".len()).any(|value| value == b"SQLite format 3"));
        assert!(
            !bytes
                .windows(b"sensitive-fixture-value".len())
                .any(|value| value == b"sensitive-fixture-value")
        );

        let plaintext = Connection::open(fixture.path()).expect("plain SQLite opens file handle");
        assert!(
            plaintext
                .query_row("SELECT COUNT(*) FROM sqlite_master", [], |row| row.get::<_, u32>(0))
                .is_err()
        );
    }

    /// Proves an unrelated key cannot open or migrate an existing protected database.
    #[test]
    fn rejects_the_wrong_database_key() {
        let fixture = TemporaryDatabase::new();
        let correct_key = test_key();
        let database =
            Database::create(fixture.path(), &correct_key).expect("database initializes");
        drop(database);
        let wrong_key = test_key();

        let error =
            Database::open(fixture.path(), &wrong_key).err().expect("wrong key is rejected");

        assert_eq!(error.code, ErrorCode::Unauthorized);
    }

    /// Proves page authentication rejects an encrypted file modified outside SQLCipher.
    #[test]
    fn rejects_tampered_ciphertext() {
        let fixture = TemporaryDatabase::new();
        let key = test_key();
        let database = Database::create(fixture.path(), &key).expect("database initializes");
        drop(database);
        let mut bytes = fs::read(fixture.path()).expect("encrypted database file is readable");
        // SQLCipher reserves the first 16 bytes for salt; offset 32 is authenticated ciphertext on
        // the first schema page and is necessarily read by the key-validation query.
        let tamper_offset = 32;
        bytes[tamper_offset] ^= 0x01;
        fs::write(fixture.path(), bytes).expect("tampered fixture is written");

        let error =
            Database::open(fixture.path(), &key).err().expect("tampered database is rejected");

        assert_eq!(error.code, ErrorCode::Unauthorized);
    }
}
