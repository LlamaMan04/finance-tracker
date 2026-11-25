//! Atomic profile-scoped persistence for transfers between protected accounts.

use crate::application::ApplicationError;
use crate::application::contracts::{TransactionDto, TransferDto};
use crate::domain::{AccountId, CurrencyCode, LocalDate, ProfileScope};
use crate::infrastructure::database::Database;
use rusqlite::params;
use uuid::Uuid;

/// Writes the paired immutable ledger facts and their transfer link in one database transaction.
pub struct TransferRepository;

impl TransferRepository {
    /// Records one positive amount from a source account to a different, same-currency destination.
    pub fn create(
        database: &Database,
        scope: &ProfileScope,
        source_account_id: &str,
        destination_account_id: &str,
        minor_units: i64,
        currency: &str,
        occurred_on: &str,
    ) -> Result<TransferDto, ApplicationError> {
        let source_account_id = AccountId::parse(source_account_id)?;
        let destination_account_id = AccountId::parse(destination_account_id)?;
        let currency = CurrencyCode::parse(currency)?;
        LocalDate::parse(occurred_on)?;
        if source_account_id == destination_account_id || minor_units <= 0 {
            return Err(ApplicationError::validation());
        }
        // Uses checked negation even though the positive validation above excludes i64::MIN.
        let source_minor_units =
            minor_units.checked_neg().ok_or_else(ApplicationError::validation)?;
        let source_transaction_id = new_id("transaction");
        let destination_transaction_id = new_id("transaction");
        let transfer_id = new_id("transfer");

        database.in_transaction(|transaction| {
            validate_active_account_currency(transaction, scope, &source_account_id, &currency)?;
            validate_active_account_currency(transaction, scope, &destination_account_id, &currency)?;
            insert_transfer_entry(
                transaction,
                scope,
                &source_transaction_id,
                &source_account_id,
                source_minor_units,
                &currency,
                occurred_on,
            )?;
            insert_transfer_entry(
                transaction,
                scope,
                &destination_transaction_id,
                &destination_account_id,
                minor_units,
                &currency,
                occurred_on,
            )?;
            transaction
                .execute(
                    "INSERT INTO transfers (id, profile_id, source_transaction_id, destination_transaction_id, created_at) VALUES (?1, ?2, ?3, ?4, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
                    params![transfer_id, scope.profile_id().as_str(), source_transaction_id, destination_transaction_id],
                )
                .map_err(|_| ApplicationError::conflict())?;

            Ok(TransferDto {
                source_transaction: transfer_transaction_dto(
                    &source_transaction_id,
                    source_account_id.as_str(),
                    source_minor_units,
                    currency.as_str(),
                    occurred_on,
                ),
                destination_transaction: transfer_transaction_dto(
                    &destination_transaction_id,
                    destination_account_id.as_str(),
                    minor_units,
                    currency.as_str(),
                    occurred_on,
                ),
            })
        })
    }
}

/// Confirms an account is active, profile-owned, and denominated in the requested currency.
fn validate_active_account_currency(
    transaction: &rusqlite::Transaction<'_>,
    scope: &ProfileScope,
    account_id: &AccountId,
    currency: &CurrencyCode,
) -> Result<(), ApplicationError> {
    let account = transaction
        .query_row(
            "SELECT currency, is_archived FROM accounts WHERE profile_id = ?1 AND id = ?2",
            params![scope.profile_id().as_str(), account_id.as_str()],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
        )
        .map_err(|_| ApplicationError::not_found())?;
    if account.0 != currency.as_str() || account.1 != 0 {
        return Err(ApplicationError::validation());
    }
    Ok(())
}

/// Persists one unclassified transfer fact so it cannot affect income, expense, or budget totals.
fn insert_transfer_entry(
    transaction: &rusqlite::Transaction<'_>,
    scope: &ProfileScope,
    transaction_id: &str,
    account_id: &AccountId,
    minor_units: i64,
    currency: &CurrencyCode,
    occurred_on: &str,
) -> Result<(), ApplicationError> {
    transaction
        .execute(
            "INSERT INTO transactions (id, profile_id, account_id, budget_id, category_id, occurred_on, entry_type, minor_units, currency, payee, description, notes, created_at, updated_at) VALUES (?1, ?2, ?3, NULL, NULL, ?4, 'transfer', ?5, ?6, NULL, NULL, NULL, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
            params![transaction_id, scope.profile_id().as_str(), account_id.as_str(), occurred_on, minor_units, currency.as_str()],
        )
        .map_err(|_| ApplicationError::conflict())?;
    Ok(())
}

