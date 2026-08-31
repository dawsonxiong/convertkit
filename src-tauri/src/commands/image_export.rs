use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::Deserialize;
use tauri::{AppHandle, Emitter};
use tokio_util::sync::CancellationToken;

use crate::engines::process::{run_process, ProcessMessages};
use crate::engines::{self, tool_command, ConversionResult};
use crate::error::ConversionError;
use crate::formats::FileCategory;
use crate::progress::ProgressPayload;

use super::output::{prepare_output, OutputOptions, PreparedOutput};

const EXPORT_TIMEOUT: Duration = Duration::from_secs(10 * 60);
const INSPECTION_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_PRESETS: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct StaticImageInfo {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub enum ImageExportPreset {
    Web,
    Email,
    Social,
    Preview,
}

impl ImageExportPreset {
    fn label(self) -> &'static str {
        match self {
            Self::Web => "Web",
            Self::Email => "Email",
            Self::Social => "Social",
            Self::Preview => "Preview",
        }
    }

    fn extension(self) -> &'static str {
        match self {
            Self::Web | Self::Preview => "webp",
            Self::Email | Self::Social => "jpg",
        }
    }

    fn suffix(self) -> &'static str {
        match self {
            Self::Web => "-web",
            Self::Email => "-email",
            Self::Social => "-social",
            Self::Preview => "-preview",
        }
    }

    fn max_dimension(self) -> u32 {
        match self {
            Self::Web => 1_920,
            Self::Email => 1_600,
            Self::Social => 2_048,
            Self::Preview => 640,
        }
    }

    fn quality(self) -> u8 {
        match self {
            Self::Web | Self::Email => 82,
            Self::Social => 90,
            Self::Preview => 78,
        }
    }

    fn expected_magick_format(self) -> &'static str {
        match self {
            Self::Web | Self::Preview => "WEBP",
            Self::Email | Self::Social => "JPEG",
        }
    }
}

pub(super) fn validate_presets(presets: &[ImageExportPreset]) -> Result<(), ConversionError> {
    if presets.is_empty() || presets.len() > MAX_PRESETS {
        return Err(ConversionError::UnsupportedConversion {
            input: format!("Choose between 1 and {MAX_PRESETS} image variants"),
            output: "image export".into(),
        });
    }

    let mut unique = HashSet::with_capacity(presets.len());
    if presets.iter().any(|preset| !unique.insert(*preset)) {
        return Err(ConversionError::UnsupportedConversion {
            input: "Duplicate image variants are not allowed".into(),
            output: "image export".into(),
        });
    }
    Ok(())
}

