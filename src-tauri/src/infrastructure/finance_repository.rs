//! Profile-scoped SQLite persistence for the first finance workspace slice.

use crate::application::ApplicationError;
use crate::application::contracts::{
    AccountDto, AllocationDto, BudgetDto, CategoryDto, TransactionDto, TransactionHistoryPageDto,
};
use crate::application::use_cases::TransactionAllocationRequest;
use crate::domain::{
    AccountId, BudgetId, CategoryId, CurrencyCode, LocalDate, ProfileScope, TransactionId,
};
use crate::infrastructure::database::Database;
use rusqlite::{Row, params, types::Value};
use std::collections::HashSet;
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

    /// Records one immutable income or expense fact after validating all active profile references.
    #[allow(clippy::too_many_arguments)]
    pub fn create_transaction(
        database: &Database,
        scope: &ProfileScope,
        account_id: &str,
        budget_id: Option<&str>,
        category_id: Option<&str>,
        entry_type: &str,
        minor_units: i64,
        currency: &str,
        occurred_on: &str,
        payee: Option<&str>,
        description: Option<&str>,
        notes: Option<&str>,
        allocations: &[TransactionAllocationRequest],
    ) -> Result<TransactionDto, ApplicationError> {
        let account_id = AccountId::parse(account_id)?;
        let budget_id = budget_id.map(BudgetId::parse).transpose()?;
        let category_id = category_id.map(CategoryId::parse).transpose()?;
        let currency = CurrencyCode::parse(currency)?;
        LocalDate::parse(occurred_on)?;
        validate_entry_amount(entry_type, minor_units)?;
        let payee = validate_optional_text(payee, 200)?;
        let description = validate_optional_text(description, 500)?;
        let notes = validate_optional_text(notes, 4_000)?;
        let allocations = validate_allocation_requests(
            allocations,
            category_id.as_ref(),
            entry_type,
            minor_units,
        )?;
        let id = new_id("transaction");

        database.in_transaction(|transaction| {
            validate_active_account(transaction, scope, &account_id, &currency)?;
            validate_active_category(transaction, scope, category_id.as_ref(), entry_type)?;
            validate_active_budget(transaction, scope, budget_id.as_ref(), &currency)?;
            for allocation in &allocations {
                validate_active_category(transaction, scope, Some(&allocation.category_id), entry_type)?;
            }
            transaction.execute(
                "INSERT INTO transactions (id, profile_id, account_id, budget_id, category_id, occurred_on, entry_type, minor_units, currency, payee, description, notes, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
                params![id, scope.profile_id().as_str(), account_id.as_str(), budget_id.as_ref().map(|value| value.as_str()), category_id.as_ref().map(|value| value.as_str()), occurred_on, entry_type, minor_units, currency.as_str(), payee, description, notes],
            ).map_err(|_| ApplicationError::conflict())?;
            let allocation_dtos = allocations
                .iter()
                .map(|allocation| {
                    let allocation_id = new_id("allocation");
                    transaction.execute(
                        "INSERT INTO transaction_allocations (id, profile_id, transaction_id, category_id, minor_units, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
                        params![allocation_id, scope.profile_id().as_str(), id, allocation.category_id.as_str(), allocation.minor_units],
                    ).map_err(|_| ApplicationError::conflict())?;
                    Ok(AllocationDto {
                        id: allocation_id,
                        transaction_id: id.clone(),
                        category_id: allocation.category_id.as_str().to_owned(),
                        minor_units: allocation.minor_units.to_string(),
                    })
                })
                .collect::<Result<Vec<_>, ApplicationError>>()?;
            Ok(TransactionDto {
                id,
                account_id: account_id.as_str().to_owned(),
                budget_id: budget_id.as_ref().map(|value| value.as_str().to_owned()),
                category_id: category_id.as_ref().map(|value| value.as_str().to_owned()),
                minor_units: minor_units.to_string(),
                currency: currency.as_str().to_owned(),
                occurred_on: occurred_on.to_owned(),
                entry_type: entry_type.to_owned(),
                payee,
                description,
                notes,
                allocations: allocation_dtos,
            })
        })
    }

    /// Reads a stable page of protected ledger history without exposing another profile's records.
    pub fn list_transactions(
        database: &Database,
        scope: &ProfileScope,
        starts_on: Option<&str>,
        ends_on: Option<&str>,
        account_id: Option<&str>,
        category_id: Option<&str>,
        entry_type: Option<&str>,
        search: Option<&str>,
        page: u32,
        page_size: u32,
    ) -> Result<TransactionHistoryPageDto, ApplicationError> {
        let parsed_starts_on = starts_on.map(LocalDate::parse).transpose()?;
        let parsed_ends_on = ends_on.map(LocalDate::parse).transpose()?;
        if parsed_starts_on
            .as_ref()
            .zip(parsed_ends_on.as_ref())
            .is_some_and(|(start, end)| start > end)
        {
            return Err(ApplicationError::validation());
        }
        let account_id = account_id.map(AccountId::parse).transpose()?;
        let category_id = category_id.map(CategoryId::parse).transpose()?;
        let entry_type = entry_type.map(validate_history_entry_type).transpose()?;
        let search = validate_optional_text(search, 200)?;
        // Bounds keep a hostile caller from forcing an unbounded protected-database read.
        let page_size = page_size.clamp(1, 100);
        let offset = u64::from(page)
            .checked_mul(u64::from(page_size))
            .ok_or_else(ApplicationError::validation)?;
        let offset = i64::try_from(offset).map_err(|_| ApplicationError::validation())?;

        database.in_transaction(|transaction| {
            let (where_clause, filter_values) = transaction_history_filters(
                scope,
                starts_on,
                ends_on,
                account_id.as_ref().map(|value| value.as_str()),
                category_id.as_ref().map(|value| value.as_str()),
                entry_type,
                search.as_deref(),
            );
            let count_sql = format!("SELECT COUNT(*) FROM transactions t WHERE {where_clause}");
            let total_count: i64 = transaction
                .query_row(&count_sql, rusqlite::params_from_iter(filter_values.iter()), |row| row.get(0))
                .map_err(|_| ApplicationError::internal())?;

            let list_sql = format!(
                "SELECT t.id, t.account_id, t.budget_id, t.category_id, t.minor_units, t.currency, t.occurred_on, t.entry_type, t.payee, t.description, t.notes FROM transactions t WHERE {where_clause} ORDER BY t.occurred_on DESC, t.id DESC LIMIT ? OFFSET ?"
            );
            let mut list_values = filter_values;
            list_values.push(Value::Integer(i64::from(page_size)));
            list_values.push(Value::Integer(offset));
            let mut statement = transaction.prepare(&list_sql).map_err(|_| ApplicationError::internal())?;
            let rows = statement
                .query_map(rusqlite::params_from_iter(list_values.iter()), transaction_from_row)
                .map_err(|_| ApplicationError::internal())?;
            let mut transactions = rows.collect::<Result<Vec<_>, _>>().map_err(|_| ApplicationError::internal())?;
            drop(statement);
            for transaction_dto in &mut transactions {
                transaction_dto.allocations = load_allocations(transaction, scope, &transaction_dto.id)?;
            }
            Ok(TransactionHistoryPageDto {
                transactions,
                total_count: u64::try_from(total_count).map_err(|_| ApplicationError::internal())?,
                page,
                page_size,
            })
        })
    }

    /// Creates an immutable equal-and-opposite correction for one income or expense entry.
    pub fn reverse_transaction(
        database: &Database,
        scope: &ProfileScope,
        transaction_id: &str,
    ) -> Result<TransactionDto, ApplicationError> {
        let transaction_id = TransactionId::parse(transaction_id)?;
        database.in_transaction(|transaction| {
            let original = transaction.query_row(
                "SELECT id, account_id, budget_id, category_id, minor_units, currency, occurred_on, entry_type, payee, description, notes FROM transactions WHERE profile_id = ?1 AND id = ?2",
                params![scope.profile_id().as_str(), transaction_id.as_str()], transaction_from_row,
            ).map_err(|_| ApplicationError::not_found())?;
            if !matches!(original.entry_type.as_str(), "income" | "expense") {
                return Err(ApplicationError::validation());
            }
            let already_reversed: i64 = transaction.query_row(
                "SELECT COUNT(*) FROM transaction_corrections WHERE profile_id = ?1 AND original_transaction_id = ?2 AND correction_kind = 'reversal'",
                params![scope.profile_id().as_str(), transaction_id.as_str()], |row| row.get(0),
            ).map_err(|_| ApplicationError::internal())?;
            if already_reversed != 0 {
                return Err(ApplicationError::conflict());
            }
            let reversed_minor_units = original.minor_units.parse::<i64>()
                .ok().and_then(i64::checked_neg).ok_or_else(ApplicationError::validation)?;
            let correction_id = new_id("transaction");
            transaction.execute(
                "INSERT INTO transactions (id, profile_id, account_id, budget_id, category_id, occurred_on, entry_type, minor_units, currency, payee, description, notes, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'correction', ?7, ?8, ?9, ?10, ?11, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
                params![correction_id, scope.profile_id().as_str(), original.account_id, original.budget_id, original.category_id, original.occurred_on, reversed_minor_units, original.currency, original.payee, original.description, original.notes],
            ).map_err(|_| ApplicationError::conflict())?;
            let original_allocations = load_allocations(transaction, scope, transaction_id.as_str())?;
            let mut allocations = Vec::with_capacity(original_allocations.len());
            for original_allocation in original_allocations {
                let minor_units = original_allocation.minor_units.parse::<i64>()
                    .ok().and_then(i64::checked_neg).ok_or_else(ApplicationError::validation)?;
                let allocation_id = new_id("allocation");
                transaction.execute(
                    "INSERT INTO transaction_allocations (id, profile_id, transaction_id, category_id, minor_units, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
                    params![allocation_id, scope.profile_id().as_str(), correction_id, original_allocation.category_id, minor_units],
                ).map_err(|_| ApplicationError::conflict())?;
                allocations.push(AllocationDto { id: allocation_id, transaction_id: correction_id.clone(), category_id: original_allocation.category_id, minor_units: minor_units.to_string() });
            }
            transaction.execute(
                "INSERT INTO transaction_corrections (id, profile_id, original_transaction_id, correction_transaction_id, correction_kind, reason, created_at) VALUES (?1, ?2, ?3, ?4, 'reversal', NULL, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
                params![new_id("correction"), scope.profile_id().as_str(), transaction_id.as_str(), correction_id],
            ).map_err(|_| ApplicationError::conflict())?;
            Ok(TransactionDto { id: correction_id, minor_units: reversed_minor_units.to_string(), entry_type: "correction".to_owned(), allocations, ..original })
        })
    }
}

