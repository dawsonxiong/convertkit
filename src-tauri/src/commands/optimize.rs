use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::Deserialize;
use tauri::{AppHandle, Emitter};
use tokio_util::sync::CancellationToken;

use crate::engines::process::{run_process, ProcessMessages};
use crate::engines::verification::{
    inspect_image_structure, magick_format_matches, ImageStructure,
};
#[cfg(test)]
use crate::engines::verification::{parse_image_structure, ImageFrameStructure};
use crate::engines::{self, tool_command, ConversionResult};
use crate::error::ConversionError;
use crate::formats::{FileCategory, Format};
use crate::progress::ProgressPayload;

use super::output::{prepare_output, OutputOptions};

const OPTIMIZE_TIMEOUT: Duration = Duration::from_secs(10 * 60);
pub(super) const MIN_IMAGE_TARGET_BYTES: u64 = 16 * 1024;
pub(super) const MAX_IMAGE_TARGET_BYTES: u64 = 100 * 1024 * 1024;
const TARGET_QUALITY_CANDIDATES: [u8; 14] = [95, 90, 85, 80, 75, 70, 60, 50, 40, 30, 20, 10, 5, 1];

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ImageOptimizationGoal {
    #[default]
    Quality,
    FileSize,
}

#[derive(Debug)]
struct VerifiedCandidate {
    path: PathBuf,
    size: u64,
}

#[derive(Debug)]
enum OptimizationOutcome {
    Optimized(ConversionResult),
    Unchanged { source_size: u64 },
}

