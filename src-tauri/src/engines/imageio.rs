//! Bundled ImageIO helper plus in-process encoders. Together they cover the
//! raster work that previously required a Homebrew ImageMagick install:
//! macOS ImageIO decodes and writes most formats, libwebp (statically linked
//! through the `webp` crate) writes WebP, and oxipng recompresses PNG.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

use tauri::{AppHandle, Emitter};
use tokio_util::sync::CancellationToken;

use crate::engines::process::{run_process, run_process_with_output, ProcessMessages};
use crate::engines::{
    codecs, imagemagick, resolve_tool, tool_command, ConversionEngine, ConversionRequest,
    ConversionResult,
};
use crate::error::ConversionError;
use crate::formats::Format;
use crate::progress::ProgressPayload;

pub const HELPER: &str = "convertkit-imageio";

const HELPER_TIMEOUT: Duration = Duration::from_secs(10 * 60);
const IDENTIFY_TIMEOUT: Duration = Duration::from_secs(60);
const MAX_IDENTIFY_BYTES: usize = 1024 * 1024;
const MAX_THUMBNAIL_BYTES: usize = 16 * 1024 * 1024;

struct Capabilities {
    readable: HashSet<String>,
    writable: HashSet<String>,
}

/// Which type identifiers this Mac's ImageIO can read and write. AVIF and
/// HEIC encoders depend on the macOS release, so this is probed rather than
/// assumed. Only a successful probe is cached, so a helper built after launch
/// is picked up on the next check.
fn capabilities() -> Option<&'static Capabilities> {
    static CACHE: OnceLock<Capabilities> = OnceLock::new();
    if let Some(cached) = CACHE.get() {
        return Some(cached);
    }
    let executable = resolve_tool(HELPER)?;
    let output = std::process::Command::new(executable)
        .arg("types")
        .output()
        .ok()
        .filter(|output| output.status.success())?;
    let mut capabilities = Capabilities {
        readable: HashSet::new(),
        writable: HashSet::new(),
    };
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        match line.split_once('\t') {
            Some(("read", uti)) => capabilities.readable.insert(uti.to_owned()),
            Some(("write", uti)) => capabilities.writable.insert(uti.to_owned()),
            _ => false,
        };
    }
    Some(CACHE.get_or_init(|| capabilities))
}

fn type_identifier(format: Format) -> Option<&'static str> {
    Some(match format {
        Format::Jpg => "public.jpeg",
        Format::Png => "public.png",
        Format::WebP => "org.webmproject.webp",
        Format::Tiff => "public.tiff",
        Format::Bmp => "com.microsoft.bmp",
        Format::Gif => "com.compuserve.gif",
        Format::Ico => "com.microsoft.ico",
        Format::Avif => "public.avif",
        Format::Heic => "public.heic",
        _ => return None,
    })
}

pub fn can_read(format: Format) -> bool {
    let (Some(capabilities), Some(uti)) = (capabilities(), type_identifier(format)) else {
        return false;
    };
    capabilities.readable.contains(uti)
}

pub fn can_write(format: Format) -> bool {
    // WebP and ICO are assembled in process from PNGs the helper renders:
    // ImageIO cannot write WebP and only writes square, standard-size icons.
    let target = if matches!(format, Format::WebP | Format::Ico) {
        Format::Png
    } else {
        format
    };
    let (Some(capabilities), Some(uti)) = (capabilities(), type_identifier(target)) else {
        return false;
    };
    capabilities.writable.contains(uti)
}

/// Whether this backend can turn `input` into `output`. Every frame layout is
/// handled in process, so only the formats this Mac decodes and encodes matter.
pub fn handles(input: Format, output: Format, _frames: usize) -> bool {
    can_read(input) && can_write(output)
}

/// Raster-to-raster conversion through the bundled helper. It shares the
/// verified conversion path with [`imagemagick::ImageMagickEngine`], which
/// remains the fallback for SVG sources and animated WebP output.
pub struct ImageIoEngine;

impl ImageIoEngine {
    /// Whether this Mac's ImageIO can read `input` and write `output`.
    pub fn is_available_for(&self, input: Format, output: Format) -> bool {
        can_read(input) && can_write(output)
    }
}

