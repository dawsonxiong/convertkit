use log::warn;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tauri::Manager;
use uuid::Uuid;

use crate::engines::{
    process::{run_process, ProcessMessages},
    resolve_tool, tool_command,
};
use crate::formats::Format;

const QUICKLOOK_PREVIEW_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_CLIPBOARD_IMAGE_BYTES: usize = 64 * 1024 * 1024;
const MAX_CLIPBOARD_IMAGE_BASE64_BYTES: usize = MAX_CLIPBOARD_IMAGE_BYTES.div_ceil(3) * 4;
const QUICKLOOK_PROCESS_MESSAGES: ProcessMessages = ProcessMessages {
    start: "Failed to start Quick Look thumbnail generation",
    wait: "Failed while waiting for Quick Look thumbnail generation",
    failure: "Quick Look thumbnail generation failed",
};

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct DependencyStatus {
    pub name: String,
    pub installed: bool,
    pub version: Option<String>,
    /// Whether this dependency is needed for core functionality.
    pub required: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileInfoResponse {
    pub path: String,
    pub name: String,
    pub extension: String,
    pub size: u64,
    pub format: String,
    pub category: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub relative_path: Option<String>,
    pub mime_type: Option<String>,
    pub created_at: Option<u64>,
    pub modified_at: Option<u64>,
    pub read_only: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InputCollectionResult {
    pub files: Vec<FileInfoResponse>,
    pub skipped_count: usize,
    pub truncated: bool,
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

/// Check which external CLI tools are installed.
#[tauri::command]
pub async fn check_dependencies() -> Vec<DependencyStatus> {
    let tools: Vec<(&str, bool)> = vec![
        ("ffmpeg", true),
        ("magick", true),
        ("pandoc", false),
        ("resvg", false),
        ("vtracer", false),
        ("soffice", false),
        ("tectonic", false),
        ("pdfinfo", false),
        ("pdftotext", false),
        ("pdfseparate", false),
        ("pdfunite", false),
        ("gs", false),
        ("whisper-cli", false),
    ];

    let mut results = Vec::with_capacity(tools.len());

    for (name, required) in tools {
        let (installed, version) = probe_tool(name).await;
        results.push(DependencyStatus {
            name: name.to_string(),
            installed,
            version,
            required,
        });
    }

    results
}

/// Return metadata about a file on disk.
#[tauri::command]
pub async fn get_file_info(path: String) -> Result<FileInfoResponse, String> {
    file_info_for_path(Path::new(&path), None).await
}

/// Expand files and folders into a validated queue payload.
///
/// Folder traversal is recursive and deterministic. Hidden entries, symlinks,
/// duplicates, and formats unsupported by the active operation are ignored.
#[tauri::command]
pub async fn collect_input_paths(
    paths: Vec<String>,
    operation: String,
    limit: usize,
    excluded_paths: Vec<String>,
) -> Result<InputCollectionResult, String> {
    let operation = InputOperation::parse(&operation)?;
    let limit = limit.min(100);
    let mut seen = excluded_paths
        .iter()
        .map(|path| normalized_path(Path::new(path)))
        .collect::<HashSet<_>>();
    let mut candidates = Vec::with_capacity(limit);
    let mut skipped_count = 0;
    let mut truncated = false;

    for raw_path in paths {
        let path = PathBuf::from(raw_path);
        if path.is_dir() {
            collect_folder_candidates(
                &path,
                operation,
                limit,
                &mut seen,
                &mut candidates,
                &mut skipped_count,
                &mut truncated,
            );
        } else if path.is_file() {
            add_candidate(
                path,
                None,
                operation,
                limit,
                &mut seen,
                &mut candidates,
                &mut skipped_count,
                &mut truncated,
            );
        } else {
            skipped_count += 1;
        }

        if truncated {
            break;
        }
    }

    let mut files = Vec::with_capacity(candidates.len());
    for (path, relative_path) in candidates {
        match file_info_for_path(&path, relative_path).await {
            Ok(info) => files.push(info),
            Err(_) => skipped_count += 1,
        }
    }

    Ok(InputCollectionResult {
        files,
        skipped_count,
        truncated,
    })
}

async fn file_info_for_path(
    p: &Path,
    relative_path: Option<String>,
) -> Result<FileInfoResponse, String> {
    let meta = std::fs::metadata(p).map_err(|e| format!("Cannot read file: {e}"))?;
    if !meta.is_file() {
        return Err("The selected path is not a file".into());
    }

    let name = p
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .to_string();

    let extension = p
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_string();

    let format = Format::from_extension(&extension);

    // Magic-byte check: if infer detects a type that conflicts with the
    // extension, log a warning but proceed anyway.
    let inferred = infer::get_from_path(p).ok().flatten();
    if let Some(inferred) = inferred.as_ref() {
        let inferred_ext = inferred.extension();
        if !extension.is_empty() && inferred_ext != extension.to_lowercase() {
            // Check if they map to different formats (aliases like jpg/jpeg are fine).
            let ext_format = Format::from_extension(&extension);
            let inferred_format = Format::from_extension(inferred_ext);
            if ext_format != inferred_format {
                warn!(
                    "File extension '.{}' does not match detected type '{}' ({})",
                    extension,
                    inferred_ext,
                    inferred.mime_type()
                );
            }
        }
    }

    let (width, height) = if matches!(
        format.map(|f| f.category()),
        Some(crate::formats::FileCategory::Image)
    ) {
        probe_image_dimensions(p).await.unwrap_or((None, None))
    } else {
        (None, None)
    };

    Ok(FileInfoResponse {
        path: p.to_string_lossy().into_owned(),
        name,
        extension: extension.clone(),
        size: meta.len(),
        format: format
            .map(|f| f.extension().to_string())
            .unwrap_or_else(|| extension.to_lowercase()),
        category: format
            .map(|f| f.category().to_string())
            .unwrap_or_else(|| "other".to_string()),
        width,
        height,
        relative_path,
        mime_type: inferred.map(|kind| kind.mime_type().to_string()),
        created_at: meta.created().ok().and_then(system_time_millis),
        modified_at: meta.modified().ok().and_then(system_time_millis),
        read_only: meta.permissions().readonly(),
    })
}

fn system_time_millis(value: std::time::SystemTime) -> Option<u64> {
    value
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|duration| u64::try_from(duration.as_millis()).ok())
}

#[derive(Debug, Clone, Copy)]
enum InputOperation {
    Convert,
    Resize,
    Optimize,
    ExportImages,
    EncodeVideo,
    RemoveAudio,
    ExtractSubtitles,
    GenerateThumbnails,
    RemoveMetadata,
    ExtractAudio,
    Transcribe,
    ExtractText,
    RecognizeText,
    MergePdf,
    SplitPdf,
    ExportPdfPages,
    CompressPdf,
    Rename,
    CreateArchive,
    ExtractArchive,
    Inspect,
}

impl InputOperation {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "convert" => Ok(Self::Convert),
            "resize" => Ok(Self::Resize),
            "optimize" => Ok(Self::Optimize),
            "exportImages" => Ok(Self::ExportImages),
            "encodeVideo" => Ok(Self::EncodeVideo),
            "removeAudio" => Ok(Self::RemoveAudio),
            "extractSubtitles" => Ok(Self::ExtractSubtitles),
            "generateThumbnails" => Ok(Self::GenerateThumbnails),
            "removeMetadata" => Ok(Self::RemoveMetadata),
            "extractAudio" => Ok(Self::ExtractAudio),
            "transcribe" => Ok(Self::Transcribe),
            "extractText" => Ok(Self::ExtractText),
            "recognizeText" => Ok(Self::RecognizeText),
            "mergePdf" => Ok(Self::MergePdf),
            "splitPdf" => Ok(Self::SplitPdf),
            "exportPdfPages" => Ok(Self::ExportPdfPages),
            "compressPdf" => Ok(Self::CompressPdf),
            "rename" => Ok(Self::Rename),
            "createArchive" => Ok(Self::CreateArchive),
            "extractArchive" => Ok(Self::ExtractArchive),
            "inspect" => Ok(Self::Inspect),
            _ => Err(format!("Unknown operation: {value}")),
        }
    }

    fn supports(self, format: Format) -> bool {
        match self {
            Self::Convert => !format.compatible_targets().is_empty(),
            Self::Resize | Self::Optimize | Self::ExportImages => {
                format.category() == crate::formats::FileCategory::Image
            }
            Self::EncodeVideo
            | Self::RemoveAudio
            | Self::ExtractSubtitles
            | Self::GenerateThumbnails => format.category() == crate::formats::FileCategory::Video,
            Self::Transcribe => matches!(
                format.category(),
                crate::formats::FileCategory::Audio | crate::formats::FileCategory::Video
            ),
            Self::RemoveMetadata => {
                matches!(
                    format,
                    Format::Jpg | Format::Png | Format::WebP | Format::Pdf
                ) || matches!(
                    format.category(),
                    crate::formats::FileCategory::Audio | crate::formats::FileCategory::Video
                )
            }
            Self::ExtractAudio => format.category() == crate::formats::FileCategory::Video,
            Self::ExtractText => super::text::supports_text_extraction(format),
            Self::RecognizeText => {
                format == Format::Pdf || format.category() == crate::formats::FileCategory::Image
            }
            Self::MergePdf => matches!(format, Format::Pdf | Format::Png | Format::Jpg),
            Self::SplitPdf | Self::ExportPdfPages | Self::CompressPdf => format == Format::Pdf,
            Self::CreateArchive | Self::Rename => true,
            Self::ExtractArchive => false,
            Self::Inspect => true,
        }
    }

    fn supports_path(self, path: &Path) -> bool {
        match self {
            Self::CreateArchive | Self::Rename | Self::Inspect => true,
            Self::ExtractArchive => super::archive::is_supported_archive_path(path),
            Self::RecognizeText => super::ocr::supports_ocr_input(path),
            _ => path
                .extension()
                .and_then(|extension| extension.to_str())
                .and_then(Format::from_extension)
                .is_some_and(|format| self.supports(format)),
        }
    }
}

