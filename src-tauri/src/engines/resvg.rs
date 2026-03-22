use tauri::{AppHandle, Emitter};
use tokio_util::sync::CancellationToken;

use crate::engines::{ConversionEngine, ConversionRequest, ConversionResult};
use crate::error::ConversionError;
use crate::formats::Format;
use crate::progress::ProgressPayload;

pub struct ResvgEngine;

impl ConversionEngine for ResvgEngine {
    fn required_tool(&self) -> &'static str {
        "resvg"
    }

    fn supports(&self, input: Format, output: Format) -> bool {
        // resvg renders SVG to PNG natively. For other raster targets,
        // ImageMagick can convert the PNG afterwards, but for now keep it
        // simple: SVG → PNG only.
        input == Format::Svg && output == Format::Png
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
            stage: "Rendering SVG…".into(),
        });

        // Default to 2x scale for retina-friendly output.
        let mut child = tokio::process::Command::new("resvg")
            .args([
                &input.to_string_lossy().to_string(),
                &output.to_string_lossy().to_string(),
                "--dpi", "192",
            ])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| ConversionError::ProcessFailed {
                message: format!("Failed to spawn resvg: {e}"),
                stderr: String::new(),
                exit_code: None,
            })?;

        let status = tokio::select! {
            result = child.wait() => {
                result.map_err(|e| ConversionError::ProcessFailed {
                    message: format!("resvg error: {e}"),
                    stderr: String::new(),
                    exit_code: None,
                })?
            }
            _ = cancel_token.cancelled() => {
                let _ = child.kill().await;
                super::cleanup_partial(output);
                return Err(ConversionError::Cancelled);
            }
        };

        if !status.success() {
            let stderr = match child.stderr {
                Some(ref mut s) => {
                    use tokio::io::AsyncReadExt;
                    let mut buf = String::new();
                    let _ = s.read_to_string(&mut buf).await;
                    buf
                }
                None => String::new(),
            };
            super::cleanup_partial(output);
            return Err(ConversionError::ProcessFailed {
                message: "resvg rendering failed".into(),
                stderr,
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