impl ConversionEngine for ImageIoEngine {
    fn required_tool(&self) -> &'static str {
        HELPER
    }

    fn supports(&self, input: Format, output: Format) -> bool {
        type_identifier(input).is_some() && type_identifier(output).is_some() && input != output
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
                stage: "Converting image".to_string(),
            },
        );
        imagemagick::convert_image(
            &request.input_path,
            &request.output_path,
            request.input_format,
            request.output_format,
            cancel_token,
        )
        .await
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resize {
    /// Fit inside the box, enlarging if needed (ImageMagick `WxH`).
    Fit(u32, u32),
    /// Exactly this size (ImageMagick `WxH!`).
    Exact(u32, u32),
    /// Fit inside the box, never enlarging (ImageMagick `WxH>`).
    Shrink(u32, u32),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImageOptions {
    pub quality: Option<u8>,
    pub resize: Option<Resize>,
    /// Drop metadata. Orientation is kept unless `orient` bakes it in.
    pub strip: bool,
    /// Apply EXIF orientation to the pixels.
    pub orient: bool,
    pub srgb: bool,
    /// Composite onto white and drop alpha.
    pub flatten: bool,
    pub lzw: bool,
    /// libwebp effort, 0 (fast) to 6 (smallest).
    pub webp_method: Option<u8>,
}

impl ImageOptions {
    fn helper_arguments(&self) -> Vec<String> {
        let mut arguments = Vec::new();
        if let Some(quality) = self.quality {
            arguments.extend(["--quality".into(), quality.to_string()]);
        }
        if let Some(resize) = self.resize {
            let (flag, width, height) = match resize {
                Resize::Fit(width, height) => ("--fit", width, height),
                Resize::Exact(width, height) => ("--exact", width, height),
                Resize::Shrink(width, height) => ("--shrink", width, height),
            };
            arguments.extend([flag.into(), width.to_string(), height.to_string()]);
        }
        for (enabled, flag) in [
            (self.strip, "--strip"),
            (self.orient, "--orient"),
            (self.srgb, "--srgb"),
            (self.flatten, "--flatten"),
            (self.lzw, "--lzw"),
        ] {
            if enabled {
                arguments.push(flag.into());
            }
        }
        arguments
    }
}

pub fn missing_helper() -> ConversionError {
    ConversionError::MissingDependency {
        tool: HELPER.into(),
        install_hint: "Run `pnpm native:helpers` to build the bundled image helper".into(),
    }
}

/// Write `input` to `output` as `format`. The helper rejects formats this Mac
/// cannot encode; callers check [`handles`] first to pick a backend.
pub async fn convert(
    input: &Path,
    output: &Path,
    format: Format,
    options: &ImageOptions,
    cancel_token: CancellationToken,
) -> Result<(), ConversionError> {
    if resolve_tool(HELPER).is_none() {
        return Err(missing_helper());
    }
    let workspace = tempfile::Builder::new()
        .prefix("convertkit-image-")
        .tempdir()
        .map_err(workspace_error)?;
    let source = decodable_source(input, workspace.path(), cancel_token.clone()).await?;
    let encoded = match format {
        Format::Jpg | Format::WebP => {
            let prepared =
                prepare_source(input, &source, workspace.path(), format, options, &cancel_token)
                    .await?;
            let quality = options.quality;
            blocking(move || prepared.encode(quality)).await?
        }
        Format::Ico => encode_ico_output(&source, workspace.path(), options, &cancel_token).await?,
        _ => {
            let uti =
                type_identifier(format).ok_or_else(|| ConversionError::UnsupportedConversion {
                    input: input.to_string_lossy().into(),
                    output: format.label().into(),
                })?;
            return run_convert(
                &source,
                output,
                uti,
                options.helper_arguments(),
                cancel_token,
            )
            .await;
        }
    };
    if cancel_token.is_cancelled() {
        return Err(ConversionError::Cancelled);
    }
    std::fs::write(output, encoded).map_err(|error| {
        super::cleanup_partial(output);
        ConversionError::ProcessFailed {
            message: format!("Could not save the {} image: {error}", format.label()),
            stderr: String::new(),
            exit_code: None,
        }
    })
}

/// ImageIO misreads icons holding several entries of the same width and some
/// bitmap entries. Icons are therefore unpacked here and rejoined as a
/// multi-page TIFF, which keeps every entry's own size.
async fn decodable_source(
    input: &Path,
    workspace: &Path,
    cancel_token: CancellationToken,
) -> Result<PathBuf, ConversionError> {
    let Some(entries) = std::fs::read(input)
        .ok()
        .filter(|data| data.starts_with(&[0, 0, 1, 0]))
        .and_then(|data| codecs::split_ico(&data))
    else {
        return Ok(input.to_path_buf());
    };
    let mut command = tool_command(HELPER);
    let assembled = workspace.join("icon-entries.tiff");
    command.arg("assemble").arg(&assembled).arg("public.tiff");
    for (index, entry) in entries.iter().enumerate() {
        let path = workspace.join(format!("icon-entry-{index}.png"));
        std::fs::write(&path, entry).map_err(|error| ConversionError::ProcessFailed {
            message: format!("Could not unpack the icon: {error}"),
            stderr: String::new(),
            exit_code: None,
        })?;
        command.arg(path);
    }
    run_process(
        command,
        cancel_token,
        HELPER_TIMEOUT,
        &[&assembled],
        ProcessMessages {
            start: "Failed to start the image helper",
            wait: "The image helper could not be awaited",
            failure: "The icon could not be unpacked",
        },
    )
    .await?;
    Ok(assembled)
}

/// The first frame of `input` as 8-bit sRGB RGBA, for in-process tools such
/// as the vector tracer.
pub async fn render_first_frame(
    input: &Path,
    cancel_token: CancellationToken,
) -> Result<codecs::RgbaFrame, ConversionError> {
    if resolve_tool(HELPER).is_none() {
        return Err(missing_helper());
    }
    let workspace = tempfile::Builder::new()
        .prefix("convertkit-frame-")
        .tempdir()
        .map_err(workspace_error)?;
    let source = decodable_source(input, workspace.path(), cancel_token.clone()).await?;
    let mut frames = render_pixels(
        &source,
        workspace.path(),
        &ImageOptions::default(),
        Some(0),
        &cancel_token,
    )
    .await?;
    Ok(frames.swap_remove(0))
}

async fn run_convert(
    input: &Path,
    output: &Path,
    uti: &str,
    arguments: Vec<String>,
    cancel_token: CancellationToken,
) -> Result<(), ConversionError> {
    let mut command = tool_command(HELPER);
    command
        .arg("convert")
        .arg(input)
        .arg(output)
        .arg(uti)
        .args(arguments);
    run_process(
        command,
        cancel_token,
        HELPER_TIMEOUT,
        &[output],
        ProcessMessages {
            start: "Failed to start the image helper",
            wait: "The image helper could not be awaited",
            failure: "Image conversion failed",
        },
    )
    .await
}

/// Every frame of `source` (or just `frame`) as 8-bit sRGB RGBA with the
/// requested pixel operations applied, ready for an in-process encoder. The
/// helper writes raw pixels in one pass; an intermediate PNG per frame cost
/// most of a second per 12 MP frame to encode and decode again.
async fn render_pixels(
    source: &Path,
    workspace: &Path,
    options: &ImageOptions,
    frame: Option<usize>,
    cancel_token: &CancellationToken,
) -> Result<Vec<codecs::RgbaFrame>, ConversionError> {
    let output = workspace.join("frames.pixels");
    let mut command = tool_command(HELPER);
    command.arg("pixels").arg(source).arg(&output).args(
        ImageOptions {
            quality: None,
            lzw: false,
            webp_method: None,
            ..options.clone()
        }
        .helper_arguments(),
    );
    if let Some(frame) = frame {
        command.args(["--frame".to_owned(), frame.to_string()]);
    }
    run_process(
        command,
        cancel_token.clone(),
        HELPER_TIMEOUT,
        &[&output],
        ProcessMessages {
            start: "Failed to start the image helper",
            wait: "The image helper could not be awaited",
            failure: "Image conversion failed",
        },
    )
    .await?;
    blocking(move || codecs::read_pixels(&output)).await
}

/// Render every frame of `source` as a lossless 8-bit RGBA PNG with the
/// requested pixel operations applied, for containers that embed PNG.
async fn render_png_frames(
    source: &Path,
    workspace: &Path,
    options: &ImageOptions,
    cancel_token: &CancellationToken,
) -> Result<Vec<PathBuf>, ConversionError> {
    let structure = crate::engines::verification::parse_image_structure(
        &identify(source, cancel_token.clone()).await?,
    )?;
    let mut arguments = ImageOptions {
        quality: None,
        lzw: false,
        webp_method: None,
        ..options.clone()
    }
    .helper_arguments();
    arguments.push("--rgba8".into());
    let mut frames = Vec::with_capacity(structure.frames.len());
    for index in 0..structure.frames.len() {
        let frame = workspace.join(format!("frame-{index}.png"));
        let mut frame_arguments = arguments.clone();
        frame_arguments.extend(["--frame".into(), index.to_string()]);
        run_convert(
            source,
            &frame,
            "public.png",
            frame_arguments,
            cancel_token.clone(),
        )
        .await?;
        frames.push(frame);
    }
    Ok(frames)
}

async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, ConversionError> + Send + 'static,
) -> Result<T, ConversionError> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|error| ConversionError::ProcessFailed {
            message: format!("The image encoder stopped unexpectedly: {error}"),
            stderr: String::new(),
            exit_code: None,
        })?
}

