//! Validated Tauri commands for encrypted local backup operations.

use crate::application::ApplicationError;
use crate::application::contracts::BackupDto;
use crate::commands::safe_error;
use crate::security::LocalProfileService;
use tauri::State;

/// Exports the active protected profile without returning package bytes or filesystem paths.
#[tauri::command]
pub fn export_backup(
    service: State<'_, LocalProfileService>,
) -> Result<BackupDto, ApplicationError> {
    service.export_backup().map_err(safe_error)
}
