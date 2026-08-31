use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{BufReader, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use md5::Md5;
use serde::{Deserialize, Serialize};
use sha1::Sha1;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter};
use tokio_util::sync::CancellationToken;

use crate::error::ConversionError;
use crate::formats::{FileCategory, Format};

use super::detect::FileInfoResponse;
use super::jobs::ActiveJobGuard;

const HASH_BUFFER_SIZE: usize = 1024 * 1024;
const PROBE_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_CHECKSUM_MANIFEST_BYTES: u64 = 1024 * 1024;
const MAX_CHECKSUM_MANIFEST_ENTRIES: usize = 100;
const MAX_CHECKSUM_MANIFEST_LINE_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileHashes {
    pub md5: String,
    pub sha1: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChecksumManifestInput {
    pub input_path: String,
    pub relative_path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChecksumManifestCreationResult {
    pub output_path: String,
    pub entry_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ChecksumManifestAlgorithm {
    Md5,
    Sha1,
    Sha256,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ChecksumManifestStatus {
    Match,
    Mismatch,
    Missing,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChecksumManifestVerificationEntry {
    pub relative_path: String,
    pub expected_digest: String,
    pub actual_digest: Option<String>,
    pub status: ChecksumManifestStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChecksumManifestVerificationResult {
    pub manifest_path: String,
    pub algorithm: ChecksumManifestAlgorithm,
    pub entries: Vec<ChecksumManifestVerificationEntry>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct InspectionProgress {
    job_id: String,
    percent: u8,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TechnicalMetadata {
    pub page_count: Option<u32>,
    pub page_size: Option<String>,
    pub container: Option<String>,
    pub duration_seconds: Option<f64>,
    pub bit_rate: Option<u64>,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub frame_rate: Option<f64>,
    pub audio_sample_rate: Option<u32>,
    pub audio_channels: Option<u32>,
    pub audio_tracks: Vec<super::audio::AudioTrack>,
    pub subtitle_tracks: Vec<super::subtitles::SubtitleTrack>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InspectionReportEntry {
    pub file: FileInfoResponse,
    pub technical_metadata: Option<TechnicalMetadata>,
    pub checksums: Option<FileHashes>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct InspectionReport<'a> {
    schema_version: u8,
    generated_by: &'static str,
    files: &'a [InspectionReportEntry],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InspectionReportFormat {
    Json,
    Csv,
}

impl InspectionReportFormat {
    fn parse(value: &str) -> Result<Self, String> {
        match value.to_ascii_lowercase().as_str() {
            "json" => Ok(Self::Json),
            "csv" => Ok(Self::Csv),
            _ => Err("Inspection reports must use JSON or CSV".into()),
        }
    }
}

#[derive(Debug, Default, Deserialize)]
struct FfprobeOutput {
    #[serde(default)]
    streams: Vec<FfprobeStream>,
    format: Option<FfprobeFormat>,
}

#[derive(Debug, Deserialize)]
struct FfprobeFormat {
    format_name: Option<String>,
    duration: Option<String>,
    bit_rate: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FfprobeStream {
    index: Option<u32>,
    codec_type: Option<String>,
    codec_name: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    r_frame_rate: Option<String>,
    sample_rate: Option<String>,
    channels: Option<u32>,
    channel_layout: Option<String>,
    #[serde(default)]
    tags: FfprobeTags,
    #[serde(default)]
    disposition: FfprobeDisposition,
}

#[derive(Debug, Default, Deserialize)]
struct FfprobeTags {
    language: Option<String>,
    title: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct FfprobeDisposition {
    #[serde(default)]
    default: i32,
    #[serde(default)]
    forced: i32,
}

/// Read bounded, format-aware metadata. Missing optional probe tools simply
/// leave their fields empty; inspecting the rest of the file still succeeds.
#[tauri::command]
pub async fn get_technical_metadata(path: String) -> Result<TechnicalMetadata, String> {
    let path = PathBuf::from(path);
    if !path.is_file() {
        return Err(format!("Cannot inspect missing file: {}", path.display()));
    }

    let format = path
        .extension()
        .and_then(|extension| extension.to_str())
        .and_then(Format::from_extension);

    match format {
        Some(Format::Pdf) => Ok(probe_pdf(&path).await.unwrap_or_default()),
        Some(format) if matches!(format.category(), FileCategory::Video | FileCategory::Audio) => {
            Ok(probe_media(&path).await.unwrap_or_default())
        }
        _ => Ok(TechnicalMetadata::default()),
    }
}

/// Write an inspection report chosen by the user. Reports are committed atomically
/// and are never allowed to replace one of the files being inspected.
#[tauri::command]
pub fn export_inspection_report(
    output_path: String,
    format: String,
    entries: Vec<InspectionReportEntry>,
) -> Result<String, String> {
    if entries.is_empty() {
        return Err("Add at least one file before exporting a report".into());
    }
    if entries.len() > 100 {
        return Err("Inspection reports are limited to 100 files".into());
    }

    let format = InspectionReportFormat::parse(&format)?;
    let output_path = PathBuf::from(output_path);
    if output_path.as_os_str().is_empty() || output_path.file_name().is_none() {
        return Err("Choose a valid report file".into());
    }
    if entries
        .iter()
        .any(|entry| same_report_path(Path::new(&entry.file.path), &output_path))
    {
        return Err("An inspection report cannot replace a file being inspected".into());
    }

    let bytes = inspection_report_bytes(format, &entries)?;
    write_report_atomically(&output_path, &bytes)?;
    Ok(output_path.to_string_lossy().into_owned())
}

fn same_report_path(source: &Path, output: &Path) -> bool {
    if source == output {
        return true;
    }
    match (source.canonicalize(), output.canonicalize()) {
        (Ok(source), Ok(output)) => source == output,
        _ => false,
    }
}

fn inspection_report_bytes(
    format: InspectionReportFormat,
    entries: &[InspectionReportEntry],
) -> Result<Vec<u8>, String> {
    match format {
        InspectionReportFormat::Json => serde_json::to_vec_pretty(&InspectionReport {
            schema_version: 1,
            generated_by: "ConvertKit",
            files: entries,
        })
        .map_err(|error| format!("Could not create JSON report: {error}")),
        InspectionReportFormat::Csv => Ok(inspection_report_csv(entries).into_bytes()),
    }
}

fn inspection_report_csv(entries: &[InspectionReportEntry]) -> String {
    const HEADERS: &[&str] = &[
        "name",
        "path",
        "extension",
        "format",
        "category",
        "mime_type",
        "size_bytes",
        "image_width",
        "image_height",
        "created_at_unix_ms",
        "modified_at_unix_ms",
        "read_only",
        "page_count",
        "page_size",
        "container",
        "duration_seconds",
        "bit_rate",
        "video_codec",
        "audio_codec",
        "media_width",
        "media_height",
        "frame_rate",
        "audio_sample_rate",
        "audio_channels",
        "subtitle_tracks",
        "md5",
        "sha1",
        "sha256",
    ];

    let mut output = String::new();
    output.push_str(&HEADERS.join(","));
    output.push_str("\r\n");
    for entry in entries {
        let technical = entry.technical_metadata.as_ref();
        let checksums = entry.checksums.as_ref();
        let values = [
            entry.file.name.clone(),
            entry.file.path.clone(),
            entry.file.extension.clone(),
            entry.file.format.clone(),
            entry.file.category.clone(),
            entry.file.mime_type.clone().unwrap_or_default(),
            entry.file.size.to_string(),
            optional_value(entry.file.width),
            optional_value(entry.file.height),
            optional_value(entry.file.created_at),
            optional_value(entry.file.modified_at),
            entry.file.read_only.to_string(),
            technical
                .and_then(|value| value.page_count)
                .map_or_else(String::new, |value| value.to_string()),
            technical
                .and_then(|value| value.page_size.clone())
                .unwrap_or_default(),
            technical
                .and_then(|value| value.container.clone())
                .unwrap_or_default(),
            technical
                .and_then(|value| value.duration_seconds)
                .map_or_else(String::new, |value| value.to_string()),
            technical
                .and_then(|value| value.bit_rate)
                .map_or_else(String::new, |value| value.to_string()),
            technical
                .and_then(|value| value.video_codec.clone())
                .unwrap_or_default(),
            technical
                .and_then(|value| value.audio_codec.clone())
                .unwrap_or_default(),
            technical
                .and_then(|value| value.width)
                .map_or_else(String::new, |value| value.to_string()),
            technical
                .and_then(|value| value.height)
                .map_or_else(String::new, |value| value.to_string()),
            technical
                .and_then(|value| value.frame_rate)
                .map_or_else(String::new, |value| value.to_string()),
            technical
                .and_then(|value| value.audio_sample_rate)
                .map_or_else(String::new, |value| value.to_string()),
            technical
                .and_then(|value| value.audio_channels)
                .map_or_else(String::new, |value| value.to_string()),
            technical
                .map(|value| {
                    value
                        .subtitle_tracks
                        .iter()
                        .map(|track| {
                            format!(
                                "{}:{}:{}",
                                track.stream_index,
                                track.language.as_deref().unwrap_or("und"),
                                track.codec
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(";")
                })
                .unwrap_or_default(),
            checksums.map(|value| value.md5.clone()).unwrap_or_default(),
            checksums
                .map(|value| value.sha1.clone())
                .unwrap_or_default(),
            checksums
                .map(|value| value.sha256.clone())
                .unwrap_or_default(),
        ];
        output.push_str(
            &values
                .iter()
                .map(|value| escape_csv(value))
                .collect::<Vec<_>>()
                .join(","),
        );
        output.push_str("\r\n");
    }
    output
}

fn optional_value(value: Option<impl ToString>) -> String {
    value.map_or_else(String::new, |value| value.to_string())
}

fn escape_csv(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_owned()
    }
}

fn write_report_atomically(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .filter(|parent| parent.is_dir())
        .ok_or_else(|| "The report folder does not exist".to_string())?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)
        .map_err(|error| format!("Could not create the report: {error}"))?;
    temporary
        .write_all(bytes)
        .and_then(|()| temporary.flush())
        .map_err(|error| format!("Could not write the report: {error}"))?;
    temporary
        .persist(path)
        .map_err(|error| format!("Could not save the report: {}", error.error))?;
    Ok(())
}

async fn probe_pdf(path: &Path) -> Option<TechnicalMetadata> {
    let output = run_probe("pdfinfo", &[path.as_os_str()]).await?;
    output
        .status
        .success()
        .then(|| parse_pdfinfo(&output.stdout))
}

fn parse_pdfinfo(bytes: &[u8]) -> TechnicalMetadata {
    let output = String::from_utf8_lossy(bytes);
    let page_count = field_value(&output, "Pages").and_then(|value| value.parse::<u32>().ok());
    let page_size = field_value(&output, "Page size").map(str::to_owned);
    TechnicalMetadata {
        page_count,
        page_size,
        ..TechnicalMetadata::default()
    }
}

fn field_value<'a>(output: &'a str, field: &str) -> Option<&'a str> {
    output.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.trim()
            .eq_ignore_ascii_case(field)
            .then(|| value.trim())
    })
}

async fn probe_media(path: &Path) -> Option<TechnicalMetadata> {
    let arguments = [
        std::ffi::OsStr::new("-v"),
        std::ffi::OsStr::new("error"),
        std::ffi::OsStr::new("-show_entries"),
        std::ffi::OsStr::new(
            "format=format_name,duration,bit_rate:stream=index,codec_type,codec_name,width,height,r_frame_rate,sample_rate,channels,channel_layout:stream_tags=language,title:stream_disposition=default,forced",
        ),
        std::ffi::OsStr::new("-of"),
        std::ffi::OsStr::new("json"),
        path.as_os_str(),
    ];
    let output = run_probe("ffprobe", &arguments).await?;
    if !output.status.success() {
        return None;
    }
    let parsed = serde_json::from_slice::<FfprobeOutput>(&output.stdout).ok()?;
    Some(metadata_from_ffprobe(parsed))
}

fn metadata_from_ffprobe(probe: FfprobeOutput) -> TechnicalMetadata {
    let video = probe
        .streams
        .iter()
        .find(|stream| stream.codec_type.as_deref() == Some("video"));
    let audio = probe
        .streams
        .iter()
        .find(|stream| stream.codec_type.as_deref() == Some("audio"));
    let audio_tracks = probe
        .streams
        .iter()
        .filter(|stream| stream.codec_type.as_deref() == Some("audio"))
        .filter_map(|stream| {
            Some(super::audio::AudioTrack::from_parts(
                super::audio::AudioTrackParts {
                    stream_index: stream.index?,
                    codec: stream
                        .codec_name
                        .clone()
                        .unwrap_or_else(|| "unknown".into()),
                    language: stream.tags.language.clone(),
                    title: stream.tags.title.clone(),
                    channels: stream.channels,
                    channel_layout: stream.channel_layout.clone(),
                    sample_rate: stream.sample_rate.clone(),
                    is_default: stream.disposition.default != 0,
                },
            ))
        })
        .collect();
    let subtitle_tracks = probe
        .streams
        .iter()
        .filter(|stream| stream.codec_type.as_deref() == Some("subtitle"))
        .filter_map(|stream| {
            Some(super::subtitles::SubtitleTrack::from_parts(
                stream.index?,
                stream
                    .codec_name
                    .clone()
                    .unwrap_or_else(|| "unknown".into()),
                stream.tags.language.clone(),
                stream.tags.title.clone(),
                stream.disposition.default != 0,
                stream.disposition.forced != 0,
            ))
        })
        .collect();
    let format = probe.format;

    TechnicalMetadata {
        container: format
            .as_ref()
            .and_then(|value| value.format_name.as_deref())
            .and_then(|value| value.split(',').next())
            .map(str::to_owned),
        duration_seconds: format
            .as_ref()
            .and_then(|value| value.duration.as_deref())
            .and_then(|value| value.parse().ok()),
        bit_rate: format
            .as_ref()
            .and_then(|value| value.bit_rate.as_deref())
            .and_then(|value| value.parse().ok()),
        video_codec: video.and_then(|stream| stream.codec_name.clone()),
        audio_codec: audio.and_then(|stream| stream.codec_name.clone()),
        width: video.and_then(|stream| stream.width),
        height: video.and_then(|stream| stream.height),
        frame_rate: video
            .and_then(|stream| stream.r_frame_rate.as_deref())
            .and_then(parse_ratio),
        audio_sample_rate: audio
            .and_then(|stream| stream.sample_rate.as_deref())
            .and_then(|value| value.parse().ok()),
        audio_channels: audio.and_then(|stream| stream.channels),
        audio_tracks,
        subtitle_tracks,
        ..TechnicalMetadata::default()
    }
}

fn parse_ratio(value: &str) -> Option<f64> {
    let (numerator, denominator) = value.split_once('/')?;
    let numerator = numerator.parse::<f64>().ok()?;
    let denominator = denominator.parse::<f64>().ok()?;
    (denominator != 0.0).then_some(numerator / denominator)
}

async fn run_probe(
    executable: &str,
    arguments: &[&std::ffi::OsStr],
) -> Option<std::process::Output> {
    crate::engines::resolve_tool(executable)?;
    let mut command = crate::engines::tool_command(executable);
    command
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    tokio::time::timeout(PROBE_TIMEOUT, command.output())
        .await
        .ok()?
        .ok()
}

#[derive(Debug)]
struct ParsedManifestEntry {
    relative_path: String,
    path: PathBuf,
    expected_digest: String,
}

#[derive(Debug)]
struct PreparedManifestSource {
    path: PathBuf,
    relative_path: String,
    size: u64,
    fingerprint: SourceFingerprint,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SourceFingerprint {
    size: u64,
    modified: Option<std::time::SystemTime>,
    #[cfg(unix)]
    device: u64,
    #[cfg(unix)]
    inode: u64,
}

impl SourceFingerprint {
    fn from_metadata(metadata: &fs::Metadata) -> Self {
        #[cfg(unix)]
        use std::os::unix::fs::MetadataExt;

        Self {
            size: metadata.len(),
            modified: metadata.modified().ok(),
            #[cfg(unix)]
            device: metadata.dev(),
            #[cfg(unix)]
            inode: metadata.ino(),
        }
    }
}

fn manifest_error(message: impl Into<String>) -> ConversionError {
    ConversionError::ProcessFailed {
        message: message.into(),
        stderr: String::new(),
        exit_code: None,
    }
}

fn emit_inspection_progress(app: &AppHandle, job_id: &str, percent: u8) {
    let _ = app.emit(
        "inspection-progress",
        InspectionProgress {
            job_id: job_id.to_owned(),
            percent,
        },
    );
}

fn safe_manifest_relative_path(value: &str) -> Result<PathBuf, ConversionError> {
    if value.is_empty() || value.as_bytes().contains(&0) {
        return Err(manifest_error(
            "A checksum manifest contains an empty filename.",
        ));
    }
    let path = Path::new(value);
    if path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(manifest_error(
            "Checksum manifests may reference only safe relative paths without traversal.",
        ));
    }
    Ok(path.to_path_buf())
}

fn validate_manifest_target(
    base: &Path,
    relative_path: &Path,
) -> Result<Option<(PathBuf, u64)>, ConversionError> {
    let mut current = base.to_path_buf();
    let components = relative_path.components().collect::<Vec<_>>();
    for (index, component) in components.iter().enumerate() {
        let Component::Normal(name) = component else {
            return Err(manifest_error(
                "Checksum manifests may reference only safe relative paths without traversal.",
            ));
        };
        current.push(name);
        let metadata = match fs::symlink_metadata(&current) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(manifest_error(format!(
                    "Could not inspect {}: {error}",
                    current.display()
                )))
            }
        };
        if metadata.file_type().is_symlink() {
            return Err(manifest_error(format!(
                "Checksum manifests cannot follow links: {}",
                current.display()
            )));
        }
        let is_last = index + 1 == components.len();
        if is_last && !metadata.file_type().is_file() {
            return Err(manifest_error(format!(
                "Checksum manifests can verify regular files only: {}",
                current.display()
            )));
        }
        if !is_last && !metadata.file_type().is_dir() {
            return Err(manifest_error(format!(
                "A checksum manifest path is not a folder: {}",
                current.display()
            )));
        }
        if is_last {
            let canonical = current.canonicalize().map_err(|error| {
                manifest_error(format!("Could not resolve {}: {error}", current.display()))
            })?;
            if !canonical.starts_with(base) {
                return Err(manifest_error(
                    "A checksum manifest path resolves outside its containing folder.",
                ));
            }
            return Ok(Some((canonical, metadata.len())));
        }
    }
    Err(manifest_error(
        "A checksum manifest contains an empty filename.",
    ))
}

fn manifest_escape_path(value: &str) -> (bool, String) {
    let escaped = value
        .replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace('\r', "\\r");
    (escaped != value, escaped)
}

fn manifest_unescape_path(value: &str) -> Result<String, ConversionError> {
    let mut output = String::with_capacity(value.len());
    let mut characters = value.chars();
    while let Some(character) = characters.next() {
        if character != '\\' {
            output.push(character);
            continue;
        }
        match characters.next() {
            Some('\\') => output.push('\\'),
            Some('n') => output.push('\n'),
            Some('r') => output.push('\r'),
            _ => {
                return Err(manifest_error(
                    "A checksum manifest contains an invalid escaped filename.",
                ))
            }
        }
    }
    Ok(output)
}

impl ChecksumManifestAlgorithm {
    fn from_manifest_path(path: &Path) -> Result<Self, ConversionError> {
        match path
            .extension()
            .and_then(|value| value.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("md5") => Ok(Self::Md5),
            Some("sha1") => Ok(Self::Sha1),
            Some("sha256") => Ok(Self::Sha256),
            _ => Err(manifest_error(
                "Choose an MD5, SHA-1, or SHA-256 checksum manifest.",
            )),
        }
    }

    fn digest_length(self) -> usize {
        match self {
            Self::Md5 => 32,
            Self::Sha1 => 40,
            Self::Sha256 => 64,
        }
    }

    fn digest(self, hashes: &FileHashes) -> &str {
        match self {
            Self::Md5 => &hashes.md5,
            Self::Sha1 => &hashes.sha1,
            Self::Sha256 => &hashes.sha256,
        }
    }
}

fn parse_manifest_line(
    line: &str,
    algorithm: ChecksumManifestAlgorithm,
) -> Result<(String, String), ConversionError> {
    if line.len() > MAX_CHECKSUM_MANIFEST_LINE_BYTES {
        return Err(manifest_error("A checksum manifest line is too long."));
    }
    let (escaped, line) = line
        .strip_prefix('\\')
        .map_or((false, line), |line| (true, line));
    let digest_length = algorithm.digest_length();
    if line.len() < digest_length + 3 {
        return Err(manifest_error("A checksum manifest line is malformed."));
    }
    let digest = line
        .get(..digest_length)
        .filter(|value| value.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .ok_or_else(|| manifest_error("A checksum manifest contains an invalid digest."))?
        .to_ascii_lowercase();
    let separator = line
        .get(digest_length..digest_length + 2)
        .ok_or_else(|| manifest_error("A checksum manifest line is malformed."))?;
    if separator != "  " && separator != " *" {
        return Err(manifest_error(
            "Checksum manifest entries must use the standard digest and filename format.",
        ));
    }
    let filename = line
        .get(digest_length + 2..)
        .ok_or_else(|| manifest_error("A checksum manifest line is malformed."))?;
    let filename = if escaped {
        manifest_unescape_path(filename)?
    } else {
        filename.to_owned()
    };
    safe_manifest_relative_path(&filename)?;
    Ok((digest, filename))
}

fn read_checksum_manifest(
    path: &Path,
) -> Result<(ChecksumManifestAlgorithm, PathBuf, Vec<ParsedManifestEntry>), ConversionError> {
    let metadata = fs::symlink_metadata(path).map_err(|error| ConversionError::InputNotFound {
        path: format!("{}: {error}", path.display()),
    })?;
    if metadata.file_type().is_symlink() || !metadata.file_type().is_file() {
        return Err(manifest_error(
            "The checksum manifest must be a regular file, not a link or special item.",
        ));
    }
    if metadata.len() > MAX_CHECKSUM_MANIFEST_BYTES {
        return Err(manifest_error("Checksum manifests are limited to 1 MiB."));
    }
    let algorithm = ChecksumManifestAlgorithm::from_manifest_path(path)?;
    let canonical = path.canonicalize().map_err(|error| {
        manifest_error(format!("Could not resolve {}: {error}", path.display()))
    })?;
    let base = canonical
        .parent()
        .ok_or_else(|| manifest_error("The checksum manifest has no containing folder."))?
        .to_path_buf();
    let mut file = File::open(&canonical).map_err(|error| {
        manifest_error(format!("Could not open {}: {error}", canonical.display()))
    })?;
    let mut bytes = Vec::new();
    std::io::Read::by_ref(&mut file)
        .take(MAX_CHECKSUM_MANIFEST_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| manifest_error(format!("Could not read the manifest: {error}")))?;
    if bytes.len() as u64 > MAX_CHECKSUM_MANIFEST_BYTES {
        return Err(manifest_error("Checksum manifests are limited to 1 MiB."));
    }
    let text = std::str::from_utf8(&bytes)
        .map_err(|_| manifest_error("Checksum manifests must use UTF-8 text."))?;
    let mut entries = Vec::new();
    let mut relative_paths = HashSet::new();
    for raw_line in text.lines() {
        let line = raw_line.strip_suffix('\r').unwrap_or(raw_line);
        if line.is_empty() {
            continue;
        }
        if entries.len() == MAX_CHECKSUM_MANIFEST_ENTRIES {
            return Err(manifest_error(
                "Checksum manifests are limited to 100 entries.",
            ));
        }
        let (expected_digest, relative_path) = parse_manifest_line(line, algorithm)?;
        let path = safe_manifest_relative_path(&relative_path)?;
        if !relative_paths.insert(path.clone()) {
            return Err(manifest_error(
                "A checksum manifest contains a duplicate filename.",
            ));
        }
        entries.push(ParsedManifestEntry {
            relative_path,
            path,
            expected_digest,
        });
    }
    if entries.is_empty() {
        return Err(manifest_error(
            "The checksum manifest does not contain any entries.",
        ));
    }
    Ok((algorithm, base, entries))
}

fn prepare_manifest_sources(
    inputs: &[ChecksumManifestInput],
) -> Result<(PathBuf, Vec<PreparedManifestSource>), ConversionError> {
    if inputs.is_empty() || inputs.len() > MAX_CHECKSUM_MANIFEST_ENTRIES {
        return Err(manifest_error(
            "Add between 1 and 100 files before creating a checksum manifest.",
        ));
    }
    let mut shared_base: Option<PathBuf> = None;
    let mut seen = HashSet::new();
    let mut sources = Vec::with_capacity(inputs.len());
    for input in inputs {
        let input_path = Path::new(&input.input_path);
        let metadata =
            fs::symlink_metadata(input_path).map_err(|error| ConversionError::InputNotFound {
                path: format!("{}: {error}", input_path.display()),
            })?;
        if metadata.file_type().is_symlink() || !metadata.file_type().is_file() {
            return Err(manifest_error(
                "Checksum manifests can include regular files only.",
            ));
        }
        let canonical = input_path.canonicalize().map_err(|error| {
            manifest_error(format!(
                "Could not resolve {}: {error}",
                input_path.display()
            ))
        })?;
        if !seen.insert(canonical.clone()) {
            return Err(manifest_error(
                "The checksum manifest selection contains a duplicate file.",
            ));
        }

        let (base, relative_path) = match input.relative_path.as_deref() {
            Some(relative) => {
                let relative_path = safe_manifest_relative_path(relative)?;
                let mut base = canonical.clone();
                for _ in relative_path.components() {
                    if !base.pop() {
                        return Err(manifest_error("A folder-derived checksum path is invalid."));
                    }
                }
                let base = base.canonicalize().map_err(|error| {
                    manifest_error(format!("Could not resolve the source folder: {error}"))
                })?;
                let resolved = validate_manifest_target(&base, &relative_path)?
                    .ok_or_else(|| manifest_error("A selected checksum source is missing."))?;
                if resolved.0 != canonical {
                    return Err(manifest_error(
                        "A folder-derived checksum path does not match its source file.",
                    ));
                }
                (base, relative.to_owned())
            }
            None => {
                let base = canonical
                    .parent()
                    .ok_or_else(|| manifest_error("A checksum source has no containing folder."))?
                    .to_path_buf();
                let name = canonical
                    .file_name()
                    .and_then(|value| value.to_str())
                    .ok_or_else(|| {
                        manifest_error("A checksum source filename is not valid UTF-8.")
                    })?
                    .to_owned();
                (base, name)
            }
        };
        if shared_base.as_ref().is_some_and(|value| value != &base) {
            return Err(manifest_error(
                "Create one checksum manifest at a time from files in the same folder or folder selection.",
            ));
        }
        shared_base.get_or_insert(base);
        sources.push(PreparedManifestSource {
            path: canonical,
            relative_path,
            size: metadata.len(),
            fingerprint: SourceFingerprint::from_metadata(&metadata),
        });
    }
    Ok((shared_base.expect("non-empty inputs have a base"), sources))
}

fn candidate_manifest_path(base: &Path, index: usize) -> PathBuf {
    if index == 0 {
        base.join("checksums.sha256")
    } else {
        base.join(format!("checksums ({index}).sha256"))
    }
}

fn persist_manifest_keep_both(base: &Path, bytes: &[u8]) -> Result<PathBuf, ConversionError> {
    for index in 0..10_000 {
        let output = candidate_manifest_path(base, index);
        let mut temporary = tempfile::NamedTempFile::new_in(base).map_err(|error| {
            manifest_error(format!("Could not create the checksum manifest: {error}"))
        })?;
        temporary
            .write_all(bytes)
            .and_then(|()| temporary.flush())
            .and_then(|()| temporary.as_file().sync_all())
            .map_err(|error| {
                manifest_error(format!("Could not write the checksum manifest: {error}"))
            })?;
        match temporary.persist_noclobber(&output) {
            Ok(_) => return Ok(output),
            Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(manifest_error(format!(
                    "Could not save the checksum manifest: {}",
                    error.error
                )))
            }
        }
    }
    Err(manifest_error(
        "Could not find an available checksum manifest filename.",
    ))
}

fn create_checksum_manifest_inner(
    inputs: &[ChecksumManifestInput],
    cancel_token: &CancellationToken,
    mut report_progress: impl FnMut(u8),
) -> Result<ChecksumManifestCreationResult, ConversionError> {
    let (base, sources) = prepare_manifest_sources(inputs)?;
    let total_work = sources
        .iter()
        .map(|source| source.size.max(1))
        .sum::<u64>()
        .max(1);
    let mut completed = 0_u64;
    let mut manifest = String::new();
    report_progress(0);
    for source in &sources {
        if cancel_token.is_cancelled() {
            return Err(ConversionError::Cancelled);
        }
        let weight = source.size.max(1);
        let hashes = hash_file(&source.path, source.size, cancel_token, |file_percent| {
            let progress = completed
                .saturating_add(weight.saturating_mul(u64::from(file_percent)) / 100)
                .saturating_mul(95)
                / total_work;
            report_progress(progress.min(95) as u8);
        })?;
        let current_metadata = fs::symlink_metadata(&source.path).map_err(|error| {
            manifest_error(format!(
                "Could not revalidate {} after hashing: {error}",
                source.path.display()
            ))
        })?;
        if current_metadata.file_type().is_symlink()
            || !current_metadata.file_type().is_file()
            || SourceFingerprint::from_metadata(&current_metadata) != source.fingerprint
        {
            return Err(manifest_error(format!(
                "{} changed while its checksum was being computed.",
                source.path.display()
            )));
        }
        let (escaped, path) = manifest_escape_path(&source.relative_path);
        if escaped {
            manifest.push('\\');
        }
        manifest.push_str(&hashes.sha256);
        manifest.push_str("  ");
        manifest.push_str(&path);
        manifest.push('\n');
        completed = completed.saturating_add(weight);
    }
    if cancel_token.is_cancelled() {
        return Err(ConversionError::Cancelled);
    }
    report_progress(98);
    let output = persist_manifest_keep_both(&base, manifest.as_bytes())?;
    report_progress(100);
    Ok(ChecksumManifestCreationResult {
        output_path: output.to_string_lossy().into_owned(),
        entry_count: sources.len(),
    })
}

fn verify_checksum_manifest_inner(
    manifest_path: &Path,
    cancel_token: &CancellationToken,
    mut report_progress: impl FnMut(u8),
) -> Result<ChecksumManifestVerificationResult, ConversionError> {
    let (algorithm, base, parsed) = read_checksum_manifest(manifest_path)?;
    let mut prepared = Vec::with_capacity(parsed.len());
    let mut canonical_paths = HashSet::new();
    let mut total_work = 0_u64;
    for entry in parsed {
        let target = validate_manifest_target(&base, &entry.path)?;
        if let Some((canonical, size)) = &target {
            if !canonical_paths.insert(canonical.clone()) {
                return Err(manifest_error(
                    "A checksum manifest resolves more than one entry to the same file.",
                ));
            }
            total_work = total_work.saturating_add((*size).max(1));
        } else {
            total_work = total_work.saturating_add(1);
        }
        prepared.push((entry, target));
    }
    total_work = total_work.max(1);
    let mut completed = 0_u64;
    let mut results = Vec::with_capacity(prepared.len());
    report_progress(0);
    for (entry, target) in prepared {
        if cancel_token.is_cancelled() {
            return Err(ConversionError::Cancelled);
        }
        let Some((path, size)) = target else {
            completed = completed.saturating_add(1);
            report_progress((completed.saturating_mul(100) / total_work).min(100) as u8);
            results.push(ChecksumManifestVerificationEntry {
                relative_path: entry.relative_path,
                expected_digest: entry.expected_digest,
                actual_digest: None,
                status: ChecksumManifestStatus::Missing,
            });
            continue;
        };
        let weight = size.max(1);
        let revalidated = validate_manifest_target(&base, &entry.path)?;
        let Some((revalidated_path, revalidated_size)) = revalidated else {
            completed = completed.saturating_add(weight);
            results.push(ChecksumManifestVerificationEntry {
                relative_path: entry.relative_path,
                expected_digest: entry.expected_digest,
                actual_digest: None,
                status: ChecksumManifestStatus::Missing,
            });
            continue;
        };
        if revalidated_path != path || revalidated_size != size {
            return Err(manifest_error(
                "A checksum source changed while the manifest was being verified.",
            ));
        }
        let hashes = hash_file(&path, size, cancel_token, |file_percent| {
            let progress = completed
                .saturating_add(weight.saturating_mul(u64::from(file_percent)) / 100)
                .saturating_mul(100)
                / total_work;
            report_progress(progress.min(100) as u8);
        })?;
        let actual_digest = algorithm.digest(&hashes).to_owned();
        let status = if actual_digest == entry.expected_digest {
            ChecksumManifestStatus::Match
        } else {
            ChecksumManifestStatus::Mismatch
        };
        results.push(ChecksumManifestVerificationEntry {
            relative_path: entry.relative_path,
            expected_digest: entry.expected_digest,
            actual_digest: Some(actual_digest),
            status,
        });
        completed = completed.saturating_add(weight);
    }
    report_progress(100);
    Ok(ChecksumManifestVerificationResult {
        manifest_path: manifest_path.to_string_lossy().into_owned(),
        algorithm,
        entries: results,
    })
}

/// Create a standard SHA-256 manifest next to one same-root Inspect queue.
#[tauri::command]
pub async fn create_checksum_manifest(
    app: AppHandle,
    inputs: Vec<ChecksumManifestInput>,
    job_id: Option<String>,
) -> Result<ChecksumManifestCreationResult, ConversionError> {
    let active_job = ActiveJobGuard::register(app.clone(), job_id)?;
    let cancel_token = active_job.cancel_token();
    let event_job_id = active_job.job_id().to_owned();
    tokio::task::spawn_blocking(move || {
        create_checksum_manifest_inner(&inputs, &cancel_token, |percent| {
            emit_inspection_progress(&app, &event_job_id, percent);
        })
    })
    .await
    .map_err(|error| manifest_error(format!("Checksum manifest worker stopped: {error}")))?
}

/// Verify every safe relative entry in a checksum manifest sequentially.
#[tauri::command]
pub async fn verify_checksum_manifest(
    app: AppHandle,
    manifest_path: String,
    job_id: Option<String>,
) -> Result<ChecksumManifestVerificationResult, ConversionError> {
    let active_job = ActiveJobGuard::register(app.clone(), job_id)?;
    let cancel_token = active_job.cancel_token();
    let event_job_id = active_job.job_id().to_owned();
    tokio::task::spawn_blocking(move || {
        verify_checksum_manifest_inner(Path::new(&manifest_path), &cancel_token, |percent| {
            emit_inspection_progress(&app, &event_job_id, percent);
        })
    })
    .await
    .map_err(|error| manifest_error(format!("Checksum verification worker stopped: {error}")))?
}

/// Compute common checksums in one streaming pass without loading the file into memory.
#[tauri::command]
pub async fn compute_file_hashes(
    app: AppHandle,
    path: String,
    job_id: Option<String>,
) -> Result<FileHashes, ConversionError> {
    let path = PathBuf::from(&path);
    let metadata = path
        .metadata()
        .map_err(|error| ConversionError::InputNotFound {
            path: format!("{path}: {error}", path = path.display()),
        })?;
    if !metadata.is_file() {
        return Err(ConversionError::UnsupportedConversion {
            input: "Path is not a regular file".into(),
            output: "checksums".into(),
        });
    }

    let active_job = ActiveJobGuard::register(app.clone(), job_id)?;
    let cancel_token = active_job.cancel_token();
    let event_job_id = active_job.job_id().to_string();
    let total_size = metadata.len();
    let path_for_worker = path.clone();
    let app_for_worker = app.clone();

    tokio::task::spawn_blocking(move || {
        hash_file(&path_for_worker, total_size, &cancel_token, |percent| {
            let _ = app_for_worker.emit(
                "inspection-progress",
                InspectionProgress {
                    job_id: event_job_id.clone(),
                    percent,
                },
            );
        })
    })
    .await
    .map_err(|error| ConversionError::ProcessFailed {
        message: format!("Checksum worker stopped unexpectedly: {error}"),
        stderr: String::new(),
        exit_code: None,
    })?
}

fn hash_file(
    path: &Path,
    total_size: u64,
    cancel_token: &CancellationToken,
    mut report_progress: impl FnMut(u8),
) -> Result<FileHashes, ConversionError> {
    let file = File::open(path).map_err(|error| ConversionError::InputNotFound {
        path: format!("{path}: {error}", path = path.display()),
    })?;
    let mut reader = BufReader::with_capacity(HASH_BUFFER_SIZE, file);
    let mut buffer = vec![0_u8; HASH_BUFFER_SIZE];
    let mut md5 = Md5::new();
    let mut sha1 = Sha1::new();
    let mut sha256 = Sha256::new();
    let mut bytes_read = 0_u64;
    let mut last_percent = u8::MAX;

    report_progress(0);
    loop {
        if cancel_token.is_cancelled() {
            return Err(ConversionError::Cancelled);
        }
        let count = reader
            .read(&mut buffer)
            .map_err(|error| ConversionError::ProcessFailed {
                message: format!("Could not read {}: {error}", path.display()),
                stderr: String::new(),
                exit_code: None,
            })?;
        if count == 0 {
            break;
        }

        let chunk = &buffer[..count];
        md5.update(chunk);
        sha1.update(chunk);
        sha256.update(chunk);
        bytes_read = bytes_read.saturating_add(count as u64);
        let percent = bytes_read
            .saturating_mul(100)
            .checked_div(total_size)
            .unwrap_or(100)
            .min(100) as u8;
        if percent != last_percent {
            report_progress(percent);
            last_percent = percent;
        }
    }

    if cancel_token.is_cancelled() {
        return Err(ConversionError::Cancelled);
    }
    report_progress(100);

    Ok(FileHashes {
        md5: format!("{:x}", md5.finalize()),
        sha1: format!("{:x}", sha1.finalize()),
        sha256: format!("{:x}", sha256.finalize()),
    })
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use tempfile::{tempdir, NamedTempFile};

    use super::*;

    #[test]
    fn hashes_known_content_in_one_pass() {
        let mut file = NamedTempFile::new().expect("temp file");
        file.write_all(b"abc").expect("write content");
        let result =
            hash_file(file.path(), 3, &CancellationToken::new(), |_| {}).expect("hash content");

        assert_eq!(result.md5, "900150983cd24fb0d6963f7d28e17f72");
        assert_eq!(result.sha1, "a9993e364706816aba3e25717850c26c9cd0d89d");
        assert_eq!(
            result.sha256,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn cancelled_hash_stops_before_reading() {
        let mut file = NamedTempFile::new().expect("temp file");
        file.write_all(b"content").expect("write content");
        let token = CancellationToken::new();
        token.cancel();

        assert!(matches!(
            hash_file(file.path(), 7, &token, |_| {}),
            Err(ConversionError::Cancelled)
        ));
    }

    fn manifest_input(path: &Path) -> ChecksumManifestInput {
        ChecksumManifestInput {
            input_path: path.to_string_lossy().into_owned(),
            relative_path: None,
        }
    }

    #[test]
    fn creates_standard_sha256_manifest_with_keep_both_and_preserves_sources() {
        let directory = tempdir().expect("temp directory");
        let first = directory.path().join("first file.txt");
        let second = directory.path().join("café.txt");
        fs::write(&first, b"first bytes").expect("write first");
        fs::write(&second, b"second bytes").expect("write second");
        let inputs = vec![manifest_input(&first), manifest_input(&second)];

        let created = create_checksum_manifest_inner(&inputs, &CancellationToken::new(), |_| {})
            .expect("create manifest");
        assert_eq!(
            Path::new(&created.output_path),
            directory
                .path()
                .canonicalize()
                .expect("canonical directory")
                .join("checksums.sha256")
        );
        assert_eq!(created.entry_count, 2);
        let text = fs::read_to_string(&created.output_path).expect("read manifest");
        assert!(text.contains("  first file.txt\n"));
        assert!(text.contains("  café.txt\n"));
        assert_eq!(fs::read(&first).expect("first preserved"), b"first bytes");
        assert_eq!(
            fs::read(&second).expect("second preserved"),
            b"second bytes"
        );

        let keep_both = create_checksum_manifest_inner(&inputs, &CancellationToken::new(), |_| {})
            .expect("create keep-both manifest");
        assert_eq!(
            Path::new(&keep_both.output_path),
            directory
                .path()
                .canonicalize()
                .expect("canonical directory")
                .join("checksums (1).sha256")
        );
    }

    #[test]
    fn verifies_manifest_matches_mismatches_and_missing_entries() {
        let directory = tempdir().expect("temp directory");
        let matching = directory.path().join("matching.txt");
        let changed = directory.path().join("changed.txt");
        let missing = directory.path().join("missing.txt");
        fs::write(&matching, b"matching").expect("write matching");
        fs::write(&changed, b"before").expect("write changed");
        fs::write(&missing, b"removed").expect("write missing");
        let inputs = vec![
            manifest_input(&matching),
            manifest_input(&changed),
            manifest_input(&missing),
        ];
        let created = create_checksum_manifest_inner(&inputs, &CancellationToken::new(), |_| {})
            .expect("create manifest");
        fs::write(&changed, b"after").expect("change file");
        fs::remove_file(&missing).expect("remove file");

        let result = verify_checksum_manifest_inner(
            Path::new(&created.output_path),
            &CancellationToken::new(),
            |_| {},
        )
        .expect("verify manifest");
        assert_eq!(result.algorithm, ChecksumManifestAlgorithm::Sha256);
        assert_eq!(result.entries.len(), 3);
        assert_eq!(result.entries[0].status, ChecksumManifestStatus::Match);
        assert_eq!(result.entries[1].status, ChecksumManifestStatus::Mismatch);
        assert_eq!(result.entries[2].status, ChecksumManifestStatus::Missing);
        assert!(result.entries[0].actual_digest.is_some());
        assert!(result.entries[2].actual_digest.is_none());
    }

    #[test]
    fn manifest_round_trips_escaped_backslash_and_newline_filenames() {
        let directory = tempdir().expect("temp directory");
        let source = directory.path().join("line\nbreak\\name.txt");
        fs::write(&source, b"escaped name bytes").expect("write source");
        let created = create_checksum_manifest_inner(
            &[manifest_input(&source)],
            &CancellationToken::new(),
            |_| {},
        )
        .expect("create manifest");
        let text = fs::read_to_string(&created.output_path).expect("read manifest");
        assert!(text.starts_with('\\'));
        assert!(text.contains("line\\nbreak\\\\name.txt"));

        let verified = verify_checksum_manifest_inner(
            Path::new(&created.output_path),
            &CancellationToken::new(),
            |_| {},
        )
        .expect("verify manifest");
        assert_eq!(verified.entries.len(), 1);
        assert_eq!(verified.entries[0].relative_path, "line\nbreak\\name.txt");
        assert_eq!(verified.entries[0].status, ChecksumManifestStatus::Match);
    }

    #[test]
    fn rejects_unsafe_duplicate_malformed_and_linked_manifest_entries() {
        let directory = tempdir().expect("temp directory");
        let outside = directory.path().join("outside.txt");
        fs::write(&outside, b"outside").expect("write outside");
        let digest = format!("{:x}", Sha256::digest(b"outside"));

        for (name, contents) in [
            ("traversal.sha256", format!("{digest}  ../outside.txt\n")),
            (
                "absolute.sha256",
                format!("{digest}  {}\n", outside.display()),
            ),
            (
                "duplicate.sha256",
                format!("{digest}  outside.txt\n{digest}  outside.txt\n"),
            ),
            ("malformed.sha256", format!("{digest} outside.txt\n")),
        ] {
            let manifest = directory.path().join(name);
            fs::write(&manifest, contents).expect("write invalid manifest");
            assert!(
                verify_checksum_manifest_inner(&manifest, &CancellationToken::new(), |_| {},)
                    .is_err()
            );
        }

        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            let link = directory.path().join("linked.txt");
            symlink(&outside, &link).expect("create link");
            let manifest = directory.path().join("linked.sha256");
            fs::write(&manifest, format!("{digest}  linked.txt\n")).expect("write linked manifest");
            let error =
                verify_checksum_manifest_inner(&manifest, &CancellationToken::new(), |_| {})
                    .expect_err("links must be rejected");
            assert!(error.to_string().contains("cannot follow links"));
        }
    }

    #[test]
    fn rejects_oversized_and_excessive_manifests_before_hashing() {
        let directory = tempdir().expect("temp directory");
        let oversized = directory.path().join("oversized.sha256");
        fs::write(
            &oversized,
            vec![b'a'; MAX_CHECKSUM_MANIFEST_BYTES as usize + 1],
        )
        .expect("write oversized manifest");
        assert!(
            verify_checksum_manifest_inner(&oversized, &CancellationToken::new(), |_| {},).is_err()
        );

        let excessive = directory.path().join("excessive.sha256");
        let digest = "0".repeat(64);
        let contents = (0..=MAX_CHECKSUM_MANIFEST_ENTRIES)
            .map(|index| format!("{digest}  missing-{index}.txt\n"))
            .collect::<String>();
        fs::write(&excessive, contents).expect("write excessive manifest");
        assert!(
            verify_checksum_manifest_inner(&excessive, &CancellationToken::new(), |_| {},).is_err()
        );
    }

    #[test]
    fn manifest_work_honors_cancellation_without_committing_output() {
        let directory = tempdir().expect("temp directory");
        let source = directory.path().join("large.bin");
        fs::write(&source, vec![7_u8; HASH_BUFFER_SIZE + 1]).expect("write source");
        let token = CancellationToken::new();
        token.cancel();
        let result = create_checksum_manifest_inner(&[manifest_input(&source)], &token, |_| {});

        assert!(matches!(result, Err(ConversionError::Cancelled)));
        assert!(!directory.path().join("checksums.sha256").exists());
        assert_eq!(
            fs::read(&source).expect("source preserved").len(),
            HASH_BUFFER_SIZE + 1
        );
    }

    #[test]
    fn parses_pdf_page_metadata() {
        let result =
            parse_pdfinfo(b"Title: Sample\nPages: 67\nPage size: 612 x 792 pts (letter)\n");
        assert_eq!(result.page_count, Some(67));
        assert_eq!(result.page_size.as_deref(), Some("612 x 792 pts (letter)"));
    }

    #[test]
    fn parses_media_stream_metadata() {
        let probe = serde_json::from_str::<FfprobeOutput>(
            r#"{
              "streams": [
                {"codec_type":"video","codec_name":"h264","width":1920,"height":1080,"r_frame_rate":"30000/1001"},
                {"index":1,"codec_type":"audio","codec_name":"aac","sample_rate":"48000","channels":2,"channel_layout":"stereo","tags":{"language":"eng","title":"Main mix"},"disposition":{"default":1}},
                {"index":2,"codec_type":"subtitle","codec_name":"subrip","tags":{"language":"eng","title":"English CC"},"disposition":{"default":1,"forced":0}}
              ],
              "format":{"format_name":"mov,mp4,m4a","duration":"12.5","bit_rate":"2400000"}
            }"#,
        )
        .expect("valid ffprobe fixture");
        let result = metadata_from_ffprobe(probe);

        assert_eq!(result.container.as_deref(), Some("mov"));
        assert_eq!(result.video_codec.as_deref(), Some("h264"));
        assert_eq!(result.audio_codec.as_deref(), Some("aac"));
        assert_eq!(result.width, Some(1920));
        assert_eq!(result.height, Some(1080));
        assert_eq!(result.duration_seconds, Some(12.5));
        assert_eq!(result.bit_rate, Some(2_400_000));
        assert_eq!(result.audio_sample_rate, Some(48_000));
        assert_eq!(result.audio_channels, Some(2));
        assert_eq!(result.audio_tracks.len(), 1);
        assert_eq!(result.audio_tracks[0].stream_index, 1);
        assert_eq!(result.audio_tracks[0].language.as_deref(), Some("eng"));
        assert_eq!(result.audio_tracks[0].title.as_deref(), Some("Main mix"));
        assert!(result.audio_tracks[0].is_default);
        assert_eq!(result.subtitle_tracks.len(), 1);
        assert_eq!(result.subtitle_tracks[0].language.as_deref(), Some("eng"));
        assert!(result.subtitle_tracks[0].supported);
        assert!(result
            .frame_rate
            .is_some_and(|value| (value - 29.97).abs() < 0.01));
    }

    #[test]
    fn rejects_invalid_or_zero_frame_rate_ratios() {
        assert_eq!(parse_ratio("25/1"), Some(25.0));
        assert_eq!(parse_ratio("25/0"), None);
        assert_eq!(parse_ratio("unknown"), None);
    }

    fn report_entry(path: impl Into<String>) -> InspectionReportEntry {
        InspectionReportEntry {
            file: FileInfoResponse {
                path: path.into(),
                name: "sample, \"final\".mp4".into(),
                extension: "mp4".into(),
                size: 1_024,
                format: "mp4".into(),
                category: "video".into(),
                width: None,
                height: None,
                relative_path: None,
                mime_type: Some("video/mp4".into()),
                created_at: Some(1_700_000_000_000),
                modified_at: Some(1_700_000_001_000),
                read_only: false,
            },
            technical_metadata: Some(TechnicalMetadata {
                container: Some("mov".into()),
                duration_seconds: Some(12.5),
                video_codec: Some("h264".into()),
                width: Some(1_920),
                height: Some(1_080),
                ..TechnicalMetadata::default()
            }),
            checksums: Some(FileHashes {
                md5: "md5-value".into(),
                sha1: "sha1-value".into(),
                sha256: "sha256-value".into(),
            }),
        }
    }

    #[test]
    fn serializes_versioned_json_inspection_report() {
        let bytes = inspection_report_bytes(
            InspectionReportFormat::Json,
            &[report_entry("/tmp/sample.mp4")],
        )
        .expect("serialize report");
        let report: serde_json::Value = serde_json::from_slice(&bytes).expect("valid json");

        assert_eq!(report["schemaVersion"], 1);
        assert_eq!(report["generatedBy"], "ConvertKit");
        assert_eq!(report["files"][0]["file"]["name"], "sample, \"final\".mp4");
        assert_eq!(
            report["files"][0]["technicalMetadata"]["videoCodec"],
            "h264"
        );
        assert_eq!(report["files"][0]["checksums"]["sha256"], "sha256-value");
    }

    #[test]
    fn csv_report_quotes_commas_and_double_quotes() {
        let report = inspection_report_csv(&[report_entry("/tmp/sample.mp4")]);

        assert!(report.starts_with("name,path,extension,format,category,mime_type"));
        assert!(report.contains("\"sample, \"\"final\"\".mp4\""));
        assert!(report.contains(",mov,12.5,,h264,"));
        assert!(report.ends_with(",md5-value,sha1-value,sha256-value\r\n"));
    }

    #[test]
    fn writes_report_atomically_and_refuses_source_overwrite() {
        let directory = tempdir().expect("temp directory");
        let source = directory.path().join("source.bin");
        std::fs::write(&source, b"source").expect("write source");
        let destination = directory.path().join("report.json");
        let entry = report_entry(source.to_string_lossy());

        let output = export_inspection_report(
            destination.to_string_lossy().into_owned(),
            "json".into(),
            vec![entry.clone()],
        )
        .expect("write report");
        assert_eq!(Path::new(&output), destination);
        assert!(std::fs::read_to_string(&destination)
            .expect("read report")
            .contains("\"schemaVersion\": 1"));

        let error = export_inspection_report(
            source.to_string_lossy().into_owned(),
            "csv".into(),
            vec![entry],
        )
        .expect_err("must preserve inspected source");
        assert!(error.contains("cannot replace"));
        assert_eq!(std::fs::read(&source).expect("source remains"), b"source");
    }
}