struct Timing {
    loop_count: i32,
    durations_ms: Vec<u32>,
}

async fn timing(source: &Path, cancel_token: CancellationToken) -> Result<Timing, ConversionError> {
    let mut command = tool_command(HELPER);
    command.arg("timing").arg(source);
    let output = run_process_with_output(
        command,
        cancel_token,
        IDENTIFY_TIMEOUT,
        MAX_IDENTIFY_BYTES,
        &[],
        ProcessMessages {
            start: "Failed to start the image helper",
            wait: "The image helper could not be awaited",
            failure: "Animation timing could not be read",
        },
    )
    .await?;
    let text = String::from_utf8_lossy(&output);
    let mut lines = text.lines();
    let loop_count = lines
        .next()
        .and_then(|line| line.strip_prefix("loop "))
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(0);
    let durations_ms = lines
        .filter_map(|line| line.trim().parse().ok())
        .collect::<Vec<u32>>();
    Ok(Timing {
        loop_count,
        durations_ms,
    })
}

/// A source decoded once for the in-process JPEG and WebP encoders, so
/// several qualities can be tried without rendering it again.
pub struct PreparedImage {
    frames: Vec<codecs::RgbaFrame>,
    kind: PreparedKind,
}

enum PreparedKind {
    Jpeg {
        segments: Vec<codecs::AppSegment>,
    },
    WebP {
        timing: Option<Timing>,
        metadata_source: Option<Vec<u8>>,
        method: u8,
    },
}

