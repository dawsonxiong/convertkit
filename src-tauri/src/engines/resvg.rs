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

        let _ = app.emit(
            "conversion-progress",
            ProgressPayload {
                job_id: request.job_id.clone(),
                percent: -1,
                stage: "Rendering SVG…".into(),
            },
        );

        // Default to 2x scale for retina-friendly output.
        let input_arg = input.to_string_lossy();
        let output_arg = output.to_string_lossy();
        let mut command = tool_command("resvg");
        command.args([input_arg.as_ref(), output_arg.as_ref(), "--dpi", "192"]);
        run_process(
            command,
            cancel_token,
            Duration::from_secs(10 * 60),
            &[output],
            ProcessMessages {
                start: "Failed to start resvg",
                wait: "resvg process could not be awaited",
                failure: "resvg rendering failed",
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