/// Builds the safe DTO for a transfer fact without querying raw protected persistence fields.
fn transfer_transaction_dto(
    id: &str,
    account_id: &str,
    minor_units: i64,
    currency: &str,
    occurred_on: &str,
) -> TransactionDto {
    TransactionDto {
        id: id.to_owned(),
        account_id: account_id.to_owned(),
        budget_id: None,
        category_id: None,
        minor_units: minor_units.to_string(),
        currency: currency.to_owned(),
        occurred_on: occurred_on.to_owned(),
        entry_type: "transfer".to_owned(),
        payee: None,
        description: None,
        notes: None,
        allocations: vec![],
    }
}

/// Creates an opaque storage identifier without accepting a caller-controlled persistence key.
fn new_id(prefix: &str) -> String {
    format!("{prefix}-{}", Uuid::new_v4())
}

#[cfg(test)]
mod tests {
    use super::TransferRepository;
    use crate::application::ErrorCode;
    use crate::domain::{ProfileId, ProfileScope};
    use crate::infrastructure::database::Database;
    use crate::infrastructure::test_support::TemporaryDatabase;
    use crate::ports::security::KeyDerivationPort;
    use crate::security::Argon2idKeyDeriver;

    /// Creates an encrypted fixture database with one authorized profile scope.
    fn fixture() -> (TemporaryDatabase, Database, ProfileScope) {
        let fixture = TemporaryDatabase::new();
        let metadata = Argon2idKeyDeriver.create_metadata().expect("test metadata is generated");
        let key = Argon2idKeyDeriver
            .derive_database_key(b"transfer repository test", &metadata)
            .expect("test key derives");
        let database = Database::create(fixture.path(), &key).expect("database opens");
        let scope = ProfileScope::new(ProfileId::parse("profile-a").expect("scope is valid"));
        database
            .in_transaction(|transaction| {
                transaction
                    .execute(
                        "INSERT INTO profile_scopes (id, created_at, updated_at) VALUES (?1, ?2, ?2)",
                        (scope.profile_id().as_str(), "2026-11-24T00:00:00Z"),
                    )
                    .map_err(|_| crate::application::ApplicationError::internal())?;
                Ok(())
            })
            .expect("profile scope is seeded");
        (fixture, database, scope)
    }

    /// Creates an active account fixture owned by the supplied profile scope.
    fn seed_account(database: &Database, scope: &ProfileScope, id: &str, currency: &str) {
        database
            .in_transaction(|transaction| {
                transaction
                    .execute(
                        "INSERT INTO accounts (id, profile_id, name, account_type, currency, opening_minor_units, is_archived, created_at, updated_at) VALUES (?1, ?2, ?3, 'checking', ?4, 0, 0, ?5, ?5)",
                        (id, scope.profile_id().as_str(), id, currency, "2026-11-24T00:00:00Z"),
                    )
                    .map_err(|_| crate::application::ApplicationError::internal())?;
                Ok(())
            })
            .expect("account is seeded");
    }

    /// Proves a transfer creates opposite signed facts, one link, and no budget allocations.
    #[test]
    fn creates_linked_atomic_transfer_entries() {
        let (_fixture, database, scope) = fixture();
        seed_account(&database, &scope, "source", "USD");
        seed_account(&database, &scope, "destination", "USD");

        let transfer = TransferRepository::create(
            &database,
            &scope,
            "source",
            "destination",
            1_250,
            "USD",
            "2024-02-29",
        )
        .expect("transfer is created");

        assert_eq!(transfer.source_transaction.minor_units, "-1250");
        assert_eq!(transfer.destination_transaction.minor_units, "1250");
        assert_eq!(transfer.source_transaction.entry_type, "transfer");
        assert_eq!(transfer.destination_transaction.allocations.len(), 0);
        database
            .in_transaction(|transaction| {
                let link_count: i64 = transaction
                    .query_row("SELECT COUNT(*) FROM transfers", [], |row| row.get(0))
                    .map_err(|_| crate::application::ApplicationError::internal())?;
                let allocation_count: i64 = transaction
                    .query_row("SELECT COUNT(*) FROM transaction_allocations", [], |row| row.get(0))
                    .map_err(|_| crate::application::ApplicationError::internal())?;
                assert_eq!(link_count, 1);
                assert_eq!(allocation_count, 0);
                Ok(())
            })
            .expect("transfer shape is verified");
    }

