use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::engines::ConversionResult;
use crate::error::ConversionError;
use crate::progress::ProgressPayload;

const MAX_RENAME_ITEMS: usize = 100;
const MANIFEST_VERSION: u32 = 1;
const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameItemRequest {
    pub input_path: String,
    pub output_name: String,
}

#[derive(Debug)]
pub(super) struct RenamePlan {
    items: Vec<PlannedRename>,
    output_paths: Vec<PathBuf>,
    total_size: u64,
}

#[derive(Debug)]
struct PlannedRename {
    source: PathBuf,
    target: PathBuf,
    temporary: PathBuf,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct RenameManifest {
    version: u32,
    created_at: u64,
    mappings: Vec<RenameMapping>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct RenameMapping {
    original_path: String,
    renamed_path: String,
}

pub async fn rename_files(
    app: AppHandle,
    items: Vec<RenameItemRequest>,
    job_id: Option<String>,
) -> Result<ConversionResult, ConversionError> {
    let plan = validate_rename_items(&items)?;
    let active_job = super::jobs::ActiveJobGuard::register(app.clone(), job_id)?;
    let manifest_path = write_manifest(&app, &items, &plan.output_paths)?;
    let cancel_token = active_job.cancel_token();
    let event_job_id = active_job.job_id().to_owned();
    let started = Instant::now();
    let app_for_progress = app.clone();
    let worker_job_id = event_job_id.clone();

    let result = execute_rename_plan(&plan, &cancel_token, |percent| {
        let _ = app_for_progress.emit(
            "conversion-progress",
            ProgressPayload {
                job_id: worker_job_id.clone(),
                percent,
                stage: "Renaming files".into(),
            },
        );
    });
    if let Err(error) = result {
        let _ = fs::remove_file(&manifest_path);
        return Err(error);
    }

    let _ = app.emit(
        "conversion-progress",
        ProgressPayload {
            job_id: event_job_id,
            percent: 100,
            stage: "Complete".into(),
        },
    );
    let output_paths = plan
        .output_paths
        .iter()
        .map(|path| path.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    let output_path = output_paths
        .first()
        .cloned()
        .ok_or(ConversionError::OutputMissing)?;
    Ok(ConversionResult {
        output_path,
        output_paths,
        output_size: plan.total_size,
        duration_ms: started.elapsed().as_millis() as u64,
        undo_manifest: Some(manifest_path.to_string_lossy().into_owned()),
    })
}

#[tauri::command]
pub fn undo_rename(app: AppHandle, manifest_path: String) -> Result<Vec<String>, ConversionError> {
    let manifest_path = managed_manifest_path(&app, &manifest_path)?;
    let metadata = fs::metadata(&manifest_path).map_err(|_| ConversionError::InputNotFound {
        path: manifest_path.to_string_lossy().into_owned(),
    })?;
    if metadata.len() > MAX_MANIFEST_BYTES {
        return Err(invalid_plan("The rename undo record is invalid."));
    }

    let bytes = fs::read(&manifest_path).map_err(|error| {
        process_error(format!("Could not read the rename undo record: {error}"))
    })?;
    let manifest: RenameManifest = serde_json::from_slice(&bytes).map_err(|error| {
        process_error(format!("Could not parse the rename undo record: {error}"))
    })?;
    if manifest.version != MANIFEST_VERSION || manifest.mappings.is_empty() {
        return Err(invalid_plan("The rename undo record is invalid."));
    }

    let items = manifest
        .mappings
        .iter()
        .map(|mapping| {
            let original = Path::new(&mapping.original_path);
            let output_name = original
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(|| invalid_plan("An original filename is no longer valid."))?;
            Ok(RenameItemRequest {
                input_path: mapping.renamed_path.clone(),
                output_name: output_name.into(),
            })
        })
        .collect::<Result<Vec<_>, ConversionError>>()?;
    let plan = validate_rename_items(&items)?;
    execute_rename_plan(&plan, &CancellationToken::new(), |_| {})?;
    fs::remove_file(&manifest_path).map_err(|error| {
        process_error(format!("Could not remove the used undo record: {error}"))
    })?;

    Ok(plan
        .output_paths
        .iter()
        .map(|path| path.to_string_lossy().into_owned())
        .collect())
}

pub(crate) fn cleanup_managed_rename_manifests(app: &AppHandle, max_age: Duration) {
    if let Ok(directory) = manifest_directory(app) {
        cleanup_manifest_directory(&directory, max_age);
    }
}

fn cleanup_manifest_directory(directory: &Path, max_age: Duration) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    let now = SystemTime::now();

    for entry in entries.flatten() {
        let path = entry.path();
        let extension = path.extension().and_then(|value| value.to_str());
        if !matches!(extension, Some("json" | "tmp")) {
            continue;
        }
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if !metadata.is_file() {
            continue;
        }
        let expired = max_age.is_zero()
            || metadata
                .modified()
                .ok()
                .and_then(|modified| now.duration_since(modified).ok())
                .is_some_and(|age| age >= max_age);
        if expired {
            let _ = fs::remove_file(path);
        }
    }
}

pub(super) fn validate_rename_items(
    items: &[RenameItemRequest],
) -> Result<RenamePlan, ConversionError> {
    if items.is_empty() || items.len() > MAX_RENAME_ITEMS {
        return Err(invalid_plan("Add between 1 and 100 files to rename."));
    }

    let mut sources = HashSet::with_capacity(items.len());
    let mut source_sizes = Vec::with_capacity(items.len());
    for item in items {
        let source = PathBuf::from(&item.input_path);
        let metadata =
            fs::symlink_metadata(&source).map_err(|_| ConversionError::InputNotFound {
                path: item.input_path.clone(),
            })?;
        if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
            return Err(invalid_plan("Batch rename accepts regular files only."));
        }
        let canonical = source
            .canonicalize()
            .map_err(|_| ConversionError::InputNotFound {
                path: item.input_path.clone(),
            })?;
        if !sources.insert(canonical) {
            return Err(invalid_plan("Remove duplicate files before renaming."));
        }
        source_sizes.push(metadata.len());
    }

    let mut targets = HashSet::with_capacity(items.len());
    let mut planned = Vec::with_capacity(items.len());
    let mut output_paths = Vec::with_capacity(items.len());
    for item in items {
        validate_output_name(&item.output_name)?;
        let source = PathBuf::from(&item.input_path);
        let parent = source
            .parent()
            .ok_or_else(|| invalid_plan("A source file has no containing folder."))?
            .to_path_buf();
        let target = parent.join(&item.output_name);
        validate_preserved_extension(&source, &target)?;

        let canonical_parent =
            parent
                .canonicalize()
                .map_err(|_| ConversionError::InputNotFound {
                    path: parent.to_string_lossy().into_owned(),
                })?;
        let target_key = canonical_parent
            .join(&item.output_name)
            .to_string_lossy()
            .to_lowercase();
        if !targets.insert(target_key) {
            return Err(ConversionError::OutputConflict {
                path: target.to_string_lossy().into_owned(),
                message: "Two files would receive the same name.".into(),
            });
        }

        if target.exists() {
            let existing = target
                .canonicalize()
                .map_err(|_| ConversionError::OutputConflict {
                    path: target.to_string_lossy().into_owned(),
                    message: "The destination could not be inspected.".into(),
                })?;
            if !sources.contains(&existing) {
                return Err(ConversionError::OutputConflict {
                    path: target.to_string_lossy().into_owned(),
                    message: "A file with this name already exists.".into(),
                });
            }
        }

        let is_changed = source.file_name() != target.file_name();
        if is_changed {
            planned.push(PlannedRename {
                source,
                target: target.clone(),
                temporary: unique_temporary_path(&parent),
            });
        }
        output_paths.push(target);
    }

    if planned.is_empty() {
        return Err(invalid_plan("Make at least one filename change."));
    }

    Ok(RenamePlan {
        items: planned,
        output_paths,
        total_size: source_sizes.into_iter().sum(),
    })
}

fn validate_output_name(name: &str) -> Result<(), ConversionError> {
    let path = Path::new(name);
    let is_single_component = path
        .parent()
        .is_some_and(|parent| parent.as_os_str().is_empty())
        && path.file_name().and_then(|value| value.to_str()) == Some(name);
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.len() > 255
        || name
            .chars()
            .any(|character| matches!(character, '/' | ':' | '\0'))
        || !is_single_component
    {
        return Err(invalid_plan("One of the new filenames is invalid."));
    }
    Ok(())
}

fn validate_preserved_extension(source: &Path, target: &Path) -> Result<(), ConversionError> {
    let extension = |path: &Path| {
        path.extension()
            .and_then(|value| value.to_str())
            .map(str::to_lowercase)
    };
    if extension(source) != extension(target) {
        return Err(invalid_plan(
            "Batch rename preserves file extensions. Change formats with Convert.",
        ));
    }
    Ok(())
}

fn unique_temporary_path(parent: &Path) -> PathBuf {
    loop {
        let candidate = parent.join(format!(".convertkit-rename-{}", Uuid::new_v4()));
        if !candidate.exists() {
            return candidate;
        }
    }
}

fn execute_rename_plan(
    plan: &RenamePlan,
    cancel_token: &CancellationToken,
    mut progress: impl FnMut(i32),
) -> Result<(), ConversionError> {
    progress(0);
    let mut staged = 0usize;
    for (index, item) in plan.items.iter().enumerate() {
        if cancel_token.is_cancelled() {
            rollback_transaction(&plan.items, 0, staged)?;
            return Err(ConversionError::Cancelled);
        }
        if let Err(error) = fs::rename(&item.source, &item.temporary) {
            rollback_transaction(&plan.items, 0, staged)?;
            return Err(process_error(format!(
                "Could not stage {} for renaming: {error}",
                item.source.display()
            )));
        }
        staged = index + 1;
        progress(((staged * 20) / plan.items.len()) as i32);
    }

    let mut committed = 0usize;
    for (index, item) in plan.items.iter().enumerate() {
        if cancel_token.is_cancelled() {
            rollback_transaction(&plan.items, committed, staged)?;
            return Err(ConversionError::Cancelled);
        }
        if let Err(error) = fs::rename(&item.temporary, &item.target) {
            rollback_transaction(&plan.items, committed, staged)?;
            return Err(process_error(format!(
                "Could not rename {}: {error}",
                item.source.display()
            )));
        }
        committed = index + 1;
        progress(20 + ((committed * 80) / plan.items.len()) as i32);
    }
    Ok(())
}

fn rollback_transaction(
    items: &[PlannedRename],
    committed: usize,
    staged: usize,
) -> Result<(), ConversionError> {
    for item in items[..committed].iter().rev() {
        fs::rename(&item.target, &item.temporary).map_err(|error| {
            process_error(format!(
                "Could not roll back {}: {error}",
                item.target.display()
            ))
        })?;
    }
    for item in items[..staged].iter().rev() {
        if item.temporary.exists() {
            fs::rename(&item.temporary, &item.source).map_err(|error| {
                process_error(format!(
                    "Could not restore {} during rollback: {error}",
                    item.source.display()
                ))
            })?;
        }
    }
    Ok(())
}

fn write_manifest(
    app: &AppHandle,
    items: &[RenameItemRequest],
    output_paths: &[PathBuf],
) -> Result<PathBuf, ConversionError> {
    let directory = manifest_directory(app)?;
    fs::create_dir_all(&directory).map_err(|error| {
        process_error(format!("Could not prepare rename undo records: {error}"))
    })?;
    let id = Uuid::new_v4();
    let path = directory.join(format!("{id}.json"));
    let temporary = directory.join(format!(".{id}.tmp"));
    let manifest = RenameManifest {
        version: MANIFEST_VERSION,
        created_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64,
        mappings: items
            .iter()
            .zip(output_paths)
            .map(|(item, renamed)| RenameMapping {
                original_path: item.input_path.clone(),
                renamed_path: renamed.to_string_lossy().into_owned(),
            })
            .collect(),
    };
    let bytes = serde_json::to_vec_pretty(&manifest).map_err(|error| {
        process_error(format!("Could not create the rename undo record: {error}"))
    })?;
    fs::write(&temporary, bytes).map_err(|error| {
        process_error(format!("Could not write the rename undo record: {error}"))
    })?;
    if let Err(error) = fs::rename(&temporary, &path) {
        let _ = fs::remove_file(&temporary);
        return Err(process_error(format!(
            "Could not finalize the rename undo record: {error}"
        )));
    }
    Ok(path)
}

fn managed_manifest_path(app: &AppHandle, requested: &str) -> Result<PathBuf, ConversionError> {
    let directory = manifest_directory(app)?;
    let directory = directory
        .canonicalize()
        .map_err(|_| invalid_plan("The rename undo record is unavailable or no longer exists."))?;
    let path = PathBuf::from(requested)
        .canonicalize()
        .map_err(|_| invalid_plan("The rename undo record is unavailable or no longer exists."))?;
    if !path.starts_with(&directory)
        || path.extension().and_then(|value| value.to_str()) != Some("json")
    {
        return Err(invalid_plan(
            "The rename undo record is not managed by ConvertKit.",
        ));
    }
    Ok(path)
}

fn manifest_directory(app: &AppHandle) -> Result<PathBuf, ConversionError> {
    app.path()
        .app_data_dir()
        .map(|path| path.join("rename-manifests"))
        .map_err(|error| process_error(format!("Could not locate app data: {error}")))
}

fn invalid_plan(message: &str) -> ConversionError {
    ConversionError::UnsupportedConversion {
        input: message.into(),
        output: "batch rename".into(),
    }
}

fn process_error(message: String) -> ConversionError {
    ConversionError::ProcessFailed {
        message,
        stderr: String::new(),
        exit_code: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(path: &Path, output_name: &str) -> RenameItemRequest {
        RenameItemRequest {
            input_path: path.to_string_lossy().into_owned(),
            output_name: output_name.into(),
        }
    }

    #[test]
    fn rejects_invalid_names_extension_changes_and_occupied_targets() {
        let directory = tempfile::tempdir().expect("tempdir");
        let source = directory.path().join("source.txt");
        let occupied = directory.path().join("occupied.txt");
        fs::write(&source, b"source").expect("source");
        fs::write(&occupied, b"occupied").expect("occupied");

        for name in ["", ".", "..", "nested/name.txt", "bad:name.txt"] {
            assert!(validate_rename_items(&[request(&source, name)]).is_err());
        }
        assert!(validate_rename_items(&[request(&source, "source.pdf")]).is_err());
        assert!(validate_rename_items(&[request(&source, "occupied.txt")]).is_err());
        assert!(validate_rename_items(&[request(&source, "source.txt")]).is_err());
    }

    #[test]
    fn rejects_duplicate_targets_case_insensitively() {
        let directory = tempfile::tempdir().expect("tempdir");
        let first = directory.path().join("first.txt");
        let second = directory.path().join("second.txt");
        fs::write(&first, b"first").expect("first");
        fs::write(&second, b"second").expect("second");

        assert!(validate_rename_items(&[
            request(&first, "same.txt"),
            request(&second, "SAME.txt"),
        ])
        .is_err());
    }

    #[test]
    fn stages_all_files_so_name_swaps_are_safe() {
        let directory = tempfile::tempdir().expect("tempdir");
        let first = directory.path().join("first.txt");
        let second = directory.path().join("second.txt");
        fs::write(&first, b"first content").expect("first");
        fs::write(&second, b"second content").expect("second");
        let plan =
            validate_rename_items(&[request(&first, "second.txt"), request(&second, "first.txt")])
                .expect("rename plan");

        execute_rename_plan(&plan, &CancellationToken::new(), |_| {}).expect("rename files");

        assert_eq!(fs::read(&first).expect("first output"), b"second content");
        assert_eq!(fs::read(&second).expect("second output"), b"first content");
    }

    #[test]
    fn cancellation_before_staging_does_not_change_files() {
        let directory = tempfile::tempdir().expect("tempdir");
        let source = directory.path().join("source.txt");
        fs::write(&source, b"source").expect("source");
        let plan = validate_rename_items(&[request(&source, "renamed.txt")]).expect("plan");
        let token = CancellationToken::new();
        token.cancel();

        assert!(matches!(
            execute_rename_plan(&plan, &token, |_| {}),
            Err(ConversionError::Cancelled)
        ));
        assert_eq!(fs::read(&source).expect("source retained"), b"source");
        assert!(!directory.path().join("renamed.txt").exists());
    }

    #[test]
    fn cancellation_during_commit_rolls_back_every_file() {
        let directory = tempfile::tempdir().expect("tempdir");
        let first = directory.path().join("first.txt");
        let second = directory.path().join("second.txt");
        fs::write(&first, b"first content").expect("first");
        fs::write(&second, b"second content").expect("second");
        let first_target = directory.path().join("renamed-first.txt");
        let second_target = directory.path().join("renamed-second.txt");
        let plan = validate_rename_items(&[
            request(&first, "renamed-first.txt"),
            request(&second, "renamed-second.txt"),
        ])
        .expect("plan");
        let token = CancellationToken::new();
        let progress_token = token.clone();

        assert!(matches!(
            execute_rename_plan(&plan, &token, |percent| {
                if percent >= 60 {
                    progress_token.cancel();
                }
            }),
            Err(ConversionError::Cancelled)
        ));
        assert_eq!(fs::read(&first).expect("first restored"), b"first content");
        assert_eq!(
            fs::read(&second).expect("second restored"),
            b"second content"
        );
        assert!(!first_target.exists());
        assert!(!second_target.exists());
    }

    #[test]
    fn cleanup_removes_only_managed_record_files() {
        let directory = tempfile::tempdir().expect("tempdir");
        let manifest = directory.path().join("undo.json");
        let temporary = directory.path().join(".undo.tmp");
        let unrelated = directory.path().join("keep.txt");
        fs::write(&manifest, b"{}").expect("manifest");
        fs::write(&temporary, b"{}").expect("temporary");
        fs::write(&unrelated, b"keep").expect("unrelated");

        cleanup_manifest_directory(directory.path(), Duration::ZERO);

        assert!(!manifest.exists());
        assert!(!temporary.exists());
        assert_eq!(fs::read(unrelated).expect("unrelated retained"), b"keep");
    }
}
