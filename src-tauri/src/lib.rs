pub mod application;
pub mod commands;
pub mod domain;
pub mod infrastructure;
pub mod ports;
pub mod security;

use crate::application::ApplicationError;
use crate::commands::profile::{create_profile, get_session_status, lock_profile, unlock_profile};
use crate::security::LocalProfileService;
use std::error::Error;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Startup retains only approved paths; the profile service does not open protected data.
    tauri::Builder::default()
        .setup(|app| {
            let application_data_path = app
                .path()
                .app_data_dir()
                .map_err(|_| Box::<dyn Error>::from(ApplicationError::internal()))?;
            let database_path = application_data_path.join("finance-tracker.sqlite3");
            let metadata_path = application_data_path.join("finance-tracker.profile.json");

            app.manage(LocalProfileService::new(database_path, metadata_path));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_session_status,
            create_profile,
            unlock_profile,
            lock_profile
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
