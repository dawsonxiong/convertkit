use std::io::Read;
use std::path::Path;
use std::time::Duration;

use tokio_util::sync::CancellationToken;

use crate::engines::process::{run_process_with_output, ProcessMessages};
use crate::error::ConversionError;
use crate::formats::Format;

use super::{cleanup_partial, imageio, resolve_tool, tool_command};

const IMAGE_INSPECTION_TIMEOUT: Duration = Duration::from_secs(60);
const MAX_IMAGE_INSPECTION_BYTES: usize = 1024 * 1024;
const MAX_IMAGE_FRAMES: usize = 10_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageFrameStructure {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageStructure {
    pub magick_format: String,
    pub frames: Vec<ImageFrameStructure>,
}

/// Proves that an output is a regular, non-empty file before it can be exposed.
/// Invalid partial files are removed so every caller gets the same cleanup behavior.
pub fn nonempty_file(path: &Path) -> Result<u64, ConversionError> {
    match std::fs::metadata(path) {
        Ok(metadata) if metadata.is_file() && metadata.len() > 0 => Ok(metadata.len()),
        _ => {
            cleanup_partial(path);
            Err(ConversionError::OutputMissing)
        }
    }
}

/// Extends the baseline file contract with an exact leading byte signature.
pub fn file_with_signature(path: &Path, signature: &[u8]) -> Result<u64, ConversionError> {
    let size = nonempty_file(path)?;
    if signature.is_empty() {
        return Ok(size);
    }

    let mut file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(_) => {
            cleanup_partial(path);
            return Err(ConversionError::OutputMissing);
        }
    };
    let mut header = vec![0u8; signature.len()];
    if file.read_exact(&mut header).is_err() || header != signature {
        cleanup_partial(path);
        return Err(ConversionError::OutputMissing);
    }
    Ok(size)
}

/// Read the logical format and ordered pixel geometry of every image frame or
/// page without decoding the entire raster into application memory.
pub async fn inspect_image_structure(
    path: &Path,
    cancel_token: CancellationToken,
) -> Result<ImageStructure, ConversionError> {
    if cancel_token.is_cancelled() {
        return Err(ConversionError::Cancelled);
    }
    if let Some(structure) = read_ico_structure(path) {
        return Ok(structure);
    }
    // The bundled helper reads every raster format ConvertKit writes. Only
    // sources it cannot decode, such as SVG, need ImageMagick.
    let helper_error = if resolve_tool(imageio::HELPER).is_some() {
        match imageio::identify(path, cancel_token.clone()).await {
            Ok(output) => return parse_image_structure(&output),
            Err(ConversionError::Cancelled) => return Err(ConversionError::Cancelled),
            Err(error) => Some(error),
        }
    } else {
        None
    };
    if resolve_tool("magick").is_none() {
        return Err(helper_error.unwrap_or_else(imageio::missing_helper));
    }
    inspect_with_magick(path, cancel_token).await
}

/// ImageMagick reports animation frames at their own size while ImageIO
/// reports the composited canvas, so files ImageMagick wrote are verified
/// with ImageMagick.
pub async fn inspect_with_magick(
    path: &Path,
    cancel_token: CancellationToken,
) -> Result<ImageStructure, ConversionError> {
    if let Some(structure) = read_ico_structure(path) {
        return Ok(structure);
    }
    let mut command = tool_command("magick");
    command
        .args(["identify", "-ping", "-format", "%m|%w|%h|%n\n"])
        .arg(path);
    let output = run_process_with_output(
        command,
        cancel_token,
        IMAGE_INSPECTION_TIMEOUT,
        MAX_IMAGE_INSPECTION_BYTES,
        &[],
        ProcessMessages {
            start: "Failed to start image verification",
            wait: "Image verification could not be awaited",
            failure: "Image verification failed",
        },
    )
    .await?;
    parse_image_structure(&output)
}

/// ICO directories are read here rather than through ImageIO, whose reader
/// rejects icons holding two entries of the same width.
fn read_ico_structure(path: &Path) -> Option<ImageStructure> {
    let mut header = [0u8; 6];
    let mut file = std::fs::File::open(path).ok()?;
    file.read_exact(&mut header).ok()?;
    if header[..4] != [0, 0, 1, 0] {
        return None;
    }
    let size = file.metadata().ok()?.len();
    if size > 64 * 1024 * 1024 {
        return None;
    }
    let mut data = header.to_vec();
    file.read_to_end(&mut data).ok()?;
    parse_ico_structure(&data)
}

pub(crate) fn parse_ico_structure(data: &[u8]) -> Option<ImageStructure> {
    const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
    let count = usize::from(u16::from_le_bytes([*data.get(4)?, *data.get(5)?]));
    if data.get(..4)? != [0, 0, 1, 0] || count == 0 {
        return None;
    }
    let mut frames = Vec::with_capacity(count);
    for index in 0..count {
        let entry = data.get(6 + index * 16..6 + (index + 1) * 16)?;
        let length = u32::from_le_bytes(entry[8..12].try_into().ok()?) as usize;
        let offset = u32::from_le_bytes(entry[12..16].try_into().ok()?) as usize;
        let payload = data.get(offset..offset.checked_add(length)?)?;
        let (width, height) = if payload.starts_with(PNG_SIGNATURE) {
            let ihdr = payload.get(16..24)?;
            (
                u32::from_be_bytes(ihdr[..4].try_into().ok()?),
                u32::from_be_bytes(ihdr[4..].try_into().ok()?),
            )
        } else {
            // A stored 0 means 256 px.
            let edge = |value: u8| if value == 0 { 256 } else { u32::from(value) };
            (edge(entry[0]), edge(entry[1]))
        };
        if width == 0 || height == 0 {
            return None;
        }
        frames.push(ImageFrameStructure { width, height });
    }
    Some(ImageStructure {
        magick_format: "ICO".into(),
        frames,
    })
}

