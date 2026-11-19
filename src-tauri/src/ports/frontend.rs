//! Frontend-facing ports. Database concerns deliberately live in `database.rs`.

use crate::application::ApplicationError;

/// Gives Tauri command adapters a result type with only safe application errors.
pub type FrontendResult<T> = Result<T, ApplicationError>;

/// Maps an application error to the safe shape permitted to cross the UI boundary.
pub trait FrontendErrorPort {
    /// Removes any adapter-specific diagnostic detail before the UI receives the error.
    fn safe_error(&self, error: ApplicationError) -> ApplicationError;
}

/// Implements the current error contract, whose messages are already reviewed as safe.
#[derive(Default)]
pub struct SafeFrontendErrorMapper;

impl FrontendErrorPort for SafeFrontendErrorMapper {
    /// Returns the reviewed error shape without adding persistence or platform diagnostics.
    fn safe_error(&self, error: ApplicationError) -> ApplicationError {
        error
    }
}
