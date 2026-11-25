//! Safe request and response shapes shared by future use cases and command adapters.

use serde::{Deserialize, Serialize};

/// Represents the lock state that is safe to render before protected data is available.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionStatusDto {
    /// Identifies the selected profile without exposing any protected profile contents.
    pub profile_id: Option<String>,
    /// Prevents the UI from assuming that protected repositories are accessible.
    pub is_locked: bool,
    /// Exposes the active session's non-sensitive idle timeout for UI scheduling only.
    pub inactivity_timeout_milliseconds: Option<u64>,
}

/// Identifies a selectable local profile without exposing protected contents or paths.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileSummaryDto {
    /// Holds the opaque profile identifier selected before authentication.
    pub id: String,
}

/// Carries the non-secret budget fields that future commands will accept or display.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BudgetDto {
    /// Holds the stable budget identifier.
    pub id: String,
    /// Holds the user-visible budget name.
    pub name: String,
    /// Holds the budget's ISO-style currency code.
    pub currency: String,
    /// Marks whether the budget remains available for new allocations.
    pub is_archived: bool,
}

/// Carries one planned category amount for an inclusive budget period.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BudgetAllocationDto {
    /// Holds the stable allocation identifier.
    pub id: String,
    /// Holds the owning budget identifier.
    pub budget_id: String,
    /// Holds the planned category identifier.
    pub category_id: String,
    /// Holds the inclusive canonical period start date.
    pub period_starts_on: String,
    /// Holds the inclusive canonical period end date.
    pub period_ends_on: String,
    /// Stores the signed planned amount as an exact string for JavaScript safety.
    pub planned_minor_units: String,
    /// Holds an optional user-provided explanation without exposing persistence details.
    pub notes: Option<String>,
}

/// Carries the non-secret category fields that future commands will accept or display.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryDto {
    /// Holds the stable category identifier.
    pub id: String,
    /// Holds the user-visible category name.
    pub name: String,
    /// Identifies whether this category classifies income or expenses.
    pub category_kind: String,
    /// Retains a presentation color only; financial meaning must not rely on it.
    pub color: Option<String>,
    /// Preserves deterministic category ordering.
    pub sort_order: i32,
    /// Marks whether the category remains available for new transactions.
    pub is_archived: bool,
}

/// Carries the non-secret account fields that future commands will accept or display.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountDto {
    /// Holds the stable account identifier.
    pub id: String,
    /// Holds the user-visible account name.
    pub name: String,
    /// Holds the approved account type used for presentation and later workflows.
    pub account_type: String,
    /// Holds the account's ISO-style currency code.
    pub currency: String,
    /// Stores the opening balance as an exact string for JavaScript safety.
    pub opening_minor_units: String,
    /// Marks whether the account remains available for new transactions.
    pub is_archived: bool,
}

/// Carries one account's Rust-derived current balance without changing its stored opening balance.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountBalanceDto {
    /// Holds the stable account identifier.
    pub account_id: String,
    /// Holds the account name, including archived accounts needed for historical reconciliation.
    pub account_name: String,
    /// Holds the currency paired with the exact balance.
    pub currency: String,
    /// Stores opening balance plus every applicable immutable ledger fact.
    pub balance_minor_units: String,
    /// Marks whether the account is no longer available for new activity.
    pub is_archived: bool,
}

/// Carries signed income and expense totals for one currency without mixing monetary units.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CurrencyTotalsDto {
    /// Holds the currency shared by the two exact totals.
    pub currency: String,
    /// Holds net income, including applicable immutable corrections.
    pub income_minor_units: String,
    /// Holds net expense using the ledger's negative sign convention.
    pub expense_minor_units: String,
}

/// Carries the protected, Rust-authoritative financial snapshot used by the workspace.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FinanceSummaryDto {
    /// Holds every profile-owned account balance, including archived accounts.
    pub account_balances: Vec<AccountBalanceDto>,
    /// Holds income and expense totals partitioned by currency.
    pub currency_totals: Vec<CurrencyTotalsDto>,
}

/// Carries one category's planned, actual, and remaining budget amounts for an exact period.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BudgetCategorySummaryDto {
    /// Holds the category identifier used by the budget plan or ledger activity.
    pub category_id: String,
    /// Holds the category label even when it is archived.
    pub category_name: String,
    /// Identifies whether the category represents income or expense activity.
    pub category_kind: String,
    /// Holds the signed planned amount for the selected exact period.
    pub planned_minor_units: String,
    /// Holds the signed actual ledger amount for the selected period.
    pub actual_minor_units: String,
    /// Holds planned minus actual using the shared signed ledger convention.
    pub remaining_minor_units: String,
}

