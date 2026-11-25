//! Shared repository fixture and focused regression coverage for the split finance modules.

use super::FinanceRepository;
use crate::application::use_cases::{
    AdjustTransactionRequest, TransactionAllocationRequest, TransactionHistoryRequest,
};
use crate::domain::{ProfileId, ProfileScope};
use crate::infrastructure::test_support::TemporaryDatabase;
use crate::ports::security::KeyDerivationPort;
use crate::security::Argon2idKeyDeriver;

/// Creates the protected profile-scoped database used by finance repository regression tests.
fn fixture() -> (TemporaryDatabase, crate::infrastructure::database::Database, ProfileScope) {
    let fixture = TemporaryDatabase::new();
    let deriver = Argon2idKeyDeriver;
    let metadata = deriver.create_metadata().expect("test metadata is generated");
    let key = deriver
        .derive_database_key(b"finance repository test", &metadata)
        .expect("test key derives");
    let database = crate::infrastructure::database::Database::create(fixture.path(), &key)
        .expect("database opens");
    let scope =
        ProfileScope::new(ProfileId::parse("profile-a").expect("fixture identifier is valid"));
    database
        .in_transaction(|transaction| {
            transaction
                .execute(
                    "INSERT INTO profile_scopes (id, created_at, updated_at) VALUES (?1, ?2, ?2)",
                    (scope.profile_id().as_str(), "2026-11-23T00:00:00Z"),
                )
                .map_err(|_| crate::application::ApplicationError::internal())?;
            Ok(())
        })
        .expect("profile scope is seeded");
    (fixture, database, scope)
}

/// Proves reference-data methods remain available through the stable repository facade.
#[test]
fn persists_profile_scoped_reference_data() {
    let (_fixture, database, scope) = fixture();
    let budget = FinanceRepository::create_budget(&database, &scope, "Home", "USD")
        .expect("budget is created");
    let category = FinanceRepository::create_category(
        &database,
        &scope,
        "Groceries",
        "expense",
        Some("green"),
        1,
    )
    .expect("category is created");
    let account =
        FinanceRepository::create_account(&database, &scope, "Checking", "checking", "USD", 0)
            .expect("account is created");

    let renamed = FinanceRepository::update_budget(&database, &scope, &budget.id, "Household")
        .expect("budget is renamed");
    let reordered = FinanceRepository::update_category(
        &database,
        &scope,
        &category.id,
        "Food",
        "expense",
        None,
        2,
    )
    .expect("category is updated");
    let updated_account = FinanceRepository::update_account(
        &database,
        &scope,
        &account.id,
        "Main checking",
        "checking",
    )
    .expect("account is updated");
    FinanceRepository::archive_budget(&database, &scope, &budget.id).expect("budget archives");
    FinanceRepository::archive_category(&database, &scope, &category.id)
        .expect("category archives");
    FinanceRepository::archive_account(&database, &scope, &account.id).expect("account archives");

    assert_eq!(renamed.name, "Household");
    assert_eq!(reordered.sort_order, 2);
    assert_eq!(updated_account.opening_minor_units, "0");
    assert!(
        FinanceRepository::list_budgets(&database, &scope).expect("budgets list")[0].is_archived
    );
    assert!(
        FinanceRepository::list_categories(&database, &scope).expect("categories list")[0]
            .is_archived
    );
    assert!(
        FinanceRepository::list_accounts(&database, &scope).expect("accounts list")[0].is_archived
    );
}

/// Proves invalid reference data and writes in another profile scope are rejected.
#[test]
fn rejects_invalid_values_and_other_profile_records() {
    let (_fixture, database, scope) = fixture();
    let invalid = FinanceRepository::create_account(&database, &scope, "Cash", "cash", "usd", 0)
        .expect_err("lowercase currency is rejected");
    let other_scope =
        ProfileScope::new(ProfileId::parse("profile-b").expect("valid fixture identifier"));
    let missing = FinanceRepository::create_budget(&database, &other_scope, "Elsewhere", "USD")
        .expect_err("unknown scope cannot write");
    assert_eq!(invalid.code, crate::application::ErrorCode::Validation);
    assert_eq!(missing.code, crate::application::ErrorCode::Conflict);
}

