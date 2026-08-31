use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

use serde::Deserialize;
use tauri::{AppHandle, Emitter};

use crate::engines::{resolve_tool, tool_command, verification, ConversionResult};
use crate::error::ConversionError;
use crate::progress::ProgressPayload;

use super::output::{prepare_output, OutputOptions};

const VERIFY_TIMEOUT: Duration = Duration::from_secs(15);
const CONTACT_COLUMNS: u32 = 4;
const CONTACT_ROWS: u32 = 3;
const CONTACT_TILE_WIDTH: u32 = 320;
const CONTACT_TILE_HEIGHT: u32 = 180;
const CONTACT_PADDING: u32 = 4;

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ThumbnailMode {
    Frame,
    ContactSheet,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ThumbnailOutputFormat {
    Jpeg,
    Png,
}

impl ThumbnailOutputFormat {
    pub(super) fn extension(self) -> &'static str {
        match self {
            Self::Jpeg => "jpg",
            Self::Png => "png",
        }
    }

    fn encoder(self) -> &'static str {
        match self {
            Self::Jpeg => "mjpeg",
            Self::Png => "png",
        }
    }

    fn codec_arguments(self) -> &'static [&'static str] {
        match self {
            Self::Jpeg => &["-c:v", "mjpeg", "-q:v", "2", "-pix_fmt", "yuvj420p"],
            Self::Png => &["-c:v", "png", "-compression_level", "6"],
        }
    }
}

pub(super) fn thumbnail_support(format: ThumbnailOutputFormat) -> Result<(), Vec<String>> {
    if super::video::video_encoder_available(format.encoder()) {
        Ok(())
    } else {
        Err(vec![format.encoder().into()])
    }
}

fn thumbnail_arguments(
    input: &Path,
    output: &Path,
    mode: ThumbnailMode,
    format: ThumbnailOutputFormat,
    duration_ms: u64,
) -> Vec<OsString> {
    let mut arguments = vec![
        OsString::from("-hide_banner"),
        OsString::from("-loglevel"),
        OsString::from("error"),
    ];

    if mode == ThumbnailMode::Frame {
        let midpoint_seconds = duration_ms as f64 / 2_000.0;
        arguments.extend([
            OsString::from("-ss"),
            OsString::from(format!("{midpoint_seconds:.3}")),
        ]);
    }

    arguments.extend([
        OsString::from("-i"),
        input.as_os_str().to_os_string(),
        OsString::from("-map"),
        OsString::from("0:v:0"),
        OsString::from("-an"),
        OsString::from("-sn"),
        OsString::from("-dn"),
        OsString::from("-vf"),
        OsString::from(match mode {
            ThumbnailMode::Frame => {
                "scale=w='min(1280,iw)':h='min(720,ih)':force_original_aspect_ratio=decrease:force_divisible_by=2".to_string()
            }
            ThumbnailMode::ContactSheet => {
                let duration_seconds = (duration_ms as f64 / 1_000.0).max(0.001);
                let sample_rate = (CONTACT_COLUMNS * CONTACT_ROWS) as f64 / duration_seconds;
                format!(
                    "fps={sample_rate:.8},scale={CONTACT_TILE_WIDTH}:{CONTACT_TILE_HEIGHT}:force_original_aspect_ratio=decrease,pad={CONTACT_TILE_WIDTH}:{CONTACT_TILE_HEIGHT}:(ow-iw)/2:(oh-ih)/2:color=0x101012,tile={CONTACT_COLUMNS}x{CONTACT_ROWS}:padding={CONTACT_PADDING}:margin=0"
                )
            }
        }),
    ]);
    arguments.extend(format.codec_arguments().iter().map(OsString::from));
    arguments.extend([
        OsString::from("-frames:v"),
        OsString::from("1"),
        OsString::from("-update"),
        OsString::from("1"),
        OsString::from("-progress"),
        OsString::from("pipe:1"),
        OsString::from("-nostats"),
        OsString::from("-y"),
        output.as_os_str().to_os_string(),
    ]);
    arguments
}

#[derive(Debug, Default, Deserialize)]
struct ImageProbeOutput {
    #[serde(default)]
    streams: Vec<ImageProbeStream>,
}

