use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter};
use tokio_util::sync::CancellationToken;

use crate::engines::process::{run_process, ProcessMessages};
use crate::engines::verification::{ImageFrameStructure, ImageStructure};
use crate::engines::{self, tool_command, verification, ConversionResult};
use crate::error::ConversionError;
use crate::formats::{FileCategory, Format};
use crate::progress::ProgressPayload;

use super::output::{prepare_output, OutputOptions};

const MAX_DIMENSION: u32 = 32_768;
const RESIZE_TIMEOUT: Duration = Duration::from_secs(10 * 60);

pub(super) async fn resize_image(
    app: AppHandle,
    input_path: String,
    width: u32,
    height: u32,
    preserve_aspect: bool,
    job_id: Option<String>,
    output_options: Option<OutputOptions>,
) -> Result<ConversionResult, ConversionError> {
    let input = PathBuf::from(&input_path);
    if !input.is_file() {
        return Err(ConversionError::InputNotFound { path: input_path });
    }
    validate_dimensions(width, height)?;

    let format = crate::formats::Format::from_extension(
        input
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or_default(),
    )
    .ok_or_else(|| ConversionError::UnsupportedConversion {
        input: input_path.clone(),
        output: "image".into(),
    })?;
    if format.category() != FileCategory::Image {
        return Err(ConversionError::UnsupportedConversion {
            input: format.label().into(),
            output: "image resize".into(),
        });
    }

    let extension = format.extension();
    let prepared_output = prepare_output(&input, extension, "-resized", output_options)?;
    let active_job = super::jobs::ActiveJobGuard::register(app.clone(), job_id)?;
    let cancel_token = active_job.cancel_token();
    let event_job_id = active_job.job_id().to_owned();

    let _ = app.emit(
        "conversion-progress",
        ProgressPayload {
            job_id: event_job_id.clone(),
            percent: -1,
            stage: "Resizing image".into(),
        },
    );

    let started = Instant::now();
    let result = run_resize(
        &input,
        prepared_output.working_path(),
        width,
        height,
        preserve_aspect,
        format,
        cancel_token,
    )
    .await;

    match result {
        Ok(result) => {
            let mut result = prepared_output.commit(result)?;
            result.duration_ms = started.elapsed().as_millis() as u64;
            let _ = app.emit(
                "conversion-progress",
                ProgressPayload {
                    job_id: event_job_id,
                    percent: 100,
                    stage: "Complete".into(),
                },
            );
            Ok(result)
        }
        Err(error) => Err(error),
    }
}

async fn run_resize(
    input: &Path,
    output: &Path,
    width: u32,
    height: u32,
    preserve_aspect: bool,
    format: Format,
    cancel_token: CancellationToken,
) -> Result<ConversionResult, ConversionError> {
    let source_structure =
        verification::inspect_image_structure(input, cancel_token.clone()).await?;
    if !verification::magick_format_matches(format, &source_structure.magick_format) {
        return Err(resize_verification_error(
            "The source contents do not match the image format in its filename",
            &source_structure.magick_format,
        ));
    }

    let geometry = resize_geometry(width, height, preserve_aspect);
    let mut command = tool_command("magick");
    command.arg(input).args(["-resize", &geometry]).arg(output);
    run_process(
        command,
        cancel_token.clone(),
        RESIZE_TIMEOUT,
        &[output],
        ProcessMessages {
            start: "Failed to start image resize",
            wait: "Image resize process could not be awaited",
            failure: "Image resize failed",
        },
    )
    .await?;

    let output_size = verify_resized_output(
        output,
        &source_structure,
        format,
        width,
        height,
        preserve_aspect,
        cancel_token,
    )
    .await?;
    Ok(ConversionResult {
        output_path: output.to_string_lossy().into(),
        output_paths: Vec::new(),
        output_size,
        duration_ms: 0,
        undo_manifest: None,
    })
}

async fn verify_resized_output(
    output: &Path,
    source: &ImageStructure,
    format: Format,
    width: u32,
    height: u32,
    preserve_aspect: bool,
    cancel_token: CancellationToken,
) -> Result<u64, ConversionError> {
    let output_size = verification::nonempty_file(output)?;
    let candidate = match verification::inspect_image_structure(output, cancel_token).await {
        Ok(candidate) => candidate,
        Err(error) => {
            engines::cleanup_partial(output);
            return Err(error);
        }
    };
    let expected_frames = expected_resize_frames(source, width, height, preserve_aspect);
    if !resized_structure_matches(source, &candidate, format, width, height, preserve_aspect) {
        engines::cleanup_partial(output);
        return Err(resize_verification_error(
            "The resized image changed format, dimensions, or frame count",
            &format!(
                "expected {} frame(s) at {:?}; found {} frame(s) at {:?}",
                expected_frames.len(),
                expected_frames,
                candidate.frames.len(),
                candidate.frames
            ),
        ));
    }
    Ok(output_size)
}