/// Builds the reviewed SQL predicates and bound values for a profile-scoped history query.
fn transaction_history_filters(
    scope: &ProfileScope,
    starts_on: Option<&str>,
    ends_on: Option<&str>,
    account_id: Option<&str>,
    category_id: Option<&str>,
    entry_type: Option<&str>,
    search: Option<&str>,
) -> (String, Vec<Value>) {
    let mut clauses = vec!["t.profile_id = ?".to_owned()];
    let mut values = vec![Value::Text(scope.profile_id().as_str().to_owned())];
    if let Some(value) = starts_on {
        clauses.push("t.occurred_on >= ?".to_owned());
        values.push(Value::Text(value.to_owned()));
    }
    if let Some(value) = ends_on {
        clauses.push("t.occurred_on <= ?".to_owned());
        values.push(Value::Text(value.to_owned()));
    }
    if let Some(value) = account_id {
        clauses.push("t.account_id = ?".to_owned());
        values.push(Value::Text(value.to_owned()));
    }
    if let Some(value) = category_id {
        clauses.push("(t.category_id = ? OR EXISTS (SELECT 1 FROM transaction_allocations a WHERE a.profile_id = t.profile_id AND a.transaction_id = t.id AND a.category_id = ?))".to_owned());
        values.push(Value::Text(value.to_owned()));
        values.push(Value::Text(value.to_owned()));
    }
    if let Some(value) = entry_type {
        clauses.push("t.entry_type = ?".to_owned());
        values.push(Value::Text(value.to_owned()));
    }
    if let Some(value) = search {
        clauses.push("(lower(coalesce(t.payee, '')) LIKE ? OR lower(coalesce(t.description, '')) LIKE ? OR lower(coalesce(t.notes, '')) LIKE ?)".to_owned());
        let pattern = format!("%{}%", value.to_lowercase());
        values.extend([
            Value::Text(pattern.clone()),
            Value::Text(pattern.clone()),
            Value::Text(pattern),
        ]);
    }
    (clauses.join(" AND "), values)
}

