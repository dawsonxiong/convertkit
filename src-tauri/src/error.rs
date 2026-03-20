use serde::Serialize;

/// Every error that can occur during a file conversion.
#[derive(Debug, thiserror::Error, Serialize)]
#[serde(tag = "kind", content = "detail")]
pub enum ConversionError {
    #[error("Missing dependency `{tool}`: {install_hint}")]
    MissingDependency { tool: String, install_hint: String },

    #[error("Cannot convert {input} to {output}")]
    UnsupportedConversion { input: String, output: String },

    #[error("Input file not found: {path}")]
    InputNotFound { path: String },

    #[error("Process failed: {message}")]
    ProcessFailed {
        message: String,
        stderr: String,
        exit_code: Option<i32>,
    },

    #[error("Conversion cancelled")]
    Cancelled,

    #[error("Conversion timed out after {seconds}s")]
    Timeout { seconds: u64 },

    #[error("Output file was not created")]
    OutputMissing,

    #[error("Disk is full")]
    DiskFull,
}

// Tauri v2 has a blanket `impl<T: Serialize> From<T> for InvokeError`, so
// `ConversionError` can be returned directly from `#[tauri::command]` functions
// as long as it implements `Serialize` (which it does via the derive above).
