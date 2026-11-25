//! Read-only, profile-scoped ledger calculations kept outside the frontend and SQL write paths.

use crate::application::ApplicationError;
use crate::application::contracts::{
    AccountBalanceDto, BudgetCategorySummaryDto, BudgetSummaryDto, CurrencyTotalsDto,
    FinanceSummaryDto,
};
use crate::domain::{BudgetId, LocalDate, PlanningPeriod, ProfileScope};
use crate::infrastructure::database::Database;
use rusqlite::params;
use std::collections::BTreeMap;

/// Computes exact ledger and budget totals from immutable protected records.
pub struct CalculationRepository;

impl CalculationRepository {
    /// Returns every account balance plus net income and expense totals partitioned by currency.
    pub fn finance_summary(
        database: &Database,
        scope: &ProfileScope,
    ) -> Result<FinanceSummaryDto, ApplicationError> {
        database.in_transaction(|transaction| {
            // Accounts remain visible after archival so their opening balance and ledger history reconcile.
            let mut account_statement = transaction
                .prepare(
                    "SELECT a.id, a.name, a.currency, a.opening_minor_units, a.is_archived, t.minor_units \
                     FROM accounts a LEFT JOIN transactions t \
                     ON t.profile_id = a.profile_id AND t.account_id = a.id \
                     WHERE a.profile_id = ?1 \
                     ORDER BY a.is_archived, a.name COLLATE NOCASE, a.id, t.id",
                )
                .map_err(|_| ApplicationError::internal())?;
            let mut account_rows = account_statement
                .query([scope.profile_id().as_str()])
                .map_err(|_| ApplicationError::internal())?;
            let mut account_totals = BTreeMap::<String, AccountAccumulator>::new();

            while let Some(row) = account_rows.next().map_err(|_| ApplicationError::internal())? {
                let account_id: String = row.get(0).map_err(|_| ApplicationError::internal())?;
                let account = account_totals.entry(account_id).or_insert(AccountAccumulator {
                    name: row.get(1).map_err(|_| ApplicationError::internal())?,
                    currency: row.get(2).map_err(|_| ApplicationError::internal())?,
                    balance_minor_units: row.get(3).map_err(|_| ApplicationError::internal())?,
                    is_archived: row.get::<_, i64>(4).map_err(|_| ApplicationError::internal())? != 0,
                });
                let transaction_minor_units: Option<i64> =
                    row.get(5).map_err(|_| ApplicationError::internal())?;
                if let Some(minor_units) = transaction_minor_units {
                    account.balance_minor_units = checked_add(account.balance_minor_units, minor_units)?;
                }
            }

            // A correction inherits its original type, while transfers intentionally contribute to neither total.
            let mut total_statement = transaction
                .prepare(
                    "SELECT t.currency, COALESCE(original.entry_type, t.entry_type), t.minor_units \
                     FROM transactions t \
                     LEFT JOIN transaction_corrections correction \
                     ON correction.profile_id = t.profile_id AND correction.correction_transaction_id = t.id \
                     LEFT JOIN transactions original \
                     ON original.profile_id = correction.profile_id AND original.id = correction.original_transaction_id \
                     WHERE t.profile_id = ?1",
                )
                .map_err(|_| ApplicationError::internal())?;
            let mut total_rows = total_statement
                .query([scope.profile_id().as_str()])
                .map_err(|_| ApplicationError::internal())?;
            let mut currency_totals = BTreeMap::<String, CurrencyAccumulator>::new();

            while let Some(row) = total_rows.next().map_err(|_| ApplicationError::internal())? {
                let currency: String = row.get(0).map_err(|_| ApplicationError::internal())?;
                let entry_type: String = row.get(1).map_err(|_| ApplicationError::internal())?;
                let minor_units: i64 = row.get(2).map_err(|_| ApplicationError::internal())?;
                let totals = currency_totals.entry(currency).or_default();
                match entry_type.as_str() {
                    "income" => totals.income_minor_units = checked_add(totals.income_minor_units, minor_units)?,
                    "expense" => totals.expense_minor_units = checked_add(totals.expense_minor_units, minor_units)?,
                    "transfer" => {}
                    _ => return Err(ApplicationError::internal()),
                }
            }

            let account_balances = account_totals
                .into_iter()
                .map(|(account_id, account)| AccountBalanceDto {
                    account_id,
                    account_name: account.name,
                    currency: account.currency,
                    balance_minor_units: account.balance_minor_units.to_string(),
                    is_archived: account.is_archived,
                })
                .collect();
            let currency_totals = currency_totals
                .into_iter()
                .map(|(currency, totals)| CurrencyTotalsDto {
                    currency,
                    income_minor_units: totals.income_minor_units.to_string(),
                    expense_minor_units: totals.expense_minor_units.to_string(),
                })
                .collect();

            Ok(FinanceSummaryDto { account_balances, currency_totals })
        })
    }

