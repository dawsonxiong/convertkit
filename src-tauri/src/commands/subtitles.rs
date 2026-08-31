use std::collections::BTreeSet;
use std::ffi::OsString;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

use crate::engines::{resolve_tool, tool_command, verification, ConversionResult};
use crate::error::ConversionError;
use crate::progress::ProgressPayload;

use super::output::{prepare_output, OutputOptions};

const PROBE_TIMEOUT: Duration = Duration::from_secs(15);
const VERIFY_PREFIX_BYTES: u64 = 128 * 1024;

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SubtitleOutputFormat {
    Srt,
    Vtt,
}

impl SubtitleOutputFormat {
    pub(super) fn extension(self) -> &'static str {
        match self {
            Self::Srt => "srt",
            Self::Vtt => "vtt",
        }
    }

    fn encoder(self) -> &'static str {
        match self {
            Self::Srt => "srt",
            Self::Vtt => "webvtt",
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubtitleTrack {
    pub stream_index: u32,
    pub codec: String,
    pub language: Option<String>,
    pub title: Option<String>,
    pub is_default: bool,
    pub is_forced: bool,
    pub supported: bool,
}

impl SubtitleTrack {
    pub(super) fn from_parts(
        stream_index: u32,
        codec: String,
        language: Option<String>,
        title: Option<String>,
        is_default: bool,
        is_forced: bool,
    ) -> Self {
        let language = bounded_tag(language, 16).filter(|value| value != "und");
        let title = bounded_tag(title, 120);
        let supported = is_supported_text_codec(&codec);
        Self {
            stream_index,
            codec,
            language,
            title,
            is_default,
            is_forced,
            supported,
        }
    }
}

#[derive(Debug, Default, Deserialize)]
struct ProbeOutput {
    #[serde(default)]
    streams: Vec<ProbeStream>,
}

#[derive(Debug, Deserialize)]
struct ProbeStream {
    index: u32,
    codec_name: Option<String>,
    #[serde(default)]
    tags: ProbeTags,
    #[serde(default)]
    disposition: ProbeDisposition,
}

#[derive(Debug, Default, Deserialize)]
struct ProbeTags {
    language: Option<String>,
    title: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct ProbeDisposition {
    #[serde(default)]
    default: i32,
    #[serde(default)]
    forced: i32,
}

fn bounded_tag(value: Option<String>, maximum: usize) -> Option<String> {
    let value = value?.trim().to_string();
    if value.is_empty() {
        return None;
    }
    Some(value.chars().take(maximum).collect())
}

pub(super) fn is_supported_text_codec(codec: &str) -> bool {
    matches!(
        codec.to_ascii_lowercase().as_str(),
        "ass"
            | "jacosub"
            | "microdvd"
            | "mov_text"
            | "mpl2"
            | "pjs"
            | "realtext"
            | "sami"
            | "ssa"
            | "srt"
            | "stl"
            | "subrip"
            | "subviewer"
            | "subviewer1"
            | "text"
            | "vplayer"
            | "webvtt"
    )
}

fn parse_tracks(bytes: &[u8]) -> Result<Vec<SubtitleTrack>, ConversionError> {
    let parsed = serde_json::from_slice::<ProbeOutput>(bytes).map_err(|error| {
        ConversionError::ProcessFailed {
            message: format!("Could not read subtitle track details: {error}"),
            stderr: String::new(),
            exit_code: None,
        }
    })?;
    Ok(parsed
        .streams
        .into_iter()
        .map(|stream| {
            SubtitleTrack::from_parts(
                stream.index,
                stream.codec_name.unwrap_or_else(|| "unknown".into()),
                stream.tags.language,
                stream.tags.title,
                stream.disposition.default != 0,
                stream.disposition.forced != 0,
            )
        })
        .collect())
}

async fn probe_tracks(path: &Path) -> Result<Vec<SubtitleTrack>, ConversionError> {
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
                "s",
                "-show_entries",
                "stream=index,codec_name:stream_tags=language,title:stream_disposition=default,forced",
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
        message: format!("Could not inspect subtitle tracks: {error}"),
        stderr: String::new(),
        exit_code: None,
    })?;
    if !output.status.success() {
        return Err(ConversionError::ProcessFailed {
            message: "Subtitle tracks could not be inspected".into(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            exit_code: output.status.code(),
        });
    }
    parse_tracks(&output.stdout)
}

#[tauri::command]
pub async fn get_subtitle_tracks(path: String) -> Result<Vec<SubtitleTrack>, ConversionError> {
    let input = super::video::validate_video_input(&path)?;
    probe_tracks(&input).await
}

pub(super) fn extraction_support(
    codec: &str,
    format: SubtitleOutputFormat,
) -> Result<(), Vec<String>> {
    let mut missing = Vec::new();
    if !is_supported_text_codec(codec) {
        missing.push(format!("unsupported subtitle codec {codec}"));
    }
    if !subtitle_encoders().contains(format.encoder()) {
        missing.push(format!("{} subtitle encoder", format.encoder()));
    }
    if missing.is_empty() {
        Ok(())
    } else {
        Err(missing)
    }
}

fn subtitle_encoders() -> &'static BTreeSet<String> {
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
            .map(|output| parse_subtitle_encoders(&String::from_utf8_lossy(&output.stdout)))
            .unwrap_or_default()
    })
}

