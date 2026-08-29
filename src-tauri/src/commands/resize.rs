use std::path::{Path, PathBuf};
use std::time::Instant;

use tauri::{AppHandle, Emitter, Manager};
use tokio::io::AsyncReadExt;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::engines::{self, tool_command, ConversionResult};
use crate::error::ConversionError;
use crate::formats::FileCategory;
use crate::progress::ProgressPayload;
use crate::ActiveJobs;

use super::output::{prepare_output, OutputOptions};

const MAX_DIMENSION: u32 = 32_768;

#[tauri::command]
pub async fn resize_image(
    app: AppHandle,
    input_path: String,
    width: u32,
    height: u32,
    preserve_aspect: bool,
    job_id: Option<String>,
    output_options: Option<OutputOptions>,
) -> Result<ConversionResult, ConversionError> {
    let input = PathBuf::from(&input_path);
    if !input.is_file() {
        return Err(ConversionError::InputNotFound { path: input_path });
    }
    validate_dimensions(width, height)?;

    let format = crate::formats::Format::from_extension(
        input
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or_default(),
    )
    .ok_or_else(|| ConversionError::UnsupportedConversion {
        input: input_path.clone(),
        output: "image".into(),
    })?;
    if format.category() != FileCategory::Image {
        return Err(ConversionError::UnsupportedConversion {
            input: format.label().into(),
            output: "image resize".into(),
        });
    }

    let extension = format.extension();
    let prepared_output = prepare_output(&input, extension, "-resized", output_options)?;
    let job_id = job_id.unwrap_or_else(|| Uuid::new_v4().to_string());
    let cancel_token = CancellationToken::new();
    {
        let jobs = app.state::<ActiveJobs>();
        let mut active = jobs.0.lock().expect("ActiveJobs lock poisoned");
        if active.contains_key(&job_id) {
            return Err(ConversionError::UnsupportedConversion {
                input: "Duplicate job ID".into(),
                output: job_id,
            });
        }
        active.insert(job_id.clone(), cancel_token.clone());
    }

    let _ = app.emit(
        "conversion-progress",
        ProgressPayload {
            percent: -1,
            stage: "Resizing image".into(),
        },
    );

    let started = Instant::now();
    let result = run_resize(
        &input,
        prepared_output.working_path(),
        width,
        height,
        preserve_aspect,
        cancel_token,
    )
    .await;
    app.state::<ActiveJobs>()
        .0
        .lock()
        .expect("ActiveJobs lock poisoned")
        .remove(&job_id);

    match result {
        Ok(result) => {
            let mut result = prepared_output.commit(result)?;
            result.duration_ms = started.elapsed().as_millis() as u64;
            let _ = app.emit(
                "conversion-progress",
                ProgressPayload {
                    percent: 100,
                    stage: "Complete".into(),
                },
            );
            Ok(result)
        }
        Err(error) => Err(error),
    }
}

async fn run_resize(
    input: &Path,
    output: &Path,
    width: u32,
    height: u32,
    preserve_aspect: bool,
    cancel_token: CancellationToken,
) -> Result<ConversionResult, ConversionError> {
    let geometry = resize_geometry(width, height, preserve_aspect);
    let mut child = tool_command("magick")
        .arg(input)
        .args(["-resize", &geometry])
        .arg(output)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|error| ConversionError::ProcessFailed {
            message: format!("Failed to spawn magick: {error}"),
            stderr: String::new(),
            exit_code: None,
        })?;

    let stderr = child.stderr.take();
    let stderr_task = tokio::spawn(async move {
        let mut output = String::new();
        if let Some(mut stderr) = stderr {
            let _ = stderr.read_to_string(&mut output).await;
        }
        output
    });

    let status = tokio::select! {
        result = child.wait() => result.map_err(|error| ConversionError::ProcessFailed {
            message: format!("magick process error: {error}"),
            stderr: String::new(),
            exit_code: None,
        })?,
        _ = cancel_token.cancelled() => {
            let _ = child.kill().await;
            engines::cleanup_partial(output);
            return Err(ConversionError::Cancelled);
        }
    };
    let stderr = stderr_task.await.unwrap_or_default();

    if !status.success() {
        engines::cleanup_partial(output);
        return Err(ConversionError::ProcessFailed {
            message: "Image resize failed".into(),
            stderr,
            exit_code: status.code(),
        });
    }

    let output_size = std::fs::metadata(output)
        .map_err(|_| ConversionError::OutputMissing)?
        .len();
    Ok(ConversionResult {
        output_path: output.to_string_lossy().into(),
        output_size,
        duration_ms: 0,
    })
}

fn validate_dimensions(width: u32, height: u32) -> Result<(), ConversionError> {
    if width == 0 || height == 0 || width > MAX_DIMENSION || height > MAX_DIMENSION {
        return Err(ConversionError::UnsupportedConversion {
            input: format!("Resize dimensions must be between 1 and {MAX_DIMENSION} pixels"),
            output: "image".into(),
        });
    }
    Ok(())
}

fn resize_geometry(width: u32, height: u32, preserve_aspect: bool) -> String {
    format!("{width}x{height}{}", if preserve_aspect { "" } else { "!" })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_dimension_bounds() {
        assert!(validate_dimensions(1, 1).is_ok());
        assert!(validate_dimensions(MAX_DIMENSION, MAX_DIMENSION).is_ok());
        assert!(validate_dimensions(0, 100).is_err());
        assert!(validate_dimensions(100, MAX_DIMENSION + 1).is_err());
    }

    #[test]
    fn builds_locked_and_unlocked_geometry() {
        assert_eq!(resize_geometry(800, 600, true), "800x600");
        assert_eq!(resize_geometry(800, 600, false), "800x600!");
    }
}
