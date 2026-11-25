//! Validated Tauri commands for encrypted local backup operations.

use crate::application::ApplicationError;
use crate::application::contracts::BackupDto;
use crate::application::use_cases::RestorePolicy;
use crate::commands::safe_error;
use crate::domain::BackupId;
use crate::security::LocalProfileService;
use serde::Deserialize;
use tauri::State;

/// Exports the active protected profile without returning package bytes or filesystem paths.
#[tauri::command]
pub fn export_backup(
    service: State<'_, LocalProfileService>,
) -> Result<BackupDto, ApplicationError> {
    service.export_backup().map_err(safe_error)
}

/// Accepts an application-owned backup identifier and an explicit destructive restore choice.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreBackupInput {
    pub backup_id: String,
    pub policy: String,
}

/// Restores a fully authenticated package without accepting paths or package bytes from the UI.
#[tauri::command]
pub fn restore_backup(
    request: RestoreBackupInput,
    service: State<'_, LocalProfileService>,
) -> Result<(), ApplicationError> {
    let backup_id = BackupId::parse(&request.backup_id).map_err(safe_error)?;
    let policy = match request.policy.as_str() {
        "replace" => RestorePolicy::Replace,
        "merge" => RestorePolicy::Merge,
        _ => return Err(safe_error(ApplicationError::validation())),
    };
    service.restore_backup(backup_id, policy).map_err(safe_error)
}