fn parse_subtitle_encoders(output: &str) -> BTreeSet<String> {
    output
        .lines()
        .filter_map(|line| {
            let mut columns = line.split_whitespace();
            let flags = columns.next()?;
            let encoder = columns.next()?;
            (flags.len() >= 6 && flags.starts_with('S')).then(|| encoder.to_string())
        })
        .collect()
}

fn extraction_arguments(
    input: &Path,
    output: &Path,
    stream_index: u32,
    format: SubtitleOutputFormat,
) -> Vec<OsString> {
    vec![
        OsString::from("-hide_banner"),
        OsString::from("-loglevel"),
        OsString::from("error"),
        OsString::from("-i"),
        input.as_os_str().to_os_string(),
        OsString::from("-map"),
        OsString::from(format!("0:{stream_index}")),
        OsString::from("-vn"),
        OsString::from("-an"),
        OsString::from("-dn"),
        OsString::from("-c:s"),
        OsString::from(format.encoder()),
        OsString::from("-progress"),
        OsString::from("pipe:1"),
        OsString::from("-nostats"),
        OsString::from("-y"),
        output.as_os_str().to_os_string(),
    ]
}

pub(super) fn verify_output(
    output: &Path,
    format: SubtitleOutputFormat,
) -> Result<(), ConversionError> {
    verification::nonempty_file(output)?;
    let file = std::fs::File::open(output).map_err(|_| ConversionError::OutputMissing)?;
    let mut prefix = Vec::new();
    file.take(VERIFY_PREFIX_BYTES)
        .read_to_end(&mut prefix)
        .map_err(|error| ConversionError::ProcessFailed {
            message: format!("Could not verify the subtitle output: {error}"),
            stderr: String::new(),
            exit_code: None,
        })?;
    let text = std::str::from_utf8(&prefix).map_err(|_| ConversionError::ProcessFailed {
        message: "The subtitle output is not valid UTF-8 text".into(),
        stderr: String::new(),
        exit_code: None,
    })?;
    let valid = text.contains("-->")
        && (format == SubtitleOutputFormat::Srt || text.trim_start().starts_with("WEBVTT"));
    if !valid {
        return Err(ConversionError::ProcessFailed {
            message: "No subtitle cues were written".into(),
            stderr: String::new(),
            exit_code: None,
        });
    }
    Ok(())
}