/// Proves entries retain normalized details and reject incompatible signs or category kinds.
#[test]
fn creates_normalized_income_and_expense_entries() {
    let (_fixture, database, scope) = fixture();
    let budget =
        FinanceRepository::create_budget(&database, &scope, "Home", "USD").expect("budget");
    let income = FinanceRepository::create_category(&database, &scope, "Salary", "income", None, 1)
        .expect("income category");
    let expense = FinanceRepository::create_category(&database, &scope, "Food", "expense", None, 2)
        .expect("expense category");
    let account =
        FinanceRepository::create_account(&database, &scope, "Checking", "checking", "USD", 0)
            .expect("account");
    let entry = FinanceRepository::create_transaction(
        &database,
        &scope,
        &account.id,
        Some(&budget.id),
        Some(&income.id),
        "income",
        12_345,
        "USD",
        "2024-02-29",
        Some("Employer"),
        Some("February salary"),
        Some("Leap-day payment"),
        &[],
    )
    .expect("income");
    let wrong_sign = FinanceRepository::create_transaction(
        &database,
        &scope,
        &account.id,
        None,
        Some(&expense.id),
        "expense",
        1,
        "USD",
        "2026-11-23",
        None,
        None,
        None,
        &[],
    )
    .expect_err("sign");
    let wrong_category = FinanceRepository::create_transaction(
        &database,
        &scope,
        &account.id,
        None,
        Some(&income.id),
        "expense",
        -1,
        "USD",
        "2026-11-23",
        None,
        None,
        None,
        &[],
    )
    .expect_err("kind");
    assert_eq!(entry.minor_units, "12345");
    assert_eq!(entry.occurred_on, "2024-02-29");
    assert_eq!(entry.budget_id, Some(budget.id));
    assert_eq!(entry.category_id, Some(income.id));
    assert_eq!(entry.payee, Some("Employer".to_owned()));
    assert_eq!(wrong_sign.code, crate::application::ErrorCode::Validation);
    assert_eq!(wrong_category.code, crate::application::ErrorCode::Validation);
}

/// Proves split entries are atomic and require an exact category total.
#[test]
fn creates_atomic_category_splits() {
    let (_fixture, database, scope) = fixture();
    let groceries =
        FinanceRepository::create_category(&database, &scope, "Groceries", "expense", None, 1)
            .expect("groceries");
    let dining =
        FinanceRepository::create_category(&database, &scope, "Dining", "expense", None, 2)
            .expect("dining");
    let income = FinanceRepository::create_category(&database, &scope, "Salary", "income", None, 3)
        .expect("income");
    let account =
        FinanceRepository::create_account(&database, &scope, "Checking", "checking", "USD", 0)
            .expect("account");
    let entry = FinanceRepository::create_transaction(
        &database,
        &scope,
        &account.id,
        None,
        None,
        "expense",
        -1_000,
        "USD",
        "2026-11-23",
        None,
        None,
        None,
        &[
            TransactionAllocationRequest { category_id: groceries.id.clone(), minor_units: -700 },
            TransactionAllocationRequest { category_id: dining.id.clone(), minor_units: -300 },
        ],
    )
    .expect("split");
    let invalid_total = FinanceRepository::create_transaction(
        &database,
        &scope,
        &account.id,
        None,
        None,
        "expense",
        -1_000,
        "USD",
        "2026-11-23",
        None,
        None,
        None,
        &[TransactionAllocationRequest { category_id: groceries.id.clone(), minor_units: -999 }],
    )
    .expect_err("split totals must equal the transaction");
    let invalid_category = FinanceRepository::create_transaction(
        &database,
        &scope,
        &account.id,
        None,
        None,
        "expense",
        -1_000,
        "USD",
        "2026-11-23",
        None,
        None,
        None,
        &[TransactionAllocationRequest { category_id: income.id, minor_units: -1_000 }],
    )
    .expect_err("kind mismatch");
    assert_eq!(entry.category_id, None);
    assert_eq!(entry.allocations.len(), 2);
    assert_eq!(entry.allocations[0].minor_units, "-700");
    assert_eq!(entry.allocations[1].category_id, dining.id);
    assert_eq!(invalid_total.code, crate::application::ErrorCode::Validation);
    assert_eq!(invalid_category.code, crate::application::ErrorCode::Validation);
    database
        .in_transaction(|transaction| {
            let transaction_count: i64 = transaction
                .query_row("SELECT COUNT(*) FROM transactions", [], |row| row.get(0))
                .map_err(|_| crate::application::ApplicationError::internal())?;
            let allocation_count: i64 = transaction
                .query_row("SELECT COUNT(*) FROM transaction_allocations", [], |row| row.get(0))
                .map_err(|_| crate::application::ApplicationError::internal())?;
            assert_eq!(transaction_count, 1);
            assert_eq!(allocation_count, 2);
            Ok(())
        })
        .expect("rejected splits leave no partial records");
}

