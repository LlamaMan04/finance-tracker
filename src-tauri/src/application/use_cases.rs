//! Application-service contracts that keep future finance workflows independent of Tauri and SQL.

use crate::application::ApplicationError;
use crate::application::contracts::{
    AccountDto, AllocationDto, BackupDto, BudgetAllocationDto, BudgetDto, CategoryDto,
    ProfileSummaryDto, SessionStatusDto, TransactionDto,
};
use crate::domain::{
    AccountId, AllocationId, BackupId, BudgetId, CategoryId, ProfileScope, TransactionId,
};
use zeroize::Zeroizing;

/// Defines profile lifecycle operations without exposing passphrases or keys in responses.
pub trait ProfileService {
    /// Lists safe local profile summaries without opening protected databases.
    fn list_profiles(&self) -> Result<Vec<ProfileSummaryDto>, ApplicationError>;

    /// Returns the selected profile's safe lock state.
    fn session_status(&self) -> Result<SessionStatusDto, ApplicationError>;

    /// Creates a protected profile after the command boundary has validated setup input.
    fn create_profile(
        &self,
        request: CreateProfileRequest,
    ) -> Result<SessionStatusDto, ApplicationError>;

    /// Unlocks a profile after the command boundary has validated unlock input.
    fn unlock_profile(
        &self,
        request: UnlockProfileRequest,
    ) -> Result<SessionStatusDto, ApplicationError>;

    /// Drops the active protected session and its in-memory key material.
    fn lock(&self) -> Result<(), ApplicationError>;

    /// Records verified local interaction or returns a locked status after the idle policy expires.
    fn record_activity(&self) -> Result<SessionStatusDto, ApplicationError>;

    /// Locks any active session and selects a known profile for a later unlock attempt.
    fn switch_profile(&self, profile_id: String) -> Result<SessionStatusDto, ApplicationError>;
}

/// Defines budget operations that will be implemented after secure sessions exist.
pub trait BudgetService {
    /// Lists budgets available in the authenticated profile, including archived history.
    fn list(&self, scope: &ProfileScope) -> Result<Vec<BudgetDto>, ApplicationError>;
    /// Creates one budget within the authenticated profile scope.
    fn create(
        &self,
        scope: &ProfileScope,
        request: CreateBudgetRequest,
    ) -> Result<BudgetDto, ApplicationError>;

    /// Archives a budget while retaining its historical records.
    fn archive(&self, scope: &ProfileScope, budget_id: &BudgetId) -> Result<(), ApplicationError>;
}

/// Defines budget-period planning operations independent of command and SQL adapters.
pub trait BudgetAllocationService {
    /// Lists the profile's planned category amounts in deterministic period order.
    fn list(&self, scope: &ProfileScope) -> Result<Vec<BudgetAllocationDto>, ApplicationError>;

    /// Creates or replaces a unique budget/category/period plan.
    fn upsert(
        &self,
        scope: &ProfileScope,
        request: UpsertBudgetAllocationRequest,
    ) -> Result<BudgetAllocationDto, ApplicationError>;

    /// Removes an unused planning amount without affecting ledger history.
    fn remove(
        &self,
        scope: &ProfileScope,
        allocation_id: &AllocationId,
    ) -> Result<(), ApplicationError>;
}

/// Defines category operations that will be implemented after secure sessions exist.
pub trait CategoryService {
    /// Lists categories in their deterministic display order.
    fn list(&self, scope: &ProfileScope) -> Result<Vec<CategoryDto>, ApplicationError>;
    /// Creates one category within the authenticated profile scope.
    fn create(
        &self,
        scope: &ProfileScope,
        request: CreateCategoryRequest,
    ) -> Result<CategoryDto, ApplicationError>;

    /// Archives a category while retaining its historical records.
    fn archive(
        &self,
        scope: &ProfileScope,
        category_id: &CategoryId,
    ) -> Result<(), ApplicationError>;
}

/// Defines account operations that will be implemented after secure sessions exist.
pub trait AccountService {
    /// Lists accounts in the authenticated profile, including archived history.
    fn list(&self, scope: &ProfileScope) -> Result<Vec<AccountDto>, ApplicationError>;
    /// Creates one account within the authenticated profile scope.
    fn create(
        &self,
        scope: &ProfileScope,
        request: CreateAccountRequest,
    ) -> Result<AccountDto, ApplicationError>;

    /// Archives an account while retaining its historical records.
    fn archive(&self, scope: &ProfileScope, account_id: &AccountId)
    -> Result<(), ApplicationError>;
}

/// Defines ledger operations that will be implemented after secure sessions exist.
pub trait TransactionService {
    /// Records a normalized income or expense entry in the authenticated profile scope.
    fn create(
        &self,
        scope: &ProfileScope,
        request: CreateTransactionRequest,
    ) -> Result<TransactionDto, ApplicationError>;

    /// Records one atomic transfer represented by the returned linked entries.
    fn create_transfer(
        &self,
        scope: &ProfileScope,
        request: CreateTransferRequest,
    ) -> Result<(TransactionDto, TransactionDto), ApplicationError>;

    /// Reverses an applicable entry without silently rewriting financial history.
    fn reverse(
        &self,
        scope: &ProfileScope,
        transaction_id: &TransactionId,
    ) -> Result<TransactionDto, ApplicationError>;
}

