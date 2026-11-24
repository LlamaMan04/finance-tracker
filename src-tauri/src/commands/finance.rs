//! Validated Tauri commands for profile-scoped budgets, categories, and accounts.

use crate::application::ApplicationError;
use crate::application::contracts::{
    AccountDto, BudgetAllocationDto, BudgetDto, BudgetSummaryDto, CategoryDto, FinanceSummaryDto,
    TransactionDto, TransactionHistoryPageDto, TransferDto,
};
use crate::application::use_cases::{AdjustTransactionRequest, TransactionAllocationRequest};
use crate::commands::safe_error;
use crate::infrastructure::budget_allocation_repository::BudgetAllocationRepository;
use crate::infrastructure::calculation_repository::CalculationRepository;
use crate::infrastructure::finance_repository::FinanceRepository;
use crate::infrastructure::transfer_repository::TransferRepository;
use crate::security::LocalProfileService;
use serde::Deserialize;
use tauri::State;

/// Accepts untrusted budget creation input at the narrow command boundary.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BudgetInput {
    pub name: String,
    pub currency: String,
}

/// Accepts untrusted mutable budget fields at the narrow command boundary.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BudgetUpdateInput {
    pub id: String,
    pub name: String,
}

/// Accepts a unique period/category budget plan while keeping financial rules in Rust.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BudgetAllocationInput {
    pub budget_id: String,
    pub category_id: String,
    pub period_starts_on: String,
    pub period_ends_on: String,
    pub planned_minor_units: String,
    pub notes: Option<String>,
}

/// Accepts untrusted category fields at the narrow command boundary.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryInput {
    pub name: String,
    pub category_kind: String,
    pub color: Option<String>,
    pub sort_order: i32,
}

/// Accepts an existing category identity and its replacement display fields.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryUpdateInput {
    pub id: String,
    pub name: String,
    pub category_kind: String,
    pub color: Option<String>,
    pub sort_order: i32,
}

/// Accepts account setup input while keeping all financial validation in Rust.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountInput {
    pub name: String,
    pub account_type: String,
    pub currency: String,
    pub opening_minor_units: String,
}

/// Accepts the mutable display fields of an existing account.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountUpdateInput {
    pub id: String,
    pub name: String,
    pub account_type: String,
}

/// Accepts untrusted income or expense fields while keeping ledger validation in Rust.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionInput {
    pub account_id: String,
    pub budget_id: Option<String>,
    pub category_id: Option<String>,
    pub entry_type: String,
    pub minor_units: String,
    pub currency: String,
    pub occurred_on: String,
    pub payee: Option<String>,
    pub description: Option<String>,
    pub notes: Option<String>,
    pub allocations: Vec<TransactionAllocationInput>,
}

/// Accepts one untrusted signed category split for a new immutable transaction.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionAllocationInput {
    pub category_id: String,
    pub minor_units: String,
}

/// Accepts replacement values that the backend records as an immutable adjustment delta.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionAdjustmentInput {
    pub transaction_id: String,
    pub adjusted_minor_units: String,
    pub category_id: Option<String>,
    pub payee: Option<String>,
    pub description: Option<String>,
    pub notes: Option<String>,
    pub allocations: Vec<TransactionAllocationInput>,
    pub reason: String,
}

/// Accepts untrusted fields for one atomic transfer between active accounts.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferInput {
    pub source_account_id: String,
    pub destination_account_id: String,
    pub minor_units: String,
    pub currency: String,
    pub occurred_on: String,
}

/// Accepts optional protected-history filters and bounded zero-based pagination.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionHistoryInput {
    pub starts_on: Option<String>,
    pub ends_on: Option<String>,
    pub account_id: Option<String>,
    pub category_id: Option<String>,
    pub entry_type: Option<String>,
    pub search: Option<String>,
    pub page: u32,
    pub page_size: u32,
}