/// Proves history filtering includes split categories and preserves deterministic paging.
#[test]
fn lists_filtered_transaction_history_in_deterministic_pages() {
    let (_fixture, database, scope) = fixture();
    let groceries =
        FinanceRepository::create_category(&database, &scope, "Groceries", "expense", None, 1)
            .expect("groceries");
    let dining =
        FinanceRepository::create_category(&database, &scope, "Dining", "expense", None, 2)
            .expect("dining");
    let account =
        FinanceRepository::create_account(&database, &scope, "Checking", "checking", "USD", 0)
            .expect("account");
    let older = FinanceRepository::create_transaction(
        &database,
        &scope,
        &account.id,
        None,
        Some(&groceries.id),
        "expense",
        -200,
        "USD",
        "2026-01-01",
        Some("Market"),
        None,
        None,
        &[],
    )
    .expect("older");
    let newer = FinanceRepository::create_transaction(
        &database,
        &scope,
        &account.id,
        None,
        None,
        "expense",
        -300,
        "USD",
        "2026-01-02",
        Some("Cafe"),
        None,
        None,
        &[TransactionAllocationRequest { category_id: dining.id.clone(), minor_units: -300 }],
    )
    .expect("newer");
    let filtered = FinanceRepository::list_transactions(
        &database,
        &scope,
        TransactionHistoryRequest {
            starts_on: Some("2026-01-01".to_owned()),
            ends_on: Some("2026-01-31".to_owned()),
            account_id: Some(account.id.clone()),
            category_id: Some(dining.id.clone()),
            entry_type: Some("expense".to_owned()),
            search: Some("cafe".to_owned()),
            page: 0,
            page_size: 20,
        },
    )
    .expect("filtered");
    let next = FinanceRepository::list_transactions(
        &database,
        &scope,
        TransactionHistoryRequest {
            starts_on: None,
            ends_on: None,
            account_id: Some(account.id.clone()),
            category_id: None,
            entry_type: None,
            search: None,
            page: 1,
            page_size: 1,
        },
    )
    .expect("next");
    assert_eq!(filtered.total_count, 1);
    assert_eq!(filtered.transactions[0].id, newer.id);
    assert_eq!(filtered.transactions[0].allocations[0].category_id, dining.id);
    assert_eq!(next.transactions[0].id, older.id);
}

/// Proves reversals are equal-and-opposite facts and a second correction is rejected.
#[test]
fn reverses_an_income_or_expense_without_rewriting_history() {
    let (_fixture, database, scope) = fixture();
    let category =
        FinanceRepository::create_category(&database, &scope, "Groceries", "expense", None, 1)
            .expect("category");
    let account =
        FinanceRepository::create_account(&database, &scope, "Checking", "checking", "USD", 0)
            .expect("account");
    let original = FinanceRepository::create_transaction(
        &database,
        &scope,
        &account.id,
        None,
        None,
        "expense",
        -750,
        "USD",
        "2026-02-01",
        None,
        None,
        None,
        &[TransactionAllocationRequest { category_id: category.id, minor_units: -750 }],
    )
    .expect("entry");
    let correction =
        FinanceRepository::reverse_transaction(&database, &scope, &original.id).expect("reversal");
    let duplicate = FinanceRepository::reverse_transaction(&database, &scope, &original.id)
        .expect_err("duplicate");
    assert_eq!(correction.entry_type, "correction");
    assert_eq!(correction.minor_units, "750");
    assert_eq!(correction.allocations[0].minor_units, "750");
    assert_eq!(duplicate.code, crate::application::ErrorCode::Conflict);
    database.in_transaction(|transaction| {
        let correction_count: i64 = transaction.query_row("SELECT COUNT(*) FROM transaction_corrections WHERE profile_id = ?1 AND original_transaction_id = ?2", (scope.profile_id().as_str(), original.id), |row| row.get(0)).map_err(|_| crate::application::ApplicationError::internal())?;
        assert_eq!(correction_count, 1);
        Ok(())
    }).expect("correction audit link is persisted");
}