/// Defines allocation operations that will be implemented after secure sessions exist.
pub trait AllocationService {
    /// Records an allocation within the authenticated profile scope.
    fn create(
        &self,
        scope: &ProfileScope,
        request: CreateAllocationRequest,
    ) -> Result<AllocationDto, ApplicationError>;

    /// Removes an allocation only where its future workflow permits it.
    fn remove(
        &self,
        scope: &ProfileScope,
        allocation_id: &AllocationId,
    ) -> Result<(), ApplicationError>;
}

/// Defines encrypted backup operations without exposing filesystem or cryptographic details to callers.
pub trait BackupService {
    /// Creates an authenticated encrypted backup for the authenticated profile scope.
    fn export(&self, scope: &ProfileScope) -> Result<BackupDto, ApplicationError>;

    /// Restores an already-validated encrypted backup under an explicit restore policy.
    fn restore(
        &self,
        scope: &ProfileScope,
        backup_id: &BackupId,
        policy: RestorePolicy,
    ) -> Result<(), ApplicationError>;
}

/// Holds validated profile-creation input until the profile service consumes it.
pub struct CreateProfileRequest {
    /// Holds the caller-selected profile identifier.
    pub profile_id: String,
    /// Holds the passphrase only during the setup operation.
    pub passphrase: Zeroizing<Vec<u8>>,
}

/// Holds validated profile-unlock input until the profile service consumes it.
pub struct UnlockProfileRequest {
    /// Holds the requested profile identifier.
    pub profile_id: String,
    /// Holds the passphrase only during the unlock operation.
    pub passphrase: Zeroizing<Vec<u8>>,
}

/// Holds the user-entered fields required to create a budget.
pub struct CreateBudgetRequest {
    /// Holds the intended display name.
    pub name: String,
    /// Holds the intended ISO-style currency code.
    pub currency: String,
}

/// Holds the user-entered fields required to create a category.
pub struct CreateCategoryRequest {
    /// Holds the intended display name.
    pub name: String,
    /// Holds the optional presentation color.
    pub color: Option<String>,
}

/// Holds the user-entered fields required to create an account.
pub struct CreateAccountRequest {
    /// Holds the intended display name.
    pub name: String,
    /// Holds the intended ISO-style currency code.
    pub currency: String,
    /// Holds the exact opening balance in minor units.
    pub opening_minor_units: i64,
}

/// Holds the user-entered fields required to record income or an expense.
pub struct CreateTransactionRequest {
    /// Holds the affected account identifier.
    pub account_id: String,
    /// Holds the optional budget identifier associated with the entry.
    pub budget_id: Option<String>,
    /// Holds the optional budget category identifier.
    pub category_id: Option<String>,
    /// Identifies whether the entry is income or an expense.
    pub entry_type: String,
    /// Holds the exact signed amount in minor units.
    pub minor_units: i64,
    /// Holds the accompanying ISO-style currency code.
    pub currency: String,
    /// Holds the canonical local transaction date.
    pub occurred_on: String,
    /// Holds an optional counterparty name.
    pub payee: Option<String>,
    /// Holds an optional short description.
    pub description: Option<String>,
    /// Holds optional long-form notes.
    pub notes: Option<String>,
    /// Holds category splits that must total the transaction amount exactly.
    pub allocations: Vec<TransactionAllocationRequest>,
}

/// Holds one exact category portion of a new immutable transaction.
pub struct TransactionAllocationRequest {
    /// Holds the category receiving this portion of the transaction.
    pub category_id: String,
    /// Holds the signed portion in the same direction as the transaction.
    pub minor_units: i64,
}

/// Holds the fields required to create or replace one period/category budget plan.
pub struct UpsertBudgetAllocationRequest {
    /// Holds the affected budget identifier.
    pub budget_id: String,
    /// Holds the planned category identifier.
    pub category_id: String,
    /// Holds the inclusive start of the planned period.
    pub period_starts_on: String,
    /// Holds the inclusive end of the planned period.
    pub period_ends_on: String,
    /// Holds the signed planned amount in exact minor units.
    pub planned_minor_units: i64,
    /// Holds optional supporting text for the plan.
    pub notes: Option<String>,
}

/// Holds the user-entered fields required to record a transfer atomically.
pub struct CreateTransferRequest {
    /// Holds the source account identifier.
    pub source_account_id: String,
    /// Holds the destination account identifier.
    pub destination_account_id: String,
    /// Holds the positive transfer amount in minor units.
    pub minor_units: i64,
    /// Holds the accompanying ISO-style currency code.
    pub currency: String,
    /// Holds the canonical local transfer date.
    pub occurred_on: String,
}

/// Holds the user-entered fields required to allocate a transaction amount.
pub struct CreateAllocationRequest {
    /// Holds the owning transaction identifier.
    pub transaction_id: String,
    /// Holds the destination category identifier.
    pub category_id: String,
    /// Holds the exact allocation amount in minor units.
    pub minor_units: i64,
}

/// Selects whether a future restore replaces or merges compatible data.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RestorePolicy {
    /// Replaces the active profile data after backup validation and snapshot safeguards.
    Replace,
    /// Merges compatible backup records after conflict validation.
    Merge,
}
