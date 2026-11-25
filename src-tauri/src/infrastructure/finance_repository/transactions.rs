//! Immutable income and expense ledger creation and history reads.

use super::FinanceRepository;
use super::mappers::{load_allocations, transaction_from_row};
use super::validation::{
    new_id, validate_active_account, validate_active_budget, validate_active_category,
    validate_entry_amount, validate_history_entry_type, validate_optional_text,
};
use crate::application::ApplicationError;
use crate::application::contracts::{AllocationDto, TransactionDto, TransactionHistoryPageDto};
use crate::application::use_cases::TransactionAllocationRequest;
use crate::domain::{AccountId, BudgetId, CategoryId, CurrencyCode, LocalDate, ProfileScope};
use crate::infrastructure::database::Database;
use rusqlite::{params, types::Value};
use std::collections::HashSet;

impl FinanceRepository {
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
            for allocation in &allocations { validate_active_category(transaction, scope, Some(&allocation.category_id), entry_type)?; }
            transaction.execute(
                "INSERT INTO transactions (id, profile_id, account_id, budget_id, category_id, occurred_on, entry_type, minor_units, currency, payee, description, notes, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
                params![id, scope.profile_id().as_str(), account_id.as_str(), budget_id.as_ref().map(|value| value.as_str()), category_id.as_ref().map(|value| value.as_str()), occurred_on, entry_type, minor_units, currency.as_str(), payee, description, notes],
            ).map_err(|_| ApplicationError::conflict())?;
            let allocation_dtos = allocations.iter().map(|allocation| {
                let allocation_id = new_id("allocation");
                transaction.execute(
                    "INSERT INTO transaction_allocations (id, profile_id, transaction_id, category_id, minor_units, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
                    params![allocation_id, scope.profile_id().as_str(), id, allocation.category_id.as_str(), allocation.minor_units],
                ).map_err(|_| ApplicationError::conflict())?;
                Ok(AllocationDto { id: allocation_id, transaction_id: id.clone(), category_id: allocation.category_id.as_str().to_owned(), minor_units: allocation.minor_units.to_string() })
            }).collect::<Result<Vec<_>, ApplicationError>>()?;
            Ok(TransactionDto { id, account_id: account_id.as_str().to_owned(), budget_id: budget_id.as_ref().map(|value| value.as_str().to_owned()), category_id: category_id.as_ref().map(|value| value.as_str().to_owned()), minor_units: minor_units.to_string(), currency: currency.as_str().to_owned(), occurred_on: occurred_on.to_owned(), entry_type: entry_type.to_owned(), payee, description, notes, allocations: allocation_dtos })
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
            let (where_clause, filter_values) = transaction_history_filters(scope, starts_on, ends_on, account_id.as_ref().map(|value| value.as_str()), category_id.as_ref().map(|value| value.as_str()), entry_type, search.as_deref());
            let count_sql = format!("SELECT COUNT(*) FROM transactions t WHERE {where_clause}");
            let total_count: i64 = transaction.query_row(&count_sql, rusqlite::params_from_iter(filter_values.iter()), |row| row.get(0)).map_err(|_| ApplicationError::internal())?;
            let list_sql = format!("SELECT t.id, t.account_id, t.budget_id, t.category_id, t.minor_units, t.currency, t.occurred_on, t.entry_type, t.payee, t.description, t.notes FROM transactions t WHERE {where_clause} ORDER BY t.occurred_on DESC, t.id DESC LIMIT ? OFFSET ?");
            let mut list_values = filter_values;
            list_values.push(Value::Integer(i64::from(page_size)));
            list_values.push(Value::Integer(offset));
            let mut statement = transaction.prepare(&list_sql).map_err(|_| ApplicationError::internal())?;
            let rows = statement.query_map(rusqlite::params_from_iter(list_values.iter()), transaction_from_row).map_err(|_| ApplicationError::internal())?;
            let mut transactions = rows.collect::<Result<Vec<_>, _>>().map_err(|_| ApplicationError::internal())?;
            drop(statement);
            for transaction_dto in &mut transactions { transaction_dto.allocations = load_allocations(transaction, scope, &transaction_dto.id)?; }
            Ok(TransactionHistoryPageDto { transactions, total_count: u64::try_from(total_count).map_err(|_| ApplicationError::internal())?, page, page_size })
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

/// Keeps parsed identifiers and exact amounts together after untrusted allocation input is validated.
pub(super) struct ValidatedAllocation {
    pub(super) category_id: CategoryId,
    pub(super) minor_units: i64,
}

/// Validates a complete split before any ledger write so the transaction and its portions commit together.
pub(super) fn validate_allocation_requests(
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
