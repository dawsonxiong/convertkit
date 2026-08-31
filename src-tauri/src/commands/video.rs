use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use serde::Deserialize;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio_util::sync::CancellationToken;

use crate::engines::process::{capture_output, finish_output};
use crate::engines::{self, resolve_tool, tool_command, ConversionResult};
use crate::error::ConversionError;
use crate::formats::{FileCategory, Format};
use crate::progress::{parse_ffmpeg_progress, ProgressPayload};

use super::output::{prepare_output, OutputOptions};

const VIDEO_ENCODING_TIMEOUT: Duration = Duration::from_secs(60 * 60);
const OUTPUT_PROBE_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_INPUT_BYTES: u64 = 8 * 1024 * 1024 * 1024 * 1024;
pub(super) const MIN_VIDEO_TARGET_BYTES: u64 = 1024 * 1024;
pub(super) const MAX_VIDEO_TARGET_BYTES: u64 = 100 * 1024 * 1024 * 1024;
const TARGET_MUX_BUDGET_PERCENT: u64 = 92;
const TARGET_RETRY_BUDGET_PERCENT: u64 = 95;
const MIN_VIDEO_BITRATE_KBPS: u64 = 120;
const MIN_AUDIO_BITRATE_KBPS: u64 = 48;
const MAX_VIDEO_BITRATE_KBPS: u64 = 200_000;

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum VideoEncodingPreset {
    Compatible,
    Smaller,
    Web,
    Archive,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum VideoResolution {
    #[default]
    Automatic,
    Original,
    FullHd,
    Hd,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum VideoQuality {
    High,
    #[default]
    Balanced,
    Smallest,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum VideoCompressionGoal {
    #[default]
    Quality,
    FileSize,
}

impl VideoEncodingPreset {
    pub(super) fn extension(self) -> &'static str {
        match self {
            Self::Compatible | Self::Smaller => "mp4",
            Self::Web => "webm",
            Self::Archive => "mkv",
        }
    }

    fn expected_video_codec(self) -> &'static str {
        match self {
            Self::Compatible => "h264",
            Self::Smaller => "hevc",
            Self::Web => "vp9",
            Self::Archive => "ffv1",
        }
    }

    fn expected_audio_codec(self) -> &'static str {
        match self {
            Self::Compatible | Self::Smaller => "aac",
            Self::Web => "opus",
            Self::Archive => "flac",
        }
    }

    fn expected_container(self) -> &'static str {
        match self {
            Self::Compatible | Self::Smaller => "mp4",
            Self::Web => "webm",
            Self::Archive => "matroska",
        }
    }

    fn required_encoders(self) -> &'static [&'static str] {
        match self {
            Self::Compatible => &["libx264", "aac"],
            Self::Smaller => &["libx265", "aac"],
            Self::Web => &["libvpx-vp9", "libopus"],
            Self::Archive => &["ffv1", "flac"],
        }
    }

    fn crf(self, quality: VideoQuality) -> Option<&'static str> {
        match (self, quality) {
            (Self::Compatible, VideoQuality::High) => Some("18"),
            (Self::Compatible, VideoQuality::Balanced) => Some("20"),
            (Self::Compatible, VideoQuality::Smallest) => Some("24"),
            (Self::Smaller, VideoQuality::High) => Some("24"),
            (Self::Smaller, VideoQuality::Balanced) => Some("28"),
            (Self::Smaller, VideoQuality::Smallest) => Some("32"),
            (Self::Web, VideoQuality::High) => Some("27"),
            (Self::Web, VideoQuality::Balanced) => Some("31"),
            (Self::Web, VideoQuality::Smallest) => Some("36"),
            (Self::Archive, _) => None,
        }
    }

    fn resolution_limit(self, resolution: VideoResolution) -> Option<(u32, u32)> {
        if self == Self::Archive {
            return None;
        }
        match resolution {
            VideoResolution::Automatic => match self {
                Self::Smaller | Self::Web => Some((1920, 1080)),
                Self::Compatible | Self::Archive => None,
            },
            VideoResolution::Original => None,
            VideoResolution::FullHd => Some((1920, 1080)),
            VideoResolution::Hd => Some((1280, 720)),
        }
    }

    fn codec_arguments(self, resolution: VideoResolution, quality: VideoQuality) -> Vec<OsString> {
        let mut arguments = Vec::new();
        if let Some((width, height)) = self.resolution_limit(resolution) {
            arguments.extend([
                OsString::from("-vf"),
                OsString::from(format!(
                    "scale=w='min({width},iw)':h='min({height},ih)':force_original_aspect_ratio=decrease:force_divisible_by=2"
                )),
            ]);
        }

        match self {
            Self::Compatible => {
                arguments
                    .extend(["-c:v", "libx264", "-preset", "medium", "-crf"].map(OsString::from));
                arguments.push(OsString::from(self.crf(quality).expect("lossy preset CRF")));
                arguments.extend(
                    [
                        "-pix_fmt",
                        "yuv420p",
                        "-c:a",
                        "aac",
                        "-b:a",
                        "192k",
                        "-movflags",
                        "+faststart",
                    ]
                    .map(OsString::from),
                );
            }
            Self::Smaller => {
                arguments
                    .extend(["-c:v", "libx265", "-preset", "medium", "-crf"].map(OsString::from));
                arguments.push(OsString::from(self.crf(quality).expect("lossy preset CRF")));
                arguments.extend(
                    [
                        "-tag:v",
                        "hvc1",
                        "-pix_fmt",
                        "yuv420p",
                        "-c:a",
                        "aac",
                        "-b:a",
                        "128k",
                        "-movflags",
                        "+faststart",
                    ]
                    .map(OsString::from),
                );
            }
            Self::Web => {
                arguments.extend(["-c:v", "libvpx-vp9", "-crf"].map(OsString::from));
                arguments.push(OsString::from(self.crf(quality).expect("lossy preset CRF")));
                arguments.extend(
                    [
                        "-b:v", "0", "-row-mt", "1", "-c:a", "libopus", "-b:a", "128k",
                    ]
                    .map(OsString::from),
                );
            }
            Self::Archive => arguments.extend(
                [
                    "-c:v",
                    "ffv1",
                    "-level",
                    "3",
                    "-coder",
                    "1",
                    "-context",
                    "1",
                    "-g",
                    "1",
                    "-slices",
                    "16",
                    "-slicecrc",
                    "1",
                    "-c:a",
                    "flac",
                ]
                .map(OsString::from),
            ),
        }
        arguments
    }

    fn target_audio_bitrate_kbps(self) -> u64 {
        match self {
            Self::Compatible | Self::Smaller => 128,
            Self::Web => 96,
            Self::Archive => 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TargetBitrates {
    video_kbps: u64,
    audio_kbps: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PrimaryTimeline {
    duration_ms: u64,
    has_audio: bool,
    exact: bool,
}

#[derive(Debug, Default, Deserialize)]
struct VideoMediaProbe {
    #[serde(default)]
    streams: Vec<VideoMediaStream>,
    #[serde(default)]
    format: VideoMediaFormat,
}

#[derive(Debug, Default, Deserialize)]
struct VideoMediaStream {
    #[serde(default)]
    codec_type: String,
    #[serde(default)]
    codec_name: String,
    width: Option<u32>,
    height: Option<u32>,
    start_time: Option<String>,
    duration: Option<String>,
    #[serde(default)]
    tags: BTreeMap<String, String>,
}

#[derive(Debug, Default, Deserialize)]
struct VideoMediaFormat {
    #[serde(default)]
    format_name: String,
    duration: Option<String>,
}

pub(super) fn validate_compression_goal(
    preset: VideoEncodingPreset,
    goal: VideoCompressionGoal,
    target_size_bytes: Option<u64>,
) -> Result<(), &'static str> {
    if goal == VideoCompressionGoal::Quality {
        return Ok(());
    }
    if preset == VideoEncodingPreset::Archive {
        return Err("File-size targeting is unavailable for the Lossless preset.");
    }
    match target_size_bytes {
        Some(MIN_VIDEO_TARGET_BYTES..=MAX_VIDEO_TARGET_BYTES) => Ok(()),
        _ => Err("Enter a target size between 1 MB and 102,400 MB."),
    }
}

fn target_bitrates(
    preset: VideoEncodingPreset,
    target_size_bytes: u64,
    duration_ms: u64,
    has_audio: bool,
) -> Result<TargetBitrates, ConversionError> {
    if duration_ms == 0 {
        return Err(target_size_error(
            "The video duration could not be measured for file-size targeting",
        ));
    }
    let usable_bits = target_size_bytes
        .checked_mul(8)
        .and_then(|bits| bits.checked_mul(TARGET_MUX_BUDGET_PERCENT))
        .map(|bits| bits / 100)
        .ok_or_else(|| target_size_error("The requested target size is too large"))?;
    let total_kbps = usable_bits / duration_ms;
    let audio_kbps = if has_audio {
        let available_for_audio = total_kbps.saturating_sub(MIN_VIDEO_BITRATE_KBPS);
        if available_for_audio < MIN_AUDIO_BITRATE_KBPS {
            return Err(target_size_error(
                "That target is too small for this video's duration and audio",
            ));
        }
        Some(
            preset
                .target_audio_bitrate_kbps()
                .min(available_for_audio)
                .max(MIN_AUDIO_BITRATE_KBPS),
        )
    } else {
        None
    };
    let video_kbps = total_kbps
        .saturating_sub(audio_kbps.unwrap_or(0))
        .min(MAX_VIDEO_BITRATE_KBPS);
    if video_kbps < MIN_VIDEO_BITRATE_KBPS {
        return Err(target_size_error(
            "That target is too small for this video's duration",
        ));
    }
    Ok(TargetBitrates {
        video_kbps,
        audio_kbps,
    })
}

fn rebudget_video_bitrate(video_kbps: u64, target_bytes: u64, actual_bytes: u64) -> u64 {
    if actual_bytes == 0 {
        return video_kbps;
    }
    video_kbps
        .saturating_mul(target_bytes)
        .saturating_mul(TARGET_RETRY_BUDGET_PERCENT)
        .checked_div(actual_bytes)
        .unwrap_or(MIN_VIDEO_BITRATE_KBPS)
        .checked_div(100)
        .unwrap_or(MIN_VIDEO_BITRATE_KBPS)
        .clamp(MIN_VIDEO_BITRATE_KBPS, MAX_VIDEO_BITRATE_KBPS)
}

fn target_size_error(message: &str) -> ConversionError {
    ConversionError::ProcessFailed {
        message: message.into(),
        stderr: String::new(),
        exit_code: None,
    }
}

pub(super) fn encoder_support(preset: VideoEncodingPreset) -> Result<(), Vec<String>> {
    let available = ffmpeg_encoders();
    let missing = preset
        .required_encoders()
        .iter()
        .filter(|encoder| !available.contains(**encoder))
        .map(|encoder| (*encoder).to_string())
        .collect::<Vec<_>>();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(missing)
    }
}

fn ffmpeg_encoders() -> &'static BTreeSet<String> {
    static ENCODERS: OnceLock<BTreeSet<String>> = OnceLock::new();
    ENCODERS.get_or_init(|| {
        let Some(executable) = resolve_tool("ffmpeg") else {
            return BTreeSet::new();
        };
        std::process::Command::new(executable)
            .args(["-hide_banner", "-encoders"])
            .output()
            .ok()
            .filter(|output| output.status.success())
            .map(|output| parse_encoder_list(&String::from_utf8_lossy(&output.stdout)))
            .unwrap_or_default()
    })
}