fn resized_structure_matches(
    source: &ImageStructure,
    candidate: &ImageStructure,
    format: Format,
    width: u32,
    height: u32,
    preserve_aspect: bool,
) -> bool {
    verification::magick_format_matches(format, &source.magick_format)
        && verification::magick_format_matches(format, &candidate.magick_format)
        && candidate.frames == expected_resize_frames(source, width, height, preserve_aspect)
}

fn expected_resize_frames(
    source: &ImageStructure,
    width: u32,
    height: u32,
    preserve_aspect: bool,
) -> Vec<ImageFrameStructure> {
    source
        .frames
        .iter()
        .map(|frame| expected_resize_frame(frame, width, height, preserve_aspect))
        .collect()
}

fn expected_resize_frame(
    source: &ImageFrameStructure,
    width: u32,
    height: u32,
    preserve_aspect: bool,
) -> ImageFrameStructure {
    if !preserve_aspect {
        return ImageFrameStructure { width, height };
    }

    let source_width = u64::from(source.width);
    let source_height = u64::from(source.height);
    let target_width = u64::from(width);
    let target_height = u64::from(height);
    if target_width.saturating_mul(source_height) <= target_height.saturating_mul(source_width) {
        ImageFrameStructure {
            width,
            height: rounded_ratio(source_height, target_width, source_width),
        }
    } else {
        ImageFrameStructure {
            width: rounded_ratio(source_width, target_height, source_height),
            height,
        }
    }
}

fn rounded_ratio(value: u64, multiplier: u64, divisor: u64) -> u32 {
    let scaled = value.saturating_mul(multiplier);
    let rounded = scaled.saturating_add(divisor / 2) / divisor;
    u32::try_from(rounded.max(1)).unwrap_or(u32::MAX)
}

fn resize_verification_error(message: &str, stderr: &str) -> ConversionError {
    ConversionError::ProcessFailed {
        message: message.into(),
        stderr: stderr.into(),
        exit_code: None,
    }
}

fn validate_dimensions(width: u32, height: u32) -> Result<(), ConversionError> {
    if width == 0 || height == 0 || width > MAX_DIMENSION || height > MAX_DIMENSION {
        return Err(ConversionError::UnsupportedConversion {
            input: format!("Resize dimensions must be between 1 and {MAX_DIMENSION} pixels"),
            output: "image".into(),
        });
    }
    Ok(())
}

