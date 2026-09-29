//! Tauri adapter for Imago.
//!
//! This crate only translates between the webview (IPC commands and events)
//! and `imago-core`. It contains no image-processing logic.

mod commands;
mod errors;
mod logging;
mod state;

use imago_core::config::{self, CONFIG_FILE};

use crate::state::AppState;

/// Builds and runs the Tauri application.
pub fn run() {
    let log_dir = config::default_log_dir();
    let _log_guard = logging::init(log_dir.as_deref());
    logging::install_panic_hook();

    let config_path = config::default_config_dir().map(|dir| dir.join(CONFIG_FILE));
    let (config, warning) = match &config_path {
        Some(path) => config::load_or_default(path),
        None => (Default::default(), None),
    };
    if let Some(warning) = &warning {
        tracing::warn!(%warning, "configuration could not be loaded; defaults are used");
    }
    tracing::info!(version = imago_core::VERSION, "Imago starting");
    let state = AppState::new(config_path, config, warning.map(|w| w.to_string()), log_dir);

    let result = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::load_config,
            commands::save_config,
            commands::pick_files,
            commands::pick_folder,
            commands::scan_paths,
            commands::start_job,
            commands::cancel_job,
            commands::skip_file,
            commands::render_image,
            commands::reveal_path,
            commands::open_folder,
            commands::export_file,
            commands::create_zip_archive,
        ])
        .run(tauri::generate_context!());

    if let Err(error) = result {
        tracing::error!(%error, "Imago failed to start");
        // Nothing else can report this: the window never opened.
        eprintln!("Imago failed to start: {error}");
        std::process::exit(1);
    }
}