    /// Returns planned, categorized actual, and remaining amounts for one exact inclusive plan period.
    pub fn budget_summary(
        database: &Database,
        scope: &ProfileScope,
        budget_id: &str,
        period_starts_on: &str,
        period_ends_on: &str,
    ) -> Result<BudgetSummaryDto, ApplicationError> {
        let budget_id = BudgetId::parse(budget_id)?;
        let period_start = LocalDate::parse(period_starts_on)?;
        let period_end = LocalDate::parse(period_ends_on)?;
        let _period = PlanningPeriod::new(period_start, period_end)?;

        database.in_transaction(|transaction| {
            let currency: String = transaction
                .query_row(
                    "SELECT currency FROM budgets WHERE profile_id = ?1 AND id = ?2",
                    params![scope.profile_id().as_str(), budget_id.as_str()],
                    |row| row.get(0),
                )
                .map_err(|_| ApplicationError::not_found())?;

            let category_details = load_category_details(transaction, scope)?;
            let mut amounts = BTreeMap::<String, CategoryAmounts>::new();
            let mut plan_statement = transaction
                .prepare(
                    "SELECT category_id, planned_minor_units FROM budget_allocations \
                     WHERE profile_id = ?1 AND budget_id = ?2 \
                     AND period_starts_on = ?3 AND period_ends_on = ?4",
                )
                .map_err(|_| ApplicationError::internal())?;
            let mut plan_rows = plan_statement
                .query(params![scope.profile_id().as_str(), budget_id.as_str(), period_starts_on, period_ends_on])
                .map_err(|_| ApplicationError::internal())?;
            while let Some(row) = plan_rows.next().map_err(|_| ApplicationError::internal())? {
                let category_id: String = row.get(0).map_err(|_| ApplicationError::internal())?;
                let planned_minor_units: i64 = row.get(1).map_err(|_| ApplicationError::internal())?;
                amounts.entry(category_id).or_default().planned_minor_units = planned_minor_units;
            }

            // Allocation rows replace the direct category for split entries, preventing double counting.
            let mut actual_statement = transaction
                .prepare(
                    "SELECT allocation.category_id, allocation.minor_units \
                     FROM transactions transaction_record \
                     INNER JOIN transaction_allocations allocation \
                     ON allocation.profile_id = transaction_record.profile_id \
                     AND allocation.transaction_id = transaction_record.id \
                     WHERE transaction_record.profile_id = ?1 AND transaction_record.budget_id = ?2 \
                     AND transaction_record.occurred_on >= ?3 AND transaction_record.occurred_on <= ?4 \
                     AND transaction_record.entry_type IN ('income', 'expense', 'correction') \
                     UNION ALL \
                     SELECT transaction_record.category_id, transaction_record.minor_units \
                     FROM transactions transaction_record \
                     WHERE transaction_record.profile_id = ?1 AND transaction_record.budget_id = ?2 \
                     AND transaction_record.occurred_on >= ?3 AND transaction_record.occurred_on <= ?4 \
                     AND transaction_record.entry_type IN ('income', 'expense', 'correction') \
                     AND transaction_record.category_id IS NOT NULL \
                     AND NOT EXISTS (SELECT 1 FROM transaction_allocations allocation \
                         WHERE allocation.profile_id = transaction_record.profile_id \
                         AND allocation.transaction_id = transaction_record.id)",
                )
                .map_err(|_| ApplicationError::internal())?;
            let mut actual_rows = actual_statement
                .query(params![scope.profile_id().as_str(), budget_id.as_str(), period_starts_on, period_ends_on])
                .map_err(|_| ApplicationError::internal())?;
            while let Some(row) = actual_rows.next().map_err(|_| ApplicationError::internal())? {
                let category_id: String = row.get(0).map_err(|_| ApplicationError::internal())?;
                let actual_minor_units: i64 = row.get(1).map_err(|_| ApplicationError::internal())?;
                let amounts = amounts.entry(category_id).or_default();
                amounts.actual_minor_units = checked_add(amounts.actual_minor_units, actual_minor_units)?;
            }

            let mut planned_total = 0_i64;
            let mut actual_total = 0_i64;
            let mut categories = Vec::with_capacity(amounts.len());
            for (category_id, amount) in amounts {
                let category = category_details.get(&category_id).ok_or_else(ApplicationError::internal)?;
                let remaining_minor_units = checked_sub(amount.planned_minor_units, amount.actual_minor_units)?;
                planned_total = checked_add(planned_total, amount.planned_minor_units)?;
                actual_total = checked_add(actual_total, amount.actual_minor_units)?;
                categories.push(BudgetCategorySummaryDto {
                    category_id,
                    category_name: category.name.clone(),
                    category_kind: category.kind.clone(),
                    planned_minor_units: amount.planned_minor_units.to_string(),
                    actual_minor_units: amount.actual_minor_units.to_string(),
                    remaining_minor_units: remaining_minor_units.to_string(),
                });
            }
            categories.sort_by(|left, right| {
                left.category_kind.cmp(&right.category_kind).then_with(|| left.category_name.cmp(&right.category_name)).then_with(|| left.category_id.cmp(&right.category_id))
            });
            let remaining_minor_units = checked_sub(planned_total, actual_total)?;

            Ok(BudgetSummaryDto {
                budget_id: budget_id.as_str().to_owned(),
                currency,
                period_starts_on: period_starts_on.to_owned(),
                period_ends_on: period_ends_on.to_owned(),
                planned_minor_units: planned_total.to_string(),
                actual_minor_units: actual_total.to_string(),
                remaining_minor_units: remaining_minor_units.to_string(),
                categories,
            })
        })
    }
}

