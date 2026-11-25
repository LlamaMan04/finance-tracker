//! Profile-owned budget, category, and account persistence.

use super::FinanceRepository;
use super::mappers::{account_from_row, budget_from_row, category_from_row};
use super::validation::{
    archive, new_id, validate_account_type, validate_category_kind, validate_color, validate_text,
};
use crate::application::ApplicationError;
use crate::application::contracts::{AccountDto, BudgetDto, CategoryDto};
use crate::domain::{AccountId, BudgetId, CategoryId, CurrencyCode, ProfileScope};
use crate::infrastructure::database::Database;
use rusqlite::params;

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
