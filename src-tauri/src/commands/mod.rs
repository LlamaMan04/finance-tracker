//! Tauri-boundary types. Commands are added only when a reviewed use case exists.

use crate::application::ApplicationError;

/// Maps an application failure to the safe response shape used by Tauri commands.
pub fn safe_error(error: ApplicationError) -> ApplicationError {
    error
}
