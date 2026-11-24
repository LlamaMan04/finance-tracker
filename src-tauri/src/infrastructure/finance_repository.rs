//! Profile-scoped SQLite persistence for the first finance workspace slice.

use crate::application::ApplicationError;
use crate::application::contracts::{AccountDto, BudgetDto, CategoryDto};
use crate::domain::{AccountId, BudgetId, CategoryId, CurrencyCode, ProfileScope};
use crate::infrastructure::database::Database;
use rusqlite::{Row, params};
use uuid::Uuid;

/// Reads or writes the protected reference data that transactions will later reference.
pub struct FinanceRepository;

impl FinanceRepository {
    /// Returns all budgets in deterministic name order while retaining archived records for history.
    pub fn list_budgets(
        database: &Database,
        scope: &ProfileScope,
    ) -> Result<Vec<BudgetDto>, ApplicationError> {
        database.in_transaction(|transaction| {
            let mut statement = transaction.prepare(
                "SELECT id, name, currency, is_archived FROM budgets WHERE profile_id = ?1 ORDER BY is_archived, name COLLATE NOCASE, id",
            ).map_err(|_| ApplicationError::internal())?;
            let rows = statement.query_map([scope.profile_id().as_str()], budget_from_row)
                .map_err(|_| ApplicationError::internal())?;
            rows.collect::<Result<Vec<_>, _>>().map_err(|_| ApplicationError::internal())
        })
    }

    /// Creates a budget after validating its user-controlled display fields and currency.
    pub fn create_budget(
        database: &Database,
        scope: &ProfileScope,
        name: &str,
        currency: &str,
    ) -> Result<BudgetDto, ApplicationError> {
        let name = validate_text(name, 200)?;
        let currency = CurrencyCode::parse(currency)?;
        let id = new_id("budget");
        database.in_transaction(|transaction| {
            transaction.execute(
                "INSERT INTO budgets (id, profile_id, name, currency, is_archived, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, 0, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
                params![id, scope.profile_id().as_str(), name, currency.as_str()],
            ).map_err(|_| ApplicationError::conflict())?;
            Ok(BudgetDto { id, name, currency: currency.as_str().to_owned(), is_archived: false })
        })
    }

    /// Updates a budget only within the authenticated profile scope.
    pub fn update_budget(
        database: &Database,
        scope: &ProfileScope,
        id: &str,
        name: &str,
    ) -> Result<BudgetDto, ApplicationError> {
        let id = BudgetId::parse(id)?;
        let name = validate_text(name, 200)?;
        database.in_transaction(|transaction| {
            let changed = transaction.execute(
                "UPDATE budgets SET name = ?1, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE profile_id = ?2 AND id = ?3",
                params![name, scope.profile_id().as_str(), id.as_str()],
            ).map_err(|_| ApplicationError::internal())?;
            if changed != 1 { return Err(ApplicationError::not_found()); }
            transaction.query_row(
                "SELECT id, name, currency, is_archived FROM budgets WHERE profile_id = ?1 AND id = ?2",
                params![scope.profile_id().as_str(), id.as_str()], budget_from_row,
            ).map_err(|_| ApplicationError::internal())
        })
    }

    /// Archives a budget without removing historical allocations or transactions.
    pub fn archive_budget(
        database: &Database,
        scope: &ProfileScope,
        id: &str,
    ) -> Result<(), ApplicationError> {
        archive(database, scope, "budgets", BudgetId::parse(id)?.as_str())
    }

    /// Returns categories in their configured deterministic order.
    pub fn list_categories(
        database: &Database,
        scope: &ProfileScope,
    ) -> Result<Vec<CategoryDto>, ApplicationError> {
        database.in_transaction(|transaction| {
            let mut statement = transaction.prepare(
                "SELECT id, name, category_kind, color, sort_order, is_archived FROM categories WHERE profile_id = ?1 ORDER BY is_archived, sort_order, name COLLATE NOCASE, id",
            ).map_err(|_| ApplicationError::internal())?;
            let rows = statement.query_map([scope.profile_id().as_str()], category_from_row)
                .map_err(|_| ApplicationError::internal())?;
            rows.collect::<Result<Vec<_>, _>>().map_err(|_| ApplicationError::internal())
        })
    }

