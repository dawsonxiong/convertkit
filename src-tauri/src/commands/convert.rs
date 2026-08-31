use std::path::PathBuf;
use std::time::{Duration, Instant};

use log::warn;
use serde::Deserialize;
use tauri::{AppHandle, Emitter};

use crate::engines::{self, ConversionRequest, ConversionResult};
use crate::error::ConversionError;
use crate::formats::{FileCategory, Format};
use crate::progress::ProgressPayload;

use super::output::{prepare_output, OutputOptions};

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AudioOutputFormat {
    Mp3,
    M4a,
    Wav,
    Flac,
}

impl AudioOutputFormat {
    pub(super) fn format(self) -> Format {
        match self {
            Self::Mp3 => Format::Mp3,
            Self::M4a => Format::M4a,
            Self::Wav => Format::Wav,
            Self::Flac => Format::Flac,
        }
    }
}

/// Main conversion command exposed to the frontend.
///
/// Spawns the appropriate engine, emits progress events, and returns the
/// result on success.
pub(super) async fn convert(
    app: AppHandle,
    input_path: String,
    output_format: String,
    job_id: Option<String>,
    output_options: Option<OutputOptions>,
) -> Result<ConversionResult, ConversionError> {
    let out_format = Format::from_extension(&output_format).ok_or_else(|| {
        ConversionError::UnsupportedConversion {
            input: "Unknown source".into(),
            output: output_format,
        }
    })?;
    convert_format(app, input_path, out_format, job_id, output_options).await
}

async fn convert_format(
    app: AppHandle,
    input_path: String,
    out_format: Format,
    job_id: Option<String>,
    output_options: Option<OutputOptions>,
) -> Result<ConversionResult, ConversionError> {
    let started = Instant::now();

    // --- Parse formats ---
    let input = PathBuf::from(&input_path);
    if !input.exists() {
        return Err(ConversionError::InputNotFound { path: input_path });
    }

    // File validation
    let input_meta = std::fs::metadata(&input).map_err(|e| ConversionError::InputNotFound {
        path: format!("{}: {}", input_path, e),
    })?;

    if input_meta.len() == 0 {
        return Err(ConversionError::UnsupportedConversion {
            input: "File is empty (0 bytes)".to_string(),
            output: out_format.label().into(),
        });
    }

    if !input_meta.is_file() {
        return Err(ConversionError::UnsupportedConversion {
            input: "Path is not a regular file".to_string(),
            output: out_format.label().into(),
        });
    }

    // Files with no extension
    let input_ext = input
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default();

    if input_ext.is_empty() {
        return Err(ConversionError::UnsupportedConversion {
            input: "File has no extension".to_string(),
            output: out_format.label().into(),
        });
    }

    // Magic-byte check via infer
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
            output: out_format.label().into(),
        }
    })?;
    // --- Resolve engine ---
    if engines::get_engines(input_format, out_format).is_empty() {
        return Err(ConversionError::UnsupportedConversion {
            input: input_format.label().to_string(),
            output: out_format.label().to_string(),
        });
    }
    let engine = engines::get_engine(input_format, out_format).ok_or_else(|| {
        let groups = engines::missing_tool_groups(input_format, out_format);
        let requirements = groups
            .iter()
            .map(|group| group.join(" + "))
            .collect::<Vec<_>>()
            .join(" or ");
        ConversionError::MissingDependency {
            tool: requirements.clone(),
            install_hint: format!("Install {requirements} to enable this conversion"),
        }
    })?;

    let prepared_output = prepare_output(&input, out_format.extension(), "", output_options)?;

    let active_job = super::jobs::ActiveJobGuard::register(app.clone(), job_id)?;
    let cancel_token = active_job.cancel_token();
    let event_job_id = active_job.job_id().to_owned();

    // Emit initial progress
    let _ = app.emit(
        "conversion-progress",
        ProgressPayload {
            job_id: event_job_id.clone(),
            percent: 0,
            stage: "Starting conversion".to_string(),
        },
    );

    let request = ConversionRequest {
        job_id: event_job_id.clone(),
        input_path: input,
        output_path: prepared_output.working_path().to_path_buf(),
        input_format,
        output_format: out_format,
    };

    // Timeout
    let timeout_duration = if input_format.category() == FileCategory::Video
        || out_format.category() == FileCategory::Video
    {
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
        Ok(res) => {
            let mut res = prepared_output.commit(res)?;
            res.duration_ms = started.elapsed().as_millis() as u64;
            let _ = app.emit(
                "conversion-progress",
                ProgressPayload {
                    job_id: event_job_id,
                    percent: 100,
                    stage: "Complete".to_string(),
                },
            );
            Ok(res)
        }
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_output_formats_map_to_audio_formats() {
        for format in [
            AudioOutputFormat::Mp3,
            AudioOutputFormat::M4a,
            AudioOutputFormat::Wav,
            AudioOutputFormat::Flac,
        ] {
            assert_eq!(format.format().category(), FileCategory::Audio);
        }
    }
}