fn resize_geometry(width: u32, height: u32, preserve_aspect: bool) -> String {
    format!("{width}x{height}{}", if preserve_aspect { "" } else { "!" })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_dimension_bounds() {
        assert!(validate_dimensions(1, 1).is_ok());
        assert!(validate_dimensions(MAX_DIMENSION, MAX_DIMENSION).is_ok());
        assert!(validate_dimensions(0, 100).is_err());
        assert!(validate_dimensions(100, MAX_DIMENSION + 1).is_err());
    }

    #[test]
    fn builds_locked_and_unlocked_geometry() {
        assert_eq!(resize_geometry(800, 600, true), "800x600");
        assert_eq!(resize_geometry(800, 600, false), "800x600!");
    }

    #[test]
    fn calculates_exact_and_aspect_fit_frame_dimensions() {
        let landscape = ImageFrameStructure {
            width: 3,
            height: 2,
        };
        let portrait = ImageFrameStructure {
            width: 5,
            height: 7,
        };

        assert_eq!(
            expected_resize_frame(&landscape, 2, 2, false),
            ImageFrameStructure {
                width: 2,
                height: 2,
            }
        );
        assert_eq!(
            expected_resize_frame(&landscape, 2, 2, true),
            ImageFrameStructure {
                width: 2,
                height: 1,
            }
        );
        assert_eq!(
            expected_resize_frame(&portrait, 3, 3, true),
            ImageFrameStructure {
                width: 2,
                height: 3,
            }
        );
        assert_eq!(
            expected_resize_frame(
                &ImageFrameStructure {
                    width: 4,
                    height: 3,
                },
                2,
                2,
                true,
            ),
            ImageFrameStructure {
                width: 2,
                height: 2,
            },
            "half pixels use ImageMagick's nearest-pixel rounding"
        );
    }

    #[test]
    fn rejects_wrong_resize_format_geometry_and_topology() {
        let source = ImageStructure {
            magick_format: "PNG".into(),
            frames: vec![
                ImageFrameStructure {
                    width: 640,
                    height: 480,
                },
                ImageFrameStructure {
                    width: 320,
                    height: 200,
                },
            ],
        };
        let valid = ImageStructure {
            magick_format: "PNG".into(),
            frames: vec![
                ImageFrameStructure {
                    width: 160,
                    height: 120,
                },
                ImageFrameStructure {
                    width: 160,
                    height: 100,
                },
            ],
        };
        assert!(resized_structure_matches(
            &source,
            &valid,
            Format::Png,
            160,
            120,
            true,
        ));

        let mut wrong_format = valid.clone();
        wrong_format.magick_format = "JPEG".into();
        assert!(!resized_structure_matches(
            &source,
            &wrong_format,
            Format::Png,
            160,
            120,
            true,
        ));
        let mut wrong_geometry = valid.clone();
        wrong_geometry.frames[1].height = 99;
        assert!(!resized_structure_matches(
            &source,
            &wrong_geometry,
            Format::Png,
            160,
            120,
            true,
        ));
        let mut missing_frame = valid;
        missing_frame.frames.pop();
        assert!(!resized_structure_matches(
            &source,
            &missing_frame,
            Format::Png,
            160,
            120,
            true,
        ));
    }

    #[tokio::test]
    async fn cancelled_verification_removes_the_uncommitted_output() {
        let directory = tempfile::tempdir().expect("tempdir");
        let output = directory.path().join("partial.png");
        std::fs::write(&output, b"uncommitted").expect("partial output");
        let source = ImageStructure {
            magick_format: "PNG".into(),
            frames: vec![ImageFrameStructure {
                width: 640,
                height: 480,
            }],
        };
        let token = CancellationToken::new();
        token.cancel();

        let result =
            verify_resized_output(&output, &source, Format::Png, 320, 240, true, token).await;

        assert!(matches!(result, Err(ConversionError::Cancelled)));
        assert!(!output.exists());
    }

    #[tokio::test]
    async fn resizes_every_advertised_format_with_verified_structure() {
        if engines::resolve_tool("magick").is_none() {
            return;
        }

        let directory = tempfile::tempdir().expect("fixture directory");
        let source = directory.path().join("source.png");
        let status = tool_command("magick")
            .args(["-size", "128x96", "gradient:#25334f-#90a8e8"])
            .arg(&source)
            .status()
            .await
            .expect("generate source image");
        assert!(status.success());

        for extension in [
            "png", "jpg", "webp", "gif", "heic", "tiff", "bmp", "avif", "ico",
        ] {
            let input = directory.path().join(format!("input.{extension}"));
            let output = directory.path().join(format!("output.{extension}"));
            let status = tool_command("magick")
                .arg(&source)
                .arg(&input)
                .status()
                .await
                .unwrap_or_else(|error| panic!("create {extension} fixture: {error}"));
            assert!(status.success(), "create {extension} fixture");

            run_resize(
                &input,
                &output,
                64,
                48,
                false,
                Format::from_extension(extension).expect("advertised format"),
                CancellationToken::new(),
            )
            .await
            .unwrap_or_else(|error| panic!("resize {extension}: {error}"));

            let structure =
                verification::inspect_image_structure(&output, CancellationToken::new())
                    .await
                    .unwrap_or_else(|error| panic!("inspect {extension} output: {error}"));
            assert_eq!(
                structure.frames,
                vec![ImageFrameStructure {
                    width: 64,
                    height: 48,
                }],
                "{extension} dimensions and topology"
            );
        }
    }

    #[tokio::test]
    async fn preserves_multiframe_gif_tiff_and_ico_topology() {
        if engines::resolve_tool("magick").is_none() {
            return;
        }
        let directory = tempfile::tempdir().expect("fixture directory");

        for (extension, format) in [
            ("gif", Format::Gif),
            ("tiff", Format::Tiff),
            ("ico", Format::Ico),
        ] {
            let input = directory.path().join(format!("animated.{extension}"));
            let output = directory.path().join(format!("resized.{extension}"));
            let status = tool_command("magick")
                .args([
                    "-size",
                    "40x30",
                    "xc:#25334f",
                    "-size",
                    "20x10",
                    "xc:#90a8e8",
                ])
                .arg(&input)
                .status()
                .await
                .unwrap_or_else(|error| panic!("create multipage {extension}: {error}"));
            assert!(status.success(), "create multipage {extension}");

            run_resize(
                &input,
                &output,
                20,
                20,
                true,
                format,
                CancellationToken::new(),
            )
            .await
            .unwrap_or_else(|error| panic!("resize multipage {extension}: {error}"));

            let structure =
                verification::inspect_image_structure(&output, CancellationToken::new())
                    .await
                    .unwrap_or_else(|error| panic!("inspect multipage {extension}: {error}"));
            assert_eq!(
                structure.frames,
                vec![
                    ImageFrameStructure {
                        width: 20,
                        height: 15,
                    },
                    ImageFrameStructure {
                        width: 20,
                        height: 10,
                    },
                ],
                "{extension} frame/page geometry and count"
            );
        }
    }
}
