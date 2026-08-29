use std::path::{Path, PathBuf};

use serde::Deserialize;
use uuid::Uuid;

use crate::engines::ConversionResult;
use crate::error::ConversionError;

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CollisionPolicy {
    #[default]
    Rename,
    Replace,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct OutputOptions {
    pub directory: Option<String>,
    pub suffix: String,
    pub collision_policy: CollisionPolicy,
}

pub struct PreparedOutput {
    final_path: PathBuf,
    working_path: PathBuf,
    committed: bool,
}

impl PreparedOutput {
    pub fn working_path(&self) -> &Path {
        &self.working_path
    }

    pub fn commit(
        mut self,
        mut result: ConversionResult,
    ) -> Result<ConversionResult, ConversionError> {
        if !self.working_path.is_file() {
            return Err(ConversionError::OutputMissing);
        }

        if self.working_path != self.final_path {
            if let Err(error) = std::fs::rename(&self.working_path, &self.final_path) {
                let _ = std::fs::remove_file(&self.working_path);
                return Err(ConversionError::ProcessFailed {
                    message: format!("Could not replace the existing output: {error}"),
                    stderr: String::new(),
                    exit_code: None,
                });
            }
        }

        let output_size = std::fs::metadata(&self.final_path)
            .map_err(|_| ConversionError::OutputMissing)?
            .len();
        result.output_path = self.final_path.to_string_lossy().into();
        result.output_size = output_size;
        self.committed = true;
        Ok(result)
    }
}

impl Drop for PreparedOutput {
    fn drop(&mut self) {
        if !self.committed && self.working_path != self.final_path {
            let _ = std::fs::remove_file(&self.working_path);
        }
    }
}

pub fn prepare_output(
    input: &Path,
    extension: &str,
    default_suffix: &str,
    options: Option<OutputOptions>,
) -> Result<PreparedOutput, ConversionError> {
    let has_custom_directory = options
        .as_ref()
        .and_then(|value| value.directory.as_deref())
        .is_some_and(|value| !value.trim().is_empty());
    let options = options.unwrap_or_else(|| OutputOptions {
        suffix: default_suffix.to_string(),
        ..OutputOptions::default()
    });
    validate_suffix(&options.suffix)?;

    let requested_directory = options
        .directory
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            input
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .to_path_buf()
        });
    let directory = resolve_writable_directory(requested_directory, has_custom_directory)?;
    let stem = input
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("output");
    let base = directory.join(format!("{stem}{}.{extension}", options.suffix));
    let final_path = match options.collision_policy {
        CollisionPolicy::Rename => deduplicate(base, stem, &options.suffix, extension),
        CollisionPolicy::Replace => base,
    };

    if paths_refer_to_same_file(input, &final_path) {
        return Err(ConversionError::OutputConflict {
            path: final_path.to_string_lossy().into(),
            message:
                "The output name would replace the source file. Add a suffix or use Keep both."
                    .into(),
        });
    }

    let working_path = if options.collision_policy == CollisionPolicy::Replace {
        directory.join(format!(".convertkit-{}.{}", Uuid::new_v4(), extension))
    } else {
        final_path.clone()
    };

    Ok(PreparedOutput {
        final_path,
        working_path,
        committed: false,
    })
}

fn validate_suffix(suffix: &str) -> Result<(), ConversionError> {
    if suffix.chars().count() > 80
        || suffix
            .chars()
            .any(|character| matches!(character, '/' | '\\' | ':' | '\0'))
    {
        return Err(ConversionError::OutputConflict {
            path: suffix.into(),
            message: "The filename suffix contains unsupported characters or is too long.".into(),
        });
    }
    Ok(())
}

fn resolve_writable_directory(
    requested: PathBuf,
    is_custom: bool,
) -> Result<PathBuf, ConversionError> {
    if requested.is_dir() && directory_is_writable(&requested) {
        return Ok(requested);
    }

    if is_custom {
        return Err(ConversionError::OutputConflict {
            path: requested.to_string_lossy().into(),
            message: "The selected output folder is unavailable or read-only.".into(),
        });
    }

    let fallback = downloads_fallback();
    std::fs::create_dir_all(&fallback).map_err(|error| ConversionError::ProcessFailed {
        message: format!("Could not create the fallback output folder: {error}"),
        stderr: String::new(),
        exit_code: None,
    })?;
    if directory_is_writable(&fallback) {
        Ok(fallback)
    } else {
        Err(ConversionError::OutputConflict {
            path: fallback.to_string_lossy().into(),
            message: "Neither the source folder nor the fallback output folder is writable.".into(),
        })
    }
}