    /// Proves invalid accounts, currencies, amounts, and dates leave no transfer facts behind.
    #[test]
    fn rejects_invalid_transfers_before_writing() {
        let (_fixture, database, scope) = fixture();
        seed_account(&database, &scope, "source", "USD");
        seed_account(&database, &scope, "eur-destination", "EUR");
        seed_account(&database, &scope, "archived-destination", "USD");
        database
            .in_transaction(|transaction| {
                transaction
                    .execute(
                        "UPDATE accounts SET is_archived = 1 WHERE profile_id = ?1 AND id = 'archived-destination'",
                        [scope.profile_id().as_str()],
                    )
                    .map_err(|_| crate::application::ApplicationError::internal())?;
                Ok(())
            })
            .expect("account is archived");

        let same_account = TransferRepository::create(
            &database,
            &scope,
            "source",
            "source",
            1,
            "USD",
            "2026-11-24",
        )
        .expect_err("same-account transfers are invalid");
        let mismatch = TransferRepository::create(
            &database,
            &scope,
            "source",
            "eur-destination",
            1,
            "USD",
            "2026-11-24",
        )
        .expect_err("currency mismatch is invalid");
        let missing = TransferRepository::create(
            &database,
            &scope,
            "source",
            "missing",
            1,
            "USD",
            "2026-11-24",
        )
        .expect_err("missing accounts are rejected");
        let invalid_amount = TransferRepository::create(
            &database,
            &scope,
            "source",
            "missing",
            0,
            "USD",
            "2026-11-24",
        )
        .expect_err("zero transfer is invalid");
        let archived = TransferRepository::create(
            &database,
            &scope,
            "source",
            "archived-destination",
            1,
            "USD",
            "2026-11-24",
        )
        .expect_err("archived accounts cannot receive transfers");

        assert_eq!(same_account.code, ErrorCode::Validation);
        assert_eq!(mismatch.code, ErrorCode::Validation);
        assert_eq!(missing.code, ErrorCode::NotFound);
        assert_eq!(invalid_amount.code, ErrorCode::Validation);
        assert_eq!(archived.code, ErrorCode::Validation);
        assert_no_transfer_records(&database);
    }

    /// Proves a failed destination write rolls back the previously inserted source entry.
    #[test]
    fn rolls_back_both_entries_when_destination_write_fails() {
        let (_fixture, database, scope) = fixture();
        seed_account(&database, &scope, "source", "USD");
        seed_account(&database, &scope, "destination", "USD");
        database
            .in_transaction(|transaction| {
                transaction
                    .execute_batch(
                        "CREATE TRIGGER reject_transfer_destination BEFORE INSERT ON transactions WHEN NEW.entry_type = 'transfer' AND NEW.minor_units > 0 BEGIN SELECT RAISE(ABORT, 'fixture'); END;",
                    )
                    .map_err(|_| crate::application::ApplicationError::internal())?;
                Ok(())
            })
            .expect("failure trigger is installed");

        let error = TransferRepository::create(
            &database,
            &scope,
            "source",
            "destination",
            100,
            "USD",
            "2026-11-24",
        )
        .expect_err("destination write fails");

        assert_eq!(error.code, ErrorCode::Conflict);
        assert_no_transfer_records(&database);
    }

    /// Confirms a rejected transfer has not persisted either ledger fact or a transfer link.
    fn assert_no_transfer_records(database: &Database) {
        database
            .in_transaction(|transaction| {
                let transaction_count: i64 = transaction
                    .query_row("SELECT COUNT(*) FROM transactions", [], |row| row.get(0))
                    .map_err(|_| crate::application::ApplicationError::internal())?;
                let transfer_count: i64 = transaction
                    .query_row("SELECT COUNT(*) FROM transfers", [], |row| row.get(0))
                    .map_err(|_| crate::application::ApplicationError::internal())?;
                assert_eq!(transaction_count, 0);
                assert_eq!(transfer_count, 0);
                Ok(())
            })
            .expect("rejection leaves no records");
    }
}