/// Holds the account fields and mutable aggregate used while rows are streamed from SQLite.
struct AccountAccumulator {
    name: String,
    currency: String,
    balance_minor_units: i64,
    is_archived: bool,
}

/// Holds independent signed totals so different currencies are never combined.
#[derive(Default)]
struct CurrencyAccumulator {
    income_minor_units: i64,
    expense_minor_units: i64,
}

/// Holds one category's accumulated plan and actual amounts before DTO formatting.
#[derive(Default)]
struct CategoryAmounts {
    planned_minor_units: i64,
    actual_minor_units: i64,
}

/// Holds stable archived-safe category metadata needed to label calculation output.
struct CategoryDetail {
    name: String,
    kind: String,
}

/// Reads all profile-owned category labels because plans and historical facts may reference archives.
fn load_category_details(
    transaction: &rusqlite::Transaction<'_>,
    scope: &ProfileScope,
) -> Result<BTreeMap<String, CategoryDetail>, ApplicationError> {
    let mut statement = transaction
        .prepare("SELECT id, name, category_kind FROM categories WHERE profile_id = ?1")
        .map_err(|_| ApplicationError::internal())?;
    let mut rows =
        statement.query([scope.profile_id().as_str()]).map_err(|_| ApplicationError::internal())?;
    let mut categories = BTreeMap::new();
    while let Some(row) = rows.next().map_err(|_| ApplicationError::internal())? {
        let id: String = row.get(0).map_err(|_| ApplicationError::internal())?;
        let detail = CategoryDetail {
            name: row.get(1).map_err(|_| ApplicationError::internal())?,
            kind: row.get(2).map_err(|_| ApplicationError::internal())?,
        };
        categories.insert(id, detail);
    }
    Ok(categories)
}

/// Adds exact signed minor units while rejecting an impossible aggregate overflow.
fn checked_add(left: i64, right: i64) -> Result<i64, ApplicationError> {
    left.checked_add(right).ok_or_else(ApplicationError::validation)
}

/// Subtracts exact signed minor units while rejecting an impossible aggregate overflow.
fn checked_sub(left: i64, right: i64) -> Result<i64, ApplicationError> {
    left.checked_sub(right).ok_or_else(ApplicationError::validation)
}

