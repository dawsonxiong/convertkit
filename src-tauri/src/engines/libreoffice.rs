use tauri::AppHandle;
use tokio_util::sync::CancellationToken;

use crate::engines::{ConversionEngine, ConversionRequest, ConversionResult};
use crate::error::ConversionError;
use crate::formats::Format;

/// Engine that shells out to LibreOffice (`soffice`) for document conversions
/// involving office formats (e.g. DOCX -> PDF).
///
/// **Phase 1 stub** -- returns `UnsupportedConversion` from `convert()`.
pub struct LibreOfficeEngine;

impl ConversionEngine for LibreOfficeEngine {
    fn required_tool(&self) -> &'static str {
        "soffice"
    }

    fn supports(&self, input: Format, output: Format) -> bool {
        // LibreOffice handles DOCX <-> PDF primarily.
        let docx_to_pdf = input == Format::Docx && output == Format::Pdf;
        let pdf_to_docx = input == Format::Pdf && output == Format::Docx;
        docx_to_pdf || pdf_to_docx
    }

    async fn convert(
        &self,
        request: ConversionRequest,
        _app: AppHandle,
        _cancel_token: CancellationToken,
    ) -> Result<ConversionResult, ConversionError> {
        Err(ConversionError::UnsupportedConversion {
            input: format!(
                "LibreOffice engine not yet implemented ({})",
                request.input_format.label()
            ),
            output: request.output_format.label().to_string(),
        })
    }
}
