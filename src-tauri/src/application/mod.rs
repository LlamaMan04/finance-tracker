//! Application-facing contracts that coordinate domain rules and infrastructure ports.

pub mod contracts;
pub mod error;
pub mod use_cases;

pub use error::{ApplicationError, ErrorCode};
