use std::path::{Path, PathBuf};

use serde::Deserialize;
use uuid::Uuid;

use crate::engines::verification;
use crate::engines::ConversionResult;
use crate::error::ConversionError;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct OutputOptions {
    pub directory: Option<String>,
    pub suffix: String,
}

pub struct PreparedOutput {
    naming: OutputNaming,
    final_path: PathBuf,
    working_path: PathBuf,
    committed: bool,
}

#[derive(Clone)]
struct OutputNaming {
    directory: PathBuf,
    stem: String,
    suffix: String,
    extension: String,
}

impl OutputNaming {
    fn candidate(&self, index: Option<u32>) -> PathBuf {
        self.directory.join(output_file_name(
            &self.stem,
            &self.suffix,
            &self.extension,
            index,
        ))
    }
}

impl PreparedOutput {
    pub fn working_path(&self) -> &Path {
        &self.working_path
    }

    pub fn commit(
        mut self,
        mut result: ConversionResult,
    ) -> Result<ConversionResult, ConversionError> {
        verification::nonempty_file(&self.working_path)?;

        self.commit_verified(&mut result, true)?;
        Ok(result)
    }

    /// Commit a verified regular file while allowing an empty payload. Archive
    /// extraction can use this for valid zero-byte results such as an empty
    /// gzip stream. Existing conversion callers retain the stricter `commit` path.
    pub fn commit_regular_file(
        mut self,
        mut result: ConversionResult,
    ) -> Result<ConversionResult, ConversionError> {
        regular_file_size(&self.working_path)?;

        self.commit_verified(&mut result, false)?;
        Ok(result)
    }

    fn commit_verified(
        &mut self,
        result: &mut ConversionResult,
        require_nonempty: bool,
    ) -> Result<(), ConversionError> {
        let final_path = loop {
            match atomic_rename_no_replace(&self.working_path, &self.final_path) {
                Ok(()) => break self.final_path.clone(),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    self.final_path = deduplicate(&self.naming);
                }
                Err(error) => {
                    let _ = std::fs::remove_file(&self.working_path);
                    return Err(finalize_error(error));
                }
            }
        };

        let verified = if require_nonempty {
            verification::nonempty_file(&final_path)
        } else {
            regular_file_size(&final_path)
        };
        let output_size = match verified {
            Ok(size) => size,
            Err(error) => {
                // The final path belongs to this transaction because every
                // commit uses an exclusive rename.
                let _ = std::fs::remove_file(&final_path);
                return Err(error);
            }
        };

        result.output_path = final_path.to_string_lossy().into();
        if result.output_paths.is_empty() {
            result.output_paths.push(result.output_path.clone());
        }
        result.output_size = output_size;
        self.committed = true;
        Ok(())
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
    let stem = input
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("output");
    prepare_output_with_stem(input, stem, extension, default_suffix, options)
}

/// Prepare an output whose logical filename is not the input's literal stem.
/// This keeps archive-derived names on the same atomic, collision-safe commit
/// boundary as normal conversions. An empty extension is valid.
pub fn prepare_output_with_stem(
    input: &Path,
    logical_stem: &str,
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
    validate_output_component(logical_stem, "filename")?;
    if !extension.is_empty() {
        validate_output_component(extension, "extension")?;
    }

    let requested_directory = options
        .directory
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            if super::detect::is_managed_clipboard_path(input) {
                downloads_fallback()
            } else {
                input
                    .parent()
                    .unwrap_or_else(|| Path::new("."))
                    .to_path_buf()
            }
        });
    let directory = resolve_writable_directory(requested_directory, has_custom_directory)?;
    let naming = OutputNaming {
        directory: directory.clone(),
        stem: logical_stem.to_owned(),
        suffix: options.suffix.clone(),
        extension: extension.to_owned(),
    };
    let final_path = deduplicate(&naming);

    if paths_refer_to_same_file(input, &final_path) {
        return Err(ConversionError::OutputConflict {
            path: final_path.to_string_lossy().into(),
            message: "The output name would replace the source file. Add a filename suffix.".into(),
        });
    }

    // Write to a hidden sibling first so incomplete work never appears as a
    // finished output and the final rename stays atomic.
    let working_name = if extension.is_empty() {
        format!(".convertkit-{}", Uuid::new_v4())
    } else {
        format!(".convertkit-{}.{}", Uuid::new_v4(), extension)
    };
    let working_path = directory.join(working_name);

    Ok(PreparedOutput {
        naming,
        final_path,
        working_path,
        committed: false,
    })
}

