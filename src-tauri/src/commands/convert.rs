use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use log::warn;
use tauri::{AppHandle, Emitter, Manager};
use uuid::Uuid;

use crate::engines::{self, ConversionRequest, ConversionResult};
use crate::error::ConversionError;
use crate::formats::{FileCategory, Format};
use crate::progress::ProgressPayload;
use crate::ActiveJobs;

/// Guard that removes a job from [`ActiveJobs`] on drop, even if a panic
/// occurs during conversion.
struct JobGuard {
    app: AppHandle,
    job_id: String,
}

impl Drop for JobGuard {
    fn drop(&mut self) {
        let state = self.app.state::<ActiveJobs>();
        let mut jobs = state.0.lock().expect("ActiveJobs lock poisoned");
        jobs.remove(&self.job_id);
    }
}

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

    // --- Bug 8: File validation ---
    let input_meta = std::fs::metadata(&input)
        .map_err(|e| ConversionError::InputNotFound { path: format!("{}: {}", input_path, e) })?;

    if input_meta.len() == 0 {
        return Err(ConversionError::UnsupportedConversion {
            input: "File is empty (0 bytes)".to_string(),
            output: output_format.clone(),
        });
    }

    if !input_meta.is_file() {
        return Err(ConversionError::UnsupportedConversion {
            input: "Path is not a regular file".to_string(),
            output: output_format.clone(),
        });
    }

    // --- Bug 20: Files with no extension ---
    let input_ext = input
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default();

    if input_ext.is_empty() {
        return Err(ConversionError::UnsupportedConversion {
            input: "File has no extension".to_string(),
            output: output_format.clone(),
        });
    }

    // --- Bug 8: Magic-byte check via infer ---
    if let Ok(Some(inferred)) = infer::get_from_path(&input) {
        let inferred_ext = inferred.extension();
        let ext_format = Format::from_extension(input_ext);
        let inferred_format = Format::from_extension(inferred_ext);
        if ext_format != inferred_format {
            warn!(
                "File extension '.{}' does not match detected type '{}' ({}); proceeding anyway",
                input_ext,
                inferred_ext,
                inferred.mime_type()
            );
        }
    }

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
    let mut output_path = dedup_output_path(&input, out_format);

    // --- Bug 12: Output directory writability check ---
    if let Some(out_dir) = output_path.parent() {
        let probe = out_dir.join(".convertkit_write_test");
        match std::fs::File::create(&probe) {
            Ok(_) => {
                let _ = std::fs::remove_file(&probe);
            }
            Err(_) => {
                // Fall back to ~/Downloads/ConvertKit/
                let fallback = dirs_downloads_fallback();
                let _ = std::fs::create_dir_all(&fallback);
                output_path = dedup_output_path_in(&input, out_format, &fallback);
                warn!(
                    "Output dir not writable, falling back to {}",
                    fallback.display()
                );
            }
        }
    }

    // --- Bug 6: Prevent double-submit ---
    let cancel_token = tokio_util::sync::CancellationToken::new();
    {
        let state = app.state::<ActiveJobs>();
        let mut jobs = state.0.lock().expect("ActiveJobs lock poisoned");
        if jobs.contains_key(&job_id) {
            return Err(ConversionError::UnsupportedConversion {
                input: "Duplicate job ID".to_string(),
                output: format!("Job '{}' is already running", job_id),
            });
        }
        jobs.insert(job_id.clone(), cancel_token.clone());
    }

    // Bug 6: Guard ensures the job is removed even on panic.
    let _guard = JobGuard {
        app: app.clone(),
        job_id: job_id.clone(),
    };

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

    // --- Bug 5: Timeout ---
    let timeout_duration = if out_format.category() == FileCategory::Video {
        Duration::from_secs(5 * 60) // 5 minutes for video
    } else {
        Duration::from_secs(2 * 60) // 2 minutes for everything else
    };

    let cancel_for_timeout = cancel_token.clone();
    let result = match tokio::time::timeout(
        timeout_duration,
        engine.convert(request, app.clone(), cancel_token),
    )
    .await
    {
        Ok(inner) => inner,
        Err(_elapsed) => {
            cancel_for_timeout.cancel();
            Err(ConversionError::Timeout {
                seconds: timeout_duration.as_secs(),
            })
        }
    };

    // Guard handles cleanup on drop, no manual remove needed.

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

/// Fallback output directory: ~/Downloads/ConvertKit/
fn dirs_downloads_fallback() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home)
        .join("Downloads")
        .join("ConvertKit")
}

/// Like [`dedup_output_path`] but places the output in a specific directory.
fn dedup_output_path_in(input: &Path, format: Format, dir: &Path) -> PathBuf {
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

    base
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
