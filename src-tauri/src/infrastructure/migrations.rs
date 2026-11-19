//! Embedded, ordered SQLite schema migrations for the local application database.

use crate::application::ApplicationError;
use rusqlite::{Connection, Transaction};

/// Describes one immutable migration that may be applied to a database exactly once.
pub(super) struct Migration {
    pub(super) version: u32,
    pub(super) name: &'static str,
    pub(super) sql: &'static str,
}

/// Lists every released bootstrap migration in the only order in which they may run.
const MIGRATIONS: [Migration; 1] = [Migration {
    version: 1,
    name: "bootstrap_schema",
    sql: include_str!("migrations/0001_bootstrap.sql"),
}];

/// Reports the migration state after a successful migration run.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct MigrationReport {
    pub(super) applied_count: u32,
    pub(super) schema_version: u32,
}

/// Applies all known migrations inside one transaction or leaves the database unchanged.
pub(super) fn run(connection: &mut Connection) -> Result<MigrationReport, ApplicationError> {
    run_with_migrations(connection, &MIGRATIONS)
}

/// Applies a supplied migration set, allowing tests to prove rollback behavior safely.
fn run_with_migrations(
    connection: &mut Connection,
    migrations: &[Migration],
) -> Result<MigrationReport, ApplicationError> {
    validate_migrations(migrations)?;

    let transaction = connection.transaction().map_err(|_| ApplicationError::internal())?;
    create_tracking_table(&transaction)?;
    let applied_versions = applied_versions(&transaction)?;
    reject_unknown_versions(&applied_versions, migrations)?;

    let mut applied_count = 0;
    for migration in migrations {
        if applied_versions.contains(&migration.version) {
            continue;
        }

        apply_migration(&transaction, migration)?;
        applied_count += 1;
    }

    let schema_version = migrations.last().map_or(0, |migration| migration.version);
    transaction.commit().map_err(|_| ApplicationError::internal())?;

    Ok(MigrationReport { applied_count, schema_version })
}

/// Rejects duplicate or non-increasing migration versions before any schema mutation occurs.
fn validate_migrations(migrations: &[Migration]) -> Result<(), ApplicationError> {
    for pair in migrations.windows(2) {
        if pair[0].version >= pair[1].version || pair[0].name.is_empty() || pair[1].name.is_empty()
        {
            return Err(ApplicationError::internal());
        }
    }

    if migrations.first().is_some_and(|migration| migration.name.is_empty()) {
        return Err(ApplicationError::internal());
    }

    Ok(())
}

/// Creates the migration ledger within the same transaction as the first schema migration.
fn create_tracking_table(transaction: &Transaction<'_>) -> Result<(), ApplicationError> {
    transaction
        .execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_migrations (\
                version INTEGER PRIMARY KEY NOT NULL,\
                name TEXT NOT NULL,\
                applied_at TEXT NOT NULL\
            ) STRICT;",
        )
        .map_err(|_| ApplicationError::internal())
}

/// Reads migration versions already committed to the database.
fn applied_versions(transaction: &Transaction<'_>) -> Result<Vec<u32>, ApplicationError> {
    let mut statement = transaction
        .prepare("SELECT version FROM schema_migrations ORDER BY version")
        .map_err(|_| ApplicationError::internal())?;
    let rows = statement
        .query_map([], |row| row.get::<_, u32>(0))
        .map_err(|_| ApplicationError::internal())?;

    rows.collect::<Result<Vec<_>, _>>().map_err(|_| ApplicationError::internal())
}

/// Refuses a database created by an unknown future or altered migration sequence.
fn reject_unknown_versions(
    applied_versions: &[u32],
    migrations: &[Migration],
) -> Result<(), ApplicationError> {
    if applied_versions
        .iter()
        .all(|version| migrations.iter().any(|migration| migration.version == *version))
    {
        Ok(())
    } else {
        Err(ApplicationError::conflict())
    }
}

/// Runs one migration and records it atomically with its schema changes.
fn apply_migration(
    transaction: &Transaction<'_>,
    migration: &Migration,
) -> Result<(), ApplicationError> {
    transaction.execute_batch(migration.sql).map_err(|_| ApplicationError::internal())?;
    transaction
        .execute(
            "INSERT INTO schema_migrations (version, name, applied_at) VALUES (?1, ?2, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
            (migration.version, migration.name),
        )
        .map_err(|_| ApplicationError::internal())?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{Migration, run_with_migrations};
    use rusqlite::Connection;

    /// Proves that a failed migration leaves neither its table nor the tracking ledger behind.
    #[test]
    fn rolls_back_the_entire_first_migration_batch_when_sql_fails() {
        let mut connection =
            Connection::open_in_memory().expect("in-memory SQLite opens for tests");
        let migrations = [Migration {
            version: 1,
            name: "failing_fixture",
            sql: "CREATE TABLE rollback_probe (id INTEGER PRIMARY KEY); INVALID SQL;",
        }];

        assert!(run_with_migrations(&mut connection, &migrations).is_err());

        let table_count: u32 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'rollback_probe'",
                [],
                |row| row.get(0),
            )
            .expect("SQLite metadata query succeeds");
        assert_eq!(table_count, 0);
    }
}