#[derive(Debug, Deserialize)]
struct ImageProbeStream {
    codec_name: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
}

async fn verify_thumbnail(
    output: &Path,
    mode: ThumbnailMode,
    format: ThumbnailOutputFormat,
) -> Result<(), ConversionError> {
    verification::nonempty_file(output)?;
    let probe = tokio::time::timeout(
        VERIFY_TIMEOUT,
        tool_command("ffprobe")
            .args([
                "-v",
                "error",
                "-select_streams",
                "v:0",
                "-show_entries",
                "stream=codec_name,width,height",
                "-of",
                "json",
            ])
            .arg(output)
            .stdin(Stdio::null())
            .output(),
    )
    .await
    .map_err(|_| ConversionError::Timeout {
        seconds: VERIFY_TIMEOUT.as_secs(),
    })?
    .map_err(|error| ConversionError::ProcessFailed {
        message: format!("Could not inspect the generated image: {error}"),
        stderr: String::new(),
        exit_code: None,
    })?;
    let parsed = serde_json::from_slice::<ImageProbeOutput>(&probe.stdout).map_err(|error| {
        ConversionError::ProcessFailed {
            message: format!("Could not read the generated image details: {error}"),
            stderr: String::new(),
            exit_code: probe.status.code(),
        }
    })?;
    let stream = parsed.streams.first();
    let width = stream.and_then(|value| value.width).unwrap_or(0);
    let height = stream.and_then(|value| value.height).unwrap_or(0);
    let codec = stream
        .and_then(|value| value.codec_name.as_deref())
        .unwrap_or_default();
    let dimensions_valid = match mode {
        ThumbnailMode::Frame => width > 0 && height > 0 && width <= 1_280 && height <= 720,
        ThumbnailMode::ContactSheet => {
            width == CONTACT_COLUMNS * CONTACT_TILE_WIDTH + (CONTACT_COLUMNS - 1) * CONTACT_PADDING
                && height
                    == CONTACT_ROWS * CONTACT_TILE_HEIGHT + (CONTACT_ROWS - 1) * CONTACT_PADDING
        }
    };
    if !probe.status.success() || codec != format.encoder() || !dimensions_valid {
        return Err(ConversionError::ProcessFailed {
            message: "The generated image could not be verified".into(),
            stderr: String::from_utf8_lossy(&probe.stderr).into_owned(),
            exit_code: probe.status.code(),
        });
    }
    Ok(())
}

