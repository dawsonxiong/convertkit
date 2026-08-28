use tauri::{AppHandle, Emitter};
use tokio_util::sync::CancellationToken;

use crate::engines::{tool_command, ConversionEngine, ConversionRequest, ConversionResult};
use crate::error::ConversionError;
use crate::formats::{FileCategory, Format};
use crate::progress::ProgressPayload;

/// Engine that shells out to ImageMagick (`magick`) for raster image
/// conversions.
pub struct ImageMagickEngine;

impl ConversionEngine for ImageMagickEngine {
    fn required_tool(&self) -> &'static str {
        "magick"
    }

    fn supports(&self, input: Format, output: Format) -> bool {
        let input_ok = input.category() == FileCategory::Image
            || input == Format::Heic
            || input == Format::Svg;
        let output_ok = output.category() == FileCategory::Image;
        input_ok && output_ok && input != output
    }

    async fn convert(
        &self,
        request: ConversionRequest,
        app: AppHandle,
        cancel_token: CancellationToken,
    ) -> Result<ConversionResult, ConversionError> {
        let input = &request.input_path;
        let output = &request.output_path;

        // Emit indeterminate progress (ImageMagick doesn't report progress).
        let _ = app.emit(
            "conversion-progress",
            ProgressPayload {
                percent: -1,
                stage: "Converting with ImageMagick".to_string(),
            },
        );

        // Build argument list.
        let mut args: Vec<String> = vec![
            "convert".to_string(),
            input.to_string_lossy().to_string(),
            "-strip".to_string(),
        ];

        // Format-specific quality / size flags.
        match request.output_format {
            Format::Jpg => {
                args.push("-quality".to_string());
                args.push("90".to_string());
            }
            Format::WebP => {
                args.push("-quality".to_string());
                args.push("85".to_string());
            }
            Format::Avif => {
                args.push("-quality".to_string());
                args.push("80".to_string());
            }
            Format::Ico => {
                args.push("-resize".to_string());
                args.push("256x256>".to_string());
            }
            _ => {}
        }

        args.push(output.to_string_lossy().to_string());

        // Spawn the child process.
        let mut child = tool_command("magick")
            .args(&args)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| ConversionError::ProcessFailed {
                message: format!("Failed to spawn magick: {e}"),
                stderr: String::new(),
                exit_code: None,
            })?;

        let stderr = child.stderr.take();
        let stderr_handle = tokio::spawn(async move {
            match stderr {
                Some(mut stderr) => {
                    use tokio::io::AsyncReadExt;
                    let mut output = String::new();
                    let _ = stderr.read_to_string(&mut output).await;
                    output
                }
                None => String::new(),
            }
        });

        // Wait for completion or cancellation.
        let status = tokio::select! {
            result = child.wait() => {
                result.map_err(|e| ConversionError::ProcessFailed {
                    message: format!("magick process error: {e}"),
                    stderr: String::new(),
                    exit_code: None,
                })?
            }
            _ = cancel_token.cancelled() => {
                // Kill the child and clean up partial output.
                let _ = child.kill().await;
                super::cleanup_partial(output);
                return Err(ConversionError::Cancelled);
            }
        };

        let stderr = stderr_handle.await.unwrap_or_default();
        if !status.success() {
            super::cleanup_partial(output);
            return Err(ConversionError::ProcessFailed {
                message: "ImageMagick conversion failed".to_string(),
                stderr,
                exit_code: status.code(),
            });
        }

        // Verify the output file exists.
        let meta = std::fs::metadata(output).map_err(|_| ConversionError::OutputMissing)?;

        Ok(ConversionResult {
            output_path: output.to_string_lossy().to_string(),
            output_size: meta.len(),
            duration_ms: 0, // filled in by the caller
        })
    }
}
