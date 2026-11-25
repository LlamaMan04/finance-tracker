//! Profile-scoped persistence for planned budget category amounts.

use crate::application::ApplicationError;
use crate::application::contracts::BudgetAllocationDto;
use crate::domain::{AllocationId, BudgetId, CategoryId, LocalDate, PlanningPeriod, ProfileScope};
use crate::infrastructure::database::Database;
use rusqlite::{Row, params};
use uuid::Uuid;

/// Reads and writes protected period/category plans without calculating budget results in SQL.
pub struct BudgetAllocationRepository;

impl BudgetAllocationRepository {
    /// Returns plans in deterministic budget, period, category, and identifier order.
    pub fn list(
        database: &Database,
        scope: &ProfileScope,
    ) -> Result<Vec<BudgetAllocationDto>, ApplicationError> {
        database.in_transaction(|transaction| {
            let mut statement = transaction
                .prepare(
                    "SELECT id, budget_id, category_id, period_starts_on, period_ends_on, planned_minor_units, notes FROM budget_allocations WHERE profile_id = ?1 ORDER BY budget_id, period_starts_on, period_ends_on, category_id, id",
                )
                .map_err(|_| ApplicationError::internal())?;
            let rows = statement
                .query_map([scope.profile_id().as_str()], budget_allocation_from_row)
                .map_err(|_| ApplicationError::internal())?;

            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|_| ApplicationError::internal())
        })
    }

    /// Creates or replaces the one plan allowed for a budget, category, and inclusive period.
    #[allow(clippy::too_many_arguments)]
    pub fn upsert(
        database: &Database,
        scope: &ProfileScope,
        budget_id: &str,
        category_id: &str,
        period_starts_on: &str,
        period_ends_on: &str,
        planned_minor_units: i64,
        notes: Option<&str>,
    ) -> Result<BudgetAllocationDto, ApplicationError> {
        let budget_id = BudgetId::parse(budget_id)?;
        let category_id = CategoryId::parse(category_id)?;
        PlanningPeriod::new(
            LocalDate::parse(period_starts_on)?,
            LocalDate::parse(period_ends_on)?,
        )?;
        let notes = validate_optional_notes(notes)?;
        validate_planned_amount(planned_minor_units)?;
        let id = new_id();

        database.in_transaction(|transaction| {
            let category_kind = validate_active_references(transaction, scope, &budget_id, &category_id)?;
            validate_planned_amount_kind(&category_kind, planned_minor_units)?;
            transaction
                .execute(
                    "INSERT INTO budget_allocations (id, profile_id, budget_id, category_id, period_starts_on, period_ends_on, planned_minor_units, notes, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), strftime('%Y-%m-%dT%H:%M:%fZ', 'now')) ON CONFLICT(profile_id, budget_id, category_id, period_starts_on, period_ends_on) DO UPDATE SET planned_minor_units = excluded.planned_minor_units, notes = excluded.notes, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')",
                    params![id, scope.profile_id().as_str(), budget_id.as_str(), category_id.as_str(), period_starts_on, period_ends_on, planned_minor_units, notes],
                )
                .map_err(|_| ApplicationError::conflict())?;
            transaction
                .query_row(
                    "SELECT id, budget_id, category_id, period_starts_on, period_ends_on, planned_minor_units, notes FROM budget_allocations WHERE profile_id = ?1 AND budget_id = ?2 AND category_id = ?3 AND period_starts_on = ?4 AND period_ends_on = ?5",
                    params![scope.profile_id().as_str(), budget_id.as_str(), category_id.as_str(), period_starts_on, period_ends_on],
                    budget_allocation_from_row,
                )
                .map_err(|_| ApplicationError::internal())
        })
    }

    /// Removes one profile-owned planning amount without deleting any financial history.
    pub fn remove(
        database: &Database,
        scope: &ProfileScope,
        id: &str,
    ) -> Result<(), ApplicationError> {
        let id = AllocationId::parse(id)?;

        database.in_transaction(|transaction| {
            let changed = transaction
                .execute(
                    "DELETE FROM budget_allocations WHERE profile_id = ?1 AND id = ?2",
                    params![scope.profile_id().as_str(), id.as_str()],
                )
                .map_err(|_| ApplicationError::internal())?;
            if changed != 1 {
                return Err(ApplicationError::not_found());
            }

            Ok(())
        })
    }
}

