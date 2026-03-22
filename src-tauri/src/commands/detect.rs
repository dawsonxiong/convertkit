use log::warn;
use serde::Serialize;
use std::path::PathBuf;

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

    Ok(FileInfoResponse {
        path,
        name,
        extension: extension.clone(),
        size: meta.len(),
        format: format.map(|f| f.extension().to_string()).unwrap_or_else(|| "unknown".to_string()),
        category: format.map(|f| f.category().to_string()).unwrap_or_else(|| "unknown".to_string()),
    })
}

/// Open Finder with the given file selected.
#[tauri::command]
pub async fn reveal_in_finder(path: String) -> Result<(), String> {
    tokio::process::Command::new("open")
        .arg("-R")
        .arg(&path)
        .status()
        .await
        .map_err(|e| format!("Failed to reveal file: {e}"))?;
    Ok(())
}

/// Read a file and return its contents as a base64 data URL (thumbnail).
/// Uses ImageMagick to resize to 200x200 for raster images; falls back to
/// reading the full file for SVGs.
#[tauri::command]
pub async fn read_file_thumbnail(path: String) -> Result<String, String> {
    let p = PathBuf::from(&path);
    let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("bin").to_lowercase();

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
        let meta = file.metadata().map_err(|e| format!("Cannot read metadata: {e}"))?;
        if meta.len() > 10 * 1024 * 1024 {
            return Err("File too large for thumbnail".into());
        }
        let mut buf = Vec::with_capacity(meta.len() as usize);
        file.read_to_end(&mut buf).map_err(|e| format!("Read error: {e}"))?;
        use base64::Engine;
        let b64 = base64::engine::general_purpose::STANDARD.encode(&buf);
        return Ok(format!("data:{};base64,{}", mime, b64));
    }

    // For raster images, shell out to magick to get a 200x200 PNG thumbnail.
    let output = tokio::process::Command::new("magick")
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

/// Fallback: read the whole file when magick thumbnail generation fails.
async fn read_file_thumbnail_fallback(p: &PathBuf, mime: &str) -> Result<String, String> {
    use std::io::Read;
    let mut file = std::fs::File::open(p).map_err(|e| format!("Cannot open file: {e}"))?;
    let meta = file.metadata().map_err(|e| format!("Cannot read metadata: {e}"))?;
    if meta.len() > 10 * 1024 * 1024 {
        return Err("File too large for thumbnail".into());
    }
    let mut buf = Vec::with_capacity(meta.len() as usize);
    file.read_to_end(&mut buf).map_err(|e| format!("Read error: {e}"))?;
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&buf);
    Ok(format!("data:{};base64,{}", mime, b64))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Try to locate a tool and grab its version string.
async fn probe_tool(name: &str) -> (bool, Option<String>) {
    let path = match which::which(name) {
        Ok(p) => p,
        Err(_) => return (false, None),
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