#[cfg(test)]
mod tests {
    use super::CalculationRepository;
    use crate::application::use_cases::AdjustTransactionRequest;
    use crate::domain::{ProfileId, ProfileScope};
    use crate::infrastructure::budget_allocation_repository::BudgetAllocationRepository;
    use crate::infrastructure::database::Database;
    use crate::infrastructure::finance_repository::FinanceRepository;
    use crate::infrastructure::test_support::TemporaryDatabase;
    use crate::infrastructure::transfer_repository::TransferRepository;
    use crate::ports::security::KeyDerivationPort;
    use crate::security::Argon2idKeyDeriver;

    /// Creates one encrypted profile-scoped fixture database for calculation tests.
    fn fixture() -> (TemporaryDatabase, Database, ProfileScope) {
        let fixture = TemporaryDatabase::new();
        let metadata = Argon2idKeyDeriver.create_metadata().expect("test metadata is generated");
        let key = Argon2idKeyDeriver
            .derive_database_key(b"calculation repository test", &metadata)
            .expect("test key derives");
        let database = Database::create(fixture.path(), &key).expect("database opens");
        let scope = ProfileScope::new(ProfileId::parse("profile-a").expect("scope is valid"));
        database
            .in_transaction(|transaction| {
                transaction
                    .execute(
                        "INSERT INTO profile_scopes (id, created_at, updated_at) VALUES (?1, ?2, ?2)",
                        (scope.profile_id().as_str(), "2026-11-24T00:00:00Z"),
                    )
                    .map_err(|_| crate::application::ApplicationError::internal())?;
                Ok(())
            })
            .expect("profile scope is seeded");
        (fixture, database, scope)
    }

    /// Proves all immutable facts reconcile balances while transfers do not alter income or expense totals.
    #[test]
    fn calculates_balances_and_totals_with_reversals_transfers_and_archived_accounts() {
        let (_fixture, database, scope) = fixture();
        let budget = FinanceRepository::create_budget(&database, &scope, "Home", "USD")
            .expect("budget is created");
        let income_category =
            FinanceRepository::create_category(&database, &scope, "Salary", "income", None, 1)
                .expect("income category is created");
        let expense_category =
            FinanceRepository::create_category(&database, &scope, "Food", "expense", None, 2)
                .expect("expense category is created");
        let checking = FinanceRepository::create_account(
            &database, &scope, "Checking", "checking", "USD", 1_000,
        )
        .expect("checking is created");
        let savings =
            FinanceRepository::create_account(&database, &scope, "Savings", "savings", "USD", 10)
                .expect("savings is created");
        let income = FinanceRepository::create_transaction(
            &database,
            &scope,
            &checking.id,
            None,
            Some(&income_category.id),
            "income",
            2_000,
            "USD",
            "2024-02-29",
            None,
            None,
            None,
            &[],
        )
        .expect("leap-day income is created");
        let reversed_expense = FinanceRepository::create_transaction(
            &database,
            &scope,
            &checking.id,
            Some(&budget.id),
            Some(&expense_category.id),
            "expense",
            -400,
            "USD",
            "2024-02-29",
            None,
            None,
            None,
            &[],
        )
        .expect("expense is created");
        FinanceRepository::reverse_transaction(&database, &scope, &reversed_expense.id)
            .expect("expense is reversed");
        FinanceRepository::create_transaction(
            &database,
            &scope,
            &checking.id,
            Some(&budget.id),
            Some(&expense_category.id),
            "expense",
            -250,
            "USD",
            "2024-03-01",
            None,
            None,
            None,
            &[],
        )
        .expect("unreversed expense is created");
        TransferRepository::create(
            &database,
            &scope,
            &checking.id,
            &savings.id,
            300,
            "USD",
            "2024-03-01",
        )
        .expect("transfer is created");
        FinanceRepository::archive_account(&database, &scope, &savings.id)
            .expect("savings is archived");

        let summary =
            CalculationRepository::finance_summary(&database, &scope).expect("summary calculates");
        let checking_balance = summary
            .account_balances
            .iter()
            .find(|account| account.account_id == checking.id)
            .expect("checking balance is present");
        let savings_balance = summary
            .account_balances
            .iter()
            .find(|account| account.account_id == savings.id)
            .expect("archived savings balance is present");
        let totals = summary
            .currency_totals
            .iter()
            .find(|total| total.currency == "USD")
            .expect("USD totals are present");

        assert_eq!(income.minor_units, "2000");
        assert_eq!(checking_balance.balance_minor_units, "2450");
        assert_eq!(savings_balance.balance_minor_units, "310");
        assert!(savings_balance.is_archived);
        assert_eq!(totals.income_minor_units, "2000");
        assert_eq!(totals.expense_minor_units, "-250");
    }