/// Confirms the plan uses active records from the authenticated profile and returns its category kind.
fn validate_active_references(
    transaction: &rusqlite::Transaction<'_>,
    scope: &ProfileScope,
    budget_id: &BudgetId,
    category_id: &CategoryId,
) -> Result<String, ApplicationError> {
    let budget_is_active = transaction
        .query_row(
            "SELECT is_archived FROM budgets WHERE profile_id = ?1 AND id = ?2",
            params![scope.profile_id().as_str(), budget_id.as_str()],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|_| ApplicationError::not_found())?
        == 0;
    if !budget_is_active {
        return Err(ApplicationError::validation());
    }

    let category = transaction
        .query_row(
            "SELECT category_kind, is_archived FROM categories WHERE profile_id = ?1 AND id = ?2",
            params![scope.profile_id().as_str(), category_id.as_str()],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
        )
        .map_err(|_| ApplicationError::not_found())?;
    if category.1 != 0 {
        return Err(ApplicationError::validation());
    }

    Ok(category.0)
}

/// Rejects zero plans so each persisted record has financial meaning.
fn validate_planned_amount(planned_minor_units: i64) -> Result<(), ApplicationError> {
    if planned_minor_units == 0 {
        return Err(ApplicationError::validation());
    }

    Ok(())
}

/// Applies the ledger's sign convention to the planned value based on its category kind.
fn validate_planned_amount_kind(
    category_kind: &str,
    planned_minor_units: i64,
) -> Result<(), ApplicationError> {
    let has_matching_sign = matches!(
        (category_kind, planned_minor_units.is_positive()),
        ("income", true) | ("expense", false)
    );
    if !has_matching_sign {
        return Err(ApplicationError::validation());
    }

    Ok(())
}

/// Normalizes optional notes and enforces the schema's maximum note size.
fn validate_optional_notes(value: Option<&str>) -> Result<Option<String>, ApplicationError> {
    value
        .filter(|notes| !notes.trim().is_empty())
        .map(|notes| {
            let normalized = notes.trim();
            if normalized.len() > 1_000 {
                return Err(ApplicationError::validation());
            }

            Ok(normalized.to_owned())
        })
        .transpose()
}

/// Creates an opaque server-generated identity for a new planning record.
fn new_id() -> String {
    format!("budget-allocation-{}", Uuid::new_v4())
}

/// Maps a SQLite row to the exact-value DTO shared with the frontend.
fn budget_allocation_from_row(row: &Row<'_>) -> rusqlite::Result<BudgetAllocationDto> {
    Ok(BudgetAllocationDto {
        id: row.get(0)?,
        budget_id: row.get(1)?,
        category_id: row.get(2)?,
        period_starts_on: row.get(3)?,
        period_ends_on: row.get(4)?,
        planned_minor_units: row.get::<_, i64>(5)?.to_string(),
        notes: row.get(6)?,
    })
}

#[cfg(test)]
mod tests {
    use super::BudgetAllocationRepository;
    use crate::application::{ApplicationError, ErrorCode};
    use crate::domain::{ProfileId, ProfileScope};
    use crate::infrastructure::database::Database;
    use crate::infrastructure::finance_repository::FinanceRepository;
    use crate::infrastructure::test_support::TemporaryDatabase;
    use crate::ports::security::KeyDerivationPort;
    use crate::security::Argon2idKeyDeriver;

    /// Creates a protected database, seeds its profile scope, and returns the active test scope.
    fn fixture() -> (TemporaryDatabase, Database, ProfileScope) {
        let fixture = TemporaryDatabase::new();
        let deriver = Argon2idKeyDeriver;
        let metadata = deriver.create_metadata().expect("test metadata is generated");
        let key = deriver
            .derive_database_key(b"budget allocation repository test", &metadata)
            .expect("test key derives");
        let database = Database::create(fixture.path(), &key).expect("database opens");
        let scope = ProfileScope::new(ProfileId::parse("profile-a").expect("valid profile id"));
        seed_profile_scope(&database, &scope);

        (fixture, database, scope)
    }

