pub mod application;
pub mod commands;
pub mod domain;
pub mod ports;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // The bootstrap app intentionally exposes no application commands yet.
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
