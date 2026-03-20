pub mod commands;
pub mod engines;
pub mod error;
pub mod formats;
pub mod progress;

use commands::{cancel_conversion, check_dependencies, convert, get_file_info, read_file_thumbnail, reveal_in_finder};
use std::collections::HashMap;
use std::sync::Mutex;
use tokio_util::sync::CancellationToken;

/// Tracks active conversion jobs so they can be cancelled.
pub struct ActiveJobs(pub Mutex<HashMap<String, CancellationToken>>);

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(
            tauri_plugin_log::Builder::new()
                .target(tauri_plugin_log::Target::new(
                    tauri_plugin_log::TargetKind::Folder {
                        path: dirs_log_path(),
                        file_name: Some("ConvertKit".into()),
                    },
                ))
                .build(),
        )
        .manage(ActiveJobs(Mutex::new(HashMap::new())))
        .invoke_handler(tauri::generate_handler![
            convert,
            cancel_conversion,
            check_dependencies,
            get_file_info,
            read_file_thumbnail,
            reveal_in_finder,
        ])
        .run(tauri::generate_context!())
        .expect("error while running ConvertKit");
}

/// Returns ~/Library/Logs/ConvertKit/ on macOS, falling back to a temp dir.
fn dirs_log_path() -> std::path::PathBuf {
    if let Some(home) = std::env::var_os("HOME") {
        let p = std::path::PathBuf::from(home)
            .join("Library")
            .join("Logs")
            .join("ConvertKit");
        let _ = std::fs::create_dir_all(&p);
        p
    } else {
        std::env::temp_dir().join("ConvertKit")
    }
}
