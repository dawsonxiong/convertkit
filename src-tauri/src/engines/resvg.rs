use std::path::Path;
use std::sync::{Arc, OnceLock};

use tauri::{AppHandle, Emitter};
use tokio_util::sync::CancellationToken;

use crate::engines::{
    imagemagick, verification, ConversionEngine, ConversionRequest, ConversionResult,
};
use crate::error::ConversionError;
use crate::formats::{FileCategory, Format};
use crate::progress::ProgressPayload;

/// Pixel edge and area limits for rendered SVGs, matching the raster tools.
const MAX_EDGE: u32 = 32_768;
const MAX_PIXELS: u64 = 256 * 1024 * 1024;

/// SVG rendering with the resvg library, linked into the app. PNG output is
/// written directly; other rasters are encoded from that PNG by the shared
/// raster path.
pub struct ResvgEngine;

impl ConversionEngine for ResvgEngine {
    fn required_tool(&self) -> &'static str {
        "resvg"
    }

    fn supports(&self, input: Format, output: Format) -> bool {
        input == Format::Svg && output.category() == FileCategory::Image
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
                stage: "Rendering SVG…".into(),
            },
        );
        convert_svg(
            &request.input_path,
            &request.output_path,
            request.output_format,
            cancel_token,
        )
        .await
    }
}

pub(crate) async fn convert_svg(
    input: &Path,
    output: &Path,
    output_format: Format,
    cancel_token: CancellationToken,
) -> Result<ConversionResult, ConversionError> {
    if output_format == Format::Png {
        render_to_png(input, output).await?;
        if cancel_token.is_cancelled() {
            super::cleanup_partial(output);
            return Err(ConversionError::Cancelled);
        }
        let output_size = verification::file_with_signature(output, b"\x89PNG\r\n\x1a\n")?;
        return Ok(ConversionResult {
            output_path: output.to_string_lossy().into(),
            output_paths: Vec::new(),
            output_size,
            duration_ms: 0,
            undo_manifest: None,
        });
    }

    let workspace = tempfile::Builder::new()
        .prefix("convertkit-svg-")
        .tempdir()
        .map_err(|error| ConversionError::ProcessFailed {
            message: format!("Could not create an SVG workspace: {error}"),
            stderr: String::new(),
            exit_code: None,
        })?;
    let rendered = workspace.path().join("rendered.png");
    render_to_png(input, &rendered).await?;
    imagemagick::convert_image(&rendered, output, Format::Png, output_format, cancel_token).await
}

/// System fonts are indexed once per launch; SVG text needs them and loading
/// them is the slowest part of a small render.
fn font_database() -> Arc<::resvg::usvg::fontdb::Database> {
    static FONTS: OnceLock<Arc<::resvg::usvg::fontdb::Database>> = OnceLock::new();
    FONTS
        .get_or_init(|| {
            let mut database = ::resvg::usvg::fontdb::Database::new();
            database.load_system_fonts();
            Arc::new(database)
        })
        .clone()
}

async fn render_to_png(input: &Path, output: &Path) -> Result<(), ConversionError> {
    let input = input.to_path_buf();
    let output = output.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let png = render_svg(&input)?;
        std::fs::write(&output, png).map_err(|error| {
            super::cleanup_partial(&output);
            render_error(format!("Could not save the rendered SVG: {error}"))
        })
    })
    .await
    .map_err(|error| render_error(format!("The SVG renderer stopped unexpectedly: {error}")))?
}