pub(crate) fn parse_image_structure(output: &[u8]) -> Result<ImageStructure, ConversionError> {
    let summary = std::str::from_utf8(output)
        .map_err(|_| image_verification_error("Image verification returned invalid text", ""))?;
    let lines = summary
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    if lines.is_empty() || lines.len() > MAX_IMAGE_FRAMES {
        return Err(image_verification_error(
            "The image frame count could not be verified",
            summary,
        ));
    }

    let mut magick_format = None;
    let mut frames = Vec::with_capacity(lines.len());
    for line in &lines {
        let values = line.split('|').collect::<Vec<_>>();
        if values.len() != 4 {
            return Err(image_verification_error(
                "The image structure could not be verified",
                summary,
            ));
        }
        let format = values[0].trim().to_ascii_uppercase();
        let width = values[1].trim().parse::<u32>().ok();
        let height = values[2].trim().parse::<u32>().ok();
        let reported_frames = values[3].trim().parse::<usize>().ok();
        let valid_format = !format.is_empty()
            && format.len() <= 32
            && format
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-');
        if !valid_format
            || width.is_none_or(|value| value == 0)
            || height.is_none_or(|value| value == 0)
            || reported_frames != Some(lines.len())
        {
            return Err(image_verification_error(
                "The image structure could not be verified",
                summary,
            ));
        }
        if magick_format
            .as_ref()
            .is_some_and(|expected| expected != &format)
        {
            return Err(image_verification_error(
                "The image contains inconsistent frame formats",
                summary,
            ));
        }
        magick_format.get_or_insert(format);
        frames.push(ImageFrameStructure {
            width: width.expect("checked above"),
            height: height.expect("checked above"),
        });
    }

    Ok(ImageStructure {
        magick_format: magick_format.expect("at least one line"),
        frames,
    })
}

pub fn magick_format_matches(expected: Format, identified: &str) -> bool {
    match expected {
        Format::Jpg => identified == "JPEG",
        Format::Png => identified == "PNG",
        Format::WebP => identified == "WEBP",
        Format::Tiff => identified == "TIFF",
        Format::Bmp => identified == "BMP",
        Format::Gif => identified == "GIF",
        Format::Ico => matches!(identified, "ICO" | "ICON"),
        Format::Avif => identified == "AVIF",
        Format::Heic => matches!(identified, "HEIC" | "HEIF"),
        _ => false,
    }
}

fn image_verification_error(message: &str, stderr: &str) -> ConversionError {
    ConversionError::ProcessFailed {
        message: message.into(),
        stderr: stderr.into(),
        exit_code: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_nonempty_regular_files() {
        let directory = tempfile::tempdir().expect("tempdir");
        let valid = directory.path().join("valid.bin");
        let empty = directory.path().join("empty.bin");
        std::fs::write(&valid, b"data").expect("valid fixture");
        std::fs::write(&empty, []).expect("empty fixture");

        assert_eq!(nonempty_file(&valid).expect("valid output"), 4);
        assert!(nonempty_file(&empty).is_err());
        assert!(!empty.exists());
        assert!(nonempty_file(directory.path()).is_err());
        assert!(nonempty_file(&directory.path().join("missing.bin")).is_err());
    }

    #[test]
    fn enforces_signatures_and_removes_mismatches() {
        let directory = tempfile::tempdir().expect("tempdir");
        let valid = directory.path().join("valid.bin");
        let invalid = directory.path().join("invalid.bin");
        std::fs::write(&valid, b"\x89PNGpayload").expect("valid fixture");
        std::fs::write(&invalid, b"GIFpayload").expect("invalid fixture");

        assert_eq!(
            file_with_signature(&valid, b"\x89PNG").expect("signature"),
            11
        );
        assert!(file_with_signature(&invalid, b"\x89PNG").is_err());
        assert!(!invalid.exists());
    }

    #[test]
    fn reads_icon_directories_including_same_width_entries() {
        let png = |width: u32, height: u32| {
            let mut data = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
            data.extend(width.to_be_bytes());
            data.extend(height.to_be_bytes());
            data
        };
        let frames = [png(20, 15), png(20, 10)];
        let mut icon = vec![0, 0, 1, 0, 2, 0];
        let mut offset = 6 + 16 * frames.len();
        for frame in &frames {
            icon.extend([20, 0, 0, 0, 1, 0, 32, 0]);
            icon.extend((frame.len() as u32).to_le_bytes());
            icon.extend((offset as u32).to_le_bytes());
            offset += frame.len();
        }
        for frame in &frames {
            icon.extend(frame);
        }

        let structure = parse_ico_structure(&icon).expect("icon structure");
        assert_eq!(structure.magick_format, "ICO");
        assert_eq!(
            structure.frames,
            vec![
                ImageFrameStructure {
                    width: 20,
                    height: 15
                },
                ImageFrameStructure {
                    width: 20,
                    height: 10
                },
            ]
        );
        assert!(parse_ico_structure(&icon[..30]).is_none());
        assert!(parse_ico_structure(b"GIF89a").is_none());
    }
}
