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

        let _ = app.emit(
            "conversion-progress",
            ProgressPayload {
                job_id: request.job_id.clone(),
                percent: -1,
                stage: "Tracing to vector…".into(),
            },
        );

        let mut command = tool_command("vtracer");
        command.args([
            "--input",
            &input.to_string_lossy(),
            "--output",
            &output.to_string_lossy(),
            "--colormode",
            "color",
        ]);
        run_process(
            command,
            cancel_token,
            Duration::from_secs(10 * 60),
            &[output],
            ProcessMessages {
                start: "Failed to start vtracer",
                wait: "vtracer process could not be awaited",
                failure: "Vector tracing failed",
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
