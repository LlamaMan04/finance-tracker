//! Validated Tauri commands for profile-scoped budgets, categories, and accounts.

use crate::application::ApplicationError;
use crate::application::contracts::{AccountDto, BudgetDto, CategoryDto};
use crate::commands::safe_error;
use crate::infrastructure::finance_repository::FinanceRepository;
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
