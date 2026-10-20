use tauri::{AppHandle, Emitter};
use tokio_util::sync::CancellationToken;

use crate::engines::{
    imageio, verification, ConversionEngine, ConversionRequest, ConversionResult,
};
use crate::error::ConversionError;
use crate::formats::{FileCategory, Format};
use crate::progress::ProgressPayload;

/// Raster-to-SVG tracing with the vtracer library, linked into the app and
/// run with the same defaults as `vtracer --colormode color`. The bundled
/// ImageIO helper decodes the source, so every raster format can be traced.
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
        let _ = app.emit(
            "conversion-progress",
            ProgressPayload {
                job_id: request.job_id.clone(),
                percent: -1,
                stage: "Tracing to vector…".into(),
            },
        );
        trace_to_svg(&request.input_path, &request.output_path, cancel_token).await
    }
}

pub(crate) async fn trace_to_svg(
    input: &std::path::Path,
    output: &std::path::Path,
    cancel_token: CancellationToken,
) -> Result<ConversionResult, ConversionError> {
    let frame = imageio::render_first_frame(input, cancel_token.clone()).await?;
    let svg = tokio::task::spawn_blocking(move || {
        let image = vtracer::ColorImage {
            pixels: frame.pixels,
            width: frame.width as usize,
            height: frame.height as usize,
        };
        vtracer::convert(image, vtracer::Config::default()).map(|svg| svg.to_string())
    })
    .await
    .map_err(|error| trace_error(format!("The vector tracer stopped unexpectedly: {error}")))?
    .map_err(|error| trace_error(format!("Vector tracing failed: {error}")))?;
    if cancel_token.is_cancelled() {
        return Err(ConversionError::Cancelled);
    }
    std::fs::write(output, svg).map_err(|error| {
        super::cleanup_partial(output);
        trace_error(format!("Could not save the SVG: {error}"))
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

fn trace_error(message: String) -> ConversionError {
    ConversionError::ProcessFailed {
        message,
        stderr: String::new(),
        exit_code: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn traces_rasters_to_svg_in_process() {
        if !imageio::can_read(Format::Png) {
            return;
        }
        let directory = tempfile::tempdir().expect("tempdir");
        let input = directory.path().join("source.png");
        imageio::fixtures::png(&input, 48, 32, [0x25, 0x33, 0x4f, 0xff]);
        let output = directory.path().join("traced.svg");
        trace_to_svg(&input, &output, CancellationToken::new())
            .await
            .expect("trace");
        let svg = std::fs::read_to_string(&output).expect("svg");
        assert!(
            svg.contains("<svg") && svg.contains("width=\"48\""),
            "{svg}"
        );
        assert!(svg.contains("<path"), "{svg}");
    }
}
