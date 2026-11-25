//! Shared validation and mutation helpers for finance persistence.

use crate::application::ApplicationError;
use crate::domain::{AccountId, BudgetId, CategoryId, CurrencyCode, ProfileScope};
use crate::infrastructure::database::Database;
use rusqlite::params;
use uuid::Uuid;

/// Confirms an entry has the only supported ledger types and their unambiguous sign convention.
pub(super) fn validate_entry_amount(
    entry_type: &str,
    minor_units: i64,
) -> Result<(), ApplicationError> {
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
pub(super) fn validate_optional_text(
    value: Option<&str>,
    maximum: usize,
) -> Result<Option<String>, ApplicationError> {
    value
        .filter(|text| !text.trim().is_empty())
        .map(|text| validate_text(text, maximum))
        .transpose()
}

/// Confirms a new ledger entry references an active account in the authenticated profile and currency.
pub(super) fn validate_active_account(
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
pub(super) fn validate_active_category(
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
pub(super) fn validate_active_budget(
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
pub(super) fn archive(
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
pub(super) fn new_id(prefix: &str) -> String {
    format!("{prefix}-{}", Uuid::new_v4())
}

/// Trims and bounds a non-empty display string before persistence.
pub(super) fn validate_text(value: &str, maximum: usize) -> Result<String, ApplicationError> {
    let normalized = value.trim();
    if normalized.is_empty() || normalized.len() > maximum {
        return Err(ApplicationError::validation());
    }
    Ok(normalized.to_owned())
}

/// Restricts category kinds to the two accounting classifications currently supported.
pub(super) fn validate_category_kind(value: &str) -> Result<&str, ApplicationError> {
    match value {
        "income" | "expense" => Ok(value),
        _ => Err(ApplicationError::validation()),
    }
}

/// Preserves an optional non-empty compact presentation color without assigning financial meaning.
pub(super) fn validate_color(value: Option<&str>) -> Result<Option<String>, ApplicationError> {
    value.map(|color| validate_text(color, 32)).transpose()
}

/// Restricts account types to reviewed initial values instead of persisting arbitrary classifications.
pub(super) fn validate_account_type(value: &str) -> Result<&str, ApplicationError> {
    match value {
        "cash" | "checking" | "savings" | "credit_card" | "other" => Ok(value),
        _ => Err(ApplicationError::validation()),
    }
}

/// Restricts history type filters to immutable ledger types represented by the schema.
pub(super) fn validate_history_entry_type(value: &str) -> Result<&str, ApplicationError> {
    match value {
        "income" | "expense" | "transfer" | "correction" => Ok(value),
        _ => Err(ApplicationError::validation()),
    }
}