/// Restricts history type filters to immutable ledger types represented by the schema.
fn validate_history_entry_type(value: &str) -> Result<&str, ApplicationError> {
    match value {
        "income" | "expense" | "transfer" | "correction" => Ok(value),
        _ => Err(ApplicationError::validation()),
    }
}

/// Maps a transaction row without allocations; callers hydrate allocations in the same read transaction.
fn transaction_from_row(row: &Row<'_>) -> rusqlite::Result<TransactionDto> {
    let minor_units: i64 = row.get(4)?;
    Ok(TransactionDto {
        id: row.get(0)?,
        account_id: row.get(1)?,
        budget_id: row.get(2)?,
        category_id: row.get(3)?,
        minor_units: minor_units.to_string(),
        currency: row.get(5)?,
        occurred_on: row.get(6)?,
        entry_type: row.get(7)?,
        payee: row.get(8)?,
        description: row.get(9)?,
        notes: row.get(10)?,
        allocations: vec![],
    })
}

/// Loads exact category portions in stable creation order for one already profile-scoped entry.
fn load_allocations(
    transaction: &rusqlite::Transaction<'_>,
    scope: &ProfileScope,
    transaction_id: &str,
) -> Result<Vec<AllocationDto>, ApplicationError> {
    let mut statement = transaction.prepare("SELECT id, transaction_id, category_id, minor_units FROM transaction_allocations WHERE profile_id = ?1 AND transaction_id = ?2 ORDER BY id").map_err(|_| ApplicationError::internal())?;
    let rows = statement
        .query_map(params![scope.profile_id().as_str(), transaction_id], |row| {
            let minor_units: i64 = row.get(3)?;
            Ok(AllocationDto {
                id: row.get(0)?,
                transaction_id: row.get(1)?,
                category_id: row.get(2)?,
                minor_units: minor_units.to_string(),
            })
        })
        .map_err(|_| ApplicationError::internal())?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|_| ApplicationError::internal())
}

