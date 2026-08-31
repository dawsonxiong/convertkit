use std::io::Read;
use std::path::Path;
use std::time::Duration;

use tokio_util::sync::CancellationToken;

use crate::engines::process::{run_process_with_output, ProcessMessages};
use crate::error::ConversionError;
use crate::formats::Format;

use super::{cleanup_partial, tool_command};

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
}
