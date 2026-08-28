use tauri::{AppHandle, Emitter};
use tokio_util::sync::CancellationToken;

use crate::engines::{tool_command, ConversionEngine, ConversionRequest, ConversionResult};
use crate::error::ConversionError;
use crate::formats::{FileCategory, Format};
use crate::progress::ProgressPayload;

pub struct PandocEngine;

impl ConversionEngine for PandocEngine {
    fn required_tool(&self) -> &'static str {
        "pandoc"
    }

    fn supports(&self, input: Format, output: Format) -> bool {
        input.category() == FileCategory::Document
            && output.category() == FileCategory::Document
            && input != Format::Pdf
            && input != output
            // LibreOffice handles DOCX→PDF better; only fall through to Pandoc
            // for other document pairs.
            && !(input == Format::Docx && output == Format::Pdf)
    }

    async fn convert(
        &self,
        request: ConversionRequest,
        app: AppHandle,
        cancel_token: CancellationToken,
    ) -> Result<ConversionResult, ConversionError> {
        let input = &request.input_path;
        let output = &request.output_path;

        let _ = app.emit(
            "conversion-progress",
            ProgressPayload {
                percent: -1,
                stage: "Converting document…".into(),
            },
        );

        let mut args: Vec<String> = vec![input.to_string_lossy().into()];

        // Output format hint for Pandoc.
        if request.output_format == Format::Md {
            args.extend(["-t".into(), "gfm".into()]);
        }

        // PDF needs a LaTeX engine.
        if request.output_format == Format::Pdf {
            args.extend(["--pdf-engine".into(), "tectonic".into()]);
        }

        args.extend(["-o".into(), output.to_string_lossy().into()]);

        let mut child = tool_command("pandoc")
            .args(&args)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| ConversionError::ProcessFailed {
                message: format!("Failed to spawn pandoc: {e}"),
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

        let status = tokio::select! {
            result = child.wait() => {
                result.map_err(|e| ConversionError::ProcessFailed {
                    message: format!("pandoc error: {e}"),
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

        let stderr = stderr_handle.await.unwrap_or_default();
        if !status.success() {
            super::cleanup_partial(output);
            return Err(ConversionError::ProcessFailed {
                message: "Pandoc conversion failed".into(),
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
