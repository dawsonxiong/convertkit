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
    pub name: String,
    pub extension: String,
    pub size: u64,
    pub format: Option<String>,
    pub category: Option<String>,
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

    Ok(FileInfoResponse {
        name,
        extension: extension.clone(),
        size: meta.len(),
        format: format.map(|f| f.extension().to_string()),
        category: format.map(|f| f.category().to_string()),
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
