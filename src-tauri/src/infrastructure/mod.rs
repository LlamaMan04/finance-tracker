//! Concrete local adapters for persistence and other platform-facing infrastructure.

pub mod database;
mod migrations;
pub mod profile_metadata;
pub mod profile_registry;

#[cfg(test)]
pub(crate) mod test_support;
