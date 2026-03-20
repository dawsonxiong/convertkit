pub mod ffmpeg;
pub mod imagemagick;
pub mod libreoffice;
pub mod pandoc;
pub mod resvg;
pub mod vtracer;

use std::path::PathBuf;

use serde::Serialize;
use tauri::AppHandle;
use tokio_util::sync::CancellationToken;

use crate::error::ConversionError;
use crate::formats::{FileCategory, Format};

// ---------------------------------------------------------------------------
// Shared types
// ---------------------------------------------------------------------------

/// Everything an engine needs to know to perform a conversion.
pub struct ConversionRequest {
    pub input_path: PathBuf,
    pub output_path: PathBuf,
    pub input_format: Format,
    pub output_format: Format,
}

/// Returned to the frontend on success.
#[derive(Debug, Clone, Serialize)]
pub struct ConversionResult {
    pub output_path: String,
    pub output_size: u64,
    pub duration_ms: u64,
}

// ---------------------------------------------------------------------------
// Engine trait
// ---------------------------------------------------------------------------

/// A backend that can convert files from one format to another.
///
/// Uses `impl Future` (RPITIT) for `convert`, which means the trait is not
/// dyn-compatible. We use the [`EngineKind`] enum for dynamic dispatch instead.
pub trait ConversionEngine: Send + Sync {
    /// Whether this engine can handle the given `(input, output)` pair.
    fn supports(&self, input: Format, output: Format) -> bool;

    /// Perform the conversion. Implementations should respect cancellation and
    /// emit progress events via the [`AppHandle`].
    fn convert(
        &self,
        request: ConversionRequest,
        app: AppHandle,
        cancel_token: CancellationToken,
    ) -> impl std::future::Future<Output = Result<ConversionResult, ConversionError>> + Send;

    /// The name of the CLI tool this engine shells out to.
    fn required_tool(&self) -> &'static str;

    /// Quick check whether the required tool is installed.
    fn is_available(&self) -> bool {
        which::which(self.required_tool()).is_ok()
    }
}

// ---------------------------------------------------------------------------
// Enum dispatch -- avoids dyn-compatibility issues
// ---------------------------------------------------------------------------

/// Concrete enum that wraps every engine so we can return a single type from
/// the router without requiring trait objects.
pub enum EngineKind {
    Resvg(resvg::ResvgEngine),
    VTracer(vtracer::VTracerEngine),
    LibreOffice(libreoffice::LibreOfficeEngine),
    ImageMagick(imagemagick::ImageMagickEngine),
    Ffmpeg(ffmpeg::FfmpegEngine),
    Pandoc(pandoc::PandocEngine),
}

impl EngineKind {
    pub fn supports(&self, input: Format, output: Format) -> bool {
        match self {
            Self::Resvg(e) => e.supports(input, output),
            Self::VTracer(e) => e.supports(input, output),
            Self::LibreOffice(e) => e.supports(input, output),
            Self::ImageMagick(e) => e.supports(input, output),
            Self::Ffmpeg(e) => e.supports(input, output),
            Self::Pandoc(e) => e.supports(input, output),
        }
    }

    pub fn required_tool(&self) -> &'static str {
        match self {
            Self::Resvg(e) => e.required_tool(),
            Self::VTracer(e) => e.required_tool(),
            Self::LibreOffice(e) => e.required_tool(),
            Self::ImageMagick(e) => e.required_tool(),
            Self::Ffmpeg(e) => e.required_tool(),
            Self::Pandoc(e) => e.required_tool(),
        }
    }

    pub fn is_available(&self) -> bool {
        match self {
            Self::Resvg(e) => e.is_available(),
            Self::VTracer(e) => e.is_available(),
            Self::LibreOffice(e) => e.is_available(),
            Self::ImageMagick(e) => e.is_available(),
            Self::Ffmpeg(e) => e.is_available(),
            Self::Pandoc(e) => e.is_available(),
        }
    }

    pub async fn convert(
        &self,
        request: ConversionRequest,
        app: AppHandle,
        cancel_token: CancellationToken,
    ) -> Result<ConversionResult, ConversionError> {
        match self {
            Self::Resvg(e) => e.convert(request, app, cancel_token).await,
            Self::VTracer(e) => e.convert(request, app, cancel_token).await,
            Self::LibreOffice(e) => e.convert(request, app, cancel_token).await,
            Self::ImageMagick(e) => e.convert(request, app, cancel_token).await,
            Self::Ffmpeg(e) => e.convert(request, app, cancel_token).await,
            Self::Pandoc(e) => e.convert(request, app, cancel_token).await,
        }
    }
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

/// Return the best engine that supports the given format pair, respecting
/// priority: Resvg > VTracer > LibreOffice > ImageMagick > FFmpeg > Pandoc.
///
/// Only returns engines whose `supports()` returns true. The caller should
/// still check `is_available()` before invoking `convert()`.
pub fn get_engine(input: Format, output: Format) -> Option<EngineKind> {
    // Priority-ordered list. We construct lightweight structs on the fly.
    let candidates: Vec<EngineKind> = vec![
        EngineKind::Resvg(resvg::ResvgEngine),
        EngineKind::VTracer(vtracer::VTracerEngine),
        EngineKind::LibreOffice(libreoffice::LibreOfficeEngine),
        EngineKind::ImageMagick(imagemagick::ImageMagickEngine),
        EngineKind::Ffmpeg(ffmpeg::FfmpegEngine),
        EngineKind::Pandoc(pandoc::PandocEngine),
    ];

    candidates.into_iter().find(|e| e.supports(input, output))
}

/// Helper: both formats are raster images (Image category).
#[allow(dead_code)]
pub(crate) fn both_images(a: Format, b: Format) -> bool {
    a.category() == FileCategory::Image && b.category() == FileCategory::Image
}