/// Carries a selected budget's protected calculation results for one inclusive planning period.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BudgetSummaryDto {
    /// Holds the selected budget identifier.
    pub budget_id: String,
    /// Holds the budget's currency paired with every returned amount.
    pub currency: String,
    /// Holds the inclusive selected period start.
    pub period_starts_on: String,
    /// Holds the inclusive selected period end.
    pub period_ends_on: String,
    /// Holds the signed total of plans matching the selected exact period.
    pub planned_minor_units: String,
    /// Holds the signed total categorized activity for the selected period.
    pub actual_minor_units: String,
    /// Holds planned minus actual using the ledger's signed convention.
    pub remaining_minor_units: String,
    /// Holds deterministic category-level planning and activity results.
    pub categories: Vec<BudgetCategorySummaryDto>,
}

/// Carries a normalized transaction representation without exposing persistence details.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionDto {
    /// Holds the stable transaction identifier.
    pub id: String,
    /// Holds the account affected by this transaction.
    pub account_id: String,
    /// Holds the optional budget selected for the entry.
    pub budget_id: Option<String>,
    /// Holds the optional budget category affected by this transaction.
    pub category_id: Option<String>,
    /// Stores the signed amount as exact minor units represented as a string at the UI boundary.
    pub minor_units: String,
    /// Holds the ISO-style currency code paired with the amount.
    pub currency: String,
    /// Holds the canonical local calendar date.
    pub occurred_on: String,
    /// Identifies whether the entry is income, expense, transfer, or correction.
    pub entry_type: String,
    /// Holds the optional counterparty name supplied for display.
    pub payee: Option<String>,
    /// Holds the optional short description supplied for display.
    pub description: Option<String>,
    /// Holds the optional long-form note supplied for display.
    pub notes: Option<String>,
    /// Holds the exact category splits recorded atomically with this entry.
    pub allocations: Vec<AllocationDto>,
}

/// Carries one deterministic page of protected transaction history.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionHistoryPageDto {
    /// Holds the transactions selected by the requested profile-scoped filters.
    pub transactions: Vec<TransactionDto>,
    /// Holds the total matching record count so the UI can render stable pagination controls.
    pub total_count: u64,
    /// Holds the zero-based page number that produced this result.
    pub page: u32,
    /// Holds the bounded page size used to produce this result.
    pub page_size: u32,
}

/// Carries the two normalized ledger facts created by one atomic account transfer.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferDto {
    /// Holds the negative transfer fact recorded against the source account.
    pub source_transaction: TransactionDto,
    /// Holds the positive transfer fact recorded against the destination account.
    pub destination_transaction: TransactionDto,
}

/// Carries a transaction-to-budget allocation without financial calculation logic in the UI.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AllocationDto {
    /// Holds the stable allocation identifier.
    pub id: String,
    /// Holds the transaction that owns the allocation.
    pub transaction_id: String,
    /// Holds the category receiving the allocation.
    pub category_id: String,
    /// Stores the exact allocated amount in minor units as a string.
    pub minor_units: String,
}

/// Carries backup metadata only; a backup's path, payload, and keys never cross this boundary.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupDto {
    /// Holds the stable backup identifier.
    pub id: String,
    /// Holds the format version used for compatibility checks.
    pub format_version: u32,
    /// Holds the creation timestamp in an agreed canonical representation.
    pub created_at: String,
}

#[cfg(test)]
mod tests {
    use super::{ProfileSummaryDto, SessionStatusDto, TransactionDto};

    #[test]
    fn serializes_safe_session_status_with_frontend_field_names() {
        let serialized = serde_json::to_value(SessionStatusDto {
            profile_id: Some("primary-profile".to_owned()),
            is_locked: true,
            inactivity_timeout_milliseconds: None,
        })
        .expect("session status must serialize for commands");

        assert_eq!(serialized["profileId"], "primary-profile");
        assert_eq!(serialized["isLocked"], true);
        assert_eq!(serialized["inactivityTimeoutMilliseconds"], serde_json::Value::Null);
        assert!(serialized.get("profile_id").is_none());
    }

    #[test]
    fn preserves_large_transaction_amounts_as_strings_for_javascript() {
        let serialized = serde_json::to_value(TransactionDto {
            id: "transaction-01".to_owned(),
            account_id: "account-01".to_owned(),
            budget_id: None,
            category_id: None,
            minor_units: "9007199254740993".to_owned(),
            currency: "USD".to_owned(),
            occurred_on: "2026-11-18".to_owned(),
            entry_type: "expense".to_owned(),
            payee: None,
            description: None,
            notes: None,
            allocations: vec![],
        })
        .expect("transaction DTO must serialize for commands");

        assert_eq!(serialized["minorUnits"], "9007199254740993");
    }

    #[test]
    fn serializes_a_non_secret_profile_summary() {
        let serialized = serde_json::to_value(ProfileSummaryDto { id: "home-profile".to_owned() })
            .expect("profile summary must serialize for commands");

        assert_eq!(serialized, serde_json::json!({ "id": "home-profile" }));
    }
}
