use std::time::Duration;

use tauri::{AppHandle, Emitter};
use tokio_util::sync::CancellationToken;

use crate::engines::process::{run_process, ProcessMessages};
use crate::engines::{
    tool_command, verification, ConversionEngine, ConversionRequest, ConversionResult,
};
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
                job_id: request.job_id.clone(),
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

        let mut command = tool_command("pandoc");
        command.args(&args);
        run_process(
            command,
            cancel_token,
            Duration::from_secs(10 * 60),
            &[output],
            ProcessMessages {
                start: "Failed to start Pandoc",
                wait: "Pandoc process could not be awaited",
                failure: "Pandoc conversion failed",
            },
        )
        .await?;

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
