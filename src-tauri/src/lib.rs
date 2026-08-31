pub mod commands;
pub mod engines;
pub mod error;
pub mod formats;
pub mod progress;

use commands::{
    cancel_conversion, check_dependencies, check_job_capabilities, collect_input_paths,
    compute_file_hashes, consume_finder_quick_action_request, create_checksum_manifest,
    delete_transcription_model, download_transcription_model, export_inspection_report,
    get_audio_tracks, get_file_info, get_finder_quick_action_status, get_opened_inputs,
    get_subtitle_tracks, get_technical_metadata, get_transcription_models,
    install_finder_quick_action, read_file_thumbnail, remove_finder_quick_action, reveal_in_finder,
    reveal_paths_in_finder, run_job, save_clipboard_image, undo_rename, verify_checksum_manifest,
    FinderQuickActionRequest, JobRequest,
};
use log::warn;
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;
#[cfg(debug_assertions)]
use tauri::Listener;
use tauri::{Emitter, Manager, RunEvent};
use tokio_util::sync::CancellationToken;

/// Tracks active conversion jobs so they can be cancelled.
pub struct ActiveJobs(pub Mutex<HashMap<String, CancellationToken>>);

const MAX_OPENED_PATHS: usize = 100;

/// Stores Finder "Open With" paths until the frontend explicitly announces that its
/// event listener is ready. This avoids losing launch-time events before React mounts.
#[derive(Default)]
pub struct OpenedFileState {
    frontend_ready: bool,
    pending_paths: Vec<PathBuf>,
    pending_quick_actions: Vec<FinderQuickActionRequest>,
}

impl OpenedFileState {
    pub(crate) fn enqueue_paths(&mut self, paths: impl IntoIterator<Item = PathBuf>) {
        for path in paths {
            if self.pending_paths.len() == MAX_OPENED_PATHS {
                break;
            }
            if !self.pending_paths.contains(&path) {
                self.pending_paths.push(path);
            }
        }
    }

    pub(crate) fn enqueue_quick_actions(
        &mut self,
        requests: impl IntoIterator<Item = FinderQuickActionRequest>,
    ) {
        for request in requests {
            if self.pending_quick_actions.len() == MAX_OPENED_PATHS {
                break;
            }
            if !self.pending_quick_actions.contains(&request) {
                self.pending_quick_actions.push(request);
            }
        }
    }

    pub(crate) fn frontend_ready(&self) -> bool {
        self.frontend_ready
    }

    pub(crate) fn mark_frontend_ready(&mut self) -> (Vec<PathBuf>, Vec<FinderQuickActionRequest>) {
        self.frontend_ready = true;
        (
            std::mem::take(&mut self.pending_paths),
            std::mem::take(&mut self.pending_quick_actions),
        )
    }
}

pub struct OpenedFiles(pub Mutex<OpenedFileState>);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenedInputs {
    pub paths: Vec<String>,
    pub quick_actions: Vec<FinderQuickActionRequest>,
}

#[cfg(debug_assertions)]
const DEBUG_SMOKE_REQUEST_ENV: &str = "CONVERTKIT_DEBUG_SMOKE_REQUEST";
#[cfg(debug_assertions)]
const MAX_DEBUG_SMOKE_REQUEST_BYTES: u64 = 1024 * 1024;

#[cfg(debug_assertions)]
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct DebugSmokeRequest {
    #[serde(default)]
    request: Option<JobRequest>,
    #[serde(default)]
    inspect: Option<DebugInspectSmoke>,
    #[serde(default)]
    checksum_manifest: Option<DebugChecksumManifestSmoke>,
    #[serde(default)]
    undo_rename_manifest_path: Option<String>,
    result_path: PathBuf,
    cancel_after_ms: Option<u64>,
    cancel_at_progress: Option<i32>,
    cancel_job_id: Option<String>,
}