    /// Creates an income or expense category owned by the authenticated profile.
    pub fn create_category(
        database: &Database,
        scope: &ProfileScope,
        name: &str,
        kind: &str,
        color: Option<&str>,
        sort_order: i32,
    ) -> Result<CategoryDto, ApplicationError> {
        let name = validate_text(name, 200)?;
        let kind = validate_category_kind(kind)?;
        let color = validate_color(color)?;
        let id = new_id("category");
        database.in_transaction(|transaction| {
            transaction.execute(
                "INSERT INTO categories (id, profile_id, name, category_kind, color, sort_order, is_archived, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
                params![id, scope.profile_id().as_str(), name, kind, color, sort_order],
            ).map_err(|_| ApplicationError::conflict())?;
            Ok(CategoryDto { id, name, category_kind: kind.to_owned(), color, sort_order, is_archived: false })
        })
    }

    /// Edits the presentation attributes of one profile-owned category.
    pub fn update_category(
        database: &Database,
        scope: &ProfileScope,
        id: &str,
        name: &str,
        kind: &str,
        color: Option<&str>,
        sort_order: i32,
    ) -> Result<CategoryDto, ApplicationError> {
        let id = CategoryId::parse(id)?;
        let name = validate_text(name, 200)?;
        let kind = validate_category_kind(kind)?;
        let color = validate_color(color)?;
        database.in_transaction(|transaction| {
            let changed = transaction.execute(
                "UPDATE categories SET name = ?1, category_kind = ?2, color = ?3, sort_order = ?4, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE profile_id = ?5 AND id = ?6",
                params![name, kind, color, sort_order, scope.profile_id().as_str(), id.as_str()],
            ).map_err(|_| ApplicationError::internal())?;
            if changed != 1 { return Err(ApplicationError::not_found()); }
            transaction.query_row(
                "SELECT id, name, category_kind, color, sort_order, is_archived FROM categories WHERE profile_id = ?1 AND id = ?2",
                params![scope.profile_id().as_str(), id.as_str()], category_from_row,
            ).map_err(|_| ApplicationError::internal())
        })
    }

    /// Archives a category without deleting ledger history.
    pub fn archive_category(
        database: &Database,
        scope: &ProfileScope,
        id: &str,
    ) -> Result<(), ApplicationError> {
        archive(database, scope, "categories", CategoryId::parse(id)?.as_str())
    }

    /// Returns profile-owned accounts in deterministic name order.
    pub fn list_accounts(
        database: &Database,
        scope: &ProfileScope,
    ) -> Result<Vec<AccountDto>, ApplicationError> {
        database.in_transaction(|transaction| {
            let mut statement = transaction.prepare(
                "SELECT id, name, account_type, currency, opening_minor_units, is_archived FROM accounts WHERE profile_id = ?1 ORDER BY is_archived, name COLLATE NOCASE, id",
            ).map_err(|_| ApplicationError::internal())?;
            let rows = statement.query_map([scope.profile_id().as_str()], account_from_row)
                .map_err(|_| ApplicationError::internal())?;
            rows.collect::<Result<Vec<_>, _>>().map_err(|_| ApplicationError::internal())
        })
    }

    /// Creates an account whose opening balance will be included by the later balance service.
    pub fn create_account(
        database: &Database,
        scope: &ProfileScope,
        name: &str,
        account_type: &str,
        currency: &str,
        opening_minor_units: i64,
    ) -> Result<AccountDto, ApplicationError> {
        let name = validate_text(name, 200)?;
        let account_type = validate_account_type(account_type)?;
        let currency = CurrencyCode::parse(currency)?;
        let id = new_id("account");
        database.in_transaction(|transaction| {
            transaction.execute(
                "INSERT INTO accounts (id, profile_id, name, account_type, currency, opening_minor_units, is_archived, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
                params![id, scope.profile_id().as_str(), name, account_type, currency.as_str(), opening_minor_units],
            ).map_err(|_| ApplicationError::conflict())?;
            Ok(AccountDto { id, name, account_type: account_type.to_owned(), currency: currency.as_str().to_owned(), opening_minor_units: opening_minor_units.to_string(), is_archived: false })
        })
    }