/// Accepts a selected budget and exact inclusive planning period for a protected calculation.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BudgetSummaryInput {
    pub budget_id: String,
    pub period_starts_on: String,
    pub period_ends_on: String,
}

/// Returns Rust-derived account balances and per-currency income and expense totals.
#[tauri::command]
pub fn get_finance_summary(
    service: State<'_, LocalProfileService>,
) -> Result<FinanceSummaryDto, ApplicationError> {
    service
        .with_active_profile(|scope, database| {
            CalculationRepository::finance_summary(database, scope)
        })
        .map_err(safe_error)
}

/// Returns Rust-derived budget results for one selected exact planning period.
#[tauri::command]
pub fn get_budget_summary(
    service: State<'_, LocalProfileService>,
    request: BudgetSummaryInput,
) -> Result<BudgetSummaryDto, ApplicationError> {
    service
        .with_active_profile(|scope, database| {
            CalculationRepository::budget_summary(
                database,
                scope,
                &request.budget_id,
                &request.period_starts_on,
                &request.period_ends_on,
            )
        })
        .map_err(safe_error)
}

/// Lists protected budgets only after the active profile has authenticated.
#[tauri::command]
pub fn list_budgets(
    service: State<'_, LocalProfileService>,
) -> Result<Vec<BudgetDto>, ApplicationError> {
    service
        .with_active_profile(|scope, database| FinanceRepository::list_budgets(database, scope))
        .map_err(safe_error)
}

/// Creates a protected budget in the active profile.
#[tauri::command]
pub fn create_budget(
    service: State<'_, LocalProfileService>,
    request: BudgetInput,
) -> Result<BudgetDto, ApplicationError> {
    service
        .with_active_profile(|scope, database| {
            FinanceRepository::create_budget(database, scope, &request.name, &request.currency)
        })
        .map_err(safe_error)
}

/// Renames a protected budget in the active profile.
#[tauri::command]
pub fn update_budget(
    service: State<'_, LocalProfileService>,
    request: BudgetUpdateInput,
) -> Result<BudgetDto, ApplicationError> {
    service
        .with_active_profile(|scope, database| {
            FinanceRepository::update_budget(database, scope, &request.id, &request.name)
        })
        .map_err(safe_error)
}

/// Archives a protected budget without deleting history.
#[tauri::command]
pub fn archive_budget(
    service: State<'_, LocalProfileService>,
    id: String,
) -> Result<(), ApplicationError> {
    service
        .with_active_profile(|scope, database| {
            FinanceRepository::archive_budget(database, scope, &id)
        })
        .map_err(safe_error)
}

/// Lists planned category amounts for the active protected profile.
#[tauri::command]
pub fn list_budget_allocations(
    service: State<'_, LocalProfileService>,
) -> Result<Vec<BudgetAllocationDto>, ApplicationError> {
    service
        .with_active_profile(|scope, database| BudgetAllocationRepository::list(database, scope))
        .map_err(safe_error)
}

/// Creates or replaces the active profile's plan for one budget, category, and period.
#[tauri::command]
pub fn upsert_budget_allocation(
    service: State<'_, LocalProfileService>,
    request: BudgetAllocationInput,
) -> Result<BudgetAllocationDto, ApplicationError> {
    let planned_minor_units = request
        .planned_minor_units
        .parse::<i64>()
        .map_err(|_| safe_error(ApplicationError::validation()))?;
    service
        .with_active_profile(|scope, database| {
            BudgetAllocationRepository::upsert(
                database,
                scope,
                &request.budget_id,
                &request.category_id,
                &request.period_starts_on,
                &request.period_ends_on,
                planned_minor_units,
                request.notes.as_deref(),
            )
        })
        .map_err(safe_error)
}

/// Deletes an active profile's plan without touching ledger transactions or reference data.
#[tauri::command]
pub fn remove_budget_allocation(
    service: State<'_, LocalProfileService>,
    id: String,
) -> Result<(), ApplicationError> {
    service
        .with_active_profile(|scope, database| {
            BudgetAllocationRepository::remove(database, scope, &id)
        })
        .map_err(safe_error)
}