pub(super) async fn optimize_image(
    app: AppHandle,
    input_path: String,
    keep_metadata: bool,
    compression_goal: ImageOptimizationGoal,
    target_size_bytes: Option<u64>,
    job_id: Option<String>,
    output_options: Option<OutputOptions>,
) -> Result<ConversionResult, ConversionError> {
    let input = PathBuf::from(&input_path);
    if !input.is_file() {
        return Err(ConversionError::InputNotFound { path: input_path });
    }

    let format = crate::formats::Format::from_extension(
        input
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or_default(),
    )
    .ok_or_else(|| ConversionError::UnsupportedConversion {
        input: input_path.clone(),
        output: "optimized image".into(),
    })?;

    if format.category() != FileCategory::Image {
        return Err(ConversionError::UnsupportedConversion {
            input: format.label().into(),
            output: "optimized image".into(),
        });
    }

    let target_size =
        validate_image_optimization_settings(&input, format, compression_goal, target_size_bytes)?;

    if engines::resolve_tool("magick").is_none() {
        return Err(ConversionError::MissingDependency {
            tool: "magick".into(),
            install_hint: "Install ImageMagick to optimize images".into(),
        });
    }

    let extension = format.extension();
    let prepared_output = prepare_output(&input, extension, "-optimized", output_options)?;
    let active_job = super::jobs::ActiveJobGuard::register(app.clone(), job_id)?;
    let cancel_token = active_job.cancel_token();
    let event_job_id = active_job.job_id().to_owned();

    let _ = app.emit(
        "conversion-progress",
        ProgressPayload {
            job_id: event_job_id.clone(),
            percent: -1,
            stage: "Optimizing image".into(),
        },
    );

    let started = Instant::now();
    let result = match target_size {
        Some(target_size) => {
            run_target_optimization(
                Some(&app),
                &event_job_id,
                &input,
                prepared_output.working_path(),
                format,
                keep_metadata,
                target_size,
                cancel_token,
            )
            .await
        }
        None => {
            run_optimization(
                &input,
                prepared_output.working_path(),
                format,
                keep_metadata,
                cancel_token,
            )
            .await
        }
    };

    match result {
        Ok(OptimizationOutcome::Optimized(result)) => {
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
        Ok(OptimizationOutcome::Unchanged { source_size }) => {
            let result = ConversionResult {
                output_path: input.to_string_lossy().into_owned(),
                output_paths: vec![input.to_string_lossy().into_owned()],
                output_size: source_size,
                duration_ms: started.elapsed().as_millis() as u64,
                undo_manifest: None,
            };
            let _ = app.emit(
                "conversion-progress",
                ProgressPayload {
                    job_id: event_job_id,
                    percent: 100,
                    stage: "Already optimized".into(),
                },
            );
            Ok(result)
        }
        Err(error) => Err(error),
    }
}

pub(super) fn supports_image_target_size(format: Format) -> bool {
    matches!(
        format,
        Format::Jpg | Format::WebP | Format::Avif | Format::Heic
    )
}

pub(super) fn validate_image_optimization_settings(
    input: &Path,
    format: Format,
    compression_goal: ImageOptimizationGoal,
    target_size_bytes: Option<u64>,
) -> Result<Option<u64>, ConversionError> {
    if compression_goal == ImageOptimizationGoal::Quality {
        return Ok(None);
    }
    if !supports_image_target_size(format) {
        return Err(ConversionError::UnsupportedConversion {
            input: "File-size targeting is available for JPEG, WebP, AVIF, and HEIC images only."
                .into(),
            output: "target-size image".into(),
        });
    }
    let target = target_size_bytes
        .filter(|target| {
            (MIN_IMAGE_TARGET_BYTES..=MAX_IMAGE_TARGET_BYTES).contains(target) && target % 1024 == 0
        })
        .ok_or_else(|| ConversionError::UnsupportedConversion {
            input: "Choose a whole-KiB target from 16 KiB to 102,400 KiB.".into(),
            output: "target-size image".into(),
        })?;
    let source_size = std::fs::metadata(input)
        .map_err(|_| ConversionError::InputNotFound {
            path: input.to_string_lossy().into_owned(),
        })?
        .len();
    if target >= source_size {
        return Err(ConversionError::UnsupportedConversion {
            input: "Choose a target smaller than the source image.".into(),
            output: "target-size image".into(),
        });
    }
    Ok(Some(target))
}

#[allow(clippy::too_many_arguments)]
async fn run_target_optimization(
    app: Option<&AppHandle>,
    job_id: &str,
    input: &Path,
    output: &Path,
    expected_format: Format,
    keep_metadata: bool,
    target_size: u64,
    cancel_token: CancellationToken,
) -> Result<OptimizationOutcome, ConversionError> {
    let source_structure = inspect_image_structure(input, cancel_token.clone()).await?;
    if !magick_format_matches(expected_format, &source_structure.magick_format) {
        return Err(optimization_verification_error(
            "The source contents do not match the image format in its filename",
            &source_structure.magick_format,
        ));
    }

    let temp = tempfile::Builder::new()
        .prefix("convertkit-image-size-")
        .tempdir()
        .map_err(|error| ConversionError::ProcessFailed {
            message: format!("Could not create target-size image workspace: {error}"),
            stderr: String::new(),
            exit_code: None,
        })?;
    let extension = expected_format.extension();
    let mut smallest_bytes = None;
    let mut first_failure = None;

    for (index, quality) in TARGET_QUALITY_CANDIDATES.iter().copied().enumerate() {
        if cancel_token.is_cancelled() {
            engines::cleanup_partial(output);
            return Err(ConversionError::Cancelled);
        }
        let candidate = temp.path().join(format!("candidate-{index}.{extension}"));
        if let Some(app) = app {
            let _ = app.emit(
                "conversion-progress",
                ProgressPayload {
                    job_id: job_id.to_owned(),
                    percent: 5 + ((index * 85) / TARGET_QUALITY_CANDIDATES.len()) as i32,
                    stage: format!(
                        "Trying image size {} of {}",
                        index + 1,
                        TARGET_QUALITY_CANDIDATES.len()
                    ),
                },
            );
        }

        let attempt = match run_target_candidate(
            input,
            &candidate,
            expected_format,
            quality,
            keep_metadata,
            cancel_token.clone(),
        )
        .await
        {
            Ok(()) => {
                verify_candidate(
                    &candidate,
                    &source_structure,
                    expected_format,
                    cancel_token.clone(),
                )
                .await
            }
            Err(error) => Err(error),
        };
        let verified = match attempt {
            Ok(candidate) => candidate,
            Err(ConversionError::Cancelled) => return Err(ConversionError::Cancelled),
            Err(error) => {
                first_failure.get_or_insert(error);
                engines::cleanup_partial(&candidate);
                continue;
            }
        };

        smallest_bytes =
            Some(smallest_bytes.map_or(verified.size, |size: u64| size.min(verified.size)));
        if verified.size > target_size {
            continue;
        }
        if cancel_token.is_cancelled() {
            return Err(ConversionError::Cancelled);
        }
        std::fs::copy(&verified.path, output).map_err(|error| ConversionError::ProcessFailed {
            message: format!("Could not save target-size image: {error}"),
            stderr: String::new(),
            exit_code: None,
        })?;
        if cancel_token.is_cancelled() {
            engines::cleanup_partial(output);
            return Err(ConversionError::Cancelled);
        }
        let output_size = verified_regular_file_size(output)?;
        if output_size > target_size {
            engines::cleanup_partial(output);
            return Err(ConversionError::TargetSizeUnreachable {
                target_bytes: target_size,
                smallest_bytes: Some(output_size),
            });
        }
        return Ok(OptimizationOutcome::Optimized(ConversionResult {
            output_path: output.to_string_lossy().into_owned(),
            output_paths: Vec::new(),
            output_size,
            duration_ms: 0,
            undo_manifest: None,
        }));
    }

    engines::cleanup_partial(output);
    if smallest_bytes.is_none() {
        if let Some(error) = first_failure {
            return Err(error);
        }
    }
    Err(ConversionError::TargetSizeUnreachable {
        target_bytes: target_size,
        smallest_bytes,
    })
}

fn target_candidate_arguments(format: Format, quality: u8) -> Vec<OsString> {
    let mut arguments = Vec::new();
    match format {
        Format::Jpg => arguments
            .extend(["-sampling-factor", "4:2:0", "-interlace", "Plane"].map(OsString::from)),
        Format::WebP => {
            arguments.extend(["-define", "webp:method=6"].map(OsString::from));
        }
        Format::Avif | Format::Heic => {}
        _ => unreachable!("target-size validation rejects lossless image formats"),
    }
    arguments.extend([
        OsString::from("-quality"),
        OsString::from(quality.to_string()),
    ]);
    arguments
}

async fn run_target_candidate(
    input: &Path,
    output: &Path,
    format: Format,
    quality: u8,
    keep_metadata: bool,
    cancel_token: CancellationToken,
) -> Result<(), ConversionError> {
    let mut command = tool_command("magick");
    command.arg(input);
    if !keep_metadata {
        command.args(["-auto-orient", "-strip"]);
    }
    command
        .args(target_candidate_arguments(format, quality))
        .arg(output);
    run_optimizer_process(command, output, cancel_token).await
}

async fn run_optimization(
    input: &Path,
    output: &Path,
    expected_format: Format,
    keep_metadata: bool,
    cancel_token: CancellationToken,
) -> Result<OptimizationOutcome, ConversionError> {
    let extension = input
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("png")
        .to_ascii_lowercase();
    let source_size = verified_regular_file_size(input)?;
    let source_structure = inspect_image_structure(input, cancel_token.clone()).await?;
    if !magick_format_matches(expected_format, &source_structure.magick_format) {
        return Err(optimization_verification_error(
            "The source contents do not match the image format in its filename",
            &source_structure.magick_format,
        ));
    }

    let temp = tempfile::tempdir().map_err(|error| ConversionError::ProcessFailed {
        message: format!("Could not create optimization workspace: {error}"),
        stderr: String::new(),
        exit_code: None,
    })?;

    let strategies = optimization_strategies(&extension);
    // The source is the baseline candidate. Only a verified, strictly smaller
    // derivative is allowed to displace it.
    let mut best: Option<VerifiedCandidate> = None;

    for (index, arguments) in strategies.iter().enumerate() {
        let candidate = temp.path().join(format!("candidate-{index}.{extension}"));
        match run_candidate(
            input,
            &candidate,
            arguments,
            keep_metadata,
            cancel_token.clone(),
        )
        .await
        {
            Ok(()) => {
                match verify_candidate(
                    &candidate,
                    &source_structure,
                    expected_format,
                    cancel_token.clone(),
                )
                .await
                {
                    Ok(candidate) => consider_candidate(&mut best, candidate, source_size),
                    Err(ConversionError::Cancelled) => return Err(ConversionError::Cancelled),
                    Err(_) => engines::cleanup_partial(&candidate),
                }
            }
            Err(ConversionError::Cancelled) => return Err(ConversionError::Cancelled),
            Err(_) => {}
        }
    }

    let specialized = temp
        .path()
        .join(format!("candidate-specialized.{extension}"));
    match run_specialized_candidate(
        input,
        &specialized,
        &extension,
        keep_metadata,
        cancel_token.clone(),
    )
    .await
    {
        Ok(true) => {
            match verify_candidate(
                &specialized,
                &source_structure,
                expected_format,
                cancel_token,
            )
            .await
            {
                Ok(candidate) => consider_candidate(&mut best, candidate, source_size),
                Err(ConversionError::Cancelled) => return Err(ConversionError::Cancelled),
                Err(_) => engines::cleanup_partial(&specialized),
            }
        }
        Ok(false) => {}
        Err(ConversionError::Cancelled) => return Err(ConversionError::Cancelled),
        Err(_) => {}
    }

    let Some(best) = best else {
        engines::cleanup_partial(output);
        return Ok(OptimizationOutcome::Unchanged { source_size });
    };

    std::fs::copy(best.path, output).map_err(|error| ConversionError::ProcessFailed {
        message: format!("Could not save optimized image: {error}"),
        stderr: String::new(),
        exit_code: None,
    })?;

    Ok(OptimizationOutcome::Optimized(ConversionResult {
        output_path: output.to_string_lossy().into(),
        output_paths: Vec::new(),
        output_size: best.size,
        duration_ms: 0,
        undo_manifest: None,
    }))
}

fn verified_regular_file_size(path: &Path) -> Result<u64, ConversionError> {
    match std::fs::metadata(path) {
        Ok(metadata) if metadata.is_file() && metadata.len() > 0 => Ok(metadata.len()),
        _ => Err(ConversionError::OutputMissing),
    }
}

async fn verify_candidate(
    path: &Path,
    source_structure: &ImageStructure,
    expected_format: Format,
    cancel_token: CancellationToken,
) -> Result<VerifiedCandidate, ConversionError> {
    let size = verified_regular_file_size(path)?;
    let structure = inspect_image_structure(path, cancel_token).await?;
    if !candidate_matches_source(source_structure, &structure, expected_format) {
        return Err(optimization_verification_error(
            "An optimization candidate changed the image format, dimensions, or frame count",
            &structure.magick_format,
        ));
    }
    Ok(VerifiedCandidate {
        path: path.to_path_buf(),
        size,
    })
}

fn candidate_matches_source(
    source: &ImageStructure,
    candidate: &ImageStructure,
    expected_format: Format,
) -> bool {
    magick_format_matches(expected_format, &source.magick_format)
        && magick_format_matches(expected_format, &candidate.magick_format)
        && candidate.frames == source.frames
}

fn consider_candidate(
    best: &mut Option<VerifiedCandidate>,
    candidate: VerifiedCandidate,
    source_size: u64,
) {
    if candidate.size >= source_size {
        return;
    }
    if best
        .as_ref()
        .is_none_or(|current| candidate.size < current.size)
    {
        *best = Some(candidate);
    }
}

fn optimization_verification_error(message: &str, stderr: &str) -> ConversionError {
    ConversionError::ProcessFailed {
        message: message.into(),
        stderr: stderr.into(),
        exit_code: None,
    }
}

async fn run_specialized_candidate(
    input: &Path,
    output: &Path,
    extension: &str,
    keep_metadata: bool,
    cancel_token: CancellationToken,
) -> Result<bool, ConversionError> {
    let (tool, mut arguments): (&str, Vec<&str>) = match extension {
        "png" if engines::resolve_tool("oxipng").is_some() => {
            ("oxipng", vec!["-o", "4", "--quiet"])
        }
        "jpg" | "jpeg" if engines::resolve_tool("jpegoptim").is_some() => {
            ("jpegoptim", vec!["--quiet", "--auto-mode"])
        }
        "gif" if engines::resolve_tool("gifsicle").is_some() => {
            let mut command = tool_command("gifsicle");
            command.args(["-O3", "--output"]).arg(output).arg(input);
            run_optimizer_process(command, output, cancel_token).await?;
            return Ok(true);
        }
        _ => return Ok(false),
    };

    std::fs::copy(input, output).map_err(|error| ConversionError::ProcessFailed {
        message: format!("Could not prepare specialized optimization: {error}"),
        stderr: String::new(),
        exit_code: None,
    })?;

    match tool {
        "oxipng" if !keep_metadata => arguments.extend(["--strip", "safe"]),
        "jpegoptim" if keep_metadata => arguments.push("--keep-all"),
        "jpegoptim" => arguments.push("--strip-all"),
        _ => {}
    }

    let mut command = tool_command(tool);
    command.args(arguments).arg(output);
    run_optimizer_process(command, output, cancel_token).await?;
    Ok(true)
}

async fn run_candidate(
    input: &Path,
    output: &Path,
    arguments: &[&str],
    keep_metadata: bool,
    cancel_token: CancellationToken,
) -> Result<(), ConversionError> {
    let mut command = tool_command("magick");
    command.arg(input);
    if !keep_metadata {
        command.args(["-auto-orient", "-strip"]);
    }
    command.args(arguments).arg(output);
    run_optimizer_process(command, output, cancel_token).await
}

async fn run_optimizer_process(
    command: tokio::process::Command,
    output: &Path,
    cancel_token: CancellationToken,
) -> Result<(), ConversionError> {
    run_process(
        command,
        cancel_token,
        OPTIMIZE_TIMEOUT,
        &[output],
        ProcessMessages {
            start: "Failed to start image optimization",
            wait: "Image optimization process could not be awaited",
            failure: "Image optimization failed",
        },
    )
    .await
}

fn optimization_strategies(extension: &str) -> Vec<Vec<&'static str>> {
    // Keep Optimize a predictable high-quality action. Lower lossy qualities saved
    // substantially more on photographs, but produced disproportionately worse text and
    // edge artifacts on interface and icon fixtures, while lossless formats have no
    // equivalent quality tier. Delivery-specific tradeoffs belong in Export images.
    match extension {
        "png" => vec![
            vec![
                "-define",
                "png:compression-level=9",
                "-define",
                "png:compression-strategy=1",
            ],
            vec![
                "-define",
                "png:compression-level=9",
                "-define",
                "png:compression-strategy=2",
            ],
        ],
        "jpg" | "jpeg" => vec![
            vec![
                "-sampling-factor",
                "4:2:0",
                "-interlace",
                "Plane",
                "-quality",
                "92",
            ],
            vec![
                "-sampling-factor",
                "4:2:0",
                "-interlace",
                "None",
                "-quality",
                "92",
            ],
        ],
        "webp" => vec![
            vec!["-quality", "90", "-define", "webp:method=4"],
            vec!["-quality", "90", "-define", "webp:method=6"],
        ],
        "gif" => vec![vec!["-layers", "Optimize"]],
        "tif" | "tiff" => vec![vec!["-compress", "Zip"]],
        "avif" | "heic" | "heif" => vec![vec!["-quality", "90"]],
        _ => vec![vec![]],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quality_values(extension: &str) -> Vec<&'static str> {
        optimization_strategies(extension)
            .iter()
            .flat_map(|arguments| {
                arguments
                    .windows(2)
                    .filter_map(|pair| (pair[0] == "-quality").then_some(pair[1]))
            })
            .collect()
    }

    #[test]
    fn keeps_one_high_quality_contract_instead_of_format_inconsistent_presets() {
        assert_eq!(quality_values("jpg"), vec!["92", "92"]);
        assert_eq!(quality_values("jpeg"), vec!["92", "92"]);
        assert_eq!(quality_values("webp"), vec!["90", "90"]);
        assert_eq!(quality_values("avif"), vec!["90"]);
        assert_eq!(quality_values("heic"), vec!["90"]);

        for lossless in ["png", "gif", "tif", "tiff"] {
            assert!(quality_values(lossless).is_empty());
        }
    }

    #[test]
    fn validates_image_target_formats_and_whole_kib_bounds() {
        let directory = tempfile::tempdir().expect("tempdir");
        let source = directory.path().join("source.jpg");
        std::fs::write(&source, vec![0_u8; 64 * 1024]).expect("source fixture");

        assert_eq!(
            validate_image_optimization_settings(
                &source,
                Format::Jpg,
                ImageOptimizationGoal::Quality,
                Some(1),
            )
            .expect("quality ignores target"),
            None
        );
        assert_eq!(
            validate_image_optimization_settings(
                &source,
                Format::Jpg,
                ImageOptimizationGoal::FileSize,
                Some(16 * 1024),
            )
            .expect("valid image target"),
            Some(16 * 1024)
        );

        for target in [
            None,
            Some(MIN_IMAGE_TARGET_BYTES - 1024),
            Some(MIN_IMAGE_TARGET_BYTES + 1),
            Some(MAX_IMAGE_TARGET_BYTES + 1024),
            Some(64 * 1024),
        ] {
            assert!(validate_image_optimization_settings(
                &source,
                Format::Jpg,
                ImageOptimizationGoal::FileSize,
                target,
            )
            .is_err());
        }
        for format in [
            Format::Png,
            Format::Gif,
            Format::Tiff,
            Format::Bmp,
            Format::Ico,
        ] {
            assert!(validate_image_optimization_settings(
                &source,
                format,
                ImageOptimizationGoal::FileSize,
                Some(16 * 1024),
            )
            .is_err());
        }
        for format in [Format::Jpg, Format::WebP, Format::Avif, Format::Heic] {
            assert!(supports_image_target_size(format));
        }
    }

    #[test]
    fn target_candidate_arguments_are_format_specific_and_bounded() {
        let jpg = target_candidate_arguments(Format::Jpg, 75);
        assert!(jpg.windows(2).any(|pair| pair == ["-quality", "75"]));
        assert!(jpg
            .windows(2)
            .any(|pair| pair == ["-sampling-factor", "4:2:0"]));

        let webp = target_candidate_arguments(Format::WebP, 40);
        assert!(webp.windows(2).any(|pair| pair == ["-quality", "40"]));
        assert!(webp
            .windows(2)
            .any(|pair| pair == ["-define", "webp:method=6"]));
        assert_eq!(TARGET_QUALITY_CANDIDATES.len(), 14);
        assert_eq!(TARGET_QUALITY_CANDIDATES.first(), Some(&95));
        assert_eq!(TARGET_QUALITY_CANDIDATES.last(), Some(&1));
        assert!(TARGET_QUALITY_CANDIDATES
            .windows(2)
            .all(|pair| pair[0] > pair[1]));
    }

    #[test]
    fn parses_and_bounds_complete_image_structure() {
        let structure = parse_image_structure(b"GIF|320|240|2\nGIF|160|120|2\n")
            .expect("valid multi-frame structure");
        assert_eq!(structure.magick_format, "GIF");
        assert_eq!(
            structure.frames,
            vec![
                ImageFrameStructure {
                    width: 320,
                    height: 240,
                },
                ImageFrameStructure {
                    width: 160,
                    height: 120,
                },
            ]
        );

        for invalid in [
            b"".as_slice(),
            b"PNG|0|100|1\n",
            b"PNG|100|100|2\n",
            b"PNG|100|100|2\nJPEG|100|100|2\n",
            b"PNG|100|100|1|unexpected\n",
        ] {
            assert!(parse_image_structure(invalid).is_err());
        }
    }

    #[test]
    fn rejects_candidates_that_change_format_dimensions_or_frame_count() {
        let source = ImageStructure {
            magick_format: "PNG".into(),
            frames: vec![ImageFrameStructure {
                width: 1_200,
                height: 942,
            }],
        };
        let wrong_format = ImageStructure {
            magick_format: "JPEG".into(),
            frames: source.frames.clone(),
        };
        let wrong_dimensions = ImageStructure {
            magick_format: "PNG".into(),
            frames: vec![ImageFrameStructure {
                width: 600,
                height: 471,
            }],
        };
        let extra_frame = ImageStructure {
            magick_format: "PNG".into(),
            frames: vec![source.frames[0].clone(), source.frames[0].clone()],
        };

        assert!(candidate_matches_source(&source, &source, Format::Png));
        assert!(!candidate_matches_source(
            &source,
            &wrong_format,
            Format::Png
        ));
        assert!(!candidate_matches_source(
            &source,
            &wrong_dimensions,
            Format::Png
        ));
        assert!(!candidate_matches_source(
            &source,
            &extra_frame,
            Format::Png
        ));

        let heif_source = ImageStructure {
            magick_format: "HEIF".into(),
            frames: source.frames.clone(),
        };
        let heic_candidate = ImageStructure {
            magick_format: "HEIC".into(),
            frames: source.frames,
        };
        assert!(candidate_matches_source(
            &heif_source,
            &heic_candidate,
            Format::Heic
        ));
    }

    #[test]
    fn source_remains_selected_until_a_strictly_smaller_candidate_exists() {
        let mut best = None;
        consider_candidate(
            &mut best,
            VerifiedCandidate {
                path: PathBuf::from("larger.png"),
                size: 101,
            },
            100,
        );
        consider_candidate(
            &mut best,
            VerifiedCandidate {
                path: PathBuf::from("equal.png"),
                size: 100,
            },
            100,
        );
        assert!(best.is_none(), "source is the valid fallback candidate");

        consider_candidate(
            &mut best,
            VerifiedCandidate {
                path: PathBuf::from("smaller.png"),
                size: 90,
            },
            100,
        );
        consider_candidate(
            &mut best,
            VerifiedCandidate {
                path: PathBuf::from("smallest.png"),
                size: 80,
            },
            100,
        );
        consider_candidate(
            &mut best,
            VerifiedCandidate {
                path: PathBuf::from("between.png"),
                size: 85,
            },
            100,
        );

        let selected = best.expect("strictly smaller candidate");
        assert_eq!(selected.path, PathBuf::from("smallest.png"));
        assert_eq!(selected.size, 80);
    }

    #[test]
    fn maps_filename_formats_to_imagemagick_identity() {
        assert!(magick_format_matches(Format::Jpg, "JPEG"));
        assert!(magick_format_matches(Format::Tiff, "TIFF"));
        assert!(magick_format_matches(Format::Heic, "HEIF"));
        assert!(!magick_format_matches(Format::Png, "JPEG"));
        assert!(!magick_format_matches(Format::Svg, "SVG"));
    }

    #[tokio::test]
    async fn preserves_the_source_when_reencoding_cannot_make_it_smaller() {
        if engines::resolve_tool("magick").is_none() {
            return;
        }
        let directory = tempfile::tempdir().expect("tempdir");
        let source = directory.path().join("source.bmp");
        let output = directory.path().join("optimized.bmp");
        let status = tool_command("magick")
            .args(["-size", "8x6", "xc:#345678"])
            .arg(&source)
            .status()
            .await
            .expect("create BMP fixture");
        assert!(status.success());
        let source_bytes = std::fs::read(&source).expect("source bytes");

        let outcome = run_optimization(
            &source,
            &output,
            Format::Bmp,
            true,
            CancellationToken::new(),
        )
        .await
        .expect("optimization outcome");

        assert!(matches!(outcome, OptimizationOutcome::Unchanged { .. }));
        assert!(!output.exists());
        assert_eq!(
            std::fs::read(&source).expect("preserved source"),
            source_bytes
        );
    }

    #[tokio::test]
    async fn commits_only_a_strictly_smaller_structurally_identical_candidate() {
        use std::io::Write;

        if engines::resolve_tool("magick").is_none() {
            return;
        }
        let directory = tempfile::tempdir().expect("tempdir");
        let source = directory.path().join("source.png");
        let output = directory.path().join("optimized.png");
        let status = tool_command("magick")
            .args(["-size", "32x24", "xc:#345678"])
            .arg(&source)
            .status()
            .await
            .expect("create PNG fixture");
        assert!(status.success());
        let expected_structure = inspect_image_structure(&source, CancellationToken::new())
            .await
            .expect("source structure");
        let mut source_file = std::fs::OpenOptions::new()
            .append(true)
            .open(&source)
            .expect("open PNG fixture");
        source_file
            .write_all(&vec![0_u8; 64 * 1024])
            .expect("add inert trailing bytes");
        drop(source_file);
        let source_size = std::fs::metadata(&source).expect("source metadata").len();

        let outcome = run_optimization(
            &source,
            &output,
            Format::Png,
            true,
            CancellationToken::new(),
        )
        .await
        .expect("optimization outcome");
        let OptimizationOutcome::Optimized(result) = outcome else {
            panic!("expected a strictly smaller candidate");
        };

        assert!(result.output_size < source_size);
        assert_eq!(
            inspect_image_structure(&output, CancellationToken::new())
                .await
                .expect("output structure"),
            expected_structure
        );
        assert_eq!(
            std::fs::metadata(&source).expect("preserved source").len(),
            source_size
        );
    }

    #[tokio::test]
    async fn target_size_uses_an_exact_ceiling_and_preserves_the_source() {
        use std::io::Write;

        if engines::resolve_tool("magick").is_none() {
            return;
        }
        let directory = tempfile::tempdir().expect("tempdir");
        let source = directory.path().join("source.jpg");
        let output = directory.path().join("optimized.jpg");
        let status = tool_command("magick")
            .args(["-size", "64x48", "xc:#345678", "-quality", "100"])
            .arg(&source)
            .status()
            .await
            .expect("create JPEG fixture");
        assert!(status.success());
        let mut source_file = std::fs::OpenOptions::new()
            .append(true)
            .open(&source)
            .expect("open JPEG fixture");
        source_file
            .write_all(&vec![0_u8; 64 * 1024])
            .expect("add inert trailing bytes");
        drop(source_file);
        let source_bytes = std::fs::read(&source).expect("source bytes");
        let source_structure = inspect_image_structure(&source, CancellationToken::new())
            .await
            .expect("source structure");
        let target = 16 * 1024;

        let outcome = run_target_optimization(
            None,
            "target-test",
            &source,
            &output,
            Format::Jpg,
            false,
            target,
            CancellationToken::new(),
        )
        .await
        .expect("reachable target");
        let OptimizationOutcome::Optimized(result) = outcome else {
            panic!("target-size work must produce a derivative");
        };

        assert!(result.output_size <= target);
        assert_eq!(
            std::fs::metadata(&output).expect("output metadata").len(),
            result.output_size
        );
        assert_eq!(
            inspect_image_structure(&output, CancellationToken::new())
                .await
                .expect("output structure"),
            source_structure
        );
        assert_eq!(
            std::fs::read(&source).expect("preserved source"),
            source_bytes
        );
    }

    #[tokio::test]
    async fn target_size_reports_typed_unreachable_and_cleans_output() {
        if engines::resolve_tool("magick").is_none() {
            return;
        }
        let directory = tempfile::tempdir().expect("tempdir");
        let source = directory.path().join("source.jpg");
        let output = directory.path().join("optimized.jpg");
        let status = tool_command("magick")
            .args(["-size", "32x24", "xc:#345678"])
            .arg(&source)
            .status()
            .await
            .expect("create JPEG fixture");
        assert!(status.success());
        let source_bytes = std::fs::read(&source).expect("source bytes");

        let error = run_target_optimization(
            None,
            "unreachable-test",
            &source,
            &output,
            Format::Jpg,
            false,
            1,
            CancellationToken::new(),
        )
        .await
        .expect_err("one-byte target must be unreachable");

        assert!(matches!(
            error,
            ConversionError::TargetSizeUnreachable {
                target_bytes: 1,
                smallest_bytes: Some(size),
            } if size > 1
        ));
        assert!(!output.exists());
        assert_eq!(
            std::fs::read(&source).expect("preserved source"),
            source_bytes
        );
    }

    #[tokio::test]
    async fn target_size_cancellation_leaves_no_partial_or_source_change() {
        if engines::resolve_tool("magick").is_none() {
            return;
        }
        let directory = tempfile::tempdir().expect("tempdir");
        let source = directory.path().join("source.jpg");
        let output = directory.path().join("optimized.jpg");
        let status = tool_command("magick")
            .args(["-size", "32x24", "xc:#345678"])
            .arg(&source)
            .status()
            .await
            .expect("create JPEG fixture");
        assert!(status.success());
        let source_bytes = std::fs::read(&source).expect("source bytes");
        let cancel_token = CancellationToken::new();
        cancel_token.cancel();

        let error = run_target_optimization(
            None,
            "cancel-test",
            &source,
            &output,
            Format::Jpg,
            false,
            16 * 1024,
            cancel_token,
        )
        .await
        .expect_err("pre-cancelled target-size work");

        assert!(matches!(error, ConversionError::Cancelled));
        assert!(!output.exists());
        assert_eq!(
            std::fs::read(&source).expect("preserved source"),
            source_bytes
        );
    }
}