/// Validates a complete split before any ledger write so the transaction and its portions commit together.
fn validate_allocation_requests(
    allocations: &[TransactionAllocationRequest],
    category_id: Option<&CategoryId>,
    entry_type: &str,
    transaction_minor_units: i64,
) -> Result<Vec<ValidatedAllocation>, ApplicationError> {
    if allocations.is_empty() {
        return Ok(vec![]);
    }
    if category_id.is_some() {
        return Err(ApplicationError::validation());
    }

    let mut category_ids = HashSet::new();
    let mut total = 0_i64;
    let mut validated = Vec::with_capacity(allocations.len());
    for allocation in allocations {
        let category_id = CategoryId::parse(&allocation.category_id)?;
        validate_entry_amount(entry_type, allocation.minor_units)?;
        if !category_ids.insert(category_id.as_str().to_owned()) {
            return Err(ApplicationError::validation());
        }
        total =
            total.checked_add(allocation.minor_units).ok_or_else(ApplicationError::validation)?;
        validated.push(ValidatedAllocation { category_id, minor_units: allocation.minor_units });
    }
    if total != transaction_minor_units {
        return Err(ApplicationError::validation());
    }
    Ok(validated)
}

/// Keeps parsed identifiers and exact amounts together after untrusted allocation input is validated.
struct ValidatedAllocation {
    category_id: CategoryId,
    minor_units: i64,
}