/// Lists protected categories in deterministic display order.
#[tauri::command]
pub fn list_categories(
    service: State<'_, LocalProfileService>,
) -> Result<Vec<CategoryDto>, ApplicationError> {
    service
        .with_active_profile(|scope, database| FinanceRepository::list_categories(database, scope))
        .map_err(safe_error)
}

/// Creates a protected category in the active profile.
#[tauri::command]
pub fn create_category(
    service: State<'_, LocalProfileService>,
    request: CategoryInput,
) -> Result<CategoryDto, ApplicationError> {
    service
        .with_active_profile(|scope, database| {
            FinanceRepository::create_category(
                database,
                scope,
                &request.name,
                &request.category_kind,
                request.color.as_deref(),
                request.sort_order,
            )
        })
        .map_err(safe_error)
}

/// Updates one protected category in the active profile.
#[tauri::command]
pub fn update_category(
    service: State<'_, LocalProfileService>,
    request: CategoryUpdateInput,
) -> Result<CategoryDto, ApplicationError> {
    service
        .with_active_profile(|scope, database| {
            FinanceRepository::update_category(
                database,
                scope,
                &request.id,
                &request.name,
                &request.category_kind,
                request.color.as_deref(),
                request.sort_order,
            )
        })
        .map_err(safe_error)
}

/// Archives a protected category without deleting history.
#[tauri::command]
pub fn archive_category(
    service: State<'_, LocalProfileService>,
    id: String,
) -> Result<(), ApplicationError> {
    service
        .with_active_profile(|scope, database| {
            FinanceRepository::archive_category(database, scope, &id)
        })
        .map_err(safe_error)
}

/// Lists protected accounts in deterministic display order.
#[tauri::command]
pub fn list_accounts(
    service: State<'_, LocalProfileService>,
) -> Result<Vec<AccountDto>, ApplicationError> {
    service
        .with_active_profile(|scope, database| FinanceRepository::list_accounts(database, scope))
        .map_err(safe_error)
}

/// Creates a protected account in the active profile.
#[tauri::command]
pub fn create_account(
    service: State<'_, LocalProfileService>,
    request: AccountInput,
) -> Result<AccountDto, ApplicationError> {
    let opening_minor_units = request
        .opening_minor_units
        .parse::<i64>()
        .map_err(|_| safe_error(ApplicationError::validation()))?;
    service
        .with_active_profile(|scope, database| {
            FinanceRepository::create_account(
                database,
                scope,
                &request.name,
                &request.account_type,
                &request.currency,
                opening_minor_units,
            )
        })
        .map_err(safe_error)
}

/// Updates mutable display fields for one protected account.
#[tauri::command]
pub fn update_account(
    service: State<'_, LocalProfileService>,
    request: AccountUpdateInput,
) -> Result<AccountDto, ApplicationError> {
    service
        .with_active_profile(|scope, database| {
            FinanceRepository::update_account(
                database,
                scope,
                &request.id,
                &request.name,
                &request.account_type,
            )
        })
        .map_err(safe_error)
}

/// Archives a protected account without deleting ledger history.
#[tauri::command]
pub fn archive_account(
    service: State<'_, LocalProfileService>,
    id: String,
) -> Result<(), ApplicationError> {
    service
        .with_active_profile(|scope, database| {
            FinanceRepository::archive_account(database, scope, &id)
        })
        .map_err(safe_error)
}

