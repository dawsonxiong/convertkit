use std::path::Path;
use std::time::Duration;

use tauri::{AppHandle, Emitter};
use tokio_util::sync::CancellationToken;

use crate::engines::process::{run_process, ProcessMessages};
use crate::engines::{
    tool_command, verification, ConversionEngine, ConversionRequest, ConversionResult,
};
use crate::error::ConversionError;
use crate::formats::{FileCategory, Format};
use crate::progress::ProgressPayload;

/// Engine that shells out to ImageMagick (`magick`) for raster image
/// conversions.
pub struct ImageMagickEngine;

impl ConversionEngine for ImageMagickEngine {
    fn required_tool(&self) -> &'static str {
        "magick"
    }

    fn supports(&self, input: Format, output: Format) -> bool {
        let input_ok = input.category() == FileCategory::Image
            || input == Format::Heic
            || input == Format::Svg;
        let output_ok = output.category() == FileCategory::Image;
        input_ok && output_ok && input != output
    }

    async fn convert(
        &self,
        request: ConversionRequest,
        app: AppHandle,
        cancel_token: CancellationToken,
    ) -> Result<ConversionResult, ConversionError> {
        // Emit indeterminate progress (ImageMagick doesn't report progress).
        let _ = app.emit(
            "conversion-progress",
            ProgressPayload {
                job_id: request.job_id.clone(),
                percent: -1,
                stage: "Converting with ImageMagick".to_string(),
            },
        );

        convert_image(
            &request.input_path,
            &request.output_path,
            request.input_format,
            request.output_format,
            cancel_token,
        )
        .await
    }
}

async fn convert_image(
    input: &Path,
    output: &Path,
    input_format: Format,
    output_format: Format,
    cancel_token: CancellationToken,
) -> Result<ConversionResult, ConversionError> {
    let source = verification::inspect_image_structure(input, cancel_token.clone()).await?;
    if !source_format_matches(input_format, &source.magick_format) {
        return Err(ConversionError::UnsupportedConversion {
            input: format!(
                "The file contents are {}, not {}",
                source.magick_format,
                input_format.label()
            ),
            output: output_format.label().into(),
        });
    }
    validate_conversion_topology(&source, output_format)?;

    // Keep ImageMagick's output in an isolated workspace. Some coders expand a
    // multi-frame source into numbered siblings instead of the requested path;
    // the workspace and explicit cleanup ensure those files never leak beside
    // the user's eventual output.
    let workspace_parent = output.parent().unwrap_or_else(|| Path::new("."));
    let workspace = tempfile::Builder::new()
        .prefix(".convertkit-image-")
        .tempdir_in(workspace_parent)
        .map_err(|error| ConversionError::ProcessFailed {
            message: format!("Could not create an image conversion workspace: {error}"),
            stderr: String::new(),
            exit_code: None,
        })?;
    let workspace_output = workspace
        .path()
        .join(format!("converted.{}", output_format.extension()));

    let mut command = tool_command("magick");
    command.arg(input).arg("-strip");

    match output_format {
        Format::Jpg => {
            command.args(["-quality", "90"]);
        }
        Format::WebP => {
            command.args(["-quality", "85"]);
        }
        Format::Avif => {
            command.args(["-quality", "80"]);
        }
        _ => {}
    }
    command.arg(&workspace_output);

    let process_result = run_process(
        command,
        cancel_token.clone(),
        Duration::from_secs(10 * 60),
        &[&workspace_output],
        ProcessMessages {
            start: "Failed to start ImageMagick",
            wait: "ImageMagick process could not be awaited",
            failure: "ImageMagick conversion failed",
        },
    )
    .await;
    if let Err(error) = process_result {
        cleanup_numbered_sidecars(&workspace_output);
        return Err(error);
    }

    let candidate = match verification::inspect_image_structure(
        &workspace_output,
        cancel_token.clone(),
    )
    .await
    {
        Ok(candidate) => candidate,
        Err(error) => {
            cleanup_numbered_sidecars(&workspace_output);
            return Err(error);
        }
    };
    cleanup_numbered_sidecars(&workspace_output);
    if !verification::magick_format_matches(output_format, &candidate.magick_format)
        || candidate.frames != source.frames
    {
        return Err(ConversionError::ProcessFailed {
            message: "The converted image changed format, dimensions, or frame count".into(),
            stderr: format!(
                "expected {} {:?}; found {} {:?}",
                output_format.label(),
                source.frames,
                candidate.magick_format,
                candidate.frames
            ),
            exit_code: None,
        });
    }
    if cancel_token.is_cancelled() {
        return Err(ConversionError::Cancelled);
    }

    std::fs::rename(&workspace_output, output).map_err(|error| {
        super::cleanup_partial(output);
        ConversionError::ProcessFailed {
            message: format!("Could not save the converted image: {error}"),
            stderr: String::new(),
            exit_code: None,
        }
    })?;
    let output_size = verification::nonempty_file(output)?;

    Ok(ConversionResult {
        output_path: output.to_string_lossy().to_string(),
        output_paths: Vec::new(),
        output_size,
        duration_ms: 0,
        undo_manifest: None,
    })
}

fn validate_conversion_topology(
    source: &verification::ImageStructure,
    output_format: Format,
) -> Result<(), ConversionError> {
    if source.frames.len() > 1 && !supports_multiframe_output(output_format) {
        return Err(ConversionError::UnsupportedConversion {
            input: format!(
                "This image contains {} frames or pages",
                source.frames.len()
            ),
            output: format!(
                "a single {} file without discarding frames",
                output_format.label()
            ),
        });
    }
    if output_format == Format::Ico
        && source
            .frames
            .iter()
            .any(|frame| frame.width > 256 || frame.height > 256)
    {
        return Err(ConversionError::UnsupportedConversion {
            input: "This image contains a frame larger than 256 × 256 px".into(),
            output: "ICO without resizing the source".into(),
        });
    }
    Ok(())
}