#[cfg(debug_assertions)]
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct DebugInspectSmoke {
    input_path: String,
    job_id: Option<String>,
    report_path: Option<String>,
    report_format: Option<String>,
}

#[cfg(debug_assertions)]
#[derive(serde::Deserialize)]
#[serde(
    tag = "action",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
enum DebugChecksumManifestSmoke {
    Create {
        inputs: Vec<commands::ChecksumManifestInput>,
        job_id: Option<String>,
    },
    Verify {
        manifest_path: String,
        job_id: Option<String>,
    },
}

#[cfg(debug_assertions)]
fn start_debug_smoke(app: &tauri::App) -> Result<bool, Box<dyn std::error::Error>> {
    let Some(request_path) = std::env::var_os(DEBUG_SMOKE_REQUEST_ENV) else {
        return Ok(false);
    };
    let request_path = PathBuf::from(request_path);
    let metadata = std::fs::symlink_metadata(&request_path)?;
    if !metadata.file_type().is_file() || metadata.len() > MAX_DEBUG_SMOKE_REQUEST_BYTES {
        return Err("the debug smoke request must be a regular file no larger than 1 MiB".into());
    }
    let bytes = std::fs::read(&request_path)?;
    let smoke: DebugSmokeRequest = serde_json::from_slice(&bytes)?;
    let action_count = usize::from(smoke.request.is_some())
        + usize::from(smoke.inspect.is_some())
        + usize::from(smoke.checksum_manifest.is_some())
        + usize::from(smoke.undo_rename_manifest_path.is_some());
    if action_count != 1 {
        return Err("a debug smoke request must contain exactly one action".into());
    }
    if smoke
        .cancel_at_progress
        .is_some_and(|percent| !(0..=100).contains(&percent))
        || (smoke.cancel_at_progress.is_some() && smoke.cancel_job_id.is_none())
    {
        return Err("progress-triggered cancellation requires a job id and 0-100 percent".into());
    }
    let _ = std::fs::remove_file(request_path);
    let handle = app.handle().clone();

    tauri::async_runtime::spawn(async move {
        let DebugSmokeRequest {
            request,
            inspect,
            checksum_manifest,
            undo_rename_manifest_path,
            result_path,
            cancel_after_ms,
            cancel_at_progress,
            cancel_job_id,
        } = smoke;
        let progress_listeners = cancel_at_progress.map(|threshold| {
            let cancel_job_id = cancel_job_id
                .clone()
                .expect("validated progress cancellation job id");
            let listen_for_progress = |event_name: &'static str| {
                let cancel_handle = handle.clone();
                let cancel_job_id = cancel_job_id.clone();
                handle.listen(event_name, move |event| {
                    let Ok(payload) = serde_json::from_str::<serde_json::Value>(event.payload())
                    else {
                        return;
                    };
                    let Some(job_id) = payload.get("jobId").and_then(|value| value.as_str()) else {
                        return;
                    };
                    let Some(percent) = payload.get("percent").and_then(|value| value.as_u64())
                    else {
                        return;
                    };
                    if job_id != cancel_job_id || percent < threshold as u64 {
                        return;
                    }
                    let token = cancel_handle
                        .state::<ActiveJobs>()
                        .0
                        .lock()
                        .expect("ActiveJobs lock poisoned")
                        .get(job_id)
                        .cloned();
                    if let Some(token) = token {
                        token.cancel();
                    }
                })
            };
            (
                listen_for_progress("conversion-progress"),
                listen_for_progress("inspection-progress"),
            )
        });

        if let Some(delay) = cancel_after_ms {
            let cancel_handle = handle.clone();
            let cancel_job_id = cancel_job_id.clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(Duration::from_millis(delay)).await;
                let _ = cancel_conversion(cancel_handle, cancel_job_id).await;
            });
        }

        let serialized = if let Some(request) = request {
            debug_smoke_bytes(&run_job(handle.clone(), request).await)
        } else if let Some(inspect) = inspect {
            debug_smoke_bytes(&run_debug_inspect(handle.clone(), inspect).await)
        } else if let Some(checksum_manifest) = checksum_manifest {
            match checksum_manifest {
                DebugChecksumManifestSmoke::Create { inputs, job_id } => debug_smoke_bytes(
                    &create_checksum_manifest(handle.clone(), inputs, job_id).await,
                ),
                DebugChecksumManifestSmoke::Verify {
                    manifest_path,
                    job_id,
                } => debug_smoke_bytes(
                    &verify_checksum_manifest(handle.clone(), manifest_path, job_id).await,
                ),
            }
        } else {
            debug_smoke_bytes(&undo_rename(
                handle.clone(),
                undo_rename_manifest_path.expect("validated undo action"),
            ))
        };
        if let Some((conversion_listener, inspection_listener)) = progress_listeners {
            handle.unlisten(conversion_listener);
            handle.unlisten(inspection_listener);
        }
        let _ = std::fs::write(&result_path, serialized);
        handle.exit(0);
    });
    Ok(true)
}

