pub mod application;
pub mod commands;
pub mod domain;
pub mod infrastructure;
pub mod ports;
pub mod security;

use crate::application::ApplicationError;
use crate::infrastructure::database::ProtectedDatabaseLocation;
use std::error::Error;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Startup retains only the approved path; Phase 2 authentication must unlock the database.
    tauri::Builder::default()
        .setup(|app| {
            let database_path = app
                .path()
                .app_data_dir()
                .map_err(|_| Box::<dyn Error>::from(ApplicationError::internal()))?
                .join("finance-tracker.sqlite3");

            app.manage(ProtectedDatabaseLocation::new(database_path));
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
