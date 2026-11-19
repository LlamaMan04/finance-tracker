//! Concrete local adapters for persistence and other platform-facing infrastructure.

pub mod database;
mod migrations;

#[cfg(test)]
pub(crate) mod test_support;
