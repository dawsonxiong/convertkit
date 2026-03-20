use tauri::AppHandle;
use tokio_util::sync::CancellationToken;

use crate::engines::{ConversionEngine, ConversionRequest, ConversionResult};
use crate::error::ConversionError;
use crate::formats::Format;

/// Engine that shells out to resvg for SVG -> raster image conversions.
///
/// **Phase 1 stub** -- returns `UnsupportedConversion` from `convert()`.
pub struct ResvgEngine;

impl ConversionEngine for ResvgEngine {
    fn required_tool(&self) -> &'static str {
        "resvg"
    }

    fn supports(&self, input: Format, output: Format) -> bool {
        // resvg converts SVG to PNG (its native output). Other raster targets
        // would need a secondary pass, so for now only SVG -> PNG.
        input == Format::Svg && output == Format::Png
    }

    async fn convert(
        &self,
        request: ConversionRequest,
        _app: AppHandle,
        _cancel_token: CancellationToken,
    ) -> Result<ConversionResult, ConversionError> {
        Err(ConversionError::UnsupportedConversion {
            input: format!(
                "Resvg engine not yet implemented ({})",
                request.input_format.label()
            ),
            output: request.output_format.label().to_string(),
        })
    }
}