    /// Edits mutable account presentation fields without rewriting its opening balance or currency.
    pub fn update_account(
        database: &Database,
        scope: &ProfileScope,
        id: &str,
        name: &str,
        account_type: &str,
    ) -> Result<AccountDto, ApplicationError> {
        let id = AccountId::parse(id)?;
        let name = validate_text(name, 200)?;
        let account_type = validate_account_type(account_type)?;
        database.in_transaction(|transaction| {
            let changed = transaction.execute(
                "UPDATE accounts SET name = ?1, account_type = ?2, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE profile_id = ?3 AND id = ?4",
                params![name, account_type, scope.profile_id().as_str(), id.as_str()],
            ).map_err(|_| ApplicationError::conflict())?;
            if changed != 1 { return Err(ApplicationError::not_found()); }
            transaction.query_row(
                "SELECT id, name, account_type, currency, opening_minor_units, is_archived FROM accounts WHERE profile_id = ?1 AND id = ?2",
                params![scope.profile_id().as_str(), id.as_str()], account_from_row,
            ).map_err(|_| ApplicationError::internal())
        })
    }

    /// Archives an account while preserving ledger records and opening balance history.
    pub fn archive_account(
        database: &Database,
        scope: &ProfileScope,
        id: &str,
    ) -> Result<(), ApplicationError> {
        archive(database, scope, "accounts", AccountId::parse(id)?.as_str())
    }
}

/// Marks exactly one profile-owned record archived using a fixed reviewed table name.
fn archive(
    database: &Database,
    scope: &ProfileScope,
    table: &str,
    id: &str,
) -> Result<(), ApplicationError> {
    let statement = format!(
        "UPDATE {table} SET is_archived = 1, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE profile_id = ?1 AND id = ?2 AND is_archived = 0"
    );
    database.in_transaction(|transaction| {
        let changed = transaction
            .execute(&statement, params![scope.profile_id().as_str(), id])
            .map_err(|_| ApplicationError::internal())?;
        if changed != 1 {
            return Err(ApplicationError::not_found());
        }
        Ok(())
    })
}

/// Creates an opaque, validated identifier without accepting a caller-controlled persistence key.
fn new_id(prefix: &str) -> String {
    format!("{prefix}-{}", Uuid::new_v4())
}

/// Trims and bounds a non-empty display string before persistence.
fn validate_text(value: &str, maximum: usize) -> Result<String, ApplicationError> {
    let normalized = value.trim();
    if normalized.is_empty() || normalized.len() > maximum {
        return Err(ApplicationError::validation());
    }
    Ok(normalized.to_owned())
}

/// Restricts category kinds to the two accounting classifications currently supported.
fn validate_category_kind(value: &str) -> Result<&str, ApplicationError> {
    match value {
        "income" | "expense" => Ok(value),
        _ => Err(ApplicationError::validation()),
    }
}

/// Preserves an optional non-empty compact presentation color without assigning financial meaning.
fn validate_color(value: Option<&str>) -> Result<Option<String>, ApplicationError> {
    value.map(|color| validate_text(color, 32)).transpose()
}

/// Restricts account types to reviewed initial values instead of persisting arbitrary classifications.
fn validate_account_type(value: &str) -> Result<&str, ApplicationError> {
    match value {
        "cash" | "checking" | "savings" | "credit_card" | "other" => Ok(value),
        _ => Err(ApplicationError::validation()),
    }
}

/// Maps a budget query result to its safe frontend representation.
fn budget_from_row(row: &Row<'_>) -> rusqlite::Result<BudgetDto> {
    Ok(BudgetDto {
        id: row.get(0)?,
        name: row.get(1)?,
        currency: row.get(2)?,
        is_archived: row.get::<_, i64>(3)? != 0,
    })
}

/// Maps a category query result to its safe frontend representation.
fn category_from_row(row: &Row<'_>) -> rusqlite::Result<CategoryDto> {
    Ok(CategoryDto {
        id: row.get(0)?,
        name: row.get(1)?,
        category_kind: row.get(2)?,
        color: row.get(3)?,
        sort_order: row.get(4)?,
        is_archived: row.get::<_, i64>(5)? != 0,
    })
}

/// Maps an account query result to a JavaScript-safe exact opening balance representation.
fn account_from_row(row: &Row<'_>) -> rusqlite::Result<AccountDto> {
    let opening_minor_units: i64 = row.get(4)?;
    Ok(AccountDto {
        id: row.get(0)?,
        name: row.get(1)?,
        account_type: row.get(2)?,
        currency: row.get(3)?,
        opening_minor_units: opening_minor_units.to_string(),
        is_archived: row.get::<_, i64>(5)? != 0,
    })
}

