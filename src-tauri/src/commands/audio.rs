use std::collections::BTreeSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio_util::sync::CancellationToken;

use crate::engines::process::{capture_output, finish_output};
use crate::engines::{self, resolve_tool, tool_command, ConversionResult};
use crate::error::ConversionError;
use crate::formats::{FileCategory, Format};
use crate::progress::{parse_ffmpeg_progress, ProgressPayload};

use super::output::{prepare_output, OutputOptions};

const AUDIO_JOB_TIMEOUT: Duration = Duration::from_secs(60 * 60);
const PROBE_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_INPUT_BYTES: u64 = 1024 * 1024 * 1024 * 1024;
const AUDIO_COMPRESSION_DURATION_TOLERANCE_MS: u64 = 250;
const AAC_SAMPLE_RATES: [u32; 13] = [
    7_350, 8_000, 11_025, 12_000, 16_000, 22_050, 24_000, 32_000, 44_100, 48_000, 64_000, 88_200,
    96_000,
];

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum AudioCompressionPreset {
    High,
    #[default]
    Balanced,
    Smallest,
}

impl AudioCompressionPreset {
    fn bitrate_kbps(self) -> u32 {
        match self {
            Self::High => 256,
            Self::Balanced => 160,
            Self::Smallest => 96,
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioTrack {
    pub stream_index: u32,
    pub codec: String,
    pub language: Option<String>,
    pub title: Option<String>,
    pub channels: Option<u32>,
    pub channel_layout: Option<String>,
    pub sample_rate: Option<u32>,
    pub is_default: bool,
}

pub(super) struct AudioTrackParts {
    pub stream_index: u32,
    pub codec: String,
    pub language: Option<String>,
    pub title: Option<String>,
    pub channels: Option<u32>,
    pub channel_layout: Option<String>,
    pub sample_rate: Option<String>,
    pub is_default: bool,
}

impl AudioTrack {
    pub(super) fn from_parts(parts: AudioTrackParts) -> Self {
        Self {
            stream_index: parts.stream_index,
            codec: parts.codec,
            language: bounded_track_tag(parts.language, 16).filter(|value| value != "und"),
            title: bounded_track_tag(parts.title, 120),
            channels: parts.channels,
            channel_layout: bounded_track_tag(parts.channel_layout, 40),
            sample_rate: parts.sample_rate.and_then(|value| value.parse().ok()),
            is_default: parts.is_default,
        }
    }
}

#[derive(Debug, Default, Deserialize)]
struct AudioProbeOutput {
    #[serde(default)]
    streams: Vec<AudioProbeStream>,
}

#[derive(Debug, Deserialize)]
struct AudioProbeStream {
    index: u32,
    codec_name: Option<String>,
    channels: Option<u32>,
    channel_layout: Option<String>,
    sample_rate: Option<String>,
    #[serde(default)]
    tags: AudioProbeTags,
    #[serde(default)]
    disposition: AudioProbeDisposition,
}

#[derive(Debug, Default, Deserialize)]
struct AudioProbeTags {
    language: Option<String>,
    title: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct AudioProbeDisposition {
    #[serde(default)]
    default: i32,
}

#[derive(Debug, Default, Deserialize)]
struct CompressionProbeOutput {
    #[serde(default)]
    streams: Vec<CompressionProbeStream>,
    #[serde(default)]
    format: CompressionProbeFormat,
}

#[derive(Debug, Deserialize)]
struct CompressionProbeStream {
    codec_type: Option<String>,
    codec_name: Option<String>,
    channels: Option<u32>,
    sample_rate: Option<String>,
    duration: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct CompressionProbeFormat {
    format_name: Option<String>,
    duration: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CompressionAudioProperties {
    duration_ms: u64,
    channels: u32,
    sample_rate: u32,
}

fn bounded_track_tag(value: Option<String>, maximum: usize) -> Option<String> {
    let value = value?.trim().to_string();
    if value.is_empty() {
        return None;
    }
    Some(value.chars().take(maximum).collect())
}

fn parse_audio_tracks(bytes: &[u8]) -> Result<Vec<AudioTrack>, ConversionError> {
    let parsed = serde_json::from_slice::<AudioProbeOutput>(bytes).map_err(|error| {
        ConversionError::ProcessFailed {
            message: format!("Could not read audio track details: {error}"),
            stderr: String::new(),
            exit_code: None,
        }
    })?;
    Ok(parsed
        .streams
        .into_iter()
        .map(|stream| {
            AudioTrack::from_parts(AudioTrackParts {
                stream_index: stream.index,
                codec: stream.codec_name.unwrap_or_else(|| "unknown".into()),
                language: stream.tags.language,
                title: stream.tags.title,
                channels: stream.channels,
                channel_layout: stream.channel_layout,
                sample_rate: stream.sample_rate,
                is_default: stream.disposition.default != 0,
            })
        })
        .collect())
}

pub(super) async fn probe_audio_tracks(path: &Path) -> Result<Vec<AudioTrack>, ConversionError> {
    if resolve_tool("ffprobe").is_none() {
        return Err(ConversionError::MissingDependency {
            tool: "ffprobe".into(),
            install_hint: "Install FFmpeg with Homebrew".into(),
        });
    }
    let output = tokio::time::timeout(
        PROBE_TIMEOUT,
        tool_command("ffprobe")
            .args([
                "-v",
                "error",
                "-select_streams",
                "a",
                "-show_entries",
                "stream=index,codec_name,channels,channel_layout,sample_rate:stream_tags=language,title:stream_disposition=default",
                "-of",
                "json",
            ])
            .arg(path)
            .stdin(Stdio::null())
            .output(),
    )
    .await
    .map_err(|_| ConversionError::Timeout {
        seconds: PROBE_TIMEOUT.as_secs(),
    })?
    .map_err(|error| ConversionError::ProcessFailed {
        message: format!("Could not inspect audio tracks: {error}"),
        stderr: String::new(),
        exit_code: None,
    })?;
    if !output.status.success() {
        return Err(ConversionError::ProcessFailed {
            message: "Audio tracks could not be inspected".into(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            exit_code: output.status.code(),
        });
    }
    parse_audio_tracks(&output.stdout)
}

#[tauri::command]
pub async fn get_audio_tracks(path: String) -> Result<Vec<AudioTrack>, ConversionError> {
    let input = super::video::validate_video_input(&path)?;
    probe_audio_tracks(&input).await
}

pub(super) fn extraction_support(
    format: super::convert::AudioOutputFormat,
) -> Result<(), Vec<String>> {
    let encoder = output_encoder(format.format());
    if ffmpeg_encoders().contains(encoder) {
        Ok(())
    } else {
        Err(vec![format!("{encoder} audio encoder")])
    }
}

pub(super) fn compression_support(format: Format) -> Result<(), Vec<String>> {
    if !matches!(
        format,
        Format::Mp3 | Format::Wav | Format::Aac | Format::Flac | Format::Ogg | Format::M4a
    ) {
        return Err(vec!["MP3, WAV, AAC, FLAC, OGG, or M4A input".into()]);
    }
    if ffmpeg_encoders().contains("aac") {
        Ok(())
    } else {
        Err(vec!["aac audio encoder".into()])
    }
}

fn extraction_arguments(
    input: &Path,
    output: &Path,
    stream_index: u32,
    format: super::convert::AudioOutputFormat,
) -> Vec<OsString> {
    let mut arguments = vec![
        OsString::from("-hide_banner"),
        OsString::from("-nostdin"),
        OsString::from("-loglevel"),
        OsString::from("error"),
        OsString::from("-i"),
        input.as_os_str().to_os_string(),
        OsString::from("-map"),
        OsString::from(format!("0:{stream_index}")),
        OsString::from("-vn"),
        OsString::from("-sn"),
        OsString::from("-dn"),
        OsString::from("-map_metadata"),
        OsString::from("-1"),
    ];
    arguments.extend(codec_arguments(format.format()).iter().map(OsString::from));
    arguments.extend([
        OsString::from("-progress"),
        OsString::from("pipe:1"),
        OsString::from("-nostats"),
        OsString::from("-y"),
        output.as_os_str().to_os_string(),
    ]);
    arguments
}

pub(super) async fn extract_audio(
    app: AppHandle,
    input_path: String,
    stream_index: u32,
    stream_codec: String,
    output_format: super::convert::AudioOutputFormat,
    job_id: Option<String>,
    output_options: Option<OutputOptions>,
) -> Result<ConversionResult, ConversionError> {
    let started = Instant::now();
    let input = super::video::validate_video_input(&input_path)?;
    if stream_index > 4_096 || stream_codec.is_empty() || stream_codec.len() > 64 {
        return Err(ConversionError::UnsupportedConversion {
            input: input_path,
            output: "invalid audio track selection".into(),
        });
    }
    for tool in ["ffmpeg", "ffprobe"] {
        if resolve_tool(tool).is_none() {
            return Err(ConversionError::MissingDependency {
                tool: tool.into(),
                install_hint: "Install FFmpeg with Homebrew".into(),
            });
        }
    }
    extraction_support(output_format).map_err(|missing| ConversionError::MissingDependency {
        tool: missing.join(", "),
        install_hint: "Install the full FFmpeg package with Homebrew".into(),
    })?;
    let track = probe_audio_tracks(&input)
        .await?
        .into_iter()
        .find(|track| track.stream_index == stream_index)
        .ok_or_else(|| ConversionError::UnsupportedConversion {
            input: input_path.clone(),
            output: format!("missing audio track {stream_index}"),
        })?;
    if !track.codec.eq_ignore_ascii_case(&stream_codec) {
        return Err(ConversionError::UnsupportedConversion {
            input: track.codec,
            output: output_format.format().extension().into(),
        });
    }

    let prepared = prepare_output(
        &input,
        output_format.format().extension(),
        "-audio",
        output_options,
    )?;
    let active_job = super::jobs::ActiveJobGuard::register(app.clone(), job_id)?;
    let cancel_token = active_job.cancel_token();
    let event_job_id = active_job.job_id().to_owned();
    let duration_ms = super::video::probe_duration_ms(&input).await.unwrap_or(0);
    let _ = app.emit(
        "conversion-progress",
        ProgressPayload {
            job_id: event_job_id.clone(),
            percent: if duration_ms > 0 { 0 } else { -1 },
            stage: "Extracting audio".into(),
        },
    );

    super::video::run_ffmpeg_job(
        prepared.working_path(),
        extraction_arguments(&input, prepared.working_path(), stream_index, output_format),
        duration_ms,
        "Extracting audio",
        "Audio extraction failed",
        app.clone(),
        event_job_id.clone(),
        cancel_token.clone(),
    )
    .await?;
    if cancel_token.is_cancelled() {
        return Err(ConversionError::Cancelled);
    }
    verify_audio_output(prepared.working_path(), output_format.format()).await?;

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

pub(super) async fn compress_audio(
    app: AppHandle,
    input_path: String,
    preset: AudioCompressionPreset,
    job_id: Option<String>,
    output_options: Option<OutputOptions>,
) -> Result<ConversionResult, ConversionError> {
    let started = Instant::now();
    let (input, format, source_bytes) =
        validate_audio_input_for(&input_path, "compressed M4A audio")?;
    for tool in ["ffmpeg", "ffprobe"] {
        if resolve_tool(tool).is_none() {
            return Err(ConversionError::MissingDependency {
                tool: tool.into(),
                install_hint: "Install FFmpeg with Homebrew".into(),
            });
        }
    }
    compression_support(format).map_err(|missing| ConversionError::MissingDependency {
        tool: missing.join(", "),
        install_hint: "Install the full FFmpeg package with Homebrew".into(),
    })?;

    let source = probe_compression_audio(&input, false).await?;
    let prepared = prepare_output(&input, "m4a", "-compressed", output_options)?;
    let active_job = super::jobs::ActiveJobGuard::register(app.clone(), job_id)?;
    let cancel_token = active_job.cancel_token();
    let event_job_id = active_job.job_id().to_owned();
    let _ = app.emit(
        "conversion-progress",
        ProgressPayload {
            job_id: event_job_id.clone(),
            percent: 0,
            stage: "Compressing audio".into(),
        },
    );

    run_ffmpeg_pass(
        Some(prepared.working_path()),
        compression_arguments(&input, prepared.working_path(), preset, &source),
        source.duration_ms,
        0,
        98,
        "Compressing audio",
        "Audio compression failed",
        app.clone(),
        event_job_id.clone(),
        cancel_token.clone(),
    )
    .await?;
    if cancel_token.is_cancelled() {
        return Err(ConversionError::Cancelled);
    }

    let output = probe_compression_audio(prepared.working_path(), true).await?;
    verify_compression_properties(&source, &output)?;
    let output_bytes = engines::verification::nonempty_file(prepared.working_path())?;
    ensure_output_is_smaller(source_bytes, output_bytes)?;
    if cancel_token.is_cancelled() {
        return Err(ConversionError::Cancelled);
    }

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

fn validate_audio_input_for(
    input_path: &str,
    output_label: &str,
) -> Result<(PathBuf, Format, u64), ConversionError> {
    let input = PathBuf::from(input_path);
    let metadata = std::fs::metadata(&input).map_err(|_| ConversionError::InputNotFound {
        path: input_path.into(),
    })?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > MAX_INPUT_BYTES {
        return Err(ConversionError::UnsupportedConversion {
            input: input_path.into(),
            output: output_label.into(),
        });
    }
    let format = input
        .extension()
        .and_then(|extension| extension.to_str())
        .and_then(Format::from_extension)
        .filter(|format| format.category() == FileCategory::Audio)
        .ok_or_else(|| ConversionError::UnsupportedConversion {
            input: input_path.into(),
            output: output_label.into(),
        })?;
    Ok((input, format, metadata.len()))
}

fn compression_arguments(
    input: &Path,
    output: &Path,
    preset: AudioCompressionPreset,
    source: &CompressionAudioProperties,
) -> Vec<OsString> {
    let mut arguments = vec![
        OsString::from("-hide_banner"),
        OsString::from("-nostdin"),
        OsString::from("-loglevel"),
        OsString::from("error"),
        OsString::from("-i"),
        input.as_os_str().to_os_string(),
        OsString::from("-map"),
        OsString::from("0:a:0"),
        OsString::from("-vn"),
        OsString::from("-sn"),
        OsString::from("-dn"),
        OsString::from("-map_metadata"),
        OsString::from("0"),
        OsString::from("-c:a"),
        OsString::from("aac"),
        OsString::from("-b:a"),
        OsString::from(format!("{}k", preset.bitrate_kbps())),
        OsString::from("-ac"),
        OsString::from(source.channels.to_string()),
    ];
    if AAC_SAMPLE_RATES.contains(&source.sample_rate) {
        arguments.extend([
            OsString::from("-ar"),
            OsString::from(source.sample_rate.to_string()),
        ]);
    }
    arguments.extend([
        OsString::from("-movflags"),
        OsString::from("+faststart"),
        OsString::from("-progress"),
        OsString::from("pipe:1"),
        OsString::from("-nostats"),
        OsString::from("-y"),
        output.as_os_str().to_os_string(),
    ]);
    arguments
}

fn output_encoder(format: Format) -> &'static str {
    match format {
        Format::Mp3 => "libmp3lame",
        Format::Wav => "pcm_s24le",
        Format::Aac | Format::M4a => "aac",
        Format::Flac => "flac",
        Format::Ogg => "libopus",
        _ => unreachable!("validated audio format"),
    }
}

fn expected_codec(format: Format) -> &'static str {
    match format {
        Format::Mp3 => "mp3",
        Format::Wav => "pcm_s24le",
        Format::Aac | Format::M4a => "aac",
        Format::Flac => "flac",
        Format::Ogg => "opus",
        _ => unreachable!("validated audio format"),
    }
}

fn codec_arguments(format: Format) -> &'static [&'static str] {
    match format {
        Format::Mp3 => &["-c:a", "libmp3lame", "-q:a", "2"],
        Format::Wav => &["-c:a", "pcm_s24le"],
        Format::Aac => &["-c:a", "aac", "-b:a", "192k"],
        Format::M4a => &["-c:a", "aac", "-b:a", "192k", "-movflags", "+faststart"],
        Format::Flac => &["-c:a", "flac"],
        Format::Ogg => &["-c:a", "libopus", "-b:a", "160k"],
        _ => unreachable!("validated audio format"),
    }
}

#[allow(clippy::too_many_arguments)]
async fn run_ffmpeg_pass(
    output: Option<&Path>,
    arguments: Vec<OsString>,
    duration_ms: u64,
    progress_start: i32,
    progress_end: i32,
    stage: &'static str,
    failure_message: &'static str,
    app: AppHandle,
    job_id: String,
    cancel_token: CancellationToken,
) -> Result<String, ConversionError> {
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
                    let scaled =
                        progress_start + (percent * (progress_end - progress_start).max(0) / 100);
                    let _ = progress_app.emit(
                        "conversion-progress",
                        ProgressPayload {
                            job_id: job_id.clone(),
                            percent: scaled,
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
        _ = tokio::time::sleep(AUDIO_JOB_TIMEOUT) => ProcessExit::TimedOut,
    };

    match exit {
        ProcessExit::Cancelled => {
            let _ = child.kill().await;
            let _ = progress_task.await;
            let _ = finish_output(stderr_task).await;
            if let Some(path) = output {
                engines::cleanup_partial(path);
            }
            Err(ConversionError::Cancelled)
        }
        ProcessExit::TimedOut => {
            let _ = child.kill().await;
            let _ = progress_task.await;
            let _ = finish_output(stderr_task).await;
            if let Some(path) = output {
                engines::cleanup_partial(path);
            }
            Err(ConversionError::Timeout {
                seconds: AUDIO_JOB_TIMEOUT.as_secs(),
            })
        }
        ProcessExit::Finished(status) => {
            let _ = progress_task.await;
            let stderr = finish_output(stderr_task).await;
            if status.success() {
                Ok(stderr)
            } else {
                if let Some(path) = output {
                    engines::cleanup_partial(path);
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

fn parse_encoder_list(output: &str) -> BTreeSet<String> {
    output
        .lines()
        .filter_map(|line| {
            let mut columns = line.split_whitespace();
            let flags = columns.next()?;
            let encoder = columns.next()?;
            (flags.len() >= 6 && flags.starts_with('A')).then(|| encoder.to_string())
        })
        .collect()
}

async fn probe_compression_audio(
    path: &Path,
    require_m4a_aac: bool,
) -> Result<CompressionAudioProperties, ConversionError> {
    let probe = tokio::time::timeout(
        PROBE_TIMEOUT,
        tool_command("ffprobe")
            .args([
                "-v",
                "error",
                "-show_entries",
                "format=format_name,duration:stream=codec_type,codec_name,channels,sample_rate,duration",
                "-of",
                "json",
            ])
            .arg(path)
            .stdin(Stdio::null())
            .output(),
    )
    .await
    .map_err(|_| ConversionError::Timeout {
        seconds: PROBE_TIMEOUT.as_secs(),
    })?
    .map_err(|error| ConversionError::ProcessFailed {
        message: format!("Could not inspect the audio file: {error}"),
        stderr: String::new(),
        exit_code: None,
    })?;
    if !probe.status.success() {
        return Err(ConversionError::ProcessFailed {
            message: "The audio file could not be read".into(),
            stderr: String::from_utf8_lossy(&probe.stderr).into_owned(),
            exit_code: probe.status.code(),
        });
    }
    let parsed =
        serde_json::from_slice::<CompressionProbeOutput>(&probe.stdout).map_err(|error| {
            ConversionError::ProcessFailed {
                message: format!("The audio details could not be read: {error}"),
                stderr: String::new(),
                exit_code: probe.status.code(),
            }
        })?;

    if require_m4a_aac {
        let format_name = parsed.format.format_name.as_deref().unwrap_or_default();
        let is_m4a = format_name
            .split(',')
            .any(|name| matches!(name, "mov" | "mp4" | "m4a"));
        let is_single_aac_stream = parsed.streams.len() == 1
            && parsed.streams[0].codec_type.as_deref() == Some("audio")
            && parsed.streams[0].codec_name.as_deref() == Some("aac");
        if !is_m4a || !is_single_aac_stream {
            return Err(ConversionError::ProcessFailed {
                message: "The compressed M4A audio could not be verified".into(),
                stderr: String::new(),
                exit_code: None,
            });
        }
    }

    let stream = parsed
        .streams
        .iter()
        .find(|stream| stream.codec_type.as_deref() == Some("audio"))
        .ok_or_else(|| ConversionError::ProcessFailed {
            message: "No readable audio stream was found".into(),
            stderr: String::new(),
            exit_code: None,
        })?;
    if stream.codec_name.as_deref().unwrap_or_default().is_empty() {
        return Err(ConversionError::ProcessFailed {
            message: "The audio codec could not be identified".into(),
            stderr: String::new(),
            exit_code: None,
        });
    }
    let channels = stream
        .channels
        .filter(|channels| (1..=8).contains(channels));
    let sample_rate = stream
        .sample_rate
        .as_deref()
        .and_then(|value| value.parse::<u32>().ok())
        .filter(|rate| (1..=384_000).contains(rate));
    let duration_ms = parsed
        .format
        .duration
        .as_deref()
        .or(stream.duration.as_deref())
        .and_then(parse_probe_duration_ms);
    match (duration_ms, channels, sample_rate) {
        (Some(duration_ms), Some(channels), Some(sample_rate)) => Ok(CompressionAudioProperties {
            duration_ms,
            channels,
            sample_rate,
        }),
        _ => Err(ConversionError::ProcessFailed {
            message: "The audio stream details are invalid or incomplete".into(),
            stderr: String::new(),
            exit_code: None,
        }),
    }
}

fn parse_probe_duration_ms(value: &str) -> Option<u64> {
    let seconds = value.trim().parse::<f64>().ok()?;
    (seconds.is_finite() && seconds > 0.0 && seconds <= (u64::MAX as f64 / 1_000.0))
        .then_some((seconds * 1_000.0).round() as u64)
}

fn verify_compression_properties(
    source: &CompressionAudioProperties,
    output: &CompressionAudioProperties,
) -> Result<(), ConversionError> {
    let duration_matches =
        source.duration_ms.abs_diff(output.duration_ms) <= AUDIO_COMPRESSION_DURATION_TOLERANCE_MS;
    let channels_match = source.channels == output.channels;
    let sample_rate_matches = if AAC_SAMPLE_RATES.contains(&source.sample_rate) {
        source.sample_rate == output.sample_rate
    } else {
        AAC_SAMPLE_RATES.contains(&output.sample_rate)
    };
    if duration_matches && channels_match && sample_rate_matches {
        Ok(())
    } else {
        Err(ConversionError::ProcessFailed {
            message: "The compressed audio did not preserve the source audio properties".into(),
            stderr: String::new(),
            exit_code: None,
        })
    }
}

fn ensure_output_is_smaller(source_bytes: u64, output_bytes: u64) -> Result<(), ConversionError> {
    if output_bytes < source_bytes {
        Ok(())
    } else {
        Err(ConversionError::OutputNotSmaller {
            source_bytes,
            output_bytes,
        })
    }
}

async fn verify_audio_output(output: &Path, format: Format) -> Result<(), ConversionError> {
    engines::verification::nonempty_file(output)?;
    let probe = tokio::time::timeout(
        PROBE_TIMEOUT,
        tool_command("ffprobe")
            .args([
                "-v",
                "error",
                "-show_entries",
                "stream=codec_type,codec_name",
                "-of",
                "csv=p=0",
            ])
            .arg(output)
            .output(),
    )
    .await
    .map_err(|_| ConversionError::Timeout {
        seconds: PROBE_TIMEOUT.as_secs(),
    })?
    .map_err(|error| ConversionError::ProcessFailed {
        message: format!("Could not inspect the extracted audio: {error}"),
        stderr: String::new(),
        exit_code: None,
    })?;
    let streams = String::from_utf8_lossy(&probe.stdout);
    let parsed = streams
        .lines()
        .filter_map(|line| line.trim().split_once(','))
        .collect::<Vec<_>>();
    let valid = probe.status.success()
        && parsed.len() == 1
        && parsed[0].0 == expected_codec(format)
        && parsed[0].1 == "audio";
    if !valid {
        engines::cleanup_partial(output);
        return Err(ConversionError::ProcessFailed {
            message: "The extracted audio could not be verified".into(),
            stderr: String::from_utf8_lossy(&probe.stderr).into_owned(),
            exit_code: probe.status.code(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(values: Vec<OsString>) -> Vec<String> {
        values
            .into_iter()
            .map(|value| value.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn compression_presets_have_stable_aac_bitrates() {
        assert_eq!(AudioCompressionPreset::High.bitrate_kbps(), 256);
        assert_eq!(AudioCompressionPreset::Balanced.bitrate_kbps(), 160);
        assert_eq!(AudioCompressionPreset::Smallest.bitrate_kbps(), 96);
    }

    #[test]
    fn compression_accepts_every_advertised_audio_extension() {
        let directory = tempfile::tempdir().expect("tempdir");
        for extension in ["mp3", "wav", "wave", "aac", "flac", "ogg", "oga", "m4a"] {
            let input = directory.path().join(format!("recording.{extension}"));
            std::fs::write(&input, b"audio fixture").expect("fixture");
            assert!(validate_audio_input_for(
                input.to_string_lossy().as_ref(),
                "compressed M4A audio"
            )
            .is_ok());
        }
    }

    #[test]
    fn compression_arguments_preserve_supported_audio_properties() {
        let source = CompressionAudioProperties {
            duration_ms: 2_000,
            channels: 2,
            sample_rate: 44_100,
        };
        let arguments = strings(compression_arguments(
            Path::new("input.wav"),
            Path::new("output.m4a"),
            AudioCompressionPreset::Balanced,
            &source,
        ));
        assert!(arguments.windows(2).any(|pair| pair == ["-map", "0:a:0"]));
        assert!(arguments.windows(2).any(|pair| pair == ["-c:a", "aac"]));
        assert!(arguments.windows(2).any(|pair| pair == ["-b:a", "160k"]));
        assert!(arguments.windows(2).any(|pair| pair == ["-ac", "2"]));
        assert!(arguments.windows(2).any(|pair| pair == ["-ar", "44100"]));
        assert!(arguments.contains(&"-vn".into()));
        assert_eq!(arguments.last().map(String::as_str), Some("output.m4a"));
    }

    #[test]
    fn compression_allows_aac_to_choose_a_supported_rate_for_unusual_sources() {
        let arguments = strings(compression_arguments(
            Path::new("input.wav"),
            Path::new("output.m4a"),
            AudioCompressionPreset::Smallest,
            &CompressionAudioProperties {
                duration_ms: 1_000,
                channels: 1,
                sample_rate: 50_000,
            },
        ));
        assert!(!arguments.iter().any(|argument| argument == "-ar"));
    }

    #[test]
    fn compressed_audio_requires_duration_channels_and_supported_sample_rate() {
        let source = CompressionAudioProperties {
            duration_ms: 10_000,
            channels: 2,
            sample_rate: 48_000,
        };
        assert!(verify_compression_properties(
            &source,
            &CompressionAudioProperties {
                duration_ms: 10_250,
                channels: 2,
                sample_rate: 48_000,
            },
        )
        .is_ok());
        for invalid in [
            CompressionAudioProperties {
                duration_ms: 10_251,
                channels: 2,
                sample_rate: 48_000,
            },
            CompressionAudioProperties {
                duration_ms: 10_000,
                channels: 1,
                sample_rate: 48_000,
            },
            CompressionAudioProperties {
                duration_ms: 10_000,
                channels: 2,
                sample_rate: 44_100,
            },
        ] {
            assert!(verify_compression_properties(&source, &invalid).is_err());
        }
    }

    #[test]
    fn compressed_audio_must_be_strictly_smaller() {
        assert!(ensure_output_is_smaller(1_000, 999).is_ok());
        for output_bytes in [1_000, 1_001] {
            assert!(matches!(
                ensure_output_is_smaller(1_000, output_bytes),
                Err(ConversionError::OutputNotSmaller {
                    source_bytes: 1_000,
                    output_bytes: actual,
                }) if actual == output_bytes
            ));
        }
    }

    #[test]
    fn parses_audio_track_identity_and_layout() {
        let tracks = parse_audio_tracks(
            br#"{"streams":[{"index":1,"codec_name":"aac","channels":2,"channel_layout":"stereo","sample_rate":"48000","tags":{"language":"eng","title":"Main mix"},"disposition":{"default":1}},{"index":3,"codec_name":"ac3","channels":6,"channel_layout":"5.1(side)","sample_rate":"48000","tags":{"language":"fra","title":"Commentary"},"disposition":{"default":0}}]}"#,
        )
        .expect("tracks");
        assert_eq!(tracks.len(), 2);
        assert_eq!(tracks[0].stream_index, 1);
        assert_eq!(tracks[0].language.as_deref(), Some("eng"));
        assert_eq!(tracks[0].title.as_deref(), Some("Main mix"));
        assert_eq!(tracks[0].channel_layout.as_deref(), Some("stereo"));
        assert_eq!(tracks[0].sample_rate, Some(48_000));
        assert!(tracks[0].is_default);
        assert_eq!(tracks[1].stream_index, 3);
        assert_eq!(tracks[1].channels, Some(6));
        assert!(!tracks[1].is_default);
    }

    #[test]
    fn extraction_maps_exactly_one_audio_stream() {
        let arguments = strings(extraction_arguments(
            Path::new("input movie.mkv"),
            Path::new("output.wav"),
            3,
            super::super::convert::AudioOutputFormat::Wav,
        ));
        assert!(arguments.windows(2).any(|pair| pair == ["-map", "0:3"]));
        assert!(arguments
            .windows(2)
            .any(|pair| pair == ["-c:a", "pcm_s24le"]));
        assert!(arguments.contains(&"-vn".into()));
        assert!(arguments.contains(&"-sn".into()));
        assert!(arguments.contains(&"-dn".into()));
        assert!(!arguments.iter().any(|argument| argument == "0:a:0"));
        assert_eq!(arguments.last().map(String::as_str), Some("output.wav"));
    }

    #[test]
    fn every_audio_format_has_an_explicit_encoder_and_codec() {
        for format in [
            Format::Mp3,
            Format::Wav,
            Format::Aac,
            Format::Flac,
            Format::Ogg,
            Format::M4a,
        ] {
            assert!(!output_encoder(format).is_empty());
            assert!(!expected_codec(format).is_empty());
            assert!(codec_arguments(format).contains(&output_encoder(format)));
        }
    }

    #[tokio::test]
    async fn discovers_and_extracts_the_selected_real_audio_track() {
        let Some(ffmpeg) = resolve_tool("ffmpeg") else {
            return;
        };
        if resolve_tool("ffprobe").is_none()
            || extraction_support(super::super::convert::AudioOutputFormat::Wav).is_err()
        {
            return;
        }

        let directory = tempfile::tempdir().expect("tempdir");
        let video = directory.path().join("multi-track.mkv");
        let generated = std::process::Command::new(&ffmpeg)
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-f",
                "lavfi",
                "-i",
                "color=size=160x90:rate=10:duration=2",
                "-f",
                "lavfi",
                "-i",
                "anullsrc=r=48000:cl=stereo:d=2",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=880:sample_rate=48000:duration=2",
                "-map",
                "0:v:0",
                "-map",
                "1:a:0",
                "-map",
                "2:a:0",
                "-c:v",
                "mpeg4",
                "-c:a",
                "pcm_s16le",
                "-metadata:s:a:0",
                "language=eng",
                "-metadata:s:a:0",
                "title=Main mix",
                "-metadata:s:a:1",
                "language=fra",
                "-metadata:s:a:1",
                "title=French commentary",
                "-disposition:a:0",
                "default",
                "-disposition:a:1",
                "0",
                "-y",
            ])
            .arg(&video)
            .status()
            .expect("generate multi-track video fixture");
        assert!(generated.success());

        let original = std::fs::read(&video).expect("source bytes");
        let tracks = probe_audio_tracks(&video)
            .await
            .expect("probe audio tracks");
        assert_eq!(tracks.len(), 2);
        assert_eq!(tracks[0].language.as_deref(), Some("eng"));
        assert_eq!(tracks[0].title.as_deref(), Some("Main mix"));
        assert!(tracks[0].is_default);
        assert_eq!(tracks[1].language.as_deref(), Some("fra"));
        assert_eq!(tracks[1].title.as_deref(), Some("French commentary"));
        assert!(!tracks[1].is_default);

        let selected = &tracks[1];
        let output = directory.path().join("french-commentary.wav");
        let extracted = std::process::Command::new(&ffmpeg)
            .args(extraction_arguments(
                &video,
                &output,
                selected.stream_index,
                super::super::convert::AudioOutputFormat::Wav,
            ))
            .status()
            .expect("extract selected audio track");
        assert!(extracted.success());
        verify_audio_output(&output, Format::Wav)
            .await
            .expect("verified audio output");

        let decoded = std::process::Command::new(&ffmpeg)
            .args(["-hide_banner", "-loglevel", "error", "-i"])
            .arg(&output)
            .args(["-f", "s16le", "-c:a", "pcm_s16le", "-"])
            .output()
            .expect("decode selected audio");
        assert!(decoded.status.success());
        assert!(decoded.stdout.iter().any(|sample| *sample != 0));
        assert_eq!(std::fs::read(&video).expect("retained source"), original);
    }

    #[tokio::test]
    async fn creates_and_verifies_smaller_m4a_without_modifying_the_source() {
        let Some(ffmpeg) = resolve_tool("ffmpeg") else {
            return;
        };
        if resolve_tool("ffprobe").is_none() || !ffmpeg_encoders().contains("aac") {
            return;
        }

        let directory = tempfile::tempdir().expect("tempdir");
        let input = directory.path().join("recording.wav");
        let generated = std::process::Command::new(&ffmpeg)
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:sample_rate=48000:duration=3",
                "-ac",
                "2",
                "-c:a",
                "pcm_s24le",
                "-y",
            ])
            .arg(&input)
            .status()
            .expect("generate WAV fixture");
        assert!(generated.success());
        let source_bytes = std::fs::read(&input).expect("source bytes");
        let source = probe_compression_audio(&input, false)
            .await
            .expect("source probe");
        let prepared = prepare_output(&input, "m4a", "-compressed", None).expect("prepared output");
        let compressed = std::process::Command::new(&ffmpeg)
            .args(compression_arguments(
                &input,
                prepared.working_path(),
                AudioCompressionPreset::Smallest,
                &source,
            ))
            .status()
            .expect("compress WAV fixture");
        assert!(compressed.success());

        let output = probe_compression_audio(prepared.working_path(), true)
            .await
            .expect("output probe");
        verify_compression_properties(&source, &output).expect("preserved audio properties");
        let output_bytes = std::fs::metadata(prepared.working_path())
            .expect("output metadata")
            .len();
        ensure_output_is_smaller(source_bytes.len() as u64, output_bytes).expect("smaller output");
        let result = prepared
            .commit(ConversionResult {
                output_path: String::new(),
                output_paths: Vec::new(),
                output_size: 0,
                duration_ms: 0,
                undo_manifest: None,
            })
            .expect("atomic commit");
        assert!(result.output_path.ends_with("recording-compressed.m4a"));
        assert_eq!(
            std::fs::read(&input).expect("retained source"),
            source_bytes
        );
    }
}