    /// Inserts the profile-scoped parent required by the finance foreign keys.
    fn seed_profile_scope(database: &Database, scope: &ProfileScope) {
        database
            .in_transaction(|transaction| {
                transaction
                    .execute(
                        "INSERT INTO profile_scopes (id, created_at, updated_at) VALUES (?1, ?2, ?2)",
                        (scope.profile_id().as_str(), "2026-11-24T00:00:00Z"),
                    )
                    .map_err(|_| ApplicationError::internal())?;
                Ok(())
            })
            .expect("profile scope seeds");
    }

    /// Proves an existing budget/category/period plan is replaced and returned deterministically.
    #[test]
    fn creates_lists_replaces_and_removes_budget_allocations() {
        let (_fixture, database, scope) = fixture();
        let budget = FinanceRepository::create_budget(&database, &scope, "Home", "USD")
            .expect("budget creates");
        let category =
            FinanceRepository::create_category(&database, &scope, "Groceries", "expense", None, 1)
                .expect("category creates");
        let initial = BudgetAllocationRepository::upsert(
            &database,
            &scope,
            &budget.id,
            &category.id,
            "2026-01-01",
            "2026-01-31",
            -50_000,
            Some("initial plan"),
        )
        .expect("plan creates");
        let replacement = BudgetAllocationRepository::upsert(
            &database,
            &scope,
            &budget.id,
            &category.id,
            "2026-01-01",
            "2026-01-31",
            -55_000,
            None,
        )
        .expect("plan replaces");

        assert_eq!(initial.id, replacement.id);
        assert_eq!(replacement.planned_minor_units, "-55000");
        assert_eq!(replacement.notes, None);
        assert_eq!(
            BudgetAllocationRepository::list(&database, &scope).expect("plans list").len(),
            1
        );
        BudgetAllocationRepository::remove(&database, &scope, &replacement.id)
            .expect("plan removes");
        assert!(
            BudgetAllocationRepository::list(&database, &scope).expect("plans list").is_empty()
        );
    }

    /// Proves the planning slice rejects invalid periods, signs, archived records, and other profiles.
    #[test]
    fn rejects_invalid_or_unavailable_budget_allocation_references() {
        let (_fixture, database, scope) = fixture();
        let budget = FinanceRepository::create_budget(&database, &scope, "Home", "USD")
            .expect("budget creates");
        let expense =
            FinanceRepository::create_category(&database, &scope, "Groceries", "expense", None, 1)
                .expect("category creates");
        let backwards_period = BudgetAllocationRepository::upsert(
            &database,
            &scope,
            &budget.id,
            &expense.id,
            "2026-02-01",
            "2026-01-31",
            -1,
            None,
        )
        .expect_err("period order is validated");
        let wrong_sign = BudgetAllocationRepository::upsert(
            &database,
            &scope,
            &budget.id,
            &expense.id,
            "2026-01-01",
            "2026-01-31",
            1,
            None,
        )
        .expect_err("expense plans are negative");
        FinanceRepository::archive_category(&database, &scope, &expense.id)
            .expect("category archives");
        let archived_category = BudgetAllocationRepository::upsert(
            &database,
            &scope,
            &budget.id,
            &expense.id,
            "2026-01-01",
            "2026-01-31",
            -1,
            None,
        )
        .expect_err("archived categories cannot receive a new plan");
        let other_scope =
            ProfileScope::new(ProfileId::parse("profile-b").expect("valid profile id"));
        seed_profile_scope(&database, &other_scope);
        let other_profile = BudgetAllocationRepository::upsert(
            &database,
            &other_scope,
            &budget.id,
            &expense.id,
            "2026-01-01",
            "2026-01-31",
            -1,
            None,
        )
        .expect_err("other profile cannot use these records");

        assert_eq!(backwards_period.code, ErrorCode::Validation);
        assert_eq!(wrong_sign.code, ErrorCode::Validation);
        assert_eq!(archived_category.code, ErrorCode::Validation);
        assert_eq!(other_profile.code, ErrorCode::NotFound);
    }
}
