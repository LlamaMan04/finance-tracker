//! Tauri-boundary types. Commands are added only when a reviewed use case exists.

pub mod backup;
pub mod finance;
pub mod profile;

use crate::application::ApplicationError;
use crate::ports::frontend::{FrontendErrorPort, SafeFrontendErrorMapper};

/// Maps an application failure to the safe response shape used by Tauri commands.
pub fn safe_error(error: ApplicationError) -> ApplicationError {
    SafeFrontendErrorMapper.safe_error(error)
}
