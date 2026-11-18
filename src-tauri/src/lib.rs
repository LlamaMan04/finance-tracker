pub mod application;
pub mod commands;
pub mod domain;
pub mod infrastructure;
pub mod ports;

use crate::application::ApplicationError;
use crate::infrastructure::database::Database;
use std::error::Error;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // The bootstrap app owns the local database; no database APIs are exposed to the frontend.
    tauri::Builder::default()
        .setup(|app| {
            let database_path = app
                .path()
                .app_data_dir()
                .map_err(|_| Box::<dyn Error>::from(ApplicationError::internal()))?
                .join("finance-tracker.sqlite3");
            let database = Database::open(&database_path)
                .map_err(|_| Box::<dyn Error>::from(ApplicationError::internal()))?;

            app.manage(database);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