type Candidate = (PathBuf, Option<String>);

#[allow(clippy::too_many_arguments)]
fn collect_folder_candidates(
    root: &Path,
    operation: InputOperation,
    limit: usize,
    seen: &mut HashSet<PathBuf>,
    candidates: &mut Vec<Candidate>,
    skipped_count: &mut usize,
    truncated: &mut bool,
) {
    let mut directories = vec![root.to_path_buf()];

    while let Some(directory) = directories.pop() {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            *skipped_count += 1;
            continue;
        };
        let mut entries = entries.filter_map(Result::ok).collect::<Vec<_>>();
        entries.sort_by_key(|entry| entry.file_name());

        for entry in entries {
            let name = entry.file_name();
            if name.to_string_lossy().starts_with('.') {
                *skipped_count += 1;
                continue;
            }

            let Ok(file_type) = entry.file_type() else {
                *skipped_count += 1;
                continue;
            };
            if file_type.is_symlink() {
                *skipped_count += 1;
                continue;
            }

            let path = entry.path();
            if file_type.is_dir() {
                directories.push(path);
                continue;
            }
            if !file_type.is_file() {
                *skipped_count += 1;
                continue;
            }

            let relative_path = path
                .strip_prefix(root)
                .ok()
                .map(|value| value.to_string_lossy().into_owned());
            add_candidate(
                path,
                relative_path,
                operation,
                limit,
                seen,
                candidates,
                skipped_count,
                truncated,
            );
            if *truncated {
                return;
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn add_candidate(
    path: PathBuf,
    relative_path: Option<String>,
    operation: InputOperation,
    limit: usize,
    seen: &mut HashSet<PathBuf>,
    candidates: &mut Vec<Candidate>,
    skipped_count: &mut usize,
    truncated: &mut bool,
) {
    if !operation.supports_path(&path) {
        *skipped_count += 1;
        return;
    }

    let normalized = normalized_path(&path);
    if !seen.insert(normalized) {
        *skipped_count += 1;
        return;
    }
    if candidates.len() >= limit {
        *truncated = true;
        return;
    }

    candidates.push((path, relative_path));
}

fn normalized_path(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

/// Mark the frontend ready for Finder events and return all launch-time inputs in order.
#[tauri::command]
pub fn get_opened_inputs(app: tauri::AppHandle) -> crate::OpenedInputs {
    let state = app.state::<crate::OpenedFiles>();
    let mut opened = state.0.lock().expect("OpenedFiles lock poisoned");
    let (paths, quick_actions) = opened.mark_frontend_ready();
    crate::OpenedInputs {
        paths: paths
            .into_iter()
            .map(|path| path.to_string_lossy().into_owned())
            .collect(),
        quick_actions,
    }
}

/// Save base64-encoded clipboard image data to a temp file and return its path.
#[tauri::command]
pub async fn save_clipboard_image(data: String, mime: String) -> Result<String, String> {
    let ext = match mime.as_str() {
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/webp" => "webp",
        "image/gif" => "gif",
        "image/bmp" => "bmp",
        "image/svg+xml" => "svg",
        _ => return Err(format!("Unsupported clipboard image type: {}", mime)),
    };

    use base64::Engine;
    validate_clipboard_payload_length(data.len())?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&data)
        .map_err(|e| format!("Invalid base64: {e}"))?;
    if bytes.len() > MAX_CLIPBOARD_IMAGE_BYTES {
        return Err("Pasted images must be 64 MB or smaller".into());
    }

    let dir = clipboard_temp_directory();
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("Failed to create clipboard workspace: {e}"))?;
    let path = dir.join(format!("clipboard-{}.{}", Uuid::new_v4(), ext));
    std::fs::write(&path, &bytes).map_err(|e| format!("Failed to write temp file: {e}"))?;

    Ok(path.to_string_lossy().to_string())
}

fn validate_clipboard_payload_length(encoded_length: usize) -> Result<(), String> {
    if encoded_length > MAX_CLIPBOARD_IMAGE_BASE64_BYTES {
        return Err("Pasted images must be 64 MB or smaller".into());
    }
    Ok(())
}

pub(crate) fn clipboard_temp_directory() -> PathBuf {
    std::env::temp_dir().join("convertkit_clipboard")
}

pub(crate) fn is_managed_clipboard_path(path: &Path) -> bool {
    path.starts_with(clipboard_temp_directory())
}

/// Remove only regular files owned by ConvertKit's dedicated clipboard temp
/// directory. A zero duration clears the directory during a normal app exit;
/// launch cleanup removes crash leftovers older than the supplied age.
pub(crate) fn cleanup_managed_clipboard_files(max_age: Duration) {
    let directory = clipboard_temp_directory();
    let Ok(entries) = std::fs::read_dir(&directory) else {
        return;
    };

    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_file() || file_type.is_symlink() {
            continue;
        }
        let old_enough = max_age.is_zero()
            || entry
                .metadata()
                .and_then(|metadata| metadata.modified())
                .ok()
                .and_then(|modified| modified.elapsed().ok())
                .is_some_and(|age| age >= max_age);
        if old_enough {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// Open Finder with the given output item selected.
#[tauri::command]
pub async fn reveal_in_finder(path: String) -> Result<(), String> {
    reveal_paths_in_finder(vec![path]).await
}

fn validated_reveal_paths(paths: Vec<String>) -> Result<Vec<PathBuf>, String> {
    if paths.is_empty() || paths.len() > 100 {
        return Err("Choose between 1 and 100 output items to reveal".into());
    }

    let mut items = Vec::with_capacity(paths.len());
    let mut seen = HashSet::with_capacity(paths.len());
    for path in paths {
        let item = PathBuf::from(&path);
        let metadata = item
            .symlink_metadata()
            .map_err(|_| format!("Cannot reveal missing output: {path}"))?;
        let file_type = metadata.file_type();
        if path.trim().is_empty()
            || file_type.is_symlink()
            || (!file_type.is_file() && !file_type.is_dir())
        {
            return Err(format!("Cannot reveal unsupported output: {path}"));
        }
        let normalized = normalized_path(&item);
        if seen.insert(normalized) {
            items.push(item);
        }
    }

    Ok(items)
}

/// Open Finder and reveal one complete output set of files or directories.
#[tauri::command]
pub async fn reveal_paths_in_finder(paths: Vec<String>) -> Result<(), String> {
    let items = validated_reveal_paths(paths)?;

    let mut command = tokio::process::Command::new("open");
    command.arg("-R").args(items);
    let process_status = command
        .status()
        .await
        .map_err(|e| format!("Failed to open Finder: {e}"))?;

    if process_status.success() {
        Ok(())
    } else {
        Err(format!("Finder exited with status {process_status}"))
    }
}

/// Read a file and return its contents as a base64 data URL (thumbnail).
/// Uses ImageMagick to resize to 200x200 for raster images; falls back to
/// reading the full file for SVGs.
#[tauri::command]
pub async fn read_file_thumbnail(path: String) -> Result<String, String> {
    let p = PathBuf::from(&path);
    let ext = p
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("bin")
        .to_lowercase();

    // Document formats — generate thumbnail via macOS Quick Look.
    match ext.as_str() {
        "pdf" | "docx" | "doc" | "epub" | "html" | "htm" | "txt" | "md" => {
            return quicklook_thumbnail(&p).await;
        }
        _ => {}
    }

    // Video formats — extract a frame with FFmpeg.
    match ext.as_str() {
        "mp4" | "m4v" | "mov" | "webm" | "mkv" | "avi" => {
            return ffmpeg_thumbnail(&p).await;
        }
        _ => {}
    }

    let mime = match ext.as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "bmp" => "image/bmp",
        "ico" => "image/x-icon",
        "avif" => "image/avif",
        "tiff" | "tif" => "image/tiff",
        _ => return Err("Not a previewable format".into()),
    };

    // For SVGs, read the full file (magick may not handle them well).
    if ext == "svg" {
        use std::io::Read;
        let mut file = std::fs::File::open(&p).map_err(|e| format!("Cannot open file: {e}"))?;
        let meta = file
            .metadata()
            .map_err(|e| format!("Cannot read metadata: {e}"))?;
        if meta.len() > 10 * 1024 * 1024 {
            return Err("File too large for thumbnail".into());
        }
        let mut buf = Vec::with_capacity(meta.len() as usize);
        file.read_to_end(&mut buf)
            .map_err(|e| format!("Read error: {e}"))?;
        use base64::Engine;
        let b64 = base64::engine::general_purpose::STANDARD.encode(&buf);
        return Ok(format!("data:{};base64,{}", mime, b64));
    }

    // For raster images, shell out to magick to get a 200x200 PNG thumbnail.
    let output = tool_command("magick")
        .arg(&p)
        .args(["-resize", "200x200", "-quality", "80", "png:-"])
        .output()
        .await
        .map_err(|e| format!("Failed to run magick for thumbnail: {e}"))?;

    if !output.status.success() {
        // Fall back to reading the full file.
        return read_file_thumbnail_fallback(&p, mime).await;
    }

    let bytes = output.stdout;
    if bytes.is_empty() {
        return read_file_thumbnail_fallback(&p, mime).await;
    }

    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    // Thumbnail is always PNG from magick.
    Ok(format!("data:image/png;base64,{}", b64))
}

/// Generate a thumbnail for documents using macOS Quick Look (qlmanage).
async fn quicklook_thumbnail(path: &std::path::Path) -> Result<String, String> {
    let workspace = tempfile::Builder::new()
        .prefix("convertkit-quicklook-")
        .tempdir()
        .map_err(|error| format!("Cannot create Quick Look workspace: {error}"))?;

    let mut command = tokio::process::Command::new("qlmanage");
    command
        .args(["-t", "-s", "400", "-o"])
        .arg(workspace.path())
        .arg(path)
        .stdin(std::process::Stdio::null());
    run_process(
        command,
        tokio_util::sync::CancellationToken::new(),
        QUICKLOOK_PREVIEW_TIMEOUT,
        &[],
        QUICKLOOK_PROCESS_MESSAGES,
    )
    .await
    .map_err(|error| error.to_string())?;

    // qlmanage writes <filename>.png in the output dir
    let fname = path.file_name().unwrap_or_default();
    let thumb_path = workspace
        .path()
        .join(format!("{}.png", fname.to_string_lossy()));

    if !thumb_path.exists() {
        return Err("Quick Look thumbnail not found".into());
    }

    let bytes = std::fs::read(&thumb_path).map_err(|e| format!("Cannot read thumbnail: {e}"))?;

    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Ok(format!("data:image/png;base64,{}", b64))
}

/// Extract a single frame from a video using FFmpeg and return as base64 PNG.
async fn ffmpeg_thumbnail(path: &std::path::Path) -> Result<String, String> {
    let output = tool_command("ffmpeg")
        .args(["-i"])
        .arg(path)
        .args([
            "-ss",
            "00:00:01",
            "-frames:v",
            "1",
            "-vf",
            "scale=400:-1",
            "-f",
            "image2pipe",
            "-vcodec",
            "png",
            "-",
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .await
        .map_err(|e| format!("Failed to run ffmpeg for thumbnail: {e}"))?;

    if !output.status.success() || output.stdout.is_empty() {
        // Try frame at 0s for very short videos
        let output2 = tool_command("ffmpeg")
            .args(["-i"])
            .arg(path)
            .args([
                "-frames:v",
                "1",
                "-vf",
                "scale=400:-1",
                "-f",
                "image2pipe",
                "-vcodec",
                "png",
                "-",
            ])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output()
            .await
            .map_err(|e| format!("Failed to run ffmpeg for thumbnail: {e}"))?;

        if !output2.status.success() || output2.stdout.is_empty() {
            return Err("FFmpeg thumbnail extraction failed".into());
        }

        use base64::Engine;
        let b64 = base64::engine::general_purpose::STANDARD.encode(&output2.stdout);
        return Ok(format!("data:image/png;base64,{}", b64));
    }

    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&output.stdout);
    Ok(format!("data:image/png;base64,{}", b64))
}

/// Fallback: read the whole file when magick thumbnail generation fails.
async fn read_file_thumbnail_fallback(p: &PathBuf, mime: &str) -> Result<String, String> {
    use std::io::Read;
    let mut file = std::fs::File::open(p).map_err(|e| format!("Cannot open file: {e}"))?;
    let meta = file
        .metadata()
        .map_err(|e| format!("Cannot read metadata: {e}"))?;
    if meta.len() > 10 * 1024 * 1024 {
        return Err("File too large for thumbnail".into());
    }
    let mut buf = Vec::with_capacity(meta.len() as usize);
    file.read_to_end(&mut buf)
        .map_err(|e| format!("Read error: {e}"))?;
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&buf);
    Ok(format!("data:{};base64,{}", mime, b64))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Try to locate a tool and grab its version string.
async fn probe_tool(name: &str) -> (bool, Option<String>) {
    let path = match resolve_tool(name) {
        Some(p) => p,
        None => return (false, None),
    };

    // Attempt to get a version string via `<tool> --version`.
    let version = match tokio::process::Command::new(&path)
        .arg("--version")
        .output()
        .await
    {
        Ok(out) => {
            let raw = String::from_utf8_lossy(&out.stdout);
            // Take the first non-empty line.
            raw.lines()
                .find(|l| !l.trim().is_empty())
                .map(|l| l.trim().to_string())
        }
        Err(_) => None,
    };

    (true, version)
}

async fn probe_image_dimensions(
    path: &std::path::Path,
) -> Result<(Option<u32>, Option<u32>), String> {
    let first_frame = format!("{}[0]", path.to_string_lossy());
    let output = tool_command("magick")
        .args(["identify", "-ping", "-format", "%w %h", &first_frame])
        .output()
        .await
        .map_err(|error| format!("Failed to inspect image dimensions: {error}"))?;

    if !output.status.success() {
        return Ok((None, None));
    }

    let value = String::from_utf8_lossy(&output.stdout);
    let mut parts = value.split_whitespace();
    let width = parts.next().and_then(|part| part.parse::<u32>().ok());
    let height = parts.next().and_then(|part| part.parse::<u32>().ok());
    Ok((width, height))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn collects_supported_folder_files_and_relative_paths() {
        let directory = tempfile::tempdir().expect("tempdir");
        let nested = directory.path().join("nested");
        std::fs::create_dir(&nested).expect("nested folder");
        std::fs::write(directory.path().join("first.txt"), b"first").expect("text fixture");
        std::fs::write(nested.join("second.md"), b"second").expect("markdown fixture");
        std::fs::write(directory.path().join("ignored.pdf"), b"pdf").expect("pdf fixture");
        std::fs::write(directory.path().join("ignored.bin"), b"bin").expect("binary fixture");
        std::fs::write(directory.path().join(".hidden.txt"), b"hidden").expect("hidden fixture");

        let result = collect_input_paths(
            vec![directory.path().to_string_lossy().into_owned()],
            "convert".into(),
            100,
            Vec::new(),
        )
        .await
        .expect("folder collection");

        assert_eq!(result.files.len(), 2);
        assert_eq!(result.skipped_count, 3);
        assert!(!result.truncated);
        assert_eq!(result.files[0].relative_path.as_deref(), Some("first.txt"));
        let nested_relative = format!("nested{}second.md", std::path::MAIN_SEPARATOR);
        assert_eq!(
            result.files[1].relative_path.as_deref(),
            Some(nested_relative.as_str())
        );
    }

    #[tokio::test]
    async fn respects_queue_limit_and_excluded_files() {
        let directory = tempfile::tempdir().expect("tempdir");
        let first = directory.path().join("first.txt");
        let second = directory.path().join("second.txt");
        let third = directory.path().join("third.txt");
        std::fs::write(&first, b"first").expect("first fixture");
        std::fs::write(&second, b"second").expect("second fixture");
        std::fs::write(&third, b"third").expect("third fixture");

        let result = collect_input_paths(
            vec![directory.path().to_string_lossy().into_owned()],
            "convert".into(),
            1,
            vec![first.to_string_lossy().into_owned()],
        )
        .await
        .expect("folder collection");

        assert_eq!(result.files.len(), 1);
        assert_eq!(result.files[0].name, "second.txt");
        assert_eq!(result.skipped_count, 1);
        assert!(result.truncated);
    }

    #[test]
    fn recognizes_only_the_managed_clipboard_directory() {
        let managed = clipboard_temp_directory().join("clipboard-test.png");
        let unrelated = std::env::temp_dir().join("unrelated.png");
        assert!(is_managed_clipboard_path(&managed));
        assert!(!is_managed_clipboard_path(&unrelated));
    }

    #[test]
    fn bounds_clipboard_payloads_before_base64_decoding() {
        assert!(validate_clipboard_payload_length(MAX_CLIPBOARD_IMAGE_BASE64_BYTES).is_ok());
        assert!(validate_clipboard_payload_length(MAX_CLIPBOARD_IMAGE_BASE64_BYTES + 1).is_err());
    }

    #[test]
    fn reveal_validation_accepts_files_and_directories_with_stable_deduplication() {
        let directory = tempfile::tempdir().expect("tempdir");
        let file = directory.path().join("result.txt");
        std::fs::write(&file, b"result").expect("result fixture");
        let alias = directory.path().join(".").join("result.txt");

        let items = validated_reveal_paths(vec![
            file.to_string_lossy().into_owned(),
            directory.path().to_string_lossy().into_owned(),
            alias.to_string_lossy().into_owned(),
        ])
        .expect("valid reveal outputs");

        assert_eq!(items, [file, directory.path().to_path_buf()]);
    }

    #[test]
    fn reveal_validation_enforces_bounds_and_rejects_missing_outputs() {
        assert!(validated_reveal_paths(Vec::new()).is_err());
        assert!(validated_reveal_paths(vec!["missing".into(); 101]).is_err());

        let directory = tempfile::tempdir().expect("tempdir");
        assert!(validated_reveal_paths(vec![directory
            .path()
            .join("missing")
            .to_string_lossy()
            .into_owned(),])
        .is_err());
    }

    #[cfg(unix)]
    #[test]
    fn reveal_validation_rejects_symlinks_and_special_files() {
        use std::os::unix::fs::{symlink, FileTypeExt};

        let directory = tempfile::tempdir().expect("tempdir");
        let file = directory.path().join("result.txt");
        let link = directory.path().join("result-link.txt");
        std::fs::write(&file, b"result").expect("result fixture");
        symlink(&file, &link).expect("symlink fixture");
        assert!(validated_reveal_paths(vec![link.to_string_lossy().into_owned()]).is_err());

        let null_type = std::fs::metadata("/dev/null")
            .expect("/dev/null metadata")
            .file_type();
        assert!(null_type.is_char_device());
        assert!(validated_reveal_paths(vec!["/dev/null".into()]).is_err());
    }

    #[test]
    fn archive_inputs_use_operation_specific_file_rules() {
        assert!(InputOperation::CreateArchive.supports_path(Path::new("anything.bin")));
        assert!(InputOperation::ExtractArchive.supports_path(Path::new("archive.ZIP")));
        assert!(InputOperation::ExtractArchive.supports_path(Path::new("archive.tar")));
        assert!(InputOperation::ExtractArchive.supports_path(Path::new("archive.tar.gz")));
        assert!(InputOperation::ExtractArchive.supports_path(Path::new("archive.TGZ")));
        assert!(InputOperation::ExtractArchive.supports_path(Path::new("archive.7Z")));
        assert!(InputOperation::ExtractArchive.supports_path(Path::new("archive.gz")));
        assert!(InputOperation::Inspect.supports_path(Path::new("extensionless")));
        assert!(InputOperation::Inspect.supports_path(Path::new("unknown.custom")));
        assert!(InputOperation::ExtractAudio.supports_path(Path::new("clip.MOV")));
        assert!(!InputOperation::ExtractAudio.supports_path(Path::new("recording.mp3")));
        assert!(InputOperation::Transcribe.supports_path(Path::new("clip.MOV")));
        assert!(InputOperation::Transcribe.supports_path(Path::new("recording.mp3")));
        assert!(!InputOperation::Transcribe.supports_path(Path::new("notes.txt")));
        assert!(InputOperation::EncodeVideo.supports_path(Path::new("clip.MKV")));
        assert!(!InputOperation::EncodeVideo.supports_path(Path::new("recording.wav")));
        assert!(InputOperation::RemoveAudio.supports_path(Path::new("clip.MP4")));
        assert!(!InputOperation::RemoveAudio.supports_path(Path::new("recording.m4a")));
        assert!(InputOperation::ExtractSubtitles.supports_path(Path::new("clip.MKV")));
        assert!(!InputOperation::ExtractSubtitles.supports_path(Path::new("captions.srt")));
        assert!(InputOperation::GenerateThumbnails.supports_path(Path::new("clip.MOV")));
        assert!(!InputOperation::GenerateThumbnails.supports_path(Path::new("photo.png")));
        assert!(InputOperation::RemoveMetadata.supports_path(Path::new("photo.JPEG")));
        assert!(InputOperation::RemoveMetadata.supports_path(Path::new("image.webp")));
        assert!(InputOperation::RemoveMetadata.supports_path(Path::new("clip.MOV")));
        assert!(InputOperation::RemoveMetadata.supports_path(Path::new("recording.flac")));
        assert!(InputOperation::RemoveMetadata.supports_path(Path::new("document.PDF")));
        assert!(!InputOperation::RemoveMetadata.supports_path(Path::new("image.heic")));
        assert!(InputOperation::ExportImages.supports_path(Path::new("photo.HEIC")));
        assert!(!InputOperation::ExportImages.supports_path(Path::new("clip.mov")));
        assert!(InputOperation::ExportPdfPages.supports_path(Path::new("report.PDF")));
        assert!(!InputOperation::ExportPdfPages.supports_path(Path::new("photo.png")));
        for path in ["report.PDF", "page.png", "cover.jpg", "back.JPEG"] {
            assert!(
                InputOperation::MergePdf.supports_path(Path::new(path)),
                "{path}"
            );
        }
        assert!(!InputOperation::MergePdf.supports_path(Path::new("page.webp")));
    }
}
