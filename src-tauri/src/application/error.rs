use serde::Serialize;
use std::fmt;

/// Identifies a safe, machine-readable category of application failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    Validation,
    NotFound,
    Conflict,
    Locked,
    Unauthorized,
    Backup,
    Internal,
}

/// Carries only a code and safe message across application boundaries.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ApplicationError {
    pub code: ErrorCode,
    pub message: String,
}

impl ApplicationError {
    /// Creates a validation error without echoing the supplied sensitive value.
    pub fn validation() -> Self {
        Self::new(ErrorCode::Validation, "The supplied value is invalid.")
    }

    /// Creates a not-found error without revealing whether protected data exists.
    pub fn not_found() -> Self {
        Self::new(ErrorCode::NotFound, "The requested record was not found.")
    }

    /// Creates a conflict error for an operation that cannot safely proceed.
    pub fn conflict() -> Self {
        Self::new(ErrorCode::Conflict, "The operation conflicts with the current state.")
    }

    /// Creates the response used when a protected operation is attempted while locked.
    pub fn locked() -> Self {
        Self::new(ErrorCode::Locked, "Unlock the application to continue.")
    }

    /// Creates a generic authorization failure without exposing authorization details.
    pub fn unauthorized() -> Self {
        Self::new(ErrorCode::Unauthorized, "You are not authorized to perform this operation.")
    }

    /// Creates a generic backup failure without exposing backup contents or key material.
    pub fn backup() -> Self {
        Self::new(ErrorCode::Backup, "The backup operation could not be completed.")
    }

    /// Creates the generic response for unexpected failures.
    pub fn internal() -> Self {
        Self::new(ErrorCode::Internal, "The operation could not be completed.")
    }

    /// Builds an error from a deliberately reviewed, non-sensitive message.
    fn new(code: ErrorCode, message: &str) -> Self {
        Self { code, message: message.to_owned() }
    }
}

impl fmt::Display for ApplicationError {
    /// Formats the same safe text that is eligible to cross an application boundary.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ApplicationError {}

#[cfg(test)]
mod tests {
    use super::{ApplicationError, ErrorCode};

    #[test]
    fn serializes_machine_readable_error_codes_with_safe_messages() {
        let serialized = serde_json::to_value(ApplicationError::locked())
            .expect("application errors must serialize for commands");

        assert_eq!(serialized["code"], "locked");
        assert_eq!(serialized["message"], "Unlock the application to continue.");
    }

    #[test]
    fn internal_error_does_not_expose_diagnostic_details() {
        let error = ApplicationError::internal();

        assert_eq!(error.code, ErrorCode::Internal);
        assert_eq!(error.message, "The operation could not be completed.");
    }
}
