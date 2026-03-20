use std::path::Path;

use tauri::{AppHandle, Emitter};
use tokio_util::sync::CancellationToken;

use crate::engines::{ConversionEngine, ConversionRequest, ConversionResult};
use crate::error::ConversionError;
use crate::formats::{FileCategory, Format};
use crate::progress::ProgressPayload;

pub struct VTracerEngine;

impl ConversionEngine for VTracerEngine {
    fn required_tool(&self) -> &'static str {
        "vtracer"
    }

    fn supports(&self, input: Format, output: Format) -> bool {
        input.category() == FileCategory::Image && output == Format::Svg
    }

    async fn convert(
        &self,
        request: ConversionRequest,
        app: AppHandle,
        cancel_token: CancellationToken,
    ) -> Result<ConversionResult, ConversionError> {
        let input = &request.input_path;
        let output = &request.output_path;

        let _ = app.emit("conversion-progress", ProgressPayload {
            percent: -1,
            stage: "Tracing to vector…".into(),
        });

        let mut child = tokio::process::Command::new("vtracer")
            .args([
                "--input", &input.to_string_lossy(),
                "--output", &output.to_string_lossy(),
                "--colormode", "color",
            ])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| ConversionError::ProcessFailed {
                message: format!("Failed to spawn vtracer: {e}"),
                stderr: String::new(),
                exit_code: None,
            })?;

        let status = tokio::select! {
            result = child.wait() => {
                result.map_err(|e| ConversionError::ProcessFailed {
                    message: format!("vtracer error: {e}"),
                    stderr: String::new(),
                    exit_code: None,
                })?
            }
            _ = cancel_token.cancelled() => {
                let _ = child.kill().await;
                cleanup_partial(output);
                return Err(ConversionError::Cancelled);
            }
        };

        if !status.success() {
            cleanup_partial(output);
            return Err(ConversionError::ProcessFailed {
                message: "Vector tracing failed".into(),
                stderr: String::new(),
                exit_code: status.code(),
            });
        }

        let meta = std::fs::metadata(output).map_err(|_| ConversionError::OutputMissing)?;
        Ok(ConversionResult {
            output_path: output.to_string_lossy().into(),
            output_size: meta.len(),
            duration_ms: 0,
        })
    }
}

fn cleanup_partial(path: &Path) {
    if path.exists() {
        let _ = std::fs::remove_file(path);
    }
}