#[cfg(test)]
mod tests {
    use super::FinanceRepository;
    use crate::domain::{ProfileId, ProfileScope};
    use crate::infrastructure::test_support::TemporaryDatabase;
    use crate::ports::security::KeyDerivationPort;
    use crate::security::Argon2idKeyDeriver;

    /// Creates a protected database and the required profile scope for repository integration tests.
    fn fixture() -> (TemporaryDatabase, crate::infrastructure::database::Database, ProfileScope) {
        let fixture = TemporaryDatabase::new();
        let deriver = Argon2idKeyDeriver;
        let metadata = deriver.create_metadata().expect("test metadata is generated");
        let key = deriver
            .derive_database_key(b"finance repository test", &metadata)
            .expect("test key derives");
        let database = crate::infrastructure::database::Database::create(fixture.path(), &key)
            .expect("database opens");
        let scope =
            ProfileScope::new(ProfileId::parse("profile-a").expect("fixture identifier is valid"));
        database.in_transaction(|transaction| {
            transaction.execute("INSERT INTO profile_scopes (id, created_at, updated_at) VALUES (?1, ?2, ?2)", (scope.profile_id().as_str(), "2026-11-23T00:00:00Z")).map_err(|_| crate::application::ApplicationError::internal())?;
            Ok(())
        }).expect("profile scope is seeded");
        (fixture, database, scope)
    }

    /// Proves the first workspace records are profile-scoped, editable, and archived without deletion.
    #[test]
    fn creates_lists_updates_and_archives_reference_data() {
        let (_fixture, database, scope) = fixture();
        let budget = FinanceRepository::create_budget(&database, &scope, " Home ", "USD")
            .expect("budget is created");
        let category = FinanceRepository::create_category(
            &database,
            &scope,
            "Groceries",
            "expense",
            Some("green"),
            4,
        )
        .expect("category is created");
        let account = FinanceRepository::create_account(
            &database, &scope, "Checking", "checking", "USD", 12_345,
        )
        .expect("account is created");

        let renamed = FinanceRepository::update_budget(&database, &scope, &budget.id, "Household")
            .expect("budget is renamed");
        let reordered = FinanceRepository::update_category(
            &database,
            &scope,
            &category.id,
            "Food",
            "expense",
            None,
            2,
        )
        .expect("category is updated");
        let updated_account = FinanceRepository::update_account(
            &database,
            &scope,
            &account.id,
            "Main checking",
            "checking",
        )
        .expect("account is updated");
        FinanceRepository::archive_budget(&database, &scope, &budget.id).expect("budget archives");
        FinanceRepository::archive_category(&database, &scope, &category.id)
            .expect("category archives");
        FinanceRepository::archive_account(&database, &scope, &account.id)
            .expect("account archives");

        assert_eq!(renamed.name, "Household");
        assert_eq!(reordered.sort_order, 2);
        assert_eq!(updated_account.opening_minor_units, "12345");
        assert!(
            FinanceRepository::list_budgets(&database, &scope).expect("budgets list")[0]
                .is_archived
        );
        assert!(
            FinanceRepository::list_categories(&database, &scope).expect("categories list")[0]
                .is_archived
        );
        assert!(
            FinanceRepository::list_accounts(&database, &scope).expect("accounts list")[0]
                .is_archived
        );
    }

    /// Proves input validation and profile scope keep first-workspace persistence records isolated.
    #[test]
    fn rejects_invalid_values_and_other_profile_records() {
        let (_fixture, database, scope) = fixture();
        let invalid =
            FinanceRepository::create_account(&database, &scope, "Cash", "cash", "usd", 0)
                .expect_err("lowercase currency is rejected");
        let other_scope =
            ProfileScope::new(ProfileId::parse("profile-b").expect("fixture identifier is valid"));
        let missing = FinanceRepository::create_budget(&database, &other_scope, "Elsewhere", "USD")
            .expect_err("unknown scope cannot write");

        assert_eq!(invalid.code, crate::application::ErrorCode::Validation);
        assert_eq!(missing.code, crate::application::ErrorCode::Conflict);
    }
}