/// Records a protected immutable income or expense entry in the active profile.
#[tauri::command]
pub fn create_transaction(
    service: State<'_, LocalProfileService>,
    request: TransactionInput,
) -> Result<TransactionDto, ApplicationError> {
    let minor_units = request
        .minor_units
        .parse::<i64>()
        .map_err(|_| safe_error(ApplicationError::validation()))?;
    // Parse every exact integer before opening the database transaction so malformed UI input
    // cannot create a partial ledger record.
    let allocations = request
        .allocations
        .into_iter()
        .map(|allocation| {
            allocation
                .minor_units
                .parse::<i64>()
                .map(|minor_units| TransactionAllocationRequest {
                    category_id: allocation.category_id,
                    minor_units,
                })
                .map_err(|_| safe_error(ApplicationError::validation()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    service
        .with_active_profile(|scope, database| {
            FinanceRepository::create_transaction(
                database,
                scope,
                &request.account_id,
                request.budget_id.as_deref(),
                request.category_id.as_deref(),
                &request.entry_type,
                minor_units,
                &request.currency,
                &request.occurred_on,
                request.payee.as_deref(),
                request.description.as_deref(),
                request.notes.as_deref(),
                &allocations,
            )
        })
        .map_err(safe_error)
}

/// Records linked transfer facts in the authenticated profile without using budget categories.
#[tauri::command]
pub fn create_transfer(
    service: State<'_, LocalProfileService>,
    request: TransferInput,
) -> Result<TransferDto, ApplicationError> {
    // Parses the exact amount before the repository opens its all-or-nothing write transaction.
    let minor_units = request
        .minor_units
        .parse::<i64>()
        .map_err(|_| safe_error(ApplicationError::validation()))?;
    service
        .with_active_profile(|scope, database| {
            TransferRepository::create(
                database,
                scope,
                &request.source_account_id,
                &request.destination_account_id,
                minor_units,
                &request.currency,
                &request.occurred_on,
            )
        })
        .map_err(safe_error)
}

/// Lists a filtered, deterministic page of ledger facts in the authenticated profile only.
#[tauri::command]
pub fn list_transactions(
    service: State<'_, LocalProfileService>,
    request: TransactionHistoryInput,
) -> Result<TransactionHistoryPageDto, ApplicationError> {
    service
        .with_active_profile(|scope, database| {
            FinanceRepository::list_transactions(
                database,
                scope,
                request.starts_on.as_deref(),
                request.ends_on.as_deref(),
                request.account_id.as_deref(),
                request.category_id.as_deref(),
                request.entry_type.as_deref(),
                request.search.as_deref(),
                request.page,
                request.page_size,
            )
        })
        .map_err(safe_error)
}

/// Creates an equal-and-opposite correction instead of mutating immutable finance history.
#[tauri::command]
pub fn reverse_transaction(
    service: State<'_, LocalProfileService>,
    id: String,
) -> Result<TransactionDto, ApplicationError> {
    service
        .with_active_profile(|scope, database| {
            FinanceRepository::reverse_transaction(database, scope, &id)
        })
        .map_err(safe_error)
}

/// Records an immutable amount or classification delta for one eligible ledger entry.
#[tauri::command]
pub fn adjust_transaction(
    service: State<'_, LocalProfileService>,
    request: TransactionAdjustmentInput,
) -> Result<TransactionDto, ApplicationError> {
    // Parses every exact integer before the protected write transaction begins.
    let adjusted_minor_units = request
        .adjusted_minor_units
        .parse::<i64>()
        .map_err(|_| safe_error(ApplicationError::validation()))?;
    let allocations = request
        .allocations
        .into_iter()
        .map(|allocation| {
            allocation
                .minor_units
                .parse::<i64>()
                .map(|minor_units| TransactionAllocationRequest {
                    category_id: allocation.category_id,
                    minor_units,
                })
                .map_err(|_| safe_error(ApplicationError::validation()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let adjustment = AdjustTransactionRequest {
        transaction_id: request.transaction_id,
        adjusted_minor_units,
        category_id: request.category_id,
        payee: request.payee,
        description: request.description,
        notes: request.notes,
        allocations,
        reason: request.reason,
    };
    service
        .with_active_profile(|scope, database| {
            FinanceRepository::adjust_transaction(database, scope, adjustment)
        })
        .map_err(safe_error)
}
