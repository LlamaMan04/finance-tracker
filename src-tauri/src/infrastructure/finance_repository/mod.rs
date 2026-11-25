//! Profile-scoped SQLite persistence for finance reference data and immutable ledger records.

mod corrections;
mod mappers;
mod reference_data;
mod transactions;
mod validation;

#[cfg(test)]
mod tests;

/// Provides the stable application-facing entry point for finance persistence operations.
pub struct FinanceRepository;
