//! SQLite row-to-DTO mapping and allocation hydration helpers.

use crate::application::ApplicationError;
use crate::application::contracts::{
    AccountDto, AllocationDto, BudgetDto, CategoryDto, TransactionDto,
};
use crate::domain::ProfileScope;
use rusqlite::{Row, params};

/// Maps a budget query result to its safe frontend representation.
pub(super) fn budget_from_row(row: &Row<'_>) -> rusqlite::Result<BudgetDto> {
    Ok(BudgetDto {
        id: row.get(0)?,
        name: row.get(1)?,
        currency: row.get(2)?,
        is_archived: row.get::<_, i64>(3)? != 0,
    })
}

/// Maps a category query result to its safe frontend representation.
pub(super) fn category_from_row(row: &Row<'_>) -> rusqlite::Result<CategoryDto> {
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
pub(super) fn account_from_row(row: &Row<'_>) -> rusqlite::Result<AccountDto> {
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

/// Maps a transaction row without allocations; callers hydrate allocations in the same read transaction.
pub(super) fn transaction_from_row(row: &Row<'_>) -> rusqlite::Result<TransactionDto> {
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
pub(super) fn load_allocations(
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