fn validate_output_component(value: &str, label: &str) -> Result<(), ConversionError> {
    if value.is_empty()
        || value == "."
        || value == ".."
        || value.chars().count() > 255
        || value
            .chars()
            .any(|character| matches!(character, '/' | '\\' | ':' | '\0'))
    {
        return Err(ConversionError::OutputConflict {
            path: value.into(),
            message: format!("The output {label} is unsupported or too long."),
        });
    }
    Ok(())
}

pub(super) fn validate_suffix(suffix: &str) -> Result<(), ConversionError> {
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

pub(super) fn resolve_writable_directory(
    requested: PathBuf,
    is_custom: bool,
) -> Result<PathBuf, ConversionError> {
    if is_custom && !requested.exists() {
        std::fs::create_dir_all(&requested).map_err(|error| ConversionError::OutputConflict {
            path: requested.to_string_lossy().into(),
            message: format!("Could not create the output folder: {error}"),
        })?;
    }

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

fn output_file_name(stem: &str, suffix: &str, extension: &str, index: Option<u32>) -> String {
    let number = index.map_or_else(String::new, |value| format!(" ({value})"));
    if extension.is_empty() {
        format!("{stem}{suffix}{number}")
    } else {
        format!("{stem}{suffix}{number}.{extension}")
    }
}

fn deduplicate(naming: &OutputNaming) -> PathBuf {
    let base = naming.candidate(None);
    if !base.exists() {
        return base;
    }
    for index in 1u32.. {
        let candidate = naming.candidate(Some(index));
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!()
}

fn regular_file_size(path: &Path) -> Result<u64, ConversionError> {
    let metadata = std::fs::symlink_metadata(path).map_err(|_| ConversionError::OutputMissing)?;
    if !metadata.file_type().is_file() {
        return Err(ConversionError::OutputMissing);
    }
    Ok(metadata.len())
}

fn finalize_error(error: std::io::Error) -> ConversionError {
    ConversionError::ProcessFailed {
        message: format!("Could not finalize the output: {error}"),
        stderr: String::new(),
        exit_code: None,
    }
}

/// Atomically move `source` to `destination` only if no filesystem object is
/// already present there. On macOS, RENAME_EXCL performs the existence check
/// and rename in one kernel operation for both regular files and directories.
#[cfg(target_os = "macos")]
pub(crate) fn atomic_rename_no_replace(source: &Path, destination: &Path) -> std::io::Result<()> {
    use std::ffi::CString;
    use std::os::raw::{c_char, c_int, c_uint};
    use std::os::unix::ffi::OsStrExt;

    const RENAME_EXCL: c_uint = 0x0000_0004;
    extern "C" {
        fn renamex_np(old: *const c_char, new: *const c_char, flags: c_uint) -> c_int;
    }

    let source = CString::new(source.as_os_str().as_bytes())
        .map_err(|_| std::io::Error::from(std::io::ErrorKind::InvalidInput))?;
    let destination = CString::new(destination.as_os_str().as_bytes())
        .map_err(|_| std::io::Error::from(std::io::ErrorKind::InvalidInput))?;
    // SAFETY: both pointers reference live NUL-terminated byte strings for the
    // duration of this call; renamex_np does not retain them.
    let result = unsafe { renamex_np(source.as_ptr(), destination.as_ptr(), RENAME_EXCL) };
    if result == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn atomic_rename_no_replace(source: &Path, destination: &Path) -> std::io::Result<()> {
    if source.is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "atomic no-replace directory rename requires macOS",
        ));
    }
    std::fs::hard_link(source, destination)?;
    std::fs::remove_file(source)
}

fn paths_refer_to_same_file(left: &Path, right: &Path) -> bool {
    if left == right {
        return true;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;

        if let (Ok(left), Ok(right)) = (std::fs::metadata(left), std::fs::metadata(right)) {
            if left.dev() == right.dev() && left.ino() == right.ino() {
                return true;
            }
        }
    }

    match (left.canonicalize(), right.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_result() -> ConversionResult {
        ConversionResult {
            output_path: String::new(),
            output_paths: Vec::new(),
            output_size: 0,
            duration_ms: 0,
            undo_manifest: None,
        }
    }

    fn options(directory: &Path, suffix: &str) -> OutputOptions {
        OutputOptions {
            directory: Some(directory.to_string_lossy().into()),
            suffix: suffix.into(),
        }
    }

    #[test]
    fn uses_custom_directory_and_suffix() {
        let source = tempfile::tempdir().expect("source tempdir");
        let output = tempfile::tempdir().expect("output tempdir");
        let input = source.path().join("photo.png");
        std::fs::write(&input, b"input").expect("fixture");

        let prepared = prepare_output(&input, "jpg", "", Some(options(output.path(), "-web")))
            .expect("output path");
        assert_eq!(prepared.final_path, output.path().join("photo-web.jpg"));
        assert_ne!(prepared.working_path, prepared.final_path);
        assert_eq!(prepared.working_path.parent(), Some(output.path()));
    }

    #[test]
    fn creates_nested_custom_output_directories() {
        let source = tempfile::tempdir().expect("source tempdir");
        let output = tempfile::tempdir().expect("output tempdir");
        let input = source.path().join("photo.png");
        let nested = output.path().join("album").join("edited");
        std::fs::write(&input, b"input").expect("fixture");

        let prepared =
            prepare_output(&input, "jpg", "", Some(options(&nested, ""))).expect("output path");

        assert!(nested.is_dir());
        assert_eq!(prepared.final_path, nested.join("photo.jpg"));
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
            Some(options(directory.path(), "-resized")),
        )
        .expect("output path");
        assert_eq!(
            prepared.final_path,
            directory.path().join("photo-resized (1).png")
        );
    }

    #[test]
    fn keep_both_never_replaces_a_file_created_after_prepare() {
        let directory = tempfile::tempdir().expect("tempdir");
        let input = directory.path().join("photo.png");
        let raced = directory.path().join("photo-resized.png");
        let numbered = directory.path().join("photo-resized (1).png");
        std::fs::write(&input, b"input").expect("source");

        let prepared = prepare_output(
            &input,
            "png",
            "-resized",
            Some(options(directory.path(), "-resized")),
        )
        .expect("prepared output");
        std::fs::write(&raced, b"someone else's output").expect("racing output");
        std::fs::write(prepared.working_path(), b"converted").expect("working output");

        let result = prepared.commit(empty_result()).expect("atomic commit");

        assert_eq!(
            std::fs::read(&raced).expect("racing output survives"),
            b"someone else's output"
        );
        assert_eq!(
            std::fs::read(&numbered).expect("numbered output"),
            b"converted"
        );
        assert_eq!(result.output_path, numbered.to_string_lossy());
        assert_eq!(result.output_paths, vec![numbered.to_string_lossy()]);
    }

    #[test]
    fn explicit_stem_supports_extensionless_empty_outputs() {
        let directory = tempfile::tempdir().expect("tempdir");
        let input = directory.path().join("empty.gz");
        std::fs::write(&input, b"gzip source").expect("source");

        let prepared = prepare_output_with_stem(
            &input,
            "empty-file",
            "",
            "",
            Some(options(directory.path(), "")),
        )
        .expect("prepared extensionless output");
        std::fs::write(prepared.working_path(), []).expect("empty working output");

        let result = prepared
            .commit_regular_file(empty_result())
            .expect("empty regular output is valid");
        assert_eq!(
            result.output_path,
            directory.path().join("empty-file").to_string_lossy()
        );
        assert_eq!(result.output_size, 0);
    }
}