pub(super) async fn generate_thumbnail(
    app: AppHandle,
    input_path: String,
    mode: ThumbnailMode,
    output_format: ThumbnailOutputFormat,
    job_id: Option<String>,
    output_options: Option<OutputOptions>,
) -> Result<ConversionResult, ConversionError> {
    let started = Instant::now();
    let input: PathBuf = super::video::validate_video_input(&input_path)?;
    for tool in ["ffmpeg", "ffprobe"] {
        if resolve_tool(tool).is_none() {
            return Err(ConversionError::MissingDependency {
                tool: tool.into(),
                install_hint: "Install FFmpeg with Homebrew".into(),
            });
        }
    }
    thumbnail_support(output_format).map_err(|missing| ConversionError::MissingDependency {
        tool: missing.join(", "),
        install_hint: "Install the full FFmpeg package with Homebrew".into(),
    })?;
    let duration_ms = super::video::probe_duration_ms(&input).await.unwrap_or(0);
    if duration_ms == 0 {
        return Err(ConversionError::ProcessFailed {
            message: "The video duration could not be read".into(),
            stderr: String::new(),
            exit_code: None,
        });
    }

    let prepared = prepare_output(
        &input,
        output_format.extension(),
        "-thumbnail",
        output_options,
    )?;
    let active_job = super::jobs::ActiveJobGuard::register(app.clone(), job_id)?;
    let cancel_token = active_job.cancel_token();
    let event_job_id = active_job.job_id().to_owned();
    let stage = match mode {
        ThumbnailMode::Frame => "Generating thumbnail",
        ThumbnailMode::ContactSheet => "Generating contact sheet",
    };
    let _ = app.emit(
        "conversion-progress",
        ProgressPayload {
            job_id: event_job_id.clone(),
            percent: 0,
            stage: stage.into(),
        },
    );

    super::video::run_ffmpeg_job(
        prepared.working_path(),
        thumbnail_arguments(
            &input,
            prepared.working_path(),
            mode,
            output_format,
            duration_ms,
        ),
        duration_ms,
        stage,
        "Thumbnail generation failed",
        app.clone(),
        event_job_id.clone(),
        cancel_token.clone(),
    )
    .await?;
    if cancel_token.is_cancelled() {
        return Err(ConversionError::Cancelled);
    }
    verify_thumbnail(prepared.working_path(), mode, output_format).await?;

    let mut result = prepared.commit(ConversionResult {
        output_path: String::new(),
        output_paths: Vec::new(),
        output_size: 0,
        duration_ms: started.elapsed().as_millis() as u64,
        undo_manifest: None,
    })?;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn arguments(
        mode: ThumbnailMode,
        format: ThumbnailOutputFormat,
        duration_ms: u64,
    ) -> Vec<String> {
        thumbnail_arguments(
            Path::new("input movie.mkv"),
            Path::new("output image.jpg"),
            mode,
            format,
            duration_ms,
        )
        .into_iter()
        .map(|value| value.to_string_lossy().into_owned())
        .collect()
    }

    #[test]
    fn midpoint_thumbnail_maps_only_one_video_frame() {
        let arguments = arguments(ThumbnailMode::Frame, ThumbnailOutputFormat::Jpeg, 10_000);
        assert!(arguments.windows(2).any(|pair| pair == ["-ss", "5.000"]));
        assert!(arguments.windows(2).any(|pair| pair == ["-map", "0:v:0"]));
        assert!(arguments.windows(2).any(|pair| pair == ["-frames:v", "1"]));
        assert!(arguments.windows(2).any(|pair| pair == ["-c:v", "mjpeg"]));
        assert!(arguments.contains(&"-an".into()));
        assert!(arguments.contains(&"-sn".into()));
    }

    #[test]
    fn contact_sheet_uses_a_fixed_twelve_frame_grid() {
        let arguments = arguments(
            ThumbnailMode::ContactSheet,
            ThumbnailOutputFormat::Png,
            4_000,
        );
        let filter = arguments
            .windows(2)
            .find_map(|pair| (pair[0] == "-vf").then_some(pair[1].as_str()))
            .expect("video filter");
        assert!(filter.contains("fps=3.00000000"));
        assert!(filter.contains("tile=4x3"));
        assert!(arguments.windows(2).any(|pair| pair == ["-c:v", "png"]));
        assert!(!arguments.contains(&"-ss".into()));
    }

    #[tokio::test]
    async fn creates_and_verifies_real_thumbnail_outputs() {
        let Some(ffmpeg) = resolve_tool("ffmpeg") else {
            return;
        };
        if resolve_tool("ffprobe").is_none()
            || thumbnail_support(ThumbnailOutputFormat::Jpeg).is_err()
            || thumbnail_support(ThumbnailOutputFormat::Png).is_err()
        {
            return;
        }
        let directory = tempfile::tempdir().expect("tempdir");
        let input = directory.path().join("video.mp4");
        let generated = std::process::Command::new(&ffmpeg)
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=640x360:rate=24:duration=4",
                "-c:v",
                "mpeg4",
                "-y",
            ])
            .arg(&input)
            .status()
            .expect("video fixture");
        assert!(generated.success());
        let original = std::fs::read(&input).expect("source bytes");

        for (mode, format) in [
            (ThumbnailMode::Frame, ThumbnailOutputFormat::Jpeg),
            (ThumbnailMode::ContactSheet, ThumbnailOutputFormat::Png),
        ] {
            let output = directory
                .path()
                .join(format!("output-{:?}.{}", mode, format.extension()));
            let status = std::process::Command::new(&ffmpeg)
                .args(thumbnail_arguments(&input, &output, mode, format, 4_000))
                .status()
                .expect("thumbnail generation");
            assert!(status.success());
            verify_thumbnail(&output, mode, format)
                .await
                .expect("verified thumbnail");
        }
        assert_eq!(std::fs::read(&input).expect("retained source"), original);
    }
}