/// Proves adjustments create auditable category deltas without rewriting the original fact.
#[test]
fn adjusts_an_entry_with_a_single_auditable_delta() {
    let (_fixture, database, scope) = fixture();
    let budget =
        FinanceRepository::create_budget(&database, &scope, "Home", "USD").expect("budget");
    let groceries =
        FinanceRepository::create_category(&database, &scope, "Groceries", "expense", None, 1)
            .expect("groceries");
    let dining =
        FinanceRepository::create_category(&database, &scope, "Dining", "expense", None, 2)
            .expect("dining");
    let account =
        FinanceRepository::create_account(&database, &scope, "Checking", "checking", "USD", 0)
            .expect("account");
    let original = FinanceRepository::create_transaction(
        &database,
        &scope,
        &account.id,
        Some(&budget.id),
        Some(&groceries.id),
        "expense",
        -1_000,
        "USD",
        "2026-02-01",
        Some("Market"),
        Some("Weekly shop"),
        None,
        &[],
    )
    .expect("entry");
    let correction = FinanceRepository::adjust_transaction(
        &database,
        &scope,
        AdjustTransactionRequest {
            transaction_id: original.id.clone(),
            adjusted_minor_units: -1_200,
            category_id: Some(dining.id.clone()),
            payee: Some("Restaurant".to_owned()),
            description: Some("Corrected receipt".to_owned()),
            notes: Some("Receipt category was wrong".to_owned()),
            allocations: vec![],
            reason: "Corrected receipt amount and category".to_owned(),
        },
    )
    .expect("adjustment");
    let duplicate = FinanceRepository::adjust_transaction(
        &database,
        &scope,
        AdjustTransactionRequest {
            transaction_id: original.id.clone(),
            adjusted_minor_units: -1_100,
            category_id: None,
            payee: None,
            description: None,
            notes: None,
            allocations: vec![],
            reason: "Second change".to_owned(),
        },
    )
    .expect_err("duplicate");
    assert_eq!(correction.entry_type, "correction");
    assert_eq!(correction.minor_units, "-200");
    assert_eq!(correction.category_id, None);
    assert_eq!(correction.allocations.len(), 2);
    let groceries_delta = correction
        .allocations
        .iter()
        .find(|allocation| allocation.category_id == groceries.id)
        .expect("groceries delta is present");
    let dining_delta = correction
        .allocations
        .iter()
        .find(|allocation| allocation.category_id == dining.id)
        .expect("dining delta is present");
    assert_eq!(groceries_delta.minor_units, "1000");
    assert_eq!(dining_delta.minor_units, "-1200");
    assert_eq!(duplicate.code, crate::application::ErrorCode::Conflict);
    database.in_transaction(|transaction| {
        let original_amount: i64 = transaction.query_row("SELECT minor_units FROM transactions WHERE profile_id = ?1 AND id = ?2", (scope.profile_id().as_str(), original.id.as_str()), |row| row.get(0)).map_err(|_| crate::application::ApplicationError::internal())?;
        let correction_kind: String = transaction.query_row("SELECT correction_kind FROM transaction_corrections WHERE profile_id = ?1 AND correction_transaction_id = ?2", (scope.profile_id().as_str(), correction.id.as_str()), |row| row.get(0)).map_err(|_| crate::application::ApplicationError::internal())?;
        assert_eq!(original_amount, -1_000);
        assert_eq!(correction_kind, "adjustment");
        Ok(())
    }).expect("original and audit link are retained");
}
