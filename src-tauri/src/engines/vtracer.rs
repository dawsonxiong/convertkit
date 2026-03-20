use tauri::AppHandle;
use tokio_util::sync::CancellationToken;

use crate::engines::{ConversionEngine, ConversionRequest, ConversionResult};
use crate::error::ConversionError;
use crate::formats::{FileCategory, Format};

/// Engine that shells out to vtracer for raster image -> SVG vectorisation.
///
/// **Phase 1 stub** -- returns `UnsupportedConversion` from `convert()`.
pub struct VTracerEngine;

impl ConversionEngine for VTracerEngine {
    fn required_tool(&self) -> &'static str {
        "vtracer"
    }

    fn supports(&self, input: Format, output: Format) -> bool {
        // Raster image -> SVG
        input.category() == FileCategory::Image && output == Format::Svg
    }

    async fn convert(
        &self,
        request: ConversionRequest,
        _app: AppHandle,
        _cancel_token: CancellationToken,
    ) -> Result<ConversionResult, ConversionError> {
        Err(ConversionError::UnsupportedConversion {
            input: format!(
                "VTracer engine not yet implemented ({})",
                request.input_format.label()
            ),
            output: request.output_format.label().to_string(),
        })
    }
}
