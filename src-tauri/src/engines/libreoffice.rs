use std::time::Duration;

use tauri::{AppHandle, Emitter};
use tokio_util::sync::CancellationToken;

use crate::engines::process::{run_process, ProcessMessages};
use crate::engines::{
    tool_command, verification, ConversionEngine, ConversionRequest, ConversionResult,
};
use crate::error::ConversionError;
use crate::formats::Format;
use crate::progress::ProgressPayload;

pub struct LibreOfficeEngine;

impl ConversionEngine for LibreOfficeEngine {
    fn required_tool(&self) -> &'static str {
        "soffice"
    }

    fn supports(&self, input: Format, output: Format) -> bool {
        // PDF → DOCX is intentionally excluded until its output is reliable
        // enough to advertise and verify as a supported conversion.
        input == Format::Docx && output == Format::Pdf
    }

    async fn convert(
        &self,
        request: ConversionRequest,
        app: AppHandle,
        cancel_token: CancellationToken,
    ) -> Result<ConversionResult, ConversionError> {
        let input = &request.input_path;
        let output = &request.output_path;
        let workspace = tempfile::tempdir().map_err(|error| ConversionError::ProcessFailed {
            message: format!("Could not create a LibreOffice workspace: {error}"),
            stderr: String::new(),
            exit_code: None,
        })?;
        let out_dir = workspace.path();

        let _ = app.emit(
            "conversion-progress",
            ProgressPayload {
                job_id: request.job_id.clone(),
                percent: -1,
                stage: "Converting with LibreOffice…".into(),
            },
        );

        let convert_to = "pdf";

        let mut command = tool_command("soffice");
        command
            .arg("--headless")
            .args([
                "--convert-to",
                convert_to,
                "--outdir",
                &out_dir.to_string_lossy(),
            ])
            .arg(input);
        run_process(
            command,
            cancel_token,
            Duration::from_secs(10 * 60),
            &[],
            ProcessMessages {
                start: "Failed to start LibreOffice",
                wait: "LibreOffice process could not be awaited",
                failure: "LibreOffice conversion failed",
            },
        )
        .await?;

        // LibreOffice controls its own filename, so isolate it in a temporary
        // directory and only move the verified result to the requested path.
        let stem = input
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let expected_ext = format!(".{}", convert_to);

        let lo_output = std::fs::read_dir(out_dir)
            .ok()
            .and_then(|entries| {
                entries.filter_map(|e| e.ok()).map(|e| e.path()).find(|p| {
                    let fname = p
                        .file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string();
                    let ext_matches = p
                        .extension()
                        .map(|e| format!(".{}", e.to_string_lossy().to_lowercase()))
                        == Some(expected_ext.clone());
                    ext_matches && fname.starts_with(&stem)
                })
            })
            .unwrap_or_else(|| out_dir.join(format!("{}.{}", stem, convert_to)));

        if !lo_output.is_file() {
            return Err(ConversionError::OutputMissing);
        }

        std::fs::rename(&lo_output, output)
            .or_else(|_| {
                std::fs::copy(&lo_output, output)
                    .map(|_| ())
                    .map_err(|_| std::io::Error::other("Could not save LibreOffice output"))
            })
            .map_err(|error| ConversionError::ProcessFailed {
                message: format!("Could not save LibreOffice output: {error}"),
                stderr: String::new(),
                exit_code: None,
            })?;

        let output_size = verification::nonempty_file(output)?;
        Ok(ConversionResult {
            output_path: output.to_string_lossy().into(),
            output_paths: Vec::new(),
            output_size,
            duration_ms: 0,
            undo_manifest: None,
        })
    }
}