fn directory_is_writable(directory: &Path) -> bool {
    let probe = directory.join(format!(".convertkit-write-{}", Uuid::new_v4()));
    match std::fs::File::create(&probe) {
        Ok(_) => {
            let _ = std::fs::remove_file(probe);
            true
        }
        Err(_) => false,
    }
}

fn downloads_fallback() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home).join("Downloads").join("ConvertKit")
}

fn deduplicate(base: PathBuf, stem: &str, suffix: &str, extension: &str) -> PathBuf {
    if !base.exists() {
        return base;
    }
    let directory = base.parent().unwrap_or_else(|| Path::new("."));
    for index in 1u32.. {
        let candidate = directory.join(format!("{stem}{suffix} ({index}).{extension}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!()
}

fn paths_refer_to_same_file(left: &Path, right: &Path) -> bool {
    if left == right {
        return true;
    }
    match (left.canonicalize(), right.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options(directory: &Path, suffix: &str, collision_policy: CollisionPolicy) -> OutputOptions {
        OutputOptions {
            directory: Some(directory.to_string_lossy().into()),
            suffix: suffix.into(),
            collision_policy,
        }
    }

    #[test]
    fn uses_custom_directory_and_suffix() {
        let source = tempfile::tempdir().expect("source tempdir");
        let output = tempfile::tempdir().expect("output tempdir");
        let input = source.path().join("photo.png");
        std::fs::write(&input, b"input").expect("fixture");

        let prepared = prepare_output(
            &input,
            "jpg",
            "",
            Some(options(output.path(), "-web", CollisionPolicy::Rename)),
        )
        .expect("output path");
        assert_eq!(prepared.final_path, output.path().join("photo-web.jpg"));
        assert_eq!(prepared.working_path, prepared.final_path);
    }

    #[test]
    fn uses_default_suffix_when_options_are_omitted() {
        let directory = tempfile::tempdir().expect("tempdir");
        let input = directory.path().join("photo.png");
        std::fs::write(&input, b"input").expect("fixture");

        let prepared = prepare_output(&input, "png", "-resized", None).expect("output path");
        assert_eq!(
            prepared.final_path,
            directory.path().join("photo-resized.png")
        );
    }

    #[test]
    fn keep_both_increments_the_name() {
        let directory = tempfile::tempdir().expect("tempdir");
        let input = directory.path().join("photo.png");
        std::fs::write(&input, b"input").expect("fixture");
        std::fs::write(directory.path().join("photo-resized.png"), b"existing").expect("fixture");

        let prepared = prepare_output(
            &input,
            "png",
            "-resized",
            Some(options(
                directory.path(),
                "-resized",
                CollisionPolicy::Rename,
            )),
        )
        .expect("output path");
        assert_eq!(
            prepared.final_path,
            directory.path().join("photo-resized (1).png")
        );
    }

    #[test]
    fn replace_uses_a_temporary_sibling_until_commit() {
        let directory = tempfile::tempdir().expect("tempdir");
        let input = directory.path().join("photo.png");
        let final_path = directory.path().join("photo-optimized.png");
        std::fs::write(&input, b"input").expect("fixture");
        std::fs::write(&final_path, b"old").expect("fixture");

        let prepared = prepare_output(
            &input,
            "png",
            "-optimized",
            Some(options(
                directory.path(),
                "-optimized",
                CollisionPolicy::Replace,
            )),
        )
        .expect("output path");
        assert_ne!(prepared.working_path, final_path);
        assert_eq!(std::fs::read(&final_path).expect("old output"), b"old");

        std::fs::write(&prepared.working_path, b"new").expect("working output");
        let result = prepared
            .commit(ConversionResult {
                output_path: String::new(),
                output_size: 0,
                duration_ms: 7,
            })
            .expect("commit");
        assert_eq!(std::fs::read(&final_path).expect("new output"), b"new");
        assert_eq!(result.output_path, final_path.to_string_lossy());
        assert_eq!(result.output_size, 3);
    }

    #[test]
    fn replace_never_overwrites_the_source() {
        let directory = tempfile::tempdir().expect("tempdir");
        let input = directory.path().join("photo.png");
        std::fs::write(&input, b"input").expect("fixture");

        let result = prepare_output(
            &input,
            "png",
            "-resized",
            Some(options(directory.path(), "", CollisionPolicy::Replace)),
        );
        assert!(matches!(
            result,
            Err(ConversionError::OutputConflict { .. })
        ));
        assert_eq!(std::fs::read(&input).expect("source"), b"input");
    }
}
