use tauri::AppHandle;
use tokio_util::sync::CancellationToken;

use crate::engines::{ConversionEngine, ConversionRequest, ConversionResult};
use crate::error::ConversionError;
use crate::formats::{FileCategory, Format};

/// Engine that shells out to FFmpeg for video and audio conversions.
///
/// **Phase 1 stub** -- returns `UnsupportedConversion` from `convert()`.
pub struct FfmpegEngine;

impl ConversionEngine for FfmpegEngine {
    fn required_tool(&self) -> &'static str {
        "ffmpeg"
    }

    fn supports(&self, input: Format, output: Format) -> bool {
        let video =
            input.category() == FileCategory::Video && output.category() == FileCategory::Video;
        let audio =
            input.category() == FileCategory::Audio && output.category() == FileCategory::Audio;
        let video_to_audio =
            input.category() == FileCategory::Video && output.category() == FileCategory::Audio;
        (video || audio || video_to_audio) && input != output
    }

    async fn convert(
        &self,
        request: ConversionRequest,
        _app: AppHandle,
        _cancel_token: CancellationToken,
    ) -> Result<ConversionResult, ConversionError> {
        Err(ConversionError::UnsupportedConversion {
            input: format!("FFmpeg engine not yet implemented ({})", request.input_format.label()),
            output: request.output_format.label().to_string(),
        })
    }
}
