pub mod application;
pub mod commands;
pub mod domain;
pub mod infrastructure;
pub mod ports;
pub mod security;

use crate::application::ApplicationError;
use crate::commands::finance::{
    archive_account, archive_budget, archive_category, create_account, create_budget,
    create_category, create_transaction, create_transfer, list_accounts, list_budget_allocations,
    list_budgets, list_categories, list_transactions, remove_budget_allocation,
    reverse_transaction, update_account, update_budget, update_category, upsert_budget_allocation,
};
use crate::commands::profile::{
    create_profile, get_session_status, list_profiles, lock_profile, record_session_activity,
    switch_profile, unlock_profile,
};
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
            let profile_root = application_data_path.join("profiles");

            app.manage(LocalProfileService::new(profile_root));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_session_status,
            list_profiles,
            create_profile,
            unlock_profile,
            lock_profile,
            record_session_activity,
            switch_profile,
            list_budgets,
            create_budget,
            update_budget,
            archive_budget,
            list_budget_allocations,
            upsert_budget_allocation,
            remove_budget_allocation,
            list_categories,
            create_category,
            update_category,
            archive_category,
            list_accounts,
            create_account,
            update_account,
            archive_account,
            create_transaction,
            create_transfer,
            list_transactions,
            reverse_transaction
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
