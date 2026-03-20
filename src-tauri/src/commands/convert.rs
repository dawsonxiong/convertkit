use std::path::{Path, PathBuf};
use std::time::Instant;

use tauri::{AppHandle, Emitter, Manager};
use uuid::Uuid;

use crate::engines::{self, ConversionRequest, ConversionResult};
use crate::error::ConversionError;
use crate::formats::Format;
use crate::progress::ProgressPayload;
use crate::ActiveJobs;

/// Main conversion command exposed to the frontend.
///
/// Spawns the appropriate engine, emits progress events, and returns the
/// result on success.
#[tauri::command]
pub async fn convert(
    app: AppHandle,
    input_path: String,
    output_format: String,
    job_id: Option<String>,
) -> Result<ConversionResult, ConversionError> {
    let started = Instant::now();
    let job_id = job_id.unwrap_or_else(|| Uuid::new_v4().to_string());

    // --- Parse formats ---
    let input = PathBuf::from(&input_path);
    if !input.exists() {
        return Err(ConversionError::InputNotFound { path: input_path });
    }

    let input_ext = input
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default();

    let input_format = Format::from_extension(input_ext).ok_or_else(|| {
        ConversionError::UnsupportedConversion {
            input: input_ext.to_string(),
            output: output_format.clone(),
        }
    })?;

    let out_format = Format::from_extension(&output_format).ok_or_else(|| {
        ConversionError::UnsupportedConversion {
            input: input_ext.to_string(),
            output: output_format.clone(),
        }
    })?;

    // --- Resolve engine ---
    let engine = engines::get_engine(input_format, out_format).ok_or_else(|| {
        ConversionError::UnsupportedConversion {
            input: input_format.label().to_string(),
            output: out_format.label().to_string(),
        }
    })?;

    if !engine.is_available() {
        return Err(ConversionError::MissingDependency {
            tool: engine.required_tool().to_string(),
            install_hint: format!(
                "Install {} to enable this conversion",
                engine.required_tool()
            ),
        });
    }

    // --- Build output path (dedup) ---
    let output_path = dedup_output_path(&input, out_format);

    // --- Cancellation token ---
    let cancel_token = tokio_util::sync::CancellationToken::new();
    {
        let state = app.state::<ActiveJobs>();
        let mut jobs = state.0.lock().expect("ActiveJobs lock poisoned");
        jobs.insert(job_id.clone(), cancel_token.clone());
    }

    // Emit initial progress
    let _ = app.emit(
        "conversion-progress",
        ProgressPayload {
            percent: 0,
            stage: "Starting conversion".to_string(),
        },
    );

    let request = ConversionRequest {
        input_path: input,
        output_path,
        input_format,
        output_format: out_format,
    };

    let result = engine.convert(request, app.clone(), cancel_token).await;

    // --- Cleanup job ---
    {
        let state = app.state::<ActiveJobs>();
        let mut jobs = state.0.lock().expect("ActiveJobs lock poisoned");
        jobs.remove(&job_id);
    }

    match result {
        Ok(mut res) => {
            res.duration_ms = started.elapsed().as_millis() as u64;
            let _ = app.emit(
                "conversion-progress",
                ProgressPayload {
                    percent: 100,
                    stage: "Complete".to_string(),
                },
            );
            Ok(res)
        }
        Err(e) => Err(e),
    }
}

/// Build an output path in the same directory as `input`, using the target
/// format's extension. If the file already exists, append `(1)`, `(2)`, etc.
fn dedup_output_path(input: &Path, format: Format) -> PathBuf {
    let dir = input.parent().unwrap_or_else(|| Path::new("."));
    let stem = input
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output");
    let ext = format.extension();

    let base = dir.join(format!("{}.{}", stem, ext));
    if !base.exists() {
        return base;
    }

    for i in 1u32.. {
        let candidate = dir.join(format!("{} ({}).{}", stem, i, ext));
        if !candidate.exists() {
            return candidate;
        }
    }

    // Unreachable in practice, but satisfy the compiler.
    base
}
