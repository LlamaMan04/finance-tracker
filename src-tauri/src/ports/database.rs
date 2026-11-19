//! Database-facing ports. Frontend/UI ports deliberately live in `frontend.rs`.

use crate::application::ApplicationError;
use crate::domain::{
    AccountId, AllocationId, BackupId, BudgetId, CategoryId, ProfileScope, TransactionId,
};

/// Gives database adapters a result type that preserves the reviewed error contract.
pub type DatabaseResult<T> = Result<T, ApplicationError>;

/// Reports the initialized schema version without exposing connections or SQL to callers.
pub trait DatabasePort {
    /// Returns the highest successfully applied migration version.
    fn schema_version(&self) -> DatabaseResult<u32>;
}

/// Gives database repositories one result type that preserves the reviewed error contract.
pub type RepositoryResult<T> = Result<T, ApplicationError>;

/// Defines profile and lock-state persistence without prescribing a key or database implementation.
pub trait ProfileSecurityRepository {
    /// Reports whether metadata exists for the profile scope without opening protected finance data.
    fn profile_exists(&self, scope: &ProfileScope) -> RepositoryResult<bool>;

    /// Reports whether the profile may open protected data in the current session.
    fn is_unlocked(&self, scope: &ProfileScope) -> RepositoryResult<bool>;
}

/// Defines the budget persistence boundary.
pub trait BudgetRepository {
    /// Verifies that a budget belongs to the supplied profile scope.
    fn exists(&self, scope: &ProfileScope, budget_id: &BudgetId) -> RepositoryResult<bool>;
}

/// Defines the category persistence boundary.
pub trait CategoryRepository {
    /// Verifies that a category belongs to the supplied profile scope.
    fn exists(&self, scope: &ProfileScope, category_id: &CategoryId) -> RepositoryResult<bool>;
}

/// Defines the account persistence boundary.
pub trait AccountRepository {
    /// Verifies that an account belongs to the supplied profile scope.
    fn exists(&self, scope: &ProfileScope, account_id: &AccountId) -> RepositoryResult<bool>;
}

/// Defines the transaction persistence boundary.
pub trait TransactionRepository {
    /// Verifies that a transaction belongs to the supplied profile scope.
    fn exists(
        &self,
        scope: &ProfileScope,
        transaction_id: &TransactionId,
    ) -> RepositoryResult<bool>;
}

/// Defines the transaction-allocation persistence boundary.
pub trait AllocationRepository {
    /// Verifies that an allocation belongs to the supplied profile scope.
    fn exists(&self, scope: &ProfileScope, allocation_id: &AllocationId) -> RepositoryResult<bool>;
}

/// Defines the encrypted-backup persistence boundary without exposing file or crypto details.
pub trait BackupRepository {
    /// Verifies that backup metadata belongs to the supplied profile scope.
    fn exists(&self, scope: &ProfileScope, backup_id: &BackupId) -> RepositoryResult<bool>;
}