    /// Proves exact period plans and categorized activity share the same signed remaining calculation.
    #[test]
    fn calculates_budget_actual_and_remaining_for_an_exact_period() {
        let (_fixture, database, scope) = fixture();
        let budget = FinanceRepository::create_budget(&database, &scope, "Home", "USD")
            .expect("budget is created");
        let category =
            FinanceRepository::create_category(&database, &scope, "Food", "expense", None, 1)
                .expect("expense category is created");
        let account =
            FinanceRepository::create_account(&database, &scope, "Checking", "checking", "USD", 0)
                .expect("account is created");
        BudgetAllocationRepository::upsert(
            &database,
            &scope,
            &budget.id,
            &category.id,
            "2024-02-01",
            "2024-02-29",
            -1_000,
            None,
        )
        .expect("plan is created");
        FinanceRepository::create_transaction(
            &database,
            &scope,
            &account.id,
            Some(&budget.id),
            Some(&category.id),
            "expense",
            -250,
            "USD",
            "2024-02-29",
            None,
            None,
            None,
            &[],
        )
        .expect("leap-day expense is created");

        let summary = CalculationRepository::budget_summary(
            &database,
            &scope,
            &budget.id,
            "2024-02-01",
            "2024-02-29",
        )
        .expect("budget summary calculates");

        assert_eq!(summary.planned_minor_units, "-1000");
        assert_eq!(summary.actual_minor_units, "-250");
        assert_eq!(summary.remaining_minor_units, "-750");
        assert_eq!(summary.categories[0].actual_minor_units, "-250");
        assert_eq!(summary.categories[0].remaining_minor_units, "-750");
    }

    /// Proves an adjustment delta reconciles both balances and category budget actuals.
    #[test]
    fn calculates_adjusted_balances_and_budget_category_actuals() {
        let (_fixture, database, scope) = fixture();
        let budget = FinanceRepository::create_budget(&database, &scope, "Home", "USD")
            .expect("budget is created");
        let groceries =
            FinanceRepository::create_category(&database, &scope, "Groceries", "expense", None, 1)
                .expect("groceries category is created");
        let dining =
            FinanceRepository::create_category(&database, &scope, "Dining", "expense", None, 2)
                .expect("dining category is created");
        let account =
            FinanceRepository::create_account(&database, &scope, "Checking", "checking", "USD", 0)
                .expect("account is created");
        let original = FinanceRepository::create_transaction(
            &database,
            &scope,
            &account.id,
            Some(&budget.id),
            Some(&groceries.id),
            "expense",
            -1_000,
            "USD",
            "2024-02-29",
            None,
            None,
            None,
            &[],
        )
        .expect("expense is created");
        FinanceRepository::adjust_transaction(
            &database,
            &scope,
            AdjustTransactionRequest {
                transaction_id: original.id,
                adjusted_minor_units: -1_200,
                category_id: Some(dining.id.clone()),
                payee: None,
                description: None,
                notes: None,
                allocations: vec![],
                reason: "Corrected category and amount".to_owned(),
            },
        )
        .expect("adjustment is created");

        let finance_summary =
            CalculationRepository::finance_summary(&database, &scope).expect("summary calculates");
        let budget_summary = CalculationRepository::budget_summary(
            &database,
            &scope,
            &budget.id,
            "2024-02-01",
            "2024-02-29",
        )
        .expect("budget summary calculates");
        let groceries_actual = budget_summary
            .categories
            .iter()
            .find(|category| category.category_id == groceries.id)
            .expect("groceries result is present");
        let dining_actual = budget_summary
            .categories
            .iter()
            .find(|category| category.category_id == dining.id)
            .expect("dining result is present");

        assert_eq!(finance_summary.account_balances[0].balance_minor_units, "-1200");
        assert_eq!(finance_summary.currency_totals[0].expense_minor_units, "-1200");
        assert_eq!(budget_summary.actual_minor_units, "-1200");
        assert_eq!(groceries_actual.actual_minor_units, "0");
        assert_eq!(dining_actual.actual_minor_units, "-1200");
    }
}