/// Confirms an entry has the only supported ledger types and their unambiguous sign convention.
fn validate_entry_amount(entry_type: &str, minor_units: i64) -> Result<(), ApplicationError> {
    let is_valid = match entry_type {
        "income" => minor_units > 0,
        "expense" => minor_units < 0,
        _ => false,
    };
    if !is_valid {
        return Err(ApplicationError::validation());
    }
    Ok(())
}

/// Normalizes an optional user-controlled text field and treats whitespace-only input as absent.
fn validate_optional_text(
    value: Option<&str>,
    maximum: usize,
) -> Result<Option<String>, ApplicationError> {
    value
        .filter(|text| !text.trim().is_empty())
        .map(|text| validate_text(text, maximum))
        .transpose()
}

/// Confirms a new ledger entry references an active account in the authenticated profile and currency.
fn validate_active_account(
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

/// Confirms an optional category is active, owned by the profile, and matches the entry kind.
fn validate_active_category(
    transaction: &rusqlite::Transaction<'_>,
    scope: &ProfileScope,
    category_id: Option<&CategoryId>,
    entry_type: &str,
) -> Result<(), ApplicationError> {
    let Some(category_id) = category_id else {
        return Ok(());
    };
    let category = transaction
        .query_row(
            "SELECT category_kind, is_archived FROM categories WHERE profile_id = ?1 AND id = ?2",
            params![scope.profile_id().as_str(), category_id.as_str()],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
        )
        .map_err(|_| ApplicationError::not_found())?;
    if category.0 != entry_type || category.1 != 0 {
        return Err(ApplicationError::validation());
    }
    Ok(())
}

/// Confirms an optional budget is active, owned by the profile, and uses the entry currency.
fn validate_active_budget(
    transaction: &rusqlite::Transaction<'_>,
    scope: &ProfileScope,
    budget_id: Option<&BudgetId>,
    currency: &CurrencyCode,
) -> Result<(), ApplicationError> {
    let Some(budget_id) = budget_id else {
        return Ok(());
    };
    let budget = transaction
        .query_row(
            "SELECT currency, is_archived FROM budgets WHERE profile_id = ?1 AND id = ?2",
            params![scope.profile_id().as_str(), budget_id.as_str()],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
        )
        .map_err(|_| ApplicationError::not_found())?;
    if budget.0 != currency.as_str() || budget.1 != 0 {
        return Err(ApplicationError::validation());
    }
    Ok(())
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
    use crate::application::use_cases::TransactionAllocationRequest;
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

    /// Proves income and expense entries enforce active profile references, kinds, and signs.
    #[test]
    fn creates_normalized_income_and_expense_entries() {
        let (_fixture, database, scope) = fixture();
        let budget = FinanceRepository::create_budget(&database, &scope, "Home", "USD")
            .expect("budget is created");
        let income_category =
            FinanceRepository::create_category(&database, &scope, "Salary", "income", None, 1)
                .expect("income category is created");
        let expense_category =
            FinanceRepository::create_category(&database, &scope, "Groceries", "expense", None, 2)
                .expect("expense category is created");
        let account =
            FinanceRepository::create_account(&database, &scope, "Checking", "checking", "USD", 0)
                .expect("account is created");

        let income = FinanceRepository::create_transaction(
            &database,
            &scope,
            &account.id,
            Some(&budget.id),
            Some(&income_category.id),
            "income",
            12_345,
            "USD",
            "2024-02-29",
            Some("Employer"),
            Some("February salary"),
            Some("Leap-day payment"),
            &[],
        )
        .expect("income transaction is created");
        let wrong_sign = FinanceRepository::create_transaction(
            &database,
            &scope,
            &account.id,
            None,
            Some(&expense_category.id),
            "expense",
            1,
            "USD",
            "2026-11-23",
            None,
            None,
            None,
            &[],
        )
        .expect_err("expenses require a negative amount");
        let wrong_category = FinanceRepository::create_transaction(
            &database,
            &scope,
            &account.id,
            None,
            Some(&income_category.id),
            "expense",
            -1,
            "USD",
            "2026-11-23",
            None,
            None,
            None,
            &[],
        )
        .expect_err("categories must match the entry kind");

        assert_eq!(income.minor_units, "12345");
        assert_eq!(income.occurred_on, "2024-02-29");
        assert_eq!(income.budget_id, Some(budget.id));
        assert_eq!(income.category_id, Some(income_category.id));
        assert_eq!(income.payee, Some("Employer".to_owned()));
        assert_eq!(wrong_sign.code, crate::application::ErrorCode::Validation);
        assert_eq!(wrong_category.code, crate::application::ErrorCode::Validation);
    }

    /// Proves a split entry persists every portion atomically and rejects invalid total/category mixes.
    #[test]
    fn creates_atomic_category_splits() {
        let (_fixture, database, scope) = fixture();
        let groceries =
            FinanceRepository::create_category(&database, &scope, "Groceries", "expense", None, 1)
                .expect("groceries category is created");
        let dining =
            FinanceRepository::create_category(&database, &scope, "Dining", "expense", None, 2)
                .expect("dining category is created");
        let income =
            FinanceRepository::create_category(&database, &scope, "Salary", "income", None, 3)
                .expect("income category is created");
        let account =
            FinanceRepository::create_account(&database, &scope, "Checking", "checking", "USD", 0)
                .expect("account is created");
        let allocations = vec![
            TransactionAllocationRequest { category_id: groceries.id.clone(), minor_units: -700 },
            TransactionAllocationRequest { category_id: dining.id.clone(), minor_units: -300 },
        ];

        let transaction = FinanceRepository::create_transaction(
            &database,
            &scope,
            &account.id,
            None,
            None,
            "expense",
            -1_000,
            "USD",
            "2026-11-23",
            None,
            Some("Shared shop"),
            None,
            &allocations,
        )
        .expect("split transaction is created");
        let invalid_total = FinanceRepository::create_transaction(
            &database,
            &scope,
            &account.id,
            None,
            None,
            "expense",
            -1_000,
            "USD",
            "2026-11-23",
            None,
            None,
            None,
            &[TransactionAllocationRequest {
                category_id: groceries.id.clone(),
                minor_units: -999,
            }],
        )
        .expect_err("split totals must equal the transaction");
        let invalid_category = FinanceRepository::create_transaction(
            &database,
            &scope,
            &account.id,
            None,
            None,
            "expense",
            -1_000,
            "USD",
            "2026-11-23",
            None,
            None,
            None,
            &[TransactionAllocationRequest { category_id: income.id, minor_units: -1_000 }],
        )
        .expect_err("split categories must match the transaction kind");

        assert_eq!(transaction.category_id, None);
        assert_eq!(transaction.allocations.len(), 2);
        assert_eq!(transaction.allocations[0].minor_units, "-700");
        assert_eq!(transaction.allocations[1].category_id, dining.id);
        assert_eq!(invalid_total.code, crate::application::ErrorCode::Validation);
        assert_eq!(invalid_category.code, crate::application::ErrorCode::Validation);
        database
            .in_transaction(|database_transaction| {
                // Confirms both rejected requests left no extra ledger or split rows behind.
                let transaction_count: i64 = database_transaction
                    .query_row("SELECT COUNT(*) FROM transactions", [], |row| row.get(0))
                    .map_err(|_| crate::application::ApplicationError::internal())?;
                let allocation_count: i64 = database_transaction
                    .query_row("SELECT COUNT(*) FROM transaction_allocations", [], |row| row.get(0))
                    .map_err(|_| crate::application::ApplicationError::internal())?;
                assert_eq!(transaction_count, 1);
                assert_eq!(allocation_count, 2);
                Ok(())
            })
            .expect("rejected splits leave no partial records");
    }

    /// Proves history filters, allocation category lookup, stable ordering, and page bounds stay profile-scoped.
    #[test]
    fn lists_filtered_transaction_history_in_deterministic_pages() {
        let (_fixture, database, scope) = fixture();
        let groceries =
            FinanceRepository::create_category(&database, &scope, "Groceries", "expense", None, 1)
                .expect("category is created");
        let dining =
            FinanceRepository::create_category(&database, &scope, "Dining", "expense", None, 2)
                .expect("category is created");
        let account =
            FinanceRepository::create_account(&database, &scope, "Checking", "checking", "USD", 0)
                .expect("account is created");
        let older = FinanceRepository::create_transaction(
            &database,
            &scope,
            &account.id,
            None,
            Some(&groceries.id),
            "expense",
            -200,
            "USD",
            "2026-01-01",
            Some("Market"),
            None,
            None,
            &[],
        )
        .expect("older entry is created");
        let newer = FinanceRepository::create_transaction(
            &database,
            &scope,
            &account.id,
            None,
            None,
            "expense",
            -300,
            "USD",
            "2026-01-02",
            Some("Cafe"),
            None,
            None,
            &[TransactionAllocationRequest { category_id: dining.id.clone(), minor_units: -300 }],
        )
        .expect("newer split entry is created");

        let page = FinanceRepository::list_transactions(
            &database,
            &scope,
            Some("2026-01-01"),
            Some("2026-01-31"),
            Some(&account.id),
            Some(&dining.id),
            Some("expense"),
            Some("cafe"),
            0,
            20,
        )
        .expect("filtered history is listed");
        let next_page = FinanceRepository::list_transactions(
            &database,
            &scope,
            None,
            None,
            Some(&account.id),
            None,
            None,
            None,
            1,
            1,
        )
        .expect("second page is listed");

        assert_eq!(page.total_count, 1);
        assert_eq!(page.transactions[0].id, newer.id);
        assert_eq!(page.transactions[0].allocations[0].category_id, dining.id);
        assert_eq!(next_page.transactions[0].id, older.id);
    }

    /// Proves a reversal is an atomic compensating fact with opposite category splits and no duplicate.
    #[test]
    fn reverses_an_income_or_expense_without_rewriting_history() {
        let (_fixture, database, scope) = fixture();
        let groceries =
            FinanceRepository::create_category(&database, &scope, "Groceries", "expense", None, 1)
                .expect("category is created");
        let account =
            FinanceRepository::create_account(&database, &scope, "Checking", "checking", "USD", 0)
                .expect("account is created");
        let original = FinanceRepository::create_transaction(
            &database,
            &scope,
            &account.id,
            None,
            None,
            "expense",
            -750,
            "USD",
            "2026-02-01",
            Some("Market"),
            None,
            None,
            &[TransactionAllocationRequest {
                category_id: groceries.id.clone(),
                minor_units: -750,
            }],
        )
        .expect("original split entry is created");

        let correction = FinanceRepository::reverse_transaction(&database, &scope, &original.id)
            .expect("reversal is created");
        let duplicate = FinanceRepository::reverse_transaction(&database, &scope, &original.id)
            .expect_err("a ledger fact can only have one reversal");

        assert_eq!(correction.entry_type, "correction");
        assert_eq!(correction.minor_units, "750");
        assert_eq!(correction.allocations[0].minor_units, "750");
        assert_eq!(duplicate.code, crate::application::ErrorCode::Conflict);
        database.in_transaction(|transaction| {
            let correction_count: i64 = transaction.query_row(
                "SELECT COUNT(*) FROM transaction_corrections WHERE profile_id = ?1 AND original_transaction_id = ?2",
                (scope.profile_id().as_str(), original.id), |row| row.get(0),
            ).map_err(|_| crate::application::ApplicationError::internal())?;
            assert_eq!(correction_count, 1);
            Ok(())
        }).expect("correction audit link is persisted");
    }
}