pub(super) async fn extract_subtitles(
    app: AppHandle,
    input_path: String,
    stream_index: u32,
    stream_codec: String,
    output_format: SubtitleOutputFormat,
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
    extraction_support(&stream_codec, output_format).map_err(|missing| {
        ConversionError::UnsupportedConversion {
            input: missing.join(", "),
            output: output_format.extension().into(),
        }
    })?;
    let track = probe_tracks(&input)
        .await?
        .into_iter()
        .find(|track| track.stream_index == stream_index)
        .ok_or_else(|| ConversionError::UnsupportedConversion {
            input: input_path.clone(),
            output: format!("missing subtitle track {stream_index}"),
        })?;
    if !track.supported || !track.codec.eq_ignore_ascii_case(&stream_codec) {
        return Err(ConversionError::UnsupportedConversion {
            input: track.codec,
            output: output_format.extension().into(),
        });
    }

    let prepared = prepare_output(
        &input,
        output_format.extension(),
        "-subtitles",
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
            stage: "Extracting subtitles".into(),
        },
    );

    super::video::run_ffmpeg_job(
        prepared.working_path(),
        extraction_arguments(&input, prepared.working_path(), stream_index, output_format),
        duration_ms,
        "Extracting subtitles",
        "Subtitle extraction failed",
        app.clone(),
        event_job_id.clone(),
        cancel_token.clone(),
    )
    .await?;
    if cancel_token.is_cancelled() {
        return Err(ConversionError::Cancelled);
    }
    verify_output(prepared.working_path(), output_format)?;

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

    #[test]
    fn parses_text_and_image_tracks_without_advertising_ocr() {
        let tracks = parse_tracks(
            br#"{"streams":[{"index":2,"codec_name":"subrip","tags":{"language":"eng","title":"English CC"},"disposition":{"default":1,"forced":0}},{"index":4,"codec_name":"hdmv_pgs_subtitle","tags":{"language":"fra"},"disposition":{"default":0,"forced":1}}]}"#,
        )
        .expect("tracks");
        assert_eq!(tracks.len(), 2);
        assert!(tracks[0].supported);
        assert!(tracks[0].is_default);
        assert_eq!(tracks[0].title.as_deref(), Some("English CC"));
        assert!(!tracks[1].supported);
        assert!(tracks[1].is_forced);
    }

    #[test]
    fn extraction_maps_exactly_one_subtitle_stream() {
        let arguments = extraction_arguments(
            Path::new("input movie.mkv"),
            Path::new("output.srt"),
            4,
            SubtitleOutputFormat::Srt,
        )
        .into_iter()
        .map(|value| value.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
        assert!(arguments.windows(2).any(|pair| pair == ["-map", "0:4"]));
        assert!(arguments.windows(2).any(|pair| pair == ["-c:s", "srt"]));
        assert!(arguments.contains(&"-vn".into()));
        assert!(arguments.contains(&"-an".into()));
        assert_eq!(arguments.last().map(String::as_str), Some("output.srt"));
    }

    #[test]
    fn output_verification_requires_real_text_cues() {
        let directory = tempfile::tempdir().expect("tempdir");
        let srt = directory.path().join("captions.srt");
        std::fs::write(&srt, b"1\n00:00:00,000 --> 00:00:01,000\nHello\n")
            .expect("subtitle fixture");
        verify_output(&srt, SubtitleOutputFormat::Srt).expect("valid srt");
        std::fs::write(&srt, b"not subtitles").expect("invalid fixture");
        assert!(verify_output(&srt, SubtitleOutputFormat::Srt).is_err());
    }

    #[tokio::test]
    async fn discovers_and_extracts_a_real_text_subtitle_track() {
        let Some(ffmpeg) = resolve_tool("ffmpeg") else {
            return;
        };
        if resolve_tool("ffprobe").is_none()
            || !subtitle_encoders().contains("srt")
            || !subtitle_encoders().contains("webvtt")
        {
            return;
        }

        let directory = tempfile::tempdir().expect("tempdir");
        let english_subtitle = directory.path().join("english.srt");
        let french_subtitle = directory.path().join("french.srt");
        let video = directory.path().join("movie.mkv");
        std::fs::write(
            &english_subtitle,
            b"1\n00:00:00,000 --> 00:00:01,500\nHello from ConvertKit\n",
        )
        .expect("English subtitle fixture");
        std::fs::write(
            &french_subtitle,
            b"1\n00:00:00,000 --> 00:00:01,500\nBonjour de ConvertKit\n",
        )
        .expect("French subtitle fixture");
        let generated = std::process::Command::new(&ffmpeg)
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-f",
                "lavfi",
                "-i",
                "color=size=160x90:rate=10:duration=2",
                "-i",
            ])
            .arg(&english_subtitle)
            .arg("-i")
            .arg(&french_subtitle)
            .args([
                "-map",
                "0:v:0",
                "-map",
                "1:0",
                "-map",
                "2:0",
                "-c:v",
                "mpeg4",
                "-c:s",
                "srt",
                "-metadata:s:s:0",
                "language=eng",
                "-metadata:s:s:0",
                "title=English CC",
                "-metadata:s:s:1",
                "language=fra",
                "-metadata:s:s:1",
                "title=French forced",
                "-disposition:s:0",
                "default",
                "-disposition:s:1",
                "forced",
                "-y",
            ])
            .arg(&video)
            .status()
            .expect("generate video fixture");
        assert!(generated.success());
        let original = std::fs::read(&video).expect("source bytes");
        let tracks = probe_tracks(&video).await.expect("probe tracks");
        assert_eq!(tracks.len(), 2);
        assert_eq!(tracks[0].language.as_deref(), Some("eng"));
        assert_eq!(tracks[0].title.as_deref(), Some("English CC"));
        assert!(tracks[0].is_default);
        assert!(tracks[0].supported);
        assert_eq!(tracks[1].language.as_deref(), Some("fra"));
        assert_eq!(tracks[1].title.as_deref(), Some("French forced"));
        assert!(tracks[1].is_forced);
        assert!(tracks[1].supported);

        for format in [SubtitleOutputFormat::Srt, SubtitleOutputFormat::Vtt] {
            let output = directory
                .path()
                .join(format!("export.{}", format.extension()));
            let status = std::process::Command::new(&ffmpeg)
                .args(extraction_arguments(
                    &video,
                    &output,
                    tracks[1].stream_index,
                    format,
                ))
                .status()
                .expect("extract subtitles");
            assert!(status.success());
            verify_output(&output, format).expect("verified subtitle output");
            let exported = std::fs::read_to_string(&output).expect("subtitle output");
            assert!(exported.contains("Bonjour de ConvertKit"));
            assert!(!exported.contains("Hello from ConvertKit"));
        }
        assert_eq!(std::fs::read(&video).expect("retained source"), original);
    }
}
