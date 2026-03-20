use std::path::Path;

use tauri::{AppHandle, Emitter};
use tokio_util::sync::CancellationToken;

use crate::engines::{ConversionEngine, ConversionRequest, ConversionResult};
use crate::error::ConversionError;
use crate::formats::Format;
use crate::progress::ProgressPayload;

pub struct LibreOfficeEngine;

impl ConversionEngine for LibreOfficeEngine {
    fn required_tool(&self) -> &'static str {
        "soffice"
    }

    fn supports(&self, input: Format, output: Format) -> bool {
        // LibreOffice handles DOCX → PDF (and PDF → DOCX, though quality varies).
        (input == Format::Docx && output == Format::Pdf)
            || (input == Format::Pdf && output == Format::Docx)
    }

    async fn convert(
        &self,
        request: ConversionRequest,
        app: AppHandle,
        cancel_token: CancellationToken,
    ) -> Result<ConversionResult, ConversionError> {
        let input = &request.input_path;
        let output = &request.output_path;
        let out_dir = output.parent().unwrap_or(Path::new("."));

        let _ = app.emit("conversion-progress", ProgressPayload {
            percent: -1,
            stage: "Converting with LibreOffice…".into(),
        });

        let convert_to = match request.output_format {
            Format::Pdf => "pdf",
            Format::Docx => "docx",
            _ => "pdf",
        };

        let mut child = tokio::process::Command::new("soffice")
            .args([
                "--headless",
                "--convert-to", convert_to,
                "--outdir", &out_dir.to_string_lossy(),
            ])
            .arg(input)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| ConversionError::ProcessFailed {
                message: format!("Failed to spawn soffice: {e}"),
                stderr: String::new(),
                exit_code: None,
            })?;

        let status = tokio::select! {
            result = child.wait() => {
                result.map_err(|e| ConversionError::ProcessFailed {
                    message: format!("soffice error: {e}"),
                    stderr: String::new(),
                    exit_code: None,
                })?
            }
            _ = cancel_token.cancelled() => {
                let _ = child.kill().await;
                return Err(ConversionError::Cancelled);
            }
        };

        if !status.success() {
            return Err(ConversionError::ProcessFailed {
                message: "LibreOffice conversion failed".into(),
                stderr: String::new(),
                exit_code: status.code(),
            });
        }

        // LibreOffice writes to outdir with the same stem but new extension.
        // If our desired output path differs, rename.
        let lo_output = out_dir.join(format!(
            "{}.{}",
            input.file_stem().unwrap_or_default().to_string_lossy(),
            convert_to,
        ));

        if lo_output != *output && lo_output.exists() {
            std::fs::rename(&lo_output, output).map_err(|_| ConversionError::OutputMissing)?;
        }

        let meta = std::fs::metadata(output).map_err(|_| ConversionError::OutputMissing)?;
        Ok(ConversionResult {
            output_path: output.to_string_lossy().into(),
            output_size: meta.len(),
            duration_ms: 0,
        })
    }
}