impl PreparedImage {
    /// Encode at `quality`; JPEG defaults to 90 and WebP to lossless.
    pub fn encode(&self, quality: Option<u8>) -> Result<Vec<u8>, ConversionError> {
        match &self.kind {
            PreparedKind::Jpeg { segments } => {
                codecs::encode_jpeg(&self.frames[0], quality.unwrap_or(90), segments)
            }
            PreparedKind::WebP {
                timing,
                metadata_source,
                method,
            } => {
                let encoded = match timing {
                    Some(timing) => {
                        let mut durations = timing.durations_ms.clone();
                        durations.resize(self.frames.len(), 100);
                        codecs::encode_webp_animation(
                            &self.frames,
                            &durations,
                            timing.loop_count,
                            quality,
                            *method,
                        )?
                    }
                    None => codecs::encode_webp(&self.frames[0], quality, *method)?,
                };
                Ok(match metadata_source {
                    Some(source) => codecs::copy_webp_metadata(
                        &encoded,
                        source,
                        self.frames[0].width,
                        self.frames[0].height,
                    ),
                    None => encoded,
                })
            }
        }
    }
}

/// Decode `input` for repeated JPEG or WebP encoding with `options` (its
/// quality is ignored). Other formats return `None`.
pub async fn prepare(
    input: &Path,
    format: Format,
    options: &ImageOptions,
    cancel_token: &CancellationToken,
) -> Result<Option<PreparedImage>, ConversionError> {
    if !matches!(format, Format::Jpg | Format::WebP) {
        return Ok(None);
    }
    if resolve_tool(HELPER).is_none() {
        return Err(missing_helper());
    }
    let workspace = tempfile::Builder::new()
        .prefix("convertkit-image-")
        .tempdir()
        .map_err(workspace_error)?;
    let source = decodable_source(input, workspace.path(), cancel_token.clone()).await?;
    prepare_source(input, &source, workspace.path(), format, options, cancel_token)
        .await
        .map(Some)
}

