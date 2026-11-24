//! Concrete local adapters for persistence and other platform-facing infrastructure.

pub mod budget_allocation_repository;
pub mod calculation_repository;
pub mod database;
pub mod finance_repository;
mod migrations;
pub mod profile_metadata;
pub mod profile_registry;
pub mod transfer_repository;

#[cfg(test)]
pub(crate) mod test_support;