fn supports_multiframe_output(format: Format) -> bool {
    matches!(
        format,
        Format::Gif | Format::Tiff | Format::Ico | Format::WebP | Format::Heic
    )
}

fn source_format_matches(format: Format, identified: &str) -> bool {
    (format == Format::Svg && identified == "SVG")
        || verification::magick_format_matches(format, identified)
}

fn cleanup_numbered_sidecars(output: &Path) {
    let Some(parent) = output.parent() else {
        return;
    };
    let Some(stem) = output.file_stem().and_then(|value| value.to_str()) else {
        return;
    };
    let Some(extension) = output.extension().and_then(|value| value.to_str()) else {
        return;
    };
    let prefix = format!("{stem}-");
    let suffix = format!(".{extension}");
    let Ok(entries) = std::fs::read_dir(parent) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        let Some(index) = name
            .strip_prefix(&prefix)
            .and_then(|value| value.strip_suffix(&suffix))
        else {
            continue;
        };
        if !index.is_empty() && index.bytes().all(|byte| byte.is_ascii_digit()) {
            let _ = std::fs::remove_file(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn create_multiframe(path: &Path) {
        let status = tool_command("magick")
            .args([
                "-size",
                "40x30",
                "xc:#25334f",
                "-size",
                "20x10",
                "xc:#90a8e8",
            ])
            .arg(path)
            .status()
            .await
            .expect("create multiframe fixture");
        assert!(status.success());
    }

    #[tokio::test]
    async fn converts_a_static_image_without_changing_its_geometry() {
        if super::super::resolve_tool("magick").is_none() {
            return;
        }
        let directory = tempfile::tempdir().expect("tempdir");
        let input = directory.path().join("source.png");
        let output = directory.path().join("converted.jpg");
        let status = tool_command("magick")
            .args(["-size", "37x23", "xc:#25334f"])
            .arg(&input)
            .status()
            .await
            .expect("create fixture");
        assert!(status.success());

        convert_image(
            &input,
            &output,
            Format::Png,
            Format::Jpg,
            CancellationToken::new(),
        )
        .await
        .expect("convert static image");

        let structure = verification::inspect_image_structure(&output, CancellationToken::new())
            .await
            .expect("inspect output");
        assert_eq!(structure.magick_format, "JPEG");
        assert_eq!(
            structure.frames,
            vec![verification::ImageFrameStructure {
                width: 37,
                height: 23,
            }]
        );
    }

    #[tokio::test]
    async fn preserves_supported_gif_and_ico_multiframe_topology() {
        if super::super::resolve_tool("magick").is_none() {
            return;
        }
        let directory = tempfile::tempdir().expect("tempdir");
        for (input_extension, input_format, output_extension, output_format) in [
            ("gif", Format::Gif, "tiff", Format::Tiff),
            ("ico", Format::Ico, "webp", Format::WebP),
        ] {
            let input = directory.path().join(format!("source.{input_extension}"));
            let output = directory
                .path()
                .join(format!("converted.{output_extension}"));
            create_multiframe(&input).await;

            convert_image(
                &input,
                &output,
                input_format,
                output_format,
                CancellationToken::new(),
            )
            .await
            .unwrap_or_else(|error| panic!("convert {input_extension}: {error}"));

            let structure =
                verification::inspect_image_structure(&output, CancellationToken::new())
                    .await
                    .unwrap_or_else(|error| panic!("inspect {output_extension}: {error}"));
            assert_eq!(
                structure.frames,
                vec![
                    verification::ImageFrameStructure {
                        width: 40,
                        height: 30,
                    },
                    verification::ImageFrameStructure {
                        width: 20,
                        height: 10,
                    },
                ],
                "{input_extension} to {output_extension} topology"
            );
        }
    }

    #[tokio::test]
    async fn rejects_multiframe_static_routes_without_leaking_numbered_sidecars() {
        if super::super::resolve_tool("magick").is_none() {
            return;
        }
        let directory = tempfile::tempdir().expect("tempdir");
        for (extension, format) in [("gif", Format::Gif), ("ico", Format::Ico)] {
            let input = directory.path().join(format!("source.{extension}"));
            let output = directory.path().join(format!("rejected-{extension}.png"));
            create_multiframe(&input).await;

            let result = convert_image(
                &input,
                &output,
                format,
                Format::Png,
                CancellationToken::new(),
            )
            .await;

            assert!(matches!(
                result,
                Err(ConversionError::UnsupportedConversion { .. })
            ));
            assert!(!output.exists());
            let prefix = format!("rejected-{extension}-");
            assert!(std::fs::read_dir(directory.path())
                .expect("read fixture directory")
                .flatten()
                .all(|entry| !entry.file_name().to_string_lossy().starts_with(&prefix)));
        }
    }

    #[test]
    fn removes_only_numbered_imagemagick_sidecars() {
        let directory = tempfile::tempdir().expect("tempdir");
        let output = directory.path().join("converted.png");
        let first = directory.path().join("converted-0.png");
        let second = directory.path().join("converted-12.png");
        let unrelated = directory.path().join("converted-final.png");
        std::fs::write(&first, b"first").expect("first sidecar");
        std::fs::write(&second, b"second").expect("second sidecar");
        std::fs::write(&unrelated, b"keep").expect("unrelated file");

        cleanup_numbered_sidecars(&output);

        assert!(!first.exists());
        assert!(!second.exists());
        assert!(unrelated.exists());
    }
}