async fn prepare_source(
    input: &Path,
    source: &Path,
    workspace: &Path,
    format: Format,
    options: &ImageOptions,
    cancel_token: &CancellationToken,
) -> Result<PreparedImage, ConversionError> {
    if format == Format::WebP {
        let frames = render_pixels(source, workspace, options, None, cancel_token).await?;
        let timing = if frames.len() > 1 {
            Some(timing(source, cancel_token.clone()).await?)
        } else {
            None
        };
        let metadata_source = (!options.strip)
            .then(|| std::fs::read(input).ok())
            .flatten();
        return Ok(PreparedImage {
            frames,
            kind: PreparedKind::WebP {
                timing,
                metadata_source,
                method: options.webp_method.unwrap_or(4),
            },
        });
    }

    let flattened = ImageOptions {
        flatten: true,
        ..options.clone()
    };
    let frames = render_pixels(source, workspace, &flattened, None, cancel_token).await?;
    if frames.len() != 1 {
        return Err(ConversionError::UnsupportedConversion {
            input: format!("This image contains {} frames or pages", frames.len()),
            output: "a single JPEG file without discarding frames".into(),
        });
    }
    // Kept EXIF already carries the source orientation; stripped output
    // needs a minimal EXIF block so the photo still displays upright.
    let mut segments = if options.strip {
        Vec::new()
    } else {
        std::fs::read(input)
            .map(|data| codecs::jpeg_metadata_segments(&data))
            .unwrap_or_default()
    };
    let carries_exif = segments
        .iter()
        .any(|segment| segment.marker == 1 && segment.data.starts_with(b"Exif\0\0"));
    if options.orient {
        for segment in &mut segments {
            codecs::set_exif_orientation(&mut segment.data, 1);
        }
    } else if !carries_exif {
        let orientation = orientation(input, cancel_token.clone()).await?;
        if orientation != 1 {
            segments.insert(
                0,
                codecs::AppSegment {
                    marker: 1,
                    data: codecs::orientation_exif(orientation),
                },
            );
        }
    }
    Ok(PreparedImage {
        frames,
        kind: PreparedKind::Jpeg { segments },
    })
}

async fn encode_ico_output(
    source: &Path,
    workspace: &Path,
    options: &ImageOptions,
    cancel_token: &CancellationToken,
) -> Result<Vec<u8>, ConversionError> {
    let paths = render_png_frames(source, workspace, options, cancel_token).await?;
    let frames = paths
        .iter()
        .map(|path| std::fs::read(path).map_err(|_| ConversionError::OutputMissing))
        .collect::<Result<Vec<_>, _>>()?;
    codecs::encode_ico(&frames)
}

async fn orientation(path: &Path, cancel_token: CancellationToken) -> Result<u8, ConversionError> {
    let mut command = tool_command(HELPER);
    command.arg("orientation").arg(path);
    let output = run_process_with_output(
        command,
        cancel_token,
        IDENTIFY_TIMEOUT,
        16,
        &[],
        ProcessMessages {
            start: "Failed to start the image helper",
            wait: "The image helper could not be awaited",
            failure: "Image orientation could not be read",
        },
    )
    .await?;
    Ok(std::str::from_utf8(&output)
        .ok()
        .and_then(|value| value.trim().parse::<u8>().ok())
        .filter(|value| (1..=8).contains(value))
        .unwrap_or(1))
}

fn workspace_error(error: std::io::Error) -> ConversionError {
    ConversionError::ProcessFailed {
        message: format!("Could not create an image workspace: {error}"),
        stderr: String::new(),
        exit_code: None,
    }
}

/// Losslessly recompress a PNG in place with oxipng.
pub async fn optimize_png(path: &Path, strip: bool) -> Result<(), ConversionError> {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let data = std::fs::read(&path).map_err(|error| png_error(&error))?;
        let mut options = oxipng::Options::from_preset(4);
        options.strip = if strip {
            oxipng::StripChunks::Safe
        } else {
            oxipng::StripChunks::None
        };
        let optimized =
            oxipng::optimize_from_memory(&data, &options).map_err(|error| png_error(&error))?;
        std::fs::write(&path, optimized).map_err(|error| png_error(&error))
    })
    .await
    .map_err(|error| png_error(&error))?
}