pub(super) fn video_encoder_available(encoder: &str) -> bool {
    ffmpeg_encoders().contains(encoder)
}

fn parse_encoder_list(output: &str) -> BTreeSet<String> {
    output
        .lines()
        .filter_map(|line| {
            let mut columns = line.split_whitespace();
            let flags = columns.next()?;
            let encoder = columns.next()?;
            (flags.len() >= 6 && (flags.starts_with('V') || flags.starts_with('A')))
                .then(|| encoder.to_string())
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn encode_video(
    app: AppHandle,
    input_path: String,
    preset: VideoEncodingPreset,
    resolution: VideoResolution,
    quality: VideoQuality,
    compression_goal: VideoCompressionGoal,
    target_size_bytes: Option<u64>,
    job_id: Option<String>,
    output_options: Option<OutputOptions>,
) -> Result<ConversionResult, ConversionError> {
    let started = Instant::now();
    let input = validate_video_input(&input_path)?;
    for tool in ["ffmpeg", "ffprobe"] {
        if resolve_tool(tool).is_none() {
            return Err(ConversionError::MissingDependency {
                tool: tool.into(),
                install_hint: "Install FFmpeg with Homebrew".into(),
            });
        }
    }
    if let Err(missing) = encoder_support(preset) {
        return Err(ConversionError::MissingDependency {
            tool: missing.join(", "),
            install_hint: "Install the full FFmpeg package with Homebrew".into(),
        });
    }
    validate_compression_goal(preset, compression_goal, target_size_bytes).map_err(|message| {
        ConversionError::UnsupportedConversion {
            input: input_path.clone(),
            output: message.into(),
        }
    })?;

    let prepared_output = prepare_output(&input, preset.extension(), "-encoded", output_options)?;
    let active_job = super::jobs::ActiveJobGuard::register(app.clone(), job_id)?;
    let cancel_token = active_job.cancel_token();
    let event_job_id = active_job.job_id().to_owned();
    let timeline = probe_primary_timeline(&input).await?;
    let duration_ms = timeline.duration_ms;

    let _ = app.emit(
        "conversion-progress",
        ProgressPayload {
            job_id: event_job_id.clone(),
            percent: if duration_ms > 0 { 0 } else { -1 },
            stage: "Encoding video".into(),
        },
    );

    if compression_goal == VideoCompressionGoal::FileSize {
        encode_video_to_target(
            &app,
            &input,
            prepared_output.working_path(),
            preset,
            resolution,
            target_size_bytes.expect("validated target size"),
            timeline,
            event_job_id.clone(),
            cancel_token.clone(),
        )
        .await?;
    } else {
        run_ffmpeg_job(
            prepared_output.working_path(),
            ffmpeg_arguments(
                &input,
                prepared_output.working_path(),
                preset,
                resolution,
                quality,
            ),
            duration_ms,
            "Encoding video",
            "Video encoding failed",
            app.clone(),
            event_job_id.clone(),
            cancel_token.clone(),
        )
        .await?;
    }
    if cancel_token.is_cancelled() {
        return Err(ConversionError::Cancelled);
    }
    verify_video_output(prepared_output.working_path(), preset, resolution, timeline).await?;
    if compression_goal == VideoCompressionGoal::FileSize {
        verify_target_size_ceiling(
            prepared_output.working_path(),
            target_size_bytes.expect("validated target size"),
        )?;
    }
    if cancel_token.is_cancelled() {
        engines::cleanup_partial(prepared_output.working_path());
        return Err(ConversionError::Cancelled);
    }

    let mut result = prepared_output.commit(ConversionResult {
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

pub(super) async fn remove_audio(
    app: AppHandle,
    input_path: String,
    job_id: Option<String>,
    output_options: Option<OutputOptions>,
) -> Result<ConversionResult, ConversionError> {
    let started = Instant::now();
    let input = validate_video_input(&input_path)?;
    for tool in ["ffmpeg", "ffprobe"] {
        if resolve_tool(tool).is_none() {
            return Err(ConversionError::MissingDependency {
                tool: tool.into(),
                install_hint: "Install FFmpeg with Homebrew".into(),
            });
        }
    }

    let input_streams = probe_stream_types(&input).await?;
    if !input_streams.iter().any(|stream| stream == "audio") {
        return Err(ConversionError::ProcessFailed {
            message: "No audio stream found".into(),
            stderr: String::new(),
            exit_code: None,
        });
    }

    let extension = input
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| ConversionError::UnsupportedConversion {
            input: input_path.clone(),
            output: "video without audio".into(),
        })?;
    let prepared_output = prepare_output(&input, &extension, "-silent", output_options)?;
    let active_job = super::jobs::ActiveJobGuard::register(app.clone(), job_id)?;
    let cancel_token = active_job.cancel_token();
    let event_job_id = active_job.job_id().to_owned();
    let duration_ms = probe_duration_ms(&input).await.unwrap_or(0);

    let _ = app.emit(
        "conversion-progress",
        ProgressPayload {
            job_id: event_job_id.clone(),
            percent: if duration_ms > 0 { 0 } else { -1 },
            stage: "Removing audio".into(),
        },
    );

    run_ffmpeg_job(
        prepared_output.working_path(),
        remove_audio_arguments(&input, prepared_output.working_path()),
        duration_ms,
        "Removing audio",
        "Audio removal failed",
        app.clone(),
        event_job_id.clone(),
        cancel_token.clone(),
    )
    .await?;
    if cancel_token.is_cancelled() {
        return Err(ConversionError::Cancelled);
    }
    verify_silent_video_output(prepared_output.working_path()).await?;

    let mut result = prepared_output.commit(ConversionResult {
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

pub(super) fn validate_video_input(input_path: &str) -> Result<PathBuf, ConversionError> {
    let input = PathBuf::from(input_path);
    let metadata = std::fs::metadata(&input).map_err(|_| ConversionError::InputNotFound {
        path: input_path.into(),
    })?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > MAX_INPUT_BYTES {
        return Err(ConversionError::UnsupportedConversion {
            input: input_path.into(),
            output: "encoded video".into(),
        });
    }
    let format = input
        .extension()
        .and_then(|extension| extension.to_str())
        .and_then(Format::from_extension);
    if !format.is_some_and(|format| format.category() == FileCategory::Video) {
        return Err(ConversionError::UnsupportedConversion {
            input: input_path.into(),
            output: "encoded video".into(),
        });
    }
    Ok(input)
}

fn ffmpeg_arguments(
    input: &Path,
    output: &Path,
    preset: VideoEncodingPreset,
    resolution: VideoResolution,
    quality: VideoQuality,
) -> Vec<OsString> {
    let mut arguments = vec![
        OsString::from("-hide_banner"),
        OsString::from("-loglevel"),
        OsString::from("error"),
        OsString::from("-i"),
        input.as_os_str().to_os_string(),
        OsString::from("-map"),
        OsString::from("0:v:0"),
        OsString::from("-map"),
        OsString::from("0:a:0?"),
        OsString::from("-sn"),
        OsString::from("-dn"),
        OsString::from("-map_metadata"),
        OsString::from("0"),
    ];
    arguments.extend(preset.codec_arguments(resolution, quality));
    arguments.extend([
        OsString::from("-progress"),
        OsString::from("pipe:1"),
        OsString::from("-nostats"),
        OsString::from("-y"),
        output.as_os_str().to_os_string(),
    ]);
    arguments
}

#[allow(clippy::too_many_arguments)]
async fn encode_video_to_target(
    app: &AppHandle,
    input: &Path,
    output: &Path,
    preset: VideoEncodingPreset,
    resolution: VideoResolution,
    target_size_bytes: u64,
    timeline: PrimaryTimeline,
    job_id: String,
    cancel_token: CancellationToken,
) -> Result<(), ConversionError> {
    let duration_ms = timeline.duration_ms;
    let mut bitrates = target_bitrates(preset, target_size_bytes, duration_ms, timeline.has_audio)?;
    let workspace = tempfile::Builder::new()
        .prefix("convertkit-video-size-")
        .tempdir()
        .map_err(|error| {
            target_size_error(&format!("Could not create encoding workspace: {error}"))
        })?;
    let deadline = Instant::now() + VIDEO_ENCODING_TIMEOUT;

    for attempt in 0..2 {
        let passlog = workspace.path().join(format!("pass-{attempt}"));
        let (analysis_start, analysis_end, encode_start, encode_end, stage) = if attempt == 0 {
            (0, 35, 35, 85, "Encoding to target size")
        } else {
            (85, 90, 90, 98, "Refining target size")
        };

        run_ffmpeg_job_in_range(
            None,
            target_pass_arguments(input, output, preset, resolution, bitrates, &passlog, 1),
            duration_ms,
            "Analyzing video",
            "Video size analysis failed",
            app.clone(),
            job_id.clone(),
            cancel_token.clone(),
            analysis_start,
            analysis_end,
            remaining_encoding_time(deadline)?,
        )
        .await?;
        run_ffmpeg_job_in_range(
            Some(output),
            target_pass_arguments(input, output, preset, resolution, bitrates, &passlog, 2),
            duration_ms,
            stage,
            "Target-size video encoding failed",
            app.clone(),
            job_id.clone(),
            cancel_token.clone(),
            encode_start,
            encode_end,
            remaining_encoding_time(deadline)?,
        )
        .await?;

        let actual_bytes = std::fs::metadata(output)
            .map_err(|_| ConversionError::OutputMissing)?
            .len();
        if actual_bytes <= target_size_bytes {
            return Ok(());
        }
        if attempt == 0 {
            let revised =
                rebudget_video_bitrate(bitrates.video_kbps, target_size_bytes, actual_bytes);
            if revised >= bitrates.video_kbps || revised < MIN_VIDEO_BITRATE_KBPS {
                engines::cleanup_partial(output);
                break;
            }
            bitrates.video_kbps = revised;
            engines::cleanup_partial(output);
        } else {
            engines::cleanup_partial(output);
        }
    }

    Err(target_size_error(
        "The encoded video could not be kept under the requested size",
    ))
}

fn remaining_encoding_time(deadline: Instant) -> Result<Duration, ConversionError> {
    deadline
        .checked_duration_since(Instant::now())
        .filter(|remaining| !remaining.is_zero())
        .ok_or(ConversionError::Timeout {
            seconds: VIDEO_ENCODING_TIMEOUT.as_secs(),
        })
}

fn target_pass_arguments(
    input: &Path,
    output: &Path,
    preset: VideoEncodingPreset,
    resolution: VideoResolution,
    bitrates: TargetBitrates,
    passlog: &Path,
    pass: u8,
) -> Vec<OsString> {
    let mut arguments = vec![
        OsString::from("-hide_banner"),
        OsString::from("-loglevel"),
        OsString::from("error"),
        OsString::from("-i"),
        input.as_os_str().to_os_string(),
        OsString::from("-map"),
        OsString::from("0:v:0"),
        OsString::from("-sn"),
        OsString::from("-dn"),
    ];
    if pass == 2 {
        if bitrates.audio_kbps.is_some() {
            arguments.extend([OsString::from("-map"), OsString::from("0:a:0?")]);
        } else {
            arguments.push(OsString::from("-an"));
        }
        arguments.extend([OsString::from("-map_metadata"), OsString::from("0")]);
    } else {
        arguments.push(OsString::from("-an"));
    }
    if let Some((width, height)) = preset.resolution_limit(resolution) {
        arguments.extend([
            OsString::from("-vf"),
            OsString::from(format!(
                "scale=w='min({width},iw)':h='min({height},ih)':force_original_aspect_ratio=decrease:force_divisible_by=2"
            )),
        ]);
    }

    let video_bitrate = format!("{}k", bitrates.video_kbps);
    let pass_value = pass.to_string();
    let passlog_value = passlog.to_string_lossy().into_owned();
    match preset {
        VideoEncodingPreset::Compatible => arguments.extend([
            OsString::from("-c:v"),
            OsString::from("libx264"),
            OsString::from("-preset"),
            OsString::from("medium"),
            OsString::from("-b:v"),
            OsString::from(video_bitrate),
            OsString::from("-pass"),
            OsString::from(pass_value),
            OsString::from("-passlogfile"),
            OsString::from(passlog_value),
            OsString::from("-pix_fmt"),
            OsString::from("yuv420p"),
        ]),
        VideoEncodingPreset::Smaller => arguments.extend([
            OsString::from("-c:v"),
            OsString::from("libx265"),
            OsString::from("-preset"),
            OsString::from("medium"),
            OsString::from("-b:v"),
            OsString::from(video_bitrate),
            OsString::from("-x265-params"),
            OsString::from(format!("pass={pass}:stats={passlog_value}")),
            OsString::from("-tag:v"),
            OsString::from("hvc1"),
            OsString::from("-pix_fmt"),
            OsString::from("yuv420p"),
        ]),
        VideoEncodingPreset::Web => arguments.extend([
            OsString::from("-c:v"),
            OsString::from("libvpx-vp9"),
            OsString::from("-b:v"),
            OsString::from(video_bitrate),
            OsString::from("-pass"),
            OsString::from(pass_value),
            OsString::from("-passlogfile"),
            OsString::from(passlog_value),
            OsString::from("-row-mt"),
            OsString::from("1"),
        ]),
        VideoEncodingPreset::Archive => unreachable!("validated target-size preset"),
    }

    if pass == 2 {
        if let Some(audio_kbps) = bitrates.audio_kbps {
            let codec = match preset {
                VideoEncodingPreset::Compatible | VideoEncodingPreset::Smaller => "aac",
                VideoEncodingPreset::Web => "libopus",
                VideoEncodingPreset::Archive => unreachable!("validated target-size preset"),
            };
            arguments.extend([
                OsString::from("-c:a"),
                OsString::from(codec),
                OsString::from("-b:a"),
                OsString::from(format!("{audio_kbps}k")),
            ]);
        }
        if matches!(
            preset,
            VideoEncodingPreset::Compatible | VideoEncodingPreset::Smaller
        ) {
            arguments.extend([OsString::from("-movflags"), OsString::from("+faststart")]);
        }
    }

    arguments.extend([
        OsString::from("-progress"),
        OsString::from("pipe:1"),
        OsString::from("-nostats"),
        OsString::from("-y"),
    ]);
    if pass == 1 {
        arguments.extend([
            OsString::from("-f"),
            OsString::from("null"),
            OsString::from("/dev/null"),
        ]);
    } else {
        arguments.push(output.as_os_str().to_os_string());
    }
    arguments
}

fn remove_audio_arguments(input: &Path, output: &Path) -> Vec<OsString> {
    vec![
        OsString::from("-hide_banner"),
        OsString::from("-loglevel"),
        OsString::from("error"),
        OsString::from("-i"),
        input.as_os_str().to_os_string(),
        OsString::from("-map"),
        OsString::from("0:v:0"),
        OsString::from("-c:v"),
        OsString::from("copy"),
        OsString::from("-an"),
        OsString::from("-sn"),
        OsString::from("-dn"),
        OsString::from("-map_metadata"),
        OsString::from("0"),
        OsString::from("-map_chapters"),
        OsString::from("0"),
        OsString::from("-progress"),
        OsString::from("pipe:1"),
        OsString::from("-nostats"),
        OsString::from("-y"),
        output.as_os_str().to_os_string(),
    ]
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn run_ffmpeg_job(
    output: &Path,
    arguments: Vec<OsString>,
    duration_ms: u64,
    stage: &'static str,
    failure_message: &'static str,
    app: AppHandle,
    job_id: String,
    cancel_token: CancellationToken,
) -> Result<(), ConversionError> {
    run_ffmpeg_job_in_range(
        Some(output),
        arguments,
        duration_ms,
        stage,
        failure_message,
        app,
        job_id,
        cancel_token,
        0,
        100,
        VIDEO_ENCODING_TIMEOUT,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn run_ffmpeg_job_in_range(
    output: Option<&Path>,
    arguments: Vec<OsString>,
    duration_ms: u64,
    stage: &'static str,
    failure_message: &'static str,
    app: AppHandle,
    job_id: String,
    cancel_token: CancellationToken,
    progress_start: i32,
    progress_end: i32,
    timeout: Duration,
) -> Result<(), ConversionError> {
    let mut command = tool_command("ffmpeg");
    command
        .args(arguments)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = command
        .spawn()
        .map_err(|error| ConversionError::ProcessFailed {
            message: format!("Failed to start FFmpeg: {error}"),
            stderr: String::new(),
            exit_code: None,
        })?;

    let stdout = child.stdout.take();
    let progress_app = app.clone();
    let progress_task = tokio::spawn(async move {
        if let Some(stdout) = stdout {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if let Some(percent) = parse_ffmpeg_progress(&line, duration_ms) {
                    let percent = progress_start
                        + (percent.clamp(0, 100) * (progress_end - progress_start) / 100);
                    let _ = progress_app.emit(
                        "conversion-progress",
                        ProgressPayload {
                            job_id: job_id.clone(),
                            percent,
                            stage: stage.into(),
                        },
                    );
                }
            }
        }
    });
    let stderr_task = capture_output(child.stderr.take());

    enum ProcessExit {
        Finished(std::process::ExitStatus),
        Cancelled,
        TimedOut,
    }
    let exit = tokio::select! {
        result = child.wait() => ProcessExit::Finished(result.map_err(|error| {
            ConversionError::ProcessFailed {
                message: format!("FFmpeg process failed: {error}"),
                stderr: String::new(),
                exit_code: None,
            }
        })?),
        _ = cancel_token.cancelled() => ProcessExit::Cancelled,
        _ = tokio::time::sleep(timeout) => ProcessExit::TimedOut,
    };

    match exit {
        ProcessExit::Cancelled => {
            let _ = child.kill().await;
            let _ = progress_task.await;
            let _ = finish_output(stderr_task).await;
            if let Some(output) = output {
                engines::cleanup_partial(output);
            }
            Err(ConversionError::Cancelled)
        }
        ProcessExit::TimedOut => {
            let _ = child.kill().await;
            let _ = progress_task.await;
            let _ = finish_output(stderr_task).await;
            if let Some(output) = output {
                engines::cleanup_partial(output);
            }
            Err(ConversionError::Timeout {
                seconds: VIDEO_ENCODING_TIMEOUT.as_secs(),
            })
        }
        ProcessExit::Finished(status) => {
            let _ = progress_task.await;
            let stderr = finish_output(stderr_task).await;
            if status.success() {
                Ok(())
            } else {
                if let Some(output) = output {
                    engines::cleanup_partial(output);
                }
                Err(ConversionError::ProcessFailed {
                    message: failure_message.into(),
                    stderr,
                    exit_code: status.code(),
                })
            }
        }
    }
}

async fn probe_stream_types(path: &Path) -> Result<Vec<String>, ConversionError> {
    let probe = tokio::time::timeout(
        OUTPUT_PROBE_TIMEOUT,
        tool_command("ffprobe")
            .args([
                "-v",
                "error",
                "-show_entries",
                "stream=codec_type",
                "-of",
                "csv=p=0",
            ])
            .arg(path)
            .output(),
    )
    .await
    .map_err(|_| ConversionError::Timeout {
        seconds: OUTPUT_PROBE_TIMEOUT.as_secs(),
    })?
    .map_err(|error| ConversionError::ProcessFailed {
        message: format!("Could not inspect the video streams: {error}"),
        stderr: String::new(),
        exit_code: None,
    })?;
    if !probe.status.success() {
        return Err(ConversionError::ProcessFailed {
            message: "The video streams could not be inspected".into(),
            stderr: String::from_utf8_lossy(&probe.stderr).into_owned(),
            exit_code: probe.status.code(),
        });
    }
    Ok(String::from_utf8_lossy(&probe.stdout)
        .lines()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect())
}

fn seconds_to_ms(value: &str) -> Option<u64> {
    let seconds = value.trim().parse::<f64>().ok()?;
    (seconds.is_finite() && seconds >= 0.0).then_some((seconds * 1_000.0).round() as u64)
}

fn clock_to_ms(value: &str) -> Option<u64> {
    let mut parts = value.trim().split(':');
    let hours = parts.next()?.parse::<u64>().ok()?;
    let minutes = parts.next()?.parse::<u64>().ok()?;
    let seconds = parts.next()?.parse::<f64>().ok()?;
    if parts.next().is_some()
        || minutes >= 60
        || !seconds.is_finite()
        || !(0.0..60.0).contains(&seconds)
    {
        return None;
    }
    let whole_ms = hours
        .checked_mul(60 * 60 * 1_000)?
        .checked_add(minutes.checked_mul(60 * 1_000)?)?;
    whole_ms.checked_add((seconds * 1_000.0).round() as u64)
}

fn stream_duration_ms(stream: &VideoMediaStream) -> Option<u64> {
    stream
        .duration
        .as_deref()
        .and_then(seconds_to_ms)
        .or_else(|| {
            stream
                .tags
                .get("DURATION")
                .and_then(|value| clock_to_ms(value))
        })
}

fn primary_timeline_from_probe(details: &VideoMediaProbe) -> Option<PrimaryTimeline> {
    let video = details
        .streams
        .iter()
        .find(|stream| stream.codec_type == "video")?;
    let audio = details
        .streams
        .iter()
        .find(|stream| stream.codec_type == "audio");
    let selected = [Some(video), audio]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    let exact_durations = selected
        .iter()
        .map(|stream| stream_duration_ms(stream))
        .collect::<Option<Vec<_>>>();

    if let Some(durations) = exact_durations {
        let starts = selected
            .iter()
            .map(|stream| {
                stream
                    .start_time
                    .as_deref()
                    .and_then(seconds_to_ms)
                    .unwrap_or(0)
            })
            .collect::<Vec<_>>();
        let first_start = starts.iter().copied().min().unwrap_or(0);
        let last_end = starts
            .iter()
            .zip(durations.iter())
            .filter_map(|(start, duration)| start.checked_add(*duration))
            .max()?;
        let duration_ms = last_end.saturating_sub(first_start);
        if duration_ms > 0 {
            return Some(PrimaryTimeline {
                duration_ms,
                has_audio: audio.is_some(),
                exact: true,
            });
        }
    }

    let duration_ms = details.format.duration.as_deref().and_then(seconds_to_ms)?;
    (duration_ms > 0).then_some(PrimaryTimeline {
        duration_ms,
        has_audio: audio.is_some(),
        exact: false,
    })
}

async fn probe_video_media(path: &Path) -> Result<VideoMediaProbe, ConversionError> {
    let probe = tokio::time::timeout(
        OUTPUT_PROBE_TIMEOUT,
        tool_command("ffprobe")
            .args([
                "-v",
                "error",
                "-show_entries",
                "stream=codec_type,codec_name,width,height,start_time,duration:stream_tags=DURATION:format=format_name,duration",
                "-of",
                "json",
            ])
            .arg(path)
            .output(),
    )
    .await
    .map_err(|_| ConversionError::Timeout {
        seconds: OUTPUT_PROBE_TIMEOUT.as_secs(),
    })?
    .map_err(|error| ConversionError::ProcessFailed {
        message: format!("Could not inspect the video: {error}"),
        stderr: String::new(),
        exit_code: None,
    })?;
    if !probe.status.success() {
        return Err(ConversionError::ProcessFailed {
            message: "The video could not be inspected".into(),
            stderr: String::from_utf8_lossy(&probe.stderr).into_owned(),
            exit_code: probe.status.code(),
        });
    }
    serde_json::from_slice(&probe.stdout).map_err(|error| ConversionError::ProcessFailed {
        message: format!("The video inspection result could not be read: {error}"),
        stderr: String::new(),
        exit_code: probe.status.code(),
    })
}

async fn probe_primary_timeline(path: &Path) -> Result<PrimaryTimeline, ConversionError> {
    let details = probe_video_media(path).await?;
    primary_timeline_from_probe(&details)
        .ok_or_else(|| target_size_error("The primary video timeline could not be measured"))
}

pub(super) async fn probe_duration_ms(path: &Path) -> Option<u64> {
    let output = tokio::time::timeout(
        OUTPUT_PROBE_TIMEOUT,
        tool_command("ffprobe")
            .args([
                "-v",
                "error",
                "-show_entries",
                "format=duration",
                "-of",
                "default=noprint_wrappers=1:nokey=1",
            ])
            .arg(path)
            .output(),
    )
    .await
    .ok()?
    .ok()?;
    if !output.status.success() {
        return None;
    }
    let seconds = String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<f64>()
        .ok()?;
    (seconds.is_finite() && seconds > 0.0).then_some((seconds * 1_000.0) as u64)
}

async fn verify_video_output(
    output: &Path,
    preset: VideoEncodingPreset,
    resolution: VideoResolution,
    source_timeline: PrimaryTimeline,
) -> Result<(), ConversionError> {
    engines::verification::nonempty_file(output)?;
    let details = probe_video_media(output).await?;
    if let Err(reason) = validate_encoded_video(&details, preset, resolution, source_timeline) {
        engines::cleanup_partial(output);
        return Err(ConversionError::ProcessFailed {
            message: "The encoded video could not be verified".into(),
            stderr: reason.into(),
            exit_code: None,
        });
    }
    Ok(())
}

fn validate_encoded_video(
    details: &VideoMediaProbe,
    preset: VideoEncodingPreset,
    resolution: VideoResolution,
    source_timeline: PrimaryTimeline,
) -> Result<(), &'static str> {
    if !details
        .format
        .format_name
        .split(',')
        .any(|name| name == preset.expected_container())
    {
        return Err("The output container does not match the selected preset");
    }

    let videos = details
        .streams
        .iter()
        .filter(|stream| stream.codec_type == "video")
        .collect::<Vec<_>>();
    if videos.len() != 1 || videos[0].codec_name != preset.expected_video_codec() {
        return Err("The output video codec or stream count does not match the selected preset");
    }
    let (Some(width), Some(height)) = (videos[0].width, videos[0].height) else {
        return Err("The output video dimensions are missing");
    };
    if width == 0 || height == 0 {
        return Err("The output video dimensions are invalid");
    }
    if preset
        .resolution_limit(resolution)
        .is_some_and(|(max_width, max_height)| {
            width > max_width || height > max_height || width % 2 != 0 || height % 2 != 0
        })
    {
        return Err("The output video dimensions exceed the selected resolution");
    }

    let audio = details
        .streams
        .iter()
        .filter(|stream| stream.codec_type == "audio")
        .collect::<Vec<_>>();
    let expected_audio_streams = usize::from(source_timeline.has_audio);
    if audio.len() != expected_audio_streams {
        return Err("The output audio stream count does not match the source");
    }
    if audio
        .iter()
        .any(|stream| stream.codec_name != preset.expected_audio_codec())
    {
        return Err("The output audio codec does not match the selected preset");
    }

    let output_timeline = primary_timeline_from_probe(details)
        .ok_or("The output video timeline could not be measured")?;
    if source_timeline.exact {
        let tolerance_ms = (source_timeline.duration_ms / 50).max(1_000);
        if source_timeline
            .duration_ms
            .abs_diff(output_timeline.duration_ms)
            > tolerance_ms
        {
            return Err("The output duration does not match the source");
        }
    }
    Ok(())
}

fn verify_target_size_ceiling(
    output: &Path,
    target_size_bytes: u64,
) -> Result<(), ConversionError> {
    let output_size = std::fs::metadata(output)
        .map_err(|_| ConversionError::OutputMissing)?
        .len();
    if output_size > target_size_bytes {
        engines::cleanup_partial(output);
        return Err(target_size_error(
            "The target-size video could not be verified",
        ));
    }
    Ok(())
}

async fn verify_silent_video_output(output: &Path) -> Result<(), ConversionError> {
    engines::verification::nonempty_file(output)?;
    let streams = probe_stream_types(output).await?;
    if !streams.iter().any(|stream| stream == "video")
        || streams.iter().any(|stream| stream == "audio")
    {
        engines::cleanup_partial(output);
        return Err(ConversionError::ProcessFailed {
            message: "The silent video could not be verified".into(),
            stderr: String::new(),
            exit_code: None,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn media_probe(value: serde_json::Value) -> VideoMediaProbe {
        serde_json::from_value(value).expect("valid media probe fixture")
    }

    fn encoded_probe(
        container: &str,
        video_codec: &str,
        audio_codec: Option<&str>,
        width: u32,
        height: u32,
        duration: &str,
    ) -> VideoMediaProbe {
        let mut streams = vec![serde_json::json!({
            "codec_type": "video",
            "codec_name": video_codec,
            "width": width,
            "height": height,
            "start_time": "0.000000",
            "duration": duration
        })];
        if let Some(audio_codec) = audio_codec {
            streams.push(serde_json::json!({
                "codec_type": "audio",
                "codec_name": audio_codec,
                "start_time": "0.000000",
                "duration": duration
            }));
        }
        media_probe(serde_json::json!({
            "streams": streams,
            "format": { "format_name": container, "duration": duration }
        }))
    }

    fn arguments(preset: VideoEncodingPreset) -> Vec<String> {
        arguments_with_settings(preset, VideoResolution::Automatic, VideoQuality::Balanced)
    }

    fn arguments_with_settings(
        preset: VideoEncodingPreset,
        resolution: VideoResolution,
        quality: VideoQuality,
    ) -> Vec<String> {
        ffmpeg_arguments(
            Path::new("input.mov"),
            Path::new("output.tmp"),
            preset,
            resolution,
            quality,
        )
        .into_iter()
        .map(|value| value.to_string_lossy().into_owned())
        .collect()
    }

    #[test]
    fn parses_video_and_audio_encoders_only() {
        let parsed = parse_encoder_list(
            "Encoders:\n V....D libx264 H.264\n A....D aac AAC\n S..... srt subtitle\n",
        );
        assert_eq!(parsed, BTreeSet::from(["aac".into(), "libx264".into()]));
    }

    #[test]
    fn presets_have_stable_extensions_codecs_and_encoders() {
        let cases = [
            (VideoEncodingPreset::Compatible, "mp4", "h264", "libx264"),
            (VideoEncodingPreset::Smaller, "mp4", "hevc", "libx265"),
            (VideoEncodingPreset::Web, "webm", "vp9", "libvpx-vp9"),
            (VideoEncodingPreset::Archive, "mkv", "ffv1", "ffv1"),
        ];
        for (preset, extension, codec, encoder) in cases {
            assert_eq!(preset.extension(), extension);
            assert_eq!(preset.expected_video_codec(), codec);
            assert!(preset.required_encoders().contains(&encoder));
            assert!(arguments(preset).iter().any(|argument| argument == encoder));
        }
    }

    #[test]
    fn every_preset_maps_only_the_primary_video_and_optional_audio() {
        for preset in [
            VideoEncodingPreset::Compatible,
            VideoEncodingPreset::Smaller,
            VideoEncodingPreset::Web,
            VideoEncodingPreset::Archive,
        ] {
            let arguments = arguments(preset);
            assert!(arguments.windows(2).any(|pair| pair == ["-map", "0:v:0"]));
            assert!(arguments.windows(2).any(|pair| pair == ["-map", "0:a:0?"]));
            assert!(arguments.contains(&"-sn".into()));
            assert_eq!(arguments.last().map(String::as_str), Some("output.tmp"));
        }
    }

    #[test]
    fn size_reducing_presets_cap_dimensions_without_upscaling() {
        for preset in [VideoEncodingPreset::Smaller, VideoEncodingPreset::Web] {
            let arguments = arguments(preset);
            let filter = arguments
                .windows(2)
                .find(|pair| pair[0] == "-vf")
                .map(|pair| pair[1].as_str())
                .expect("scale filter");
            assert!(filter.contains("min(1920,iw)"));
            assert!(filter.contains("min(1080,ih)"));
        }
        assert!(!arguments(VideoEncodingPreset::Compatible).contains(&"-vf".into()));
        assert!(!arguments(VideoEncodingPreset::Archive).contains(&"-vf".into()));
    }

    #[test]
    fn resolution_controls_are_bounded_and_never_upscale() {
        let full_hd = arguments_with_settings(
            VideoEncodingPreset::Compatible,
            VideoResolution::FullHd,
            VideoQuality::Balanced,
        );
        let filter = full_hd
            .windows(2)
            .find(|pair| pair[0] == "-vf")
            .map(|pair| pair[1].as_str())
            .expect("1080p scale filter");
        assert!(filter.contains("min(1920,iw)"));
        assert!(filter.contains("min(1080,ih)"));
        assert!(filter.contains("force_original_aspect_ratio=decrease"));

        let hd = arguments_with_settings(
            VideoEncodingPreset::Web,
            VideoResolution::Hd,
            VideoQuality::Balanced,
        );
        let filter = hd
            .windows(2)
            .find(|pair| pair[0] == "-vf")
            .map(|pair| pair[1].as_str())
            .expect("720p scale filter");
        assert!(filter.contains("min(1280,iw)"));
        assert!(filter.contains("min(720,ih)"));

        assert!(!arguments_with_settings(
            VideoEncodingPreset::Smaller,
            VideoResolution::Original,
            VideoQuality::Balanced,
        )
        .contains(&"-vf".into()));
        assert!(!arguments_with_settings(
            VideoEncodingPreset::Archive,
            VideoResolution::Hd,
            VideoQuality::Smallest,
        )
        .contains(&"-vf".into()));
    }

    #[test]
    fn quality_controls_map_to_codec_appropriate_crf_values() {
        let crf = |arguments: Vec<String>| {
            arguments
                .windows(2)
                .find(|pair| pair[0] == "-crf")
                .map(|pair| pair[1].clone())
        };

        assert_eq!(
            crf(arguments_with_settings(
                VideoEncodingPreset::Compatible,
                VideoResolution::Original,
                VideoQuality::High,
            )),
            Some("18".into())
        );
        assert_eq!(
            crf(arguments_with_settings(
                VideoEncodingPreset::Smaller,
                VideoResolution::Automatic,
                VideoQuality::Smallest,
            )),
            Some("32".into())
        );
        assert_eq!(
            crf(arguments_with_settings(
                VideoEncodingPreset::Web,
                VideoResolution::Hd,
                VideoQuality::Balanced,
            )),
            Some("31".into())
        );
        assert_eq!(
            crf(arguments_with_settings(
                VideoEncodingPreset::Archive,
                VideoResolution::Automatic,
                VideoQuality::High,
            )),
            None
        );
    }

    #[test]
    fn target_size_settings_are_bounded_and_exclude_lossless() {
        assert!(validate_compression_goal(
            VideoEncodingPreset::Compatible,
            VideoCompressionGoal::Quality,
            None,
        )
        .is_ok());
        assert!(validate_compression_goal(
            VideoEncodingPreset::Web,
            VideoCompressionGoal::FileSize,
            Some(MIN_VIDEO_TARGET_BYTES),
        )
        .is_ok());
        assert!(validate_compression_goal(
            VideoEncodingPreset::Compatible,
            VideoCompressionGoal::FileSize,
            Some(MIN_VIDEO_TARGET_BYTES - 1),
        )
        .is_err());
        assert!(validate_compression_goal(
            VideoEncodingPreset::Archive,
            VideoCompressionGoal::FileSize,
            Some(100 * 1024 * 1024),
        )
        .is_err());
    }

    #[test]
    fn target_bitrate_budget_reserves_muxing_and_audio_space() {
        let with_audio = target_bitrates(
            VideoEncodingPreset::Compatible,
            10 * 1024 * 1024,
            60_000,
            true,
        )
        .expect("target budget");
        let without_audio = target_bitrates(
            VideoEncodingPreset::Compatible,
            10 * 1024 * 1024,
            60_000,
            false,
        )
        .expect("silent target budget");

        assert_eq!(with_audio.audio_kbps, Some(128));
        assert!(with_audio.video_kbps >= MIN_VIDEO_BITRATE_KBPS);
        assert_eq!(
            without_audio.video_kbps,
            with_audio.video_kbps + with_audio.audio_kbps.unwrap()
        );
        assert!(target_bitrates(
            VideoEncodingPreset::Compatible,
            MIN_VIDEO_TARGET_BYTES,
            60 * 60 * 1_000,
            true,
        )
        .is_err());
    }

    #[test]
    fn primary_timeline_ignores_longer_secondary_streams() {
        let probe = media_probe(serde_json::json!({
            "streams": [
                {
                    "codec_type": "video",
                    "start_time": "0.000000",
                    "tags": { "DURATION": "00:00:05.000000000" }
                },
                {
                    "codec_type": "audio",
                    "start_time": "0.000000",
                    "tags": { "DURATION": "00:00:05.023000000" }
                },
                {
                    "codec_type": "audio",
                    "start_time": "0.000000",
                    "tags": { "DURATION": "00:00:15.023000000" }
                }
            ],
            "format": { "duration": "15.023000" }
        }));

        assert_eq!(
            primary_timeline_from_probe(&probe),
            Some(PrimaryTimeline {
                duration_ms: 5_023,
                has_audio: true,
                exact: true,
            })
        );
    }

    #[test]
    fn primary_timeline_falls_back_without_claiming_exact_duration() {
        let probe = media_probe(serde_json::json!({
            "streams": [{ "codec_type": "video" }],
            "format": { "duration": "8.250000" }
        }));

        assert_eq!(
            primary_timeline_from_probe(&probe),
            Some(PrimaryTimeline {
                duration_ms: 8_250,
                has_audio: false,
                exact: false,
            })
        );
    }

    #[test]
    fn encoded_video_validation_accepts_every_preset_contract() {
        let source = PrimaryTimeline {
            duration_ms: 5_000,
            has_audio: true,
            exact: true,
        };
        for (preset, container, video_codec, audio_codec) in [
            (
                VideoEncodingPreset::Compatible,
                "mov,mp4,m4a,3gp,3g2,mj2",
                "h264",
                "aac",
            ),
            (
                VideoEncodingPreset::Smaller,
                "mov,mp4,m4a,3gp,3g2,mj2",
                "hevc",
                "aac",
            ),
            (VideoEncodingPreset::Web, "matroska,webm", "vp9", "opus"),
            (
                VideoEncodingPreset::Archive,
                "matroska,webm",
                "ffv1",
                "flac",
            ),
        ] {
            let output = encoded_probe(
                container,
                video_codec,
                Some(audio_codec),
                640,
                360,
                "5.000000",
            );
            assert_eq!(
                validate_encoded_video(&output, preset, VideoResolution::Automatic, source),
                Ok(())
            );
        }
    }

    #[test]
    fn encoded_video_validation_rejects_wrong_structure_and_timeline() {
        let source = PrimaryTimeline {
            duration_ms: 5_000,
            has_audio: true,
            exact: true,
        };
        let valid = || {
            encoded_probe(
                "mov,mp4,m4a,3gp,3g2,mj2",
                "h264",
                Some("aac"),
                640,
                360,
                "5.000000",
            )
        };

        let mut wrong_container = valid();
        wrong_container.format.format_name = "matroska,webm".into();
        assert!(validate_encoded_video(
            &wrong_container,
            VideoEncodingPreset::Compatible,
            VideoResolution::Automatic,
            source,
        )
        .is_err());

        let wrong_video_codec = encoded_probe(
            "mov,mp4,m4a,3gp,3g2,mj2",
            "mpeg4",
            Some("aac"),
            640,
            360,
            "5.000000",
        );
        assert!(validate_encoded_video(
            &wrong_video_codec,
            VideoEncodingPreset::Compatible,
            VideoResolution::Automatic,
            source,
        )
        .is_err());

        let mut no_dimensions = valid();
        no_dimensions.streams[0].width = None;
        assert!(validate_encoded_video(
            &no_dimensions,
            VideoEncodingPreset::Compatible,
            VideoResolution::Original,
            source,
        )
        .is_err());

        let oversized = encoded_probe(
            "mov,mp4,m4a,3gp,3g2,mj2",
            "h264",
            Some("aac"),
            1922,
            1080,
            "5.000000",
        );
        assert!(validate_encoded_video(
            &oversized,
            VideoEncodingPreset::Compatible,
            VideoResolution::FullHd,
            source,
        )
        .is_err());

        let dropped_audio = encoded_probe(
            "mov,mp4,m4a,3gp,3g2,mj2",
            "h264",
            None,
            640,
            360,
            "5.000000",
        );
        assert!(validate_encoded_video(
            &dropped_audio,
            VideoEncodingPreset::Compatible,
            VideoResolution::Automatic,
            source,
        )
        .is_err());

        let wrong_audio = encoded_probe(
            "mov,mp4,m4a,3gp,3g2,mj2",
            "h264",
            Some("mp3"),
            640,
            360,
            "5.000000",
        );
        assert!(validate_encoded_video(
            &wrong_audio,
            VideoEncodingPreset::Compatible,
            VideoResolution::Automatic,
            source,
        )
        .is_err());

        let mut extra_audio = valid();
        extra_audio.streams.push(VideoMediaStream {
            codec_type: "audio".into(),
            codec_name: "aac".into(),
            start_time: Some("0.000000".into()),
            duration: Some("5.000000".into()),
            ..VideoMediaStream::default()
        });
        assert!(validate_encoded_video(
            &extra_audio,
            VideoEncodingPreset::Compatible,
            VideoResolution::Automatic,
            source,
        )
        .is_err());

        let truncated = encoded_probe(
            "mov,mp4,m4a,3gp,3g2,mj2",
            "h264",
            Some("aac"),
            640,
            360,
            "3.000000",
        );
        assert!(validate_encoded_video(
            &truncated,
            VideoEncodingPreset::Compatible,
            VideoResolution::Automatic,
            source,
        )
        .is_err());
    }

    #[test]
    fn encoded_video_validation_preserves_silence_and_duration_tolerance() {
        let silent_source = PrimaryTimeline {
            duration_ms: 5_000,
            has_audio: false,
            exact: true,
        };
        let silent = encoded_probe(
            "mov,mp4,m4a,3gp,3g2,mj2",
            "h264",
            None,
            640,
            360,
            "6.000000",
        );
        assert_eq!(
            validate_encoded_video(
                &silent,
                VideoEncodingPreset::Compatible,
                VideoResolution::Original,
                silent_source,
            ),
            Ok(())
        );

        let unexpected_audio = encoded_probe(
            "mov,mp4,m4a,3gp,3g2,mj2",
            "h264",
            Some("aac"),
            640,
            360,
            "5.000000",
        );
        assert!(validate_encoded_video(
            &unexpected_audio,
            VideoEncodingPreset::Compatible,
            VideoResolution::Original,
            silent_source,
        )
        .is_err());

        let outside_tolerance = encoded_probe(
            "mov,mp4,m4a,3gp,3g2,mj2",
            "h264",
            None,
            640,
            360,
            "6.001000",
        );
        assert!(validate_encoded_video(
            &outside_tolerance,
            VideoEncodingPreset::Compatible,
            VideoResolution::Original,
            silent_source,
        )
        .is_err());
    }

    #[test]
    fn target_size_passes_are_isolated_and_map_only_primary_streams() {
        let passlog = Path::new("/tmp/convertkit-pass-log");
        for preset in [
            VideoEncodingPreset::Compatible,
            VideoEncodingPreset::Smaller,
            VideoEncodingPreset::Web,
        ] {
            let bitrates = TargetBitrates {
                video_kbps: 1_200,
                audio_kbps: Some(96),
            };
            let first = target_pass_arguments(
                Path::new("input.mov"),
                Path::new("output.mp4"),
                preset,
                VideoResolution::Hd,
                bitrates,
                passlog,
                1,
            )
            .into_iter()
            .map(|value| value.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
            let second = target_pass_arguments(
                Path::new("input.mov"),
                Path::new("output.mp4"),
                preset,
                VideoResolution::Hd,
                bitrates,
                passlog,
                2,
            )
            .into_iter()
            .map(|value| value.to_string_lossy().into_owned())
            .collect::<Vec<_>>();

            assert!(first.windows(2).any(|pair| pair == ["-map", "0:v:0"]));
            assert!(first.contains(&"-an".into()));
            assert!(first.windows(2).any(|pair| pair == ["-f", "null"]));
            assert_eq!(first.last().map(String::as_str), Some("/dev/null"));
            assert!(second.windows(2).any(|pair| pair == ["-map", "0:a:0?"]));
            assert!(second.windows(2).any(|pair| pair == ["-b:v", "1200k"]));
            assert!(second.windows(2).any(|pair| pair == ["-b:a", "96k"]));
            assert_eq!(second.last().map(String::as_str), Some("output.mp4"));
            assert!(first
                .iter()
                .any(|argument| argument.contains("convertkit-pass-log")));
            assert!(second
                .iter()
                .any(|argument| argument.contains("convertkit-pass-log")));
        }
    }

    #[test]
    fn overshoot_retry_only_reduces_the_video_budget() {
        assert_eq!(rebudget_video_bitrate(1_000, 900, 1_000), 855);
        assert_eq!(
            rebudget_video_bitrate(MIN_VIDEO_BITRATE_KBPS, 1, 10_000),
            MIN_VIDEO_BITRATE_KBPS
        );
    }

    #[test]
    fn audio_removal_copies_only_the_primary_video_stream() {
        let arguments = remove_audio_arguments(Path::new("input.mov"), Path::new("output.mov"))
            .into_iter()
            .map(|value| value.to_string_lossy().into_owned())
            .collect::<Vec<_>>();

        assert!(arguments.windows(2).any(|pair| pair == ["-map", "0:v:0"]));
        assert!(arguments.windows(2).any(|pair| pair == ["-c:v", "copy"]));
        assert!(arguments.contains(&"-an".into()));
        assert!(arguments.contains(&"-sn".into()));
        assert!(arguments.contains(&"-dn".into()));
        assert!(!arguments
            .iter()
            .any(|argument| argument.starts_with("libx")));
        assert_eq!(arguments.last().map(String::as_str), Some("output.mov"));
    }

    #[test]
    fn audio_removal_produces_a_silent_video_without_changing_the_codec() {
        let Some(ffmpeg) = resolve_tool("ffmpeg") else {
            return;
        };
        let Some(ffprobe) = resolve_tool("ffprobe") else {
            return;
        };
        let encoders = ffmpeg_encoders();
        if !encoders.contains("mpeg4") || !encoders.contains("aac") {
            return;
        }

        let directory = tempfile::tempdir().expect("tempdir");
        let input = directory.path().join("source.mp4");
        let output = directory.path().join("source-silent.mp4");
        let generated = std::process::Command::new(&ffmpeg)
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-f",
                "lavfi",
                "-i",
                "testsrc=size=160x90:rate=10",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=1000:sample_rate=44100",
                "-t",
                "0.5",
                "-c:v",
                "mpeg4",
                "-pix_fmt",
                "yuv420p",
                "-c:a",
                "aac",
                "-y",
            ])
            .arg(&input)
            .status()
            .expect("generate video fixture");
        assert!(generated.success());

        let removed = std::process::Command::new(&ffmpeg)
            .args(remove_audio_arguments(&input, &output))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .expect("remove audio");
        assert!(removed.success());

        let streams = std::process::Command::new(&ffprobe)
            .args([
                "-v",
                "error",
                "-show_entries",
                "stream=codec_type,codec_name",
                "-of",
                "csv=p=0",
            ])
            .arg(&output)
            .output()
            .expect("probe silent video");
        assert!(streams.status.success());
        assert_eq!(
            String::from_utf8_lossy(&streams.stdout).trim(),
            "mpeg4,video"
        );
    }
}
