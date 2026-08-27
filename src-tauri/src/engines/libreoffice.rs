use std::path::Path;

use tauri::{AppHandle, Emitter};
use tokio_util::sync::CancellationToken;

use crate::engines::{tool_command, ConversionEngine, ConversionRequest, ConversionResult};
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

        let mut cmd = tool_command("soffice");
        cmd.arg("--headless");

        // PDF inputs need an explicit import filter so LibreOffice opens them
        // as editable Writer documents instead of Draw pages.
        if request.input_format == Format::Pdf {
            cmd.arg("--infilter=writer_pdf_import");
        }

        let mut child = cmd
            .args(["--convert-to", convert_to, "--outdir", &out_dir.to_string_lossy()])
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
                super::cleanup_partial(output);
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
        // However, the naming can be unpredictable (e.g. spaces replaced, etc.),
        // so search for any file matching {stem}.{convert_to} in the output dir.
        let stem = input.file_stem().unwrap_or_default().to_string_lossy().to_string();
        let expected_ext = format!(".{}", convert_to);

        let lo_output = std::fs::read_dir(out_dir)
            .ok()
            .and_then(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .map(|e| e.path())
                    .find(|p| {
                        let fname = p.file_stem().unwrap_or_default().to_string_lossy().to_string();
                        let ext_matches = p
                            .extension()
                            .map(|e| format!(".{}", e.to_string_lossy().to_lowercase()))
                            == Some(expected_ext.clone());
                        ext_matches && fname.starts_with(&stem)
                    })
            })
            .unwrap_or_else(|| {
                // Fall back to the original predictable name.
                out_dir.join(format!("{}.{}", stem, convert_to))
            });

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