fn png_error(error: &dyn std::fmt::Display) -> ConversionError {
    ConversionError::ProcessFailed {
        message: "PNG recompression failed".into(),
        stderr: error.to_string(),
        exit_code: None,
    }
}

/// `FORMAT|width|height|frames` per frame, the same summary `magick identify`
/// prints for the verification parser.
pub async fn identify(
    path: &Path,
    cancel_token: CancellationToken,
) -> Result<Vec<u8>, ConversionError> {
    let mut command = tool_command(HELPER);
    command.arg("identify").arg(path);
    run_process_with_output(
        command,
        cancel_token,
        IDENTIFY_TIMEOUT,
        MAX_IDENTIFY_BYTES,
        &[],
        ProcessMessages {
            start: "Failed to start image verification",
            wait: "Image verification could not be awaited",
            failure: "Image verification failed",
        },
    )
    .await
}

/// A PNG thumbnail whose longest edge is at most `max_edge`, with EXIF
/// orientation applied.
pub async fn thumbnail(path: &Path, max_edge: u32) -> Result<Vec<u8>, ConversionError> {
    let workspace = tempfile::Builder::new()
        .prefix("convertkit-thumbnail-")
        .tempdir()
        .map_err(|error| ConversionError::ProcessFailed {
            message: format!("Could not create a thumbnail workspace: {error}"),
            stderr: String::new(),
            exit_code: None,
        })?;
    let output = workspace.path().join("thumbnail.png");
    let mut command = tool_command(HELPER);
    command
        .arg("thumbnail")
        .arg(path)
        .arg(&output)
        .arg(max_edge.to_string());
    run_process(
        command,
        CancellationToken::new(),
        IDENTIFY_TIMEOUT,
        &[&output],
        ProcessMessages {
            start: "Failed to start the thumbnail helper",
            wait: "The thumbnail helper could not be awaited",
            failure: "Thumbnail generation failed",
        },
    )
    .await?;
    let bytes = std::fs::read(&output).map_err(|_| ConversionError::OutputMissing)?;
    if bytes.is_empty() || bytes.len() > MAX_THUMBNAIL_BYTES {
        return Err(ConversionError::OutputMissing);
    }
    Ok(bytes)
}

#[cfg(test)]
pub(crate) mod fixtures {
    use std::path::Path;

    /// A solid RGBA PNG, written without any external tool.
    pub fn png(path: &Path, width: u32, height: u32, rgba: [u8; 4]) {
        let file = std::fs::File::create(path).expect("png fixture");
        let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().expect("png header");
        let pixels = rgba.repeat((width * height) as usize);
        writer.write_image_data(&pixels).expect("png data");
    }