#[cfg(debug_assertions)]
fn debug_smoke_bytes(value: &impl Serialize) -> Vec<u8> {
    serde_json::to_vec_pretty(value)
        .unwrap_or_else(|_| br#"{"Err":{"kind":"SmokeSerialization","detail":{}}}"#.to_vec())
}

#[cfg(debug_assertions)]
async fn run_debug_inspect(
    app: tauri::AppHandle,
    request: DebugInspectSmoke,
) -> Result<serde_json::Value, String> {
    let file = get_file_info(request.input_path.clone()).await?;
    let technical_metadata = get_technical_metadata(request.input_path.clone()).await?;
    let checksums = compute_file_hashes(app, request.input_path, request.job_id)
        .await
        .map_err(|error| error.to_string())?;
    let report_path = match request.report_path {
        Some(path) => {
            let entry = serde_json::from_value(serde_json::json!({
                "file": &file,
                "technicalMetadata": &technical_metadata,
                "checksums": &checksums,
            }))
            .map_err(|error| format!("Could not prepare inspection report data: {error}"))?;
            Some(export_inspection_report(
                path,
                request.report_format.unwrap_or_else(|| "json".into()),
                vec![entry],
            )?)
        }
        None => None,
    };
    Ok(serde_json::json!({
        "file": file,
        "technicalMetadata": technical_metadata,
        "checksums": checksums,
        "reportPath": report_path,
    }))
}

/// Ensure common tool directories are on PATH so bundled .app can find
/// Homebrew/Cargo binaries that aren't on the default macOS PATH.
fn ensure_path() {
    let extra = ["/opt/homebrew/bin", "/opt/homebrew/sbin", "/usr/local/bin"];

    let mut path = std::env::var("PATH").unwrap_or_default();

    if let Some(home) = std::env::var_os("HOME") {
        let cargo_bin = std::path::PathBuf::from(&home).join(".cargo/bin");
        let mut dirs: Vec<String> = extra.iter().map(|s| s.to_string()).collect();
        dirs.push(cargo_bin.to_string_lossy().to_string());

        for dir in dirs {
            if !path.split(':').any(|p| p == dir) {
                path = format!("{}:{}", dir, path);
            }
        }
    }

    std::env::set_var("PATH", &path);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    ensure_path();
    commands::cleanup_managed_clipboard_files(Duration::from_secs(24 * 60 * 60));
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
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
        .manage(OpenedFiles(Mutex::new(OpenedFileState::default())))
        .setup(|app| {
            #[cfg(debug_assertions)]
            if start_debug_smoke(app)? {
                return Ok(());
            }

            #[cfg(target_os = "macos")]
            app.handle()
                .set_activation_policy(tauri::ActivationPolicy::Regular)?;

            if let Some(window) = app.get_webview_window("main") {
                window.show()?;
                window.set_focus()?;
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            cancel_conversion,
            check_dependencies,
            check_job_capabilities,
            collect_input_paths,
            compute_file_hashes,
            create_checksum_manifest,
            export_inspection_report,
            get_audio_tracks,
            get_file_info,
            get_finder_quick_action_status,
            get_subtitle_tracks,
            get_transcription_models,
            download_transcription_model,
            delete_transcription_model,
            get_technical_metadata,
            get_opened_inputs,
            install_finder_quick_action,
            read_file_thumbnail,
            remove_finder_quick_action,
            reveal_in_finder,
            reveal_paths_in_finder,
            save_clipboard_image,
            undo_rename,
            verify_checksum_manifest,
            run_job,
        ])
        .build(tauri::generate_context!())
        .expect("error while building ConvertKit");

    commands::cleanup_managed_rename_manifests(
        app.handle(),
        Duration::from_secs(30 * 24 * 60 * 60),
    );

    app.run(|app_handle, event| match event {
        RunEvent::Opened { urls } => {
            let mut paths = Vec::new();
            let mut quick_actions = Vec::new();
            for path in urls
                .iter()
                .filter_map(|url| url.to_file_path().ok())
                .take(MAX_OPENED_PATHS)
            {
                match consume_finder_quick_action_request(&path) {
                    Ok(Some(request)) => quick_actions.push(request),
                    Ok(None) => paths.push(path),
                    Err(error) => warn!("Ignored invalid Finder Quick Action request: {error}"),
                }
            }

            if !paths.is_empty() || !quick_actions.is_empty() {
                let state = app_handle.try_state::<OpenedFiles>();
                let frontend_ready = state.as_ref().is_some_and(|state| {
                    state
                        .0
                        .lock()
                        .expect("OpenedFiles lock poisoned")
                        .frontend_ready()
                });
                let payload = paths
                    .iter()
                    .map(|path| path.to_string_lossy().into_owned())
                    .collect::<Vec<_>>();
                let paths_delivered = paths.is_empty()
                    || (frontend_ready && app_handle.emit("files-opened", payload).is_ok());
                let actions_delivered = quick_actions.is_empty()
                    || (frontend_ready
                        && app_handle
                            .emit("finder-quick-actions-opened", quick_actions.clone())
                            .is_ok());

                if !paths_delivered || !actions_delivered {
                    if let Some(state) = state {
                        let mut state = state.0.lock().expect("OpenedFiles lock poisoned");
                        if !paths_delivered {
                            state.enqueue_paths(paths);
                        }
                        if !actions_delivered {
                            state.enqueue_quick_actions(quick_actions);
                        }
                    }
                }
            }
        }
        RunEvent::Exit => commands::cleanup_managed_clipboard_files(Duration::ZERO),
        _ => {}
    });
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opened_file_state_deduplicates_bounds_and_drains_in_order() {
        let mut state = OpenedFileState::default();
        let paths = (0..=MAX_OPENED_PATHS)
            .map(|index| PathBuf::from(format!("/tmp/open-{index}.txt")))
            .collect::<Vec<_>>();
        state.enqueue_paths(paths.iter().cloned());
        state.enqueue_paths([paths[0].clone()]);
        let request = FinderQuickActionRequest {
            recipe_id: "recipe-1".into(),
            paths: vec!["/tmp/input.png".into()],
        };
        state.enqueue_quick_actions([request.clone(), request.clone()]);

        assert!(!state.frontend_ready());
        let (opened, actions) = state.mark_frontend_ready();
        assert!(state.frontend_ready());
        assert_eq!(opened.len(), MAX_OPENED_PATHS);
        assert_eq!(opened[0], paths[0]);
        assert_eq!(opened[MAX_OPENED_PATHS - 1], paths[MAX_OPENED_PATHS - 1]);
        assert_eq!(actions, [request]);
        let (opened, actions) = state.mark_frontend_ready();
        assert!(opened.is_empty());
        assert!(actions.is_empty());
    }
}