fn render_svg(input: &Path) -> Result<Vec<u8>, ConversionError> {
    let data = std::fs::read(input).map_err(|_| ConversionError::InputNotFound {
        path: input.to_string_lossy().into(),
    })?;
    // 192 DPI matches the resvg CLI settings ConvertKit used before: absolute
    // units (in, mm, pt) render at Retina density, pixel sizes stay as drawn.
    let options = ::resvg::usvg::Options {
        resources_dir: input.parent().map(Path::to_path_buf),
        dpi: 192.0,
        fontdb: font_database(),
        ..::resvg::usvg::Options::default()
    };
    let tree = ::resvg::usvg::Tree::from_data(&data, &options)
        .map_err(|error| render_error(format!("This SVG could not be parsed: {error}")))?;
    let size = tree.size().to_int_size();
    let (width, height) = (size.width(), size.height());
    if width > MAX_EDGE || height > MAX_EDGE || u64::from(width) * u64::from(height) > MAX_PIXELS {
        return Err(ConversionError::UnsupportedConversion {
            input: format!("This SVG renders at {width} × {height} px"),
            output: format!("an image of at most {MAX_EDGE} px per side"),
        });
    }
    let mut pixmap = ::resvg::tiny_skia::Pixmap::new(width, height)
        .ok_or_else(|| render_error("The SVG canvas could not be allocated".into()))?;
    ::resvg::render(
        &tree,
        ::resvg::tiny_skia::Transform::default(),
        &mut pixmap.as_mut(),
    );
    pixmap
        .encode_png()
        .map_err(|error| render_error(format!("The rendered SVG could not be encoded: {error}")))
}

fn render_error(message: String) -> ConversionError {
    ConversionError::ProcessFailed {
        message,
        stderr: String::new(),
        exit_code: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="120" height="80"><rect width="120" height="80" fill="#25334f"/><text x="10" y="50" font-size="30" fill="white">Hi</text></svg>"##;

    #[test]
    fn renders_pixel_and_absolute_units_like_the_cli() {
        let directory = tempfile::tempdir().expect("tempdir");
        for (name, svg, expected) in [
            ("pixels.svg", SVG, (120, 80)),
            (
                "inches.svg",
                r#"<svg xmlns="http://www.w3.org/2000/svg" width="1in" height="0.5in"><rect width="100%" height="100%" fill="red"/></svg>"#,
                (192, 96),
            ),
        ] {
            let path = directory.path().join(name);
            std::fs::write(&path, svg).expect("svg fixture");
            let png = render_svg(&path).expect("render");
            assert_eq!(
                crate::engines::codecs::png_dimensions(&png),
                Some(expected),
                "{name}"
            );
        }
    }

    #[test]
    fn rejects_malformed_and_oversized_svgs() {
        let directory = tempfile::tempdir().expect("tempdir");
        let broken = directory.path().join("broken.svg");
        std::fs::write(&broken, "<svg").expect("fixture");
        assert!(render_svg(&broken).is_err());
        let huge = directory.path().join("huge.svg");
        std::fs::write(
            &huge,
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="40000" height="10"/>"#,
        )
        .expect("fixture");
        assert!(matches!(
            render_svg(&huge),
            Err(ConversionError::UnsupportedConversion { .. })
        ));
    }

    #[tokio::test]
    async fn converts_svg_to_every_raster_target() {
        if !crate::engines::imageio::can_write(Format::Png) {
            return;
        }
        let directory = tempfile::tempdir().expect("tempdir");
        let input = directory.path().join("source.svg");
        std::fs::write(&input, SVG).expect("svg fixture");
        for format in [
            Format::Png,
            Format::Jpg,
            Format::WebP,
            Format::Tiff,
            Format::Bmp,
            Format::Gif,
            Format::Ico,
            Format::Avif,
            Format::Heic,
        ] {
            if !crate::engines::imageio::can_write(format) {
                continue;
            }
            let output = directory
                .path()
                .join(format!("converted.{}", format.extension()));
            convert_svg(&input, &output, format, CancellationToken::new())
                .await
                .unwrap_or_else(|error| panic!("svg to {}: {error}", format.label()));
            let structure =
                verification::inspect_image_structure(&output, CancellationToken::new())
                    .await
                    .expect("inspect");
            assert_eq!(
                structure.frames,
                vec![verification::ImageFrameStructure {
                    width: 120,
                    height: 80
                }],
                "{}",
                format.label()
            );
        }
    }
}
