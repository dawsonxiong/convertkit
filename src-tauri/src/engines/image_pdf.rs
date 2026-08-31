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

/// Bundled Quartz/ImageIO bridge for static raster image to single-page PDF.
pub struct ImagePdfEngine;

pub(crate) async fn create_single_page_pdf(
    input: &std::path::Path,
    output: &std::path::Path,
    cancel_token: CancellationToken,
) -> Result<u64, ConversionError> {
    let mut command = tool_command("convertkit-image-pdf");
    command.arg(input).arg(output);
    run_process(
        command,
        cancel_token,
        Duration::from_secs(10 * 60),
        &[output],
        ProcessMessages {
            start: "Failed to start the image to PDF helper",
            wait: "The image to PDF helper could not be awaited",
            failure: "Image to PDF conversion failed",
        },
    )
    .await?;

    verification::file_with_signature(output, b"%PDF-")
}

impl ConversionEngine for ImagePdfEngine {
    fn required_tool(&self) -> &'static str {
        "convertkit-image-pdf"
    }

    fn supports(&self, input: Format, output: Format) -> bool {
        matches!(input, Format::Jpg | Format::Png) && output == Format::Pdf
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
                stage: "Creating PDF".to_string(),
            },
        );

        let output_size = create_single_page_pdf(input, output, cancel_token).await?;
        Ok(ConversionResult {
            output_path: output.to_string_lossy().to_string(),
            output_paths: Vec::new(),
            output_size,
            duration_ms: 0,
            undo_manifest: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_png_and_jpeg_to_pdf_only() {
        let engine = ImagePdfEngine;
        for input in [Format::Jpg, Format::Png] {
            assert!(engine.supports(input, Format::Pdf));
        }
        assert!(!engine.supports(Format::WebP, Format::Pdf));
        assert!(!engine.supports(Format::Svg, Format::Pdf));
        assert!(!engine.supports(Format::Png, Format::Jpg));
    }
}
