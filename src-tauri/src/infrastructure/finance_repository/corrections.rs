//! Immutable transaction reversal and adjustment persistence.

use super::FinanceRepository;
use super::mappers::{load_allocations, transaction_from_row};
use super::transactions::{ValidatedAllocation, validate_allocation_requests};
use super::validation::{
    new_id, validate_active_category, validate_entry_amount, validate_optional_text, validate_text,
};
use crate::application::ApplicationError;
use crate::application::contracts::{AllocationDto, TransactionDto};
use crate::application::use_cases::AdjustTransactionRequest;
use crate::domain::{CategoryId, ProfileScope, TransactionId};
use crate::infrastructure::database::Database;
use rusqlite::params;
use std::collections::{BTreeMap, HashSet};

impl FinanceRepository {
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
            if !matches!(original.entry_type.as_str(), "income" | "expense") { return Err(ApplicationError::validation()); }
            assert_uncorrected(transaction, scope, transaction_id.as_str())?;
            let reversed_minor_units = original.minor_units.parse::<i64>().ok().and_then(i64::checked_neg).ok_or_else(ApplicationError::validation)?;
            let correction_id = new_id("transaction");
            transaction.execute(
                "INSERT INTO transactions (id, profile_id, account_id, budget_id, category_id, occurred_on, entry_type, minor_units, currency, payee, description, notes, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'correction', ?7, ?8, ?9, ?10, ?11, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
                params![correction_id, scope.profile_id().as_str(), original.account_id, original.budget_id, original.category_id, original.occurred_on, reversed_minor_units, original.currency, original.payee, original.description, original.notes],
            ).map_err(|_| ApplicationError::conflict())?;
            let original_allocations = load_allocations(transaction, scope, transaction_id.as_str())?;
            let mut allocations = Vec::with_capacity(original_allocations.len());
            for original_allocation in original_allocations {
                let minor_units = original_allocation.minor_units.parse::<i64>().ok().and_then(i64::checked_neg).ok_or_else(ApplicationError::validation)?;
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

    /// Records one immutable adjustment delta while preserving the original ledger fact unchanged.
    pub fn adjust_transaction(
        database: &Database,
        scope: &ProfileScope,
        request: AdjustTransactionRequest,
    ) -> Result<TransactionDto, ApplicationError> {
        let transaction_id = TransactionId::parse(&request.transaction_id)?;
        let category_id = request.category_id.as_deref().map(CategoryId::parse).transpose()?;
        let payee = validate_optional_text(request.payee.as_deref(), 200)?;
        let description = validate_optional_text(request.description.as_deref(), 500)?;
        let notes = validate_optional_text(request.notes.as_deref(), 4_000)?;
        let reason = validate_text(&request.reason, 500)?;
        database.in_transaction(|transaction| {
            let original = transaction.query_row(
                "SELECT id, account_id, budget_id, category_id, minor_units, currency, occurred_on, entry_type, payee, description, notes FROM transactions WHERE profile_id = ?1 AND id = ?2",
                params![scope.profile_id().as_str(), transaction_id.as_str()], transaction_from_row,
            ).map_err(|_| ApplicationError::not_found())?;
            if !matches!(original.entry_type.as_str(), "income" | "expense") { return Err(ApplicationError::validation()); }
            assert_uncorrected(transaction, scope, transaction_id.as_str())?;
            validate_entry_amount(&original.entry_type, request.adjusted_minor_units)?;
            let target_allocations = validate_allocation_requests(&request.allocations, category_id.as_ref(), &original.entry_type, request.adjusted_minor_units)?;
            validate_active_category(transaction, scope, category_id.as_ref(), &original.entry_type)?;
            for allocation in &target_allocations { validate_active_category(transaction, scope, Some(&allocation.category_id), &original.entry_type)?; }
            let original_minor_units = original.minor_units.parse::<i64>().map_err(|_| ApplicationError::internal())?;
            let correction_minor_units = request.adjusted_minor_units.checked_sub(original_minor_units).ok_or_else(ApplicationError::validation)?;
            let original_allocations = load_allocations(transaction, scope, transaction_id.as_str())?;
            let original_amounts = allocation_amounts(original.category_id.as_deref(), original_minor_units, &original_allocations)?;
            let target_amounts = requested_allocation_amounts(category_id.as_ref(), request.adjusted_minor_units, &target_allocations);
            let correction_amounts = correction_allocation_amounts(&original_amounts, &target_amounts)?;
            let correction_id = new_id("transaction");
            transaction.execute(
                "INSERT INTO transactions (id, profile_id, account_id, budget_id, category_id, occurred_on, entry_type, minor_units, currency, payee, description, notes, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, NULL, ?5, 'correction', ?6, ?7, ?8, ?9, ?10, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
                params![correction_id, scope.profile_id().as_str(), original.account_id, original.budget_id, original.occurred_on, correction_minor_units, original.currency, payee, description, notes],
            ).map_err(|_| ApplicationError::conflict())?;
            let mut allocations = Vec::with_capacity(correction_amounts.len());
            for (category_id, minor_units) in correction_amounts {
                let allocation_id = new_id("allocation");
                transaction.execute(
                    "INSERT INTO transaction_allocations (id, profile_id, transaction_id, category_id, minor_units, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
                    params![allocation_id, scope.profile_id().as_str(), correction_id, category_id, minor_units],
                ).map_err(|_| ApplicationError::conflict())?;
                allocations.push(AllocationDto { id: allocation_id, transaction_id: correction_id.clone(), category_id, minor_units: minor_units.to_string() });
            }
            transaction.execute(
                "INSERT INTO transaction_corrections (id, profile_id, original_transaction_id, correction_transaction_id, correction_kind, reason, created_at) VALUES (?1, ?2, ?3, ?4, 'adjustment', ?5, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
                params![new_id("correction"), scope.profile_id().as_str(), transaction_id.as_str(), correction_id, reason],
            ).map_err(|_| ApplicationError::conflict())?;
            Ok(TransactionDto { id: correction_id, category_id: None, minor_units: correction_minor_units.to_string(), entry_type: "correction".to_owned(), payee, description, notes, allocations, ..original })
        })
    }
}

/// Rejects a second correction before a new immutable correction fact is recorded.
fn assert_uncorrected(
    transaction: &rusqlite::Transaction<'_>,
    scope: &ProfileScope,
    transaction_id: &str,
) -> Result<(), ApplicationError> {
    let count: i64 = transaction.query_row(
        "SELECT COUNT(*) FROM transaction_corrections WHERE profile_id = ?1 AND original_transaction_id = ?2",
        params![scope.profile_id().as_str(), transaction_id], |row| row.get(0),
    ).map_err(|_| ApplicationError::internal())?;
    if count != 0 {
        return Err(ApplicationError::conflict());
    }
    Ok(())
}

/// Represents direct categories and split rows as category totals for immutable adjustment math.
fn allocation_amounts(
    category_id: Option<&str>,
    minor_units: i64,
    allocations: &[AllocationDto],
) -> Result<BTreeMap<String, i64>, ApplicationError> {
    let mut amounts = BTreeMap::new();
    if allocations.is_empty() {
        if let Some(category_id) = category_id {
            amounts.insert(category_id.to_owned(), minor_units);
        }
        return Ok(amounts);
    }
    for allocation in allocations {
        amounts.insert(
            allocation.category_id.clone(),
            allocation.minor_units.parse::<i64>().map_err(|_| ApplicationError::internal())?,
        );
    }
    Ok(amounts)
}

/// Converts validated replacement splits into their category totals for delta calculation.
fn requested_allocation_amounts(
    category_id: Option<&CategoryId>,
    minor_units: i64,
    allocations: &[ValidatedAllocation],
) -> BTreeMap<String, i64> {
    let mut amounts = BTreeMap::new();
    if allocations.is_empty() {
        if let Some(category_id) = category_id {
            amounts.insert(category_id.as_str().to_owned(), minor_units);
        }
        return amounts;
    }
    for allocation in allocations {
        amounts.insert(allocation.category_id.as_str().to_owned(), allocation.minor_units);
    }
    amounts
}

/// Calculates category deltas so an adjustment moves budget actuals from original to corrected values.
fn correction_allocation_amounts(
    original: &BTreeMap<String, i64>,
    adjusted: &BTreeMap<String, i64>,
) -> Result<BTreeMap<String, i64>, ApplicationError> {
    let mut amounts = BTreeMap::new();
    let category_ids = original.keys().chain(adjusted.keys()).collect::<HashSet<_>>();
    for category_id in category_ids {
        let original_amount = original.get(category_id).copied().unwrap_or_default();
        let adjusted_amount = adjusted.get(category_id).copied().unwrap_or_default();
        let delta = adjusted_amount
            .checked_sub(original_amount)
            .ok_or_else(ApplicationError::validation)?;
        if delta != 0 {
            amounts.insert(category_id.clone(), delta);
        }
    }
    Ok(amounts)
}