pub(super) async fn export_image_variants(
    app: AppHandle,
    input_path: String,
    presets: Vec<ImageExportPreset>,
    job_id: Option<String>,
    output_options: Option<OutputOptions>,
) -> Result<ConversionResult, ConversionError> {
    let input = PathBuf::from(&input_path);
    if !input.is_file() {
        return Err(ConversionError::InputNotFound { path: input_path });
    }
    validate_presets(&presets)?;

    let format = input
        .extension()
        .and_then(|extension| extension.to_str())
        .and_then(crate::formats::Format::from_extension)
        .ok_or_else(|| ConversionError::UnsupportedConversion {
            input: input.to_string_lossy().into(),
            output: "image export".into(),
        })?;
    if format.category() != FileCategory::Image {
        return Err(ConversionError::UnsupportedConversion {
            input: format.label().into(),
            output: "image export".into(),
        });
    }
    if engines::resolve_tool("magick").is_none() {
        return Err(ConversionError::MissingDependency {
            tool: "magick".into(),
            install_hint: "Install ImageMagick to export image variants".into(),
        });
    }
    inspect_static_source(&input).await?;

    let prepared = prepare_variant_outputs(&input, &presets, output_options.as_ref())?;
    let active_job = super::jobs::ActiveJobGuard::register(app.clone(), job_id)?;
    let cancel_token = active_job.cancel_token();
    let event_job_id = active_job.job_id().to_owned();
    let started = Instant::now();

    for (index, (preset, output)) in presets.iter().zip(&prepared).enumerate() {
        let _ = app.emit(
            "conversion-progress",
            ProgressPayload {
                job_id: event_job_id.clone(),
                percent: ((index * 90) / presets.len()) as i32,
                stage: format!("Creating {} variant", preset.label()),
            },
        );
        render_variant(&input, output.working_path(), *preset, cancel_token.clone()).await?;
    }

    let mut result = commit_variant_outputs(prepared)?;
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

fn prepare_variant_outputs(
    input: &Path,
    presets: &[ImageExportPreset],
    output_options: Option<&OutputOptions>,
) -> Result<Vec<PreparedOutput>, ConversionError> {
    presets
        .iter()
        .map(|preset| {
            let mut options = output_options.cloned().unwrap_or_default();
            options.suffix.push_str(preset.suffix());
            prepare_output(input, preset.extension(), preset.suffix(), Some(options))
        })
        .collect()
}

async fn render_variant(
    input: &Path,
    output: &Path,
    preset: ImageExportPreset,
    cancel_token: CancellationToken,
) -> Result<(), ConversionError> {
    let geometry = format!("{}x{}>", preset.max_dimension(), preset.max_dimension());
    let quality = preset.quality().to_string();
    let mut command = tool_command("magick");
    command
        .arg(input)
        .args(["-auto-orient", "-resize", &geometry, "-colorspace", "sRGB"]);

    match preset {
        ImageExportPreset::Web | ImageExportPreset::Preview => {
            command.args(["-strip", "-quality", &quality, "-define", "webp:method=6"]);
        }
        ImageExportPreset::Email | ImageExportPreset::Social => {
            command.args([
                "-background",
                "#ffffff",
                "-alpha",
                "remove",
                "-alpha",
                "off",
                "-strip",
                "-sampling-factor",
                "4:2:0",
                "-interlace",
                "Plane",
                "-quality",
                &quality,
            ]);
        }
    }
    command.arg(output);

    run_image_process(command, output, cancel_token, EXPORT_TIMEOUT).await?;
    verify_variant(output, preset).await
}

pub(super) async fn inspect_static_source(
    input: &Path,
) -> Result<StaticImageInfo, ConversionError> {
    let mut command = tool_command("magick");
    command
        .args(["identify", "-ping", "-format", "%w|%h|%n\n"])
        .arg(input);
    let inspection = tokio::time::timeout(INSPECTION_TIMEOUT, command.output())
        .await
        .map_err(|_| ConversionError::Timeout {
            seconds: INSPECTION_TIMEOUT.as_secs(),
        })?
        .map_err(|error| ConversionError::ProcessFailed {
            message: format!("Could not inspect the source image: {error}"),
            stderr: String::new(),
            exit_code: None,
        })?;

    if !inspection.status.success() {
        return Err(ConversionError::ProcessFailed {
            message: "The source image could not be read".into(),
            stderr: String::from_utf8_lossy(&inspection.stderr).into(),
            exit_code: inspection.status.code(),
        });
    }

    let summary = String::from_utf8_lossy(&inspection.stdout);
    let values = summary
        .lines()
        .next()
        .map(|line| line.split('|').collect::<Vec<_>>())
        .unwrap_or_default();
    let width = values
        .first()
        .and_then(|value| value.trim().parse::<u32>().ok());
    let height = values
        .get(1)
        .and_then(|value| value.trim().parse::<u32>().ok());
    let frame_count = values
        .get(2)
        .and_then(|value| value.trim().parse::<u32>().ok());

    if frame_count.is_some_and(|count| count > 1) {
        return Err(ConversionError::ProcessFailed {
            message: "This tool accepts static images only. Convert animated or multi-page files to a static image first.".into(),
            stderr: String::new(),
            exit_code: None,
        });
    }
    match (width, height, frame_count) {
        (Some(width), Some(height), Some(1)) if width > 0 && height > 0 => {
            Ok(StaticImageInfo { width, height })
        }
        _ => Err(ConversionError::ProcessFailed {
            message: "The source image dimensions and frame count could not be verified".into(),
            stderr: summary.into(),
            exit_code: None,
        }),
    }
}

pub(super) async fn run_image_process(
    command: tokio::process::Command,
    output: &Path,
    cancel_token: CancellationToken,
    timeout: Duration,
) -> Result<(), ConversionError> {
    run_process(
        command,
        cancel_token,
        timeout,
        &[output],
        ProcessMessages {
            start: "Failed to start image generation",
            wait: "Image generation process could not be awaited",
            failure: "Image generation failed",
        },
    )
    .await
}

async fn verify_variant(output: &Path, preset: ImageExportPreset) -> Result<(), ConversionError> {
    engines::verification::nonempty_file(output)?;

    let inspection = tool_command("magick")
        .args(["identify", "-ping", "-format", "%m|%w|%h|%n"])
        .arg(output)
        .output()
        .await
        .map_err(|error| ConversionError::ProcessFailed {
            message: format!("Could not verify the exported image: {error}"),
            stderr: String::new(),
            exit_code: None,
        })?;
    if !inspection.status.success() {
        engines::cleanup_partial(output);
        return Err(ConversionError::ProcessFailed {
            message: "The exported image could not be verified".into(),
            stderr: String::from_utf8_lossy(&inspection.stderr).into(),
            exit_code: inspection.status.code(),
        });
    }

    let summary = String::from_utf8_lossy(&inspection.stdout);
    let values = summary.split('|').collect::<Vec<_>>();
    let width = values.get(1).and_then(|value| value.parse::<u32>().ok());
    let height = values.get(2).and_then(|value| value.parse::<u32>().ok());
    let frames = values.get(3).and_then(|value| value.parse::<u32>().ok());
    let valid = values.first().copied() == Some(preset.expected_magick_format())
        && width.is_some_and(|value| value > 0 && value <= preset.max_dimension())
        && height.is_some_and(|value| value > 0 && value <= preset.max_dimension())
        && frames == Some(1);
    if !valid {
        engines::cleanup_partial(output);
        return Err(ConversionError::ProcessFailed {
            message: "The exported image did not match the selected variant".into(),
            stderr: summary.into(),
            exit_code: None,
        });
    }
    Ok(())
}

fn commit_variant_outputs(
    prepared: Vec<PreparedOutput>,
) -> Result<ConversionResult, ConversionError> {
    let mut output_paths = Vec::with_capacity(prepared.len());
    let mut output_size = 0u64;
    for output in prepared {
        match output.commit(ConversionResult {
            output_path: String::new(),
            output_paths: Vec::new(),
            output_size: 0,
            duration_ms: 0,
            undo_manifest: None,
        }) {
            Ok(result) => {
                output_size = output_size.saturating_add(result.output_size);
                output_paths.push(result.output_path);
            }
            Err(error) => {
                for path in &output_paths {
                    let _ = std::fs::remove_file(path);
                }
                return Err(error);
            }
        }
    }

    let output_path = output_paths
        .first()
        .cloned()
        .ok_or(ConversionError::OutputMissing)?;
    Ok(ConversionResult {
        output_path,
        output_paths,
        output_size,
        duration_ms: 0,
        undo_manifest: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_and_duplicate_variant_sets() {
        assert!(validate_presets(&[]).is_err());
        assert!(validate_presets(&[ImageExportPreset::Web]).is_ok());
        assert!(validate_presets(&[ImageExportPreset::Web, ImageExportPreset::Web]).is_err());
    }

    #[test]
    fn exposes_stable_delivery_specs() {
        let expected = [
            (ImageExportPreset::Web, "webp", "-web", 1_920, 82),
            (ImageExportPreset::Email, "jpg", "-email", 1_600, 82),
            (ImageExportPreset::Social, "jpg", "-social", 2_048, 90),
            (ImageExportPreset::Preview, "webp", "-preview", 640, 78),
        ];
        for (preset, extension, suffix, dimension, quality) in expected {
            assert_eq!(preset.extension(), extension);
            assert_eq!(preset.suffix(), suffix);
            assert_eq!(preset.max_dimension(), dimension);
            assert_eq!(preset.quality(), quality);
        }
    }

    #[test]
    fn failed_commit_removes_already_committed_variants() {
        let directory = tempfile::tempdir().expect("fixture directory");
        let source = directory.path().join("source.png");
        std::fs::write(&source, b"source").expect("source fixture");
        let prepared = prepare_variant_outputs(
            &source,
            &[ImageExportPreset::Web, ImageExportPreset::Email],
            None,
        )
        .expect("prepared outputs");
        std::fs::write(prepared[0].working_path(), b"first output").expect("first output");
        let first_final = directory.path().join("source-web.webp");

        assert!(commit_variant_outputs(prepared).is_err());
        assert!(!first_final.exists());
    }

    #[tokio::test]
    async fn renders_verified_variants_without_upscaling_or_changing_the_source() {
        if engines::resolve_tool("magick").is_none() {
            return;
        }

        let directory = tempfile::tempdir().expect("fixture directory");
        let source = directory.path().join("source[final].png");
        let status = tool_command("magick")
            .args(["-size", "2400x1200", "gradient:#112244-#99bbff"])
            .arg(&source)
            .status()
            .await
            .expect("generate source image");
        assert!(status.success());
        let source_before = std::fs::read(&source).expect("source bytes");

        let cases = [
            (ImageExportPreset::Web, 1920, 960),
            (ImageExportPreset::Email, 1600, 800),
            (ImageExportPreset::Social, 2048, 1024),
            (ImageExportPreset::Preview, 640, 320),
        ];
        for (preset, expected_width, expected_height) in cases {
            let output =
                directory
                    .path()
                    .join(format!("{}.{}", preset.label(), preset.extension()));
            render_variant(&source, &output, preset, CancellationToken::new())
                .await
                .unwrap_or_else(|error| panic!("render {}: {error:?}", preset.label()));
            let dimensions = tool_command("magick")
                .args(["identify", "-ping", "-format", "%wx%h"])
                .arg(&output)
                .output()
                .await
                .expect("inspect output");
            assert!(dimensions.status.success());
            assert_eq!(
                String::from_utf8_lossy(&dimensions.stdout),
                format!("{expected_width}x{expected_height}")
            );
        }
        assert_eq!(std::fs::read(&source).expect("source after"), source_before);
    }

    #[tokio::test]
    async fn rejects_animated_sources_instead_of_silently_exporting_one_frame() {
        if engines::resolve_tool("magick").is_none() {
            return;
        }

        let directory = tempfile::tempdir().expect("fixture directory");
        let source = directory.path().join("animated.gif");
        let status = tool_command("magick")
            .args(["-size", "32x32", "xc:red", "-size", "32x32", "xc:blue"])
            .arg(&source)
            .status()
            .await
            .expect("generate animated source");
        assert!(status.success());

        let result = inspect_static_source(&source).await;
        assert!(matches!(result, Err(ConversionError::ProcessFailed { .. })));
    }

    #[tokio::test]
    async fn cancellation_removes_a_partial_variant() {
        let directory = tempfile::tempdir().expect("fixture directory");
        let output = directory.path().join("partial.webp");
        std::fs::write(&output, b"partial").expect("partial fixture");
        let command = tool_command("sleep");
        let cancel_token = CancellationToken::new();
        let cancellation = cancel_token.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(20)).await;
            cancellation.cancel();
        });
        let mut command = command;
        command.arg("30");

        let result =
            run_image_process(command, &output, cancel_token, Duration::from_secs(5)).await;
        assert!(matches!(result, Err(ConversionError::Cancelled)));
        assert!(!output.exists());
    }
}