    /// A two-frame GIF whose frames have different sizes, mirroring the
    /// multiframe ImageMagick fixtures.
    pub fn multiframe_gif(path: &Path) {
        let file = std::fs::File::create(path).expect("gif fixture");
        let palette = [0x25, 0x33, 0x4f, 0x90, 0xa8, 0xe8];
        let mut encoder = gif::Encoder::new(file, 40, 30, &palette).expect("gif encoder");
        for (width, height, index) in [(40u16, 30u16, 0u8), (20, 10, 1)] {
            let mut frame = gif::Frame::from_indexed_pixels(
                width,
                height,
                vec![index; usize::from(width) * usize::from(height)],
                None,
            );
            frame.delay = 10;
            encoder.write_frame(&frame).expect("gif frame");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engines::verification::{parse_image_structure, ImageFrameStructure};

    fn helper_ready() -> bool {
        capabilities().is_some()
    }

    async fn structure(path: &Path) -> crate::engines::verification::ImageStructure {
        let summary = identify(path, CancellationToken::new())
            .await
            .expect("identify");
        parse_image_structure(&summary).expect("parse")
    }

    #[test]
    fn builds_helper_arguments_in_a_stable_order() {
        let options = ImageOptions {
            quality: Some(82),
            resize: Some(Resize::Shrink(1920, 1920)),
            strip: true,
            orient: true,
            srgb: true,
            flatten: true,
            lzw: false,
            webp_method: Some(6),
        };
        assert_eq!(
            options.helper_arguments(),
            [
                "--quality",
                "82",
                "--shrink",
                "1920",
                "1920",
                "--strip",
                "--orient",
                "--srgb",
                "--flatten",
            ]
        );
    }

    #[test]
    fn handles_animated_and_multi_entry_sources() {
        if !helper_ready() {
            return;
        }
        assert!(handles(Format::Png, Format::WebP, 1));
        assert!(handles(Format::Gif, Format::WebP, 2));
        assert!(handles(Format::Ico, Format::Tiff, 3));
    }

    #[tokio::test]
    async fn converts_every_writable_format_without_changing_geometry() {
        if !helper_ready() {
            return;
        }
        let directory = tempfile::tempdir().expect("tempdir");
        let input = directory.path().join("source.png");
        fixtures::png(&input, 37, 23, [0x25, 0x33, 0x4f, 0x80]);
        for (format, name) in [
            (Format::Jpg, "JPEG"),
            (Format::WebP, "WEBP"),
            (Format::Tiff, "TIFF"),
            (Format::Bmp, "BMP"),
            (Format::Gif, "GIF"),
            (Format::Ico, "ICO"),
            (Format::Avif, "AVIF"),
            (Format::Heic, "HEIC"),
        ] {
            if !can_write(format) {
                continue;
            }
            let output = directory
                .path()
                .join(format!("converted.{}", format.extension()));
            convert(
                &input,
                &output,
                format,
                &ImageOptions {
                    quality: Some(85),
                    strip: true,
                    ..ImageOptions::default()
                },
                CancellationToken::new(),
            )
            .await
            .unwrap_or_else(|error| panic!("convert to {name}: {error}"));
            let written = structure(&output).await;
            assert_eq!(written.magick_format, name);
            assert_eq!(
                written.frames,
                vec![ImageFrameStructure {
                    width: 37,
                    height: 23
                }],
                "{name} geometry"
            );
        }
    }

    #[tokio::test]
    async fn resizes_with_imagemagick_geometry_rounding() {
        if !helper_ready() {
            return;
        }
        let directory = tempfile::tempdir().expect("tempdir");
        let input = directory.path().join("source.png");
        fixtures::png(&input, 4, 3, [0, 0, 0, 255]);
        for (resize, expected) in [
            (Resize::Fit(2, 2), (2, 2)),
            (Resize::Fit(8, 8), (8, 6)),
            (Resize::Exact(5, 9), (5, 9)),
            (Resize::Shrink(8, 8), (4, 3)),
            (Resize::Shrink(2, 2), (2, 2)),
        ] {
            let output = directory.path().join("resized.png");
            convert(
                &input,
                &output,
                Format::Png,
                &ImageOptions {
                    resize: Some(resize),
                    ..ImageOptions::default()
                },
                CancellationToken::new(),
            )
            .await
            .expect("resize");
            assert_eq!(
                structure(&output).await.frames,
                vec![ImageFrameStructure {
                    width: expected.0,
                    height: expected.1
                }],
                "{resize:?}"
            );
        }
    }

    #[tokio::test]
    async fn preserves_multiframe_topology_including_animated_webp() {
        if !helper_ready() {
            return;
        }
        let directory = tempfile::tempdir().expect("tempdir");
        let input = directory.path().join("source.gif");
        fixtures::multiframe_gif(&input);
        let source = structure(&input).await;
        assert_eq!(source.magick_format, "GIF");
        assert_eq!(source.frames.len(), 2);

        for format in [Format::Tiff, Format::WebP, Format::Gif] {
            let output = directory
                .path()
                .join(format!("converted.{}", format.extension()));
            convert(
                &input,
                &output,
                format,
                &ImageOptions {
                    quality: Some(80),
                    ..ImageOptions::default()
                },
                CancellationToken::new(),
            )
            .await
            .unwrap_or_else(|error| panic!("gif to {}: {error}", format.label()));
            assert_eq!(
                structure(&output).await.frames,
                source.frames,
                "{}",
                format.label()
            );
        }
        let timing = timing(
            &directory.path().join("converted.webp"),
            CancellationToken::new(),
        )
        .await
        .expect("webp timing");
        assert_eq!(timing.durations_ms, vec![100, 100], "GIF delays survive");
    }

    #[tokio::test]
    async fn unpacks_multi_entry_icons_that_imageio_misreads() {
        if !helper_ready() {
            return;
        }
        let directory = tempfile::tempdir().expect("tempdir");
        let mut entries = Vec::new();
        for (index, (width, height)) in [(20, 15), (20, 10), (16, 16)].into_iter().enumerate() {
            let path = directory.path().join(format!("entry-{index}.png"));
            fixtures::png(&path, width, height, [0x90, 0xa8, 0xe8, 0xff]);
            entries.push(std::fs::read(path).expect("entry"));
        }
        let input = directory.path().join("source.ico");
        std::fs::write(&input, codecs::encode_ico(&entries).expect("icon")).expect("write icon");

        let output = directory.path().join("pages.tiff");
        convert(
            &input,
            &output,
            Format::Tiff,
            &ImageOptions::default(),
            CancellationToken::new(),
        )
        .await
        .expect("icon to tiff");
        let pages = structure(&output).await;
        assert_eq!(
            pages.frames,
            [(20, 15), (20, 10), (16, 16)]
                .map(|(width, height)| ImageFrameStructure { width, height })
                .to_vec()
        );
    }

    #[tokio::test]
    async fn jpeg_output_uses_4_2_0_and_keeps_orientation_after_stripping() {
        if !helper_ready() {
            return;
        }
        let directory = tempfile::tempdir().expect("tempdir");
        let png = directory.path().join("source.png");
        fixtures::png(&png, 64, 32, [0x75, 0x80, 0xaa, 0xff]);
        let plain = directory.path().join("plain.jpg");
        convert(
            &png,
            &plain,
            Format::Jpg,
            &ImageOptions {
                quality: Some(90),
                strip: true,
                ..ImageOptions::default()
            },
            CancellationToken::new(),
        )
        .await
        .expect("encode jpeg");

        // Splice an orientation=6 EXIF block into the plain JPEG.
        let bytes = std::fs::read(&plain).expect("jpeg bytes");
        let exif = codecs::orientation_exif(6);
        let mut rotated = bytes[..2].to_vec();
        rotated.extend([0xff, 0xe1]);
        rotated.extend(u16::try_from(exif.len() + 2).expect("length").to_be_bytes());
        rotated.extend(&exif);
        rotated.extend(&bytes[2..]);
        let source = directory.path().join("rotated.jpg");
        std::fs::write(&source, rotated).expect("rotated fixture");
        assert_eq!(
            orientation(&source, CancellationToken::new())
                .await
                .expect("read"),
            6
        );

        let stripped = directory.path().join("stripped.jpg");
        let options = ImageOptions {
            quality: Some(80),
            strip: true,
            ..ImageOptions::default()
        };
        convert(
            &source,
            &stripped,
            Format::Jpg,
            &options,
            CancellationToken::new(),
        )
        .await
        .expect("strip jpeg");
        assert_eq!(
            orientation(&stripped, CancellationToken::new())
                .await
                .expect("read"),
            6
        );
        assert_eq!(
            structure(&stripped).await.frames,
            vec![ImageFrameStructure {
                width: 64,
                height: 32
            }]
        );

        let baked = directory.path().join("baked.jpg");
        let options = ImageOptions {
            orient: true,
            ..options
        };
        convert(
            &source,
            &baked,
            Format::Jpg,
            &options,
            CancellationToken::new(),
        )
        .await
        .expect("bake jpeg");
        assert_eq!(
            orientation(&baked, CancellationToken::new())
                .await
                .expect("read"),
            1
        );
        assert_eq!(
            structure(&baked).await.frames,
            vec![ImageFrameStructure {
                width: 32,
                height: 64
            }]
        );
    }

    #[tokio::test]
    async fn recompresses_png_losslessly_and_thumbnails() {
        if !helper_ready() {
            return;
        }
        let directory = tempfile::tempdir().expect("tempdir");
        let input = directory.path().join("source.png");
        fixtures::png(&input, 400, 300, [10, 20, 30, 255]);
        let before = std::fs::metadata(&input).expect("size").len();
        optimize_png(&input, true).await.expect("optimize");
        assert!(std::fs::metadata(&input).expect("size").len() <= before);
        assert_eq!(
            structure(&input).await.frames,
            vec![ImageFrameStructure {
                width: 400,
                height: 300
            }]
        );

        let preview = thumbnail(&input, 200).await.expect("thumbnail");
        assert!(preview.starts_with(b"\x89PNG"));
    }
}
