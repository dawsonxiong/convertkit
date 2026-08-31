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

    #[error("Output conflict at {path}: {message}")]
    OutputConflict { path: String, message: String },

    #[error("This encrypted archive requires a password")]
    ArchivePasswordRequired,

    #[error("The archive password is incorrect")]
    IncorrectArchivePassword,

    #[error("The requested {target_bytes}-byte file-size target could not be reached")]
    TargetSizeUnreachable {
        #[serde(rename = "targetBytes")]
        target_bytes: u64,
        #[serde(rename = "smallestBytes")]
        smallest_bytes: Option<u64>,
    },

    #[error("The compressed output is not smaller than the source")]
    OutputNotSmaller {
        #[serde(rename = "sourceBytes")]
        source_bytes: u64,
        #[serde(rename = "outputBytes")]
        output_bytes: u64,
    },

    #[error("Disk is full")]
    DiskFull,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_not_smaller_has_a_stable_typed_payload() {
        let serialized = serde_json::to_value(ConversionError::OutputNotSmaller {
            source_bytes: 1_024,
            output_bytes: 1_100,
        })
        .expect("serialized error");
        assert_eq!(serialized["kind"], "OutputNotSmaller");
        assert_eq!(serialized["detail"]["sourceBytes"], 1_024);
        assert_eq!(serialized["detail"]["outputBytes"], 1_100);
    }
}

// Tauri v2 has a blanket `impl<T: Serialize> From<T> for InvokeError`, so
// `ConversionError` can be returned directly from `#[tauri::command]` functions
// as long as it implements `Serialize` (which it does via the derive above).
