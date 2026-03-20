use tauri::AppHandle;
use tokio_util::sync::CancellationToken;

use crate::engines::{ConversionEngine, ConversionRequest, ConversionResult};
use crate::error::ConversionError;
use crate::formats::{FileCategory, Format};

/// Engine that shells out to Pandoc for document conversions.
///
/// **Phase 1 stub** -- returns `UnsupportedConversion` from `convert()`.
pub struct PandocEngine;

impl ConversionEngine for PandocEngine {
    fn required_tool(&self) -> &'static str {
        "pandoc"
    }

    fn supports(&self, input: Format, output: Format) -> bool {
        input.category() == FileCategory::Document
            && output.category() == FileCategory::Document
            && input != output
    }

    async fn convert(
        &self,
        request: ConversionRequest,
        _app: AppHandle,
        _cancel_token: CancellationToken,
    ) -> Result<ConversionResult, ConversionError> {
        Err(ConversionError::UnsupportedConversion {
            input: format!(
                "Pandoc engine not yet implemented ({})",
                request.input_format.label()
            ),
            output: request.output_format.label().to_string(),
        })
    }
}
