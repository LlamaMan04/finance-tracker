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

/// Carries the non-secret category fields that future commands will accept or display.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryDto {
    /// Holds the stable category identifier.
    pub id: String,
    /// Holds the user-visible category name.
    pub name: String,
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
    /// Holds the account's ISO-style currency code.
    pub currency: String,
    /// Marks whether the account remains available for new transactions.
    pub is_archived: bool,
}

/// Carries a normalized transaction representation without exposing persistence details.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionDto {
    /// Holds the stable transaction identifier.
    pub id: String,
    /// Holds the account affected by this transaction.
    pub account_id: String,
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
    use super::{SessionStatusDto, TransactionDto};

    #[test]
    fn serializes_safe_session_status_with_frontend_field_names() {
        let serialized = serde_json::to_value(SessionStatusDto {
            profile_id: Some("primary-profile".to_owned()),
            is_locked: true,
        })
        .expect("session status must serialize for commands");

        assert_eq!(serialized["profileId"], "primary-profile");
        assert_eq!(serialized["isLocked"], true);
        assert!(serialized.get("profile_id").is_none());
    }

    #[test]
    fn preserves_large_transaction_amounts_as_strings_for_javascript() {
        let serialized = serde_json::to_value(TransactionDto {
            id: "transaction-01".to_owned(),
            account_id: "account-01".to_owned(),
            category_id: None,
            minor_units: "9007199254740993".to_owned(),
            currency: "USD".to_owned(),
            occurred_on: "2026-11-18".to_owned(),
            entry_type: "expense".to_owned(),
        })
        .expect("transaction DTO must serialize for commands");

        assert_eq!(serialized["minorUnits"], "9007199254740993");
    }
}
