use tauri::{AppHandle, Manager};

use crate::ActiveJobs;

/// Cancel a running conversion job.
///
/// If `job_id` is provided, only that job is cancelled.
/// If `job_id` is empty or omitted, **all** active jobs are cancelled.
#[tauri::command]
pub async fn cancel_conversion(app: AppHandle, job_id: Option<String>) -> Result<(), String> {
    let state = app.state::<ActiveJobs>();
    let mut jobs = state.0.lock().map_err(|e| e.to_string())?;

    match job_id {
        Some(id) if !id.is_empty() => {
            if let Some(token) = jobs.remove(&id) {
                token.cancel();
                log::info!("Cancelled job {}", id);
            }
        }
        _ => {
            let count = jobs.len();
            for (_, token) in jobs.drain() {
                token.cancel();
            }
            log::info!("Cancelled all {} active jobs", count);
        }
    }

    Ok(())
}
