use log::warn;
use serde::Serialize;
use std::path::PathBuf;
use tauri::Manager;

use crate::engines::{resolve_tool, tool_command};
use crate::formats::Format;

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

#[derive(Debug, Clone, Serialize)]
pub struct FileInfoResponse {
    pub path: String,
    pub name: String,
    pub extension: String,
    pub size: u64,
    pub format: String,
    pub category: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
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
    let p = PathBuf::from(&path);
    let meta = std::fs::metadata(&p).map_err(|e| format!("Cannot read file: {e}"))?;

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
    if let Ok(Some(inferred)) = infer::get_from_path(&p) {
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
        probe_image_dimensions(&p).await.unwrap_or((None, None))
    } else {
        (None, None)
    };

    Ok(FileInfoResponse {
        path,
        name,
        extension: extension.clone(),
        size: meta.len(),
        format: format
            .map(|f| f.extension().to_string())
            .unwrap_or_else(|| "unknown".to_string()),
        category: format
            .map(|f| f.category().to_string())
            .unwrap_or_else(|| "unknown".to_string()),
        width,
        height,
    })
}

/// Return the first file opened via Finder "Open With", if any.
#[tauri::command]
pub async fn get_opened_file(app: tauri::AppHandle) -> Option<String> {
    let state = app.state::<crate::OpenedFiles>();
    let mut opened = state.0.lock().expect("OpenedFiles lock poisoned");
    opened.pop().map(|p| p.to_string_lossy().to_string())
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
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&data)
        .map_err(|e| format!("Invalid base64: {e}"))?;

    let dir = std::env::temp_dir().join("convertkit_clipboard");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join(format!("clipboard.{}", ext));
    std::fs::write(&path, &bytes).map_err(|e| format!("Failed to write temp file: {e}"))?;

    Ok(path.to_string_lossy().to_string())
}

/// Open Finder with the given file selected.
#[tauri::command]
pub async fn reveal_in_finder(path: String) -> Result<(), String> {
    let file = std::path::Path::new(&path);
    if path.trim().is_empty() || !file.is_file() {
        return Err(format!("Cannot reveal missing file: {path}"));
    }

    let status = tokio::process::Command::new("open")
        .arg("-R")
        .arg(file)
        .status()
        .await
        .map_err(|e| format!("Failed to open Finder: {e}"))?;

    if status.success() {
        Ok(())
    } else {
        Err(format!("Finder exited with status {status}"))
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
    let tmp = std::env::temp_dir().join("convertkit_ql");
    let _ = std::fs::create_dir_all(&tmp);

    let output = tokio::process::Command::new("qlmanage")
        .args(["-t", "-s", "400", "-o"])
        .arg(&tmp)
        .arg(path)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .await
        .map_err(|e| format!("Failed to run qlmanage: {e}"))?;

    if !output.status.success() {
        return Err("Quick Look thumbnail generation failed".into());
    }

    // qlmanage writes <filename>.png in the output dir
    let fname = path.file_name().unwrap_or_default();
    let thumb_path = tmp.join(format!("{}.png", fname.to_string_lossy()));

    if !thumb_path.exists() {
        return Err("Quick Look thumbnail not found".into());
    }

    let bytes = std::fs::read(&thumb_path).map_err(|e| format!("Cannot read thumbnail: {e}"))?;
    let _ = std::fs::remove_file(&thumb_path);

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
