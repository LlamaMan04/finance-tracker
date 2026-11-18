//! Application-facing contracts that coordinate domain rules and infrastructure ports.

pub mod error;
pub mod services;

pub use error::{ApplicationError, ErrorCode};
