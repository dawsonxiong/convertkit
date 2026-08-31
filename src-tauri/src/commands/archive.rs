use std::collections::{HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Component, Path, PathBuf};
use std::time::Instant;

use flate2::read::{GzDecoder, MultiGzDecoder};
use flate2::write::GzEncoder;
use flate2::Compression;
use serde::{Deserialize, Deserializer};
use sevenz_rust2::{
    encoder_options::AesEncoderOptions, ArchiveEntry, ArchiveReader, ArchiveWriter, EncoderMethod,
    Password,
};
use tauri::{AppHandle, Emitter};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
use zeroize::Zeroize;
use zip::write::SimpleFileOptions;

use crate::engines::ConversionResult;
use crate::error::ConversionError;
use crate::progress::ProgressPayload;

use super::output::{
    atomic_rename_no_replace, prepare_output, prepare_output_with_stem, resolve_writable_directory,
    validate_suffix, OutputOptions,
};

const MAX_ARCHIVE_INPUTS: usize = 100;
const MAX_ARCHIVE_ENTRIES: usize = 10_000;
const MAX_EXTRACTED_BYTES: u64 = 100 * 1024 * 1024 * 1024;
const MAX_7Z_DECODER_MEMORY_BYTES: u64 = 512 * 1024 * 1024;
const MAX_ARCHIVE_PASSWORD_BYTES: usize = 1_024;
const MAX_ARCHIVE_PASSWORD_CHARS: usize = 256;
const BUFFER_SIZE: usize = 128 * 1024;

#[derive(Clone, Copy)]
struct ArchiveProgress<'a> {
    app: &'a AppHandle,
    job_id: &'a str,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ArchiveFormat {
    Zip,
    Tar,
    TarGz,
    SevenZ,
    Gzip,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ExtractArchiveFormat {
    Zip,
    Tar,
    TarGz,
    Gzip,
    SevenZ,
}

impl ExtractArchiveFormat {
    fn label(self) -> &'static str {
        match self {
            Self::Zip => "ZIP",
            Self::Tar => "TAR",
            Self::TarGz => "TAR.GZ",
            Self::Gzip => "GZIP",
            Self::SevenZ => "7Z",
        }
    }
}

impl ArchiveFormat {
    fn output_extension(self, input: &Path) -> String {
        match self {
            Self::Zip => "zip".into(),
            Self::Tar => "tar".into(),
            Self::TarGz => "tar.gz".into(),
            Self::SevenZ => "7z".into(),
            Self::Gzip => input
                .extension()
                .and_then(|value| value.to_str())
                .filter(|value| !value.is_empty())
                .map(|value| format!("{value}.gz"))
                .unwrap_or_else(|| "gz".into()),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Zip => "ZIP",
            Self::Tar => "TAR",
            Self::TarGz => "TAR.GZ",
            Self::SevenZ => "7Z",
            Self::Gzip => "GZIP",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveEntryRequest {
    pub input_path: String,
    pub archive_path: String,
    #[serde(default)]
    pub folder_derived: bool,
}

/// An invocation-scoped archive secret. Debug output is always redacted and
/// the UTF-8 request buffer is wiped when it leaves scope.
pub struct ArchivePassword(String);

impl ArchivePassword {
    fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for ArchivePassword {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("ArchivePassword([REDACTED])")
    }
}

impl Drop for ArchivePassword {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl<'de> Deserialize<'de> for ArchivePassword {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer).map(Self)
    }
}

pub async fn create_archive(
    app: AppHandle,
    entries: Vec<ArchiveEntryRequest>,
    format: ArchiveFormat,
    password: Option<ArchivePassword>,
    job_id: Option<String>,
    output_options: Option<OutputOptions>,
) -> Result<ConversionResult, ConversionError> {
    validate_password_for_creation(format, password.as_ref())?;
    let entries = validate_create_entries_for_format(entries, format)?;
    let first_input = entries
        .first()
        .map(|entry| entry.0.as_path())
        .ok_or_else(|| invalid_archive("Add at least one file."))?;
    let output_extension = format.output_extension(first_input);
    let output = prepare_output(first_input, &output_extension, "-archive", output_options)?;
    let working_path = output.working_path().to_path_buf();
    let active_job = super::jobs::ActiveJobGuard::register(app.clone(), job_id)?;
    let cancel_token = active_job.cancel_token();
    let event_job_id = active_job.job_id().to_owned();
    let started = Instant::now();
    let worker_app = app.clone();
    let worker_job_id = event_job_id.clone();

    let result = tokio::task::spawn_blocking(move || match format {
        ArchiveFormat::Zip => write_zip_archive(
            &working_path,
            &entries,
            Some(ArchiveProgress {
                app: &worker_app,
                job_id: &worker_job_id,
            }),
            &cancel_token,
        ),
        ArchiveFormat::Tar => write_tar_archive(
            &working_path,
            &entries,
            Some(ArchiveProgress {
                app: &worker_app,
                job_id: &worker_job_id,
            }),
            &cancel_token,
        ),
        ArchiveFormat::TarGz => write_tar_gz_archive(
            &working_path,
            &entries,
            Some(ArchiveProgress {
                app: &worker_app,
                job_id: &worker_job_id,
            }),
            &cancel_token,
        ),
        ArchiveFormat::SevenZ => write_7z_archive(
            &working_path,
            &entries,
            password.as_ref(),
            Some(ArchiveProgress {
                app: &worker_app,
                job_id: &worker_job_id,
            }),
            &cancel_token,
        ),
        ArchiveFormat::Gzip => write_gzip_file(
            &working_path,
            &entries[0],
            Some(ArchiveProgress {
                app: &worker_app,
                job_id: &worker_job_id,
            }),
            &cancel_token,
        ),
    })
    .await
    .map_err(|error| {
        process_error(format!(
            "{} creation stopped unexpectedly: {error}",
            format.label()
        ))
    })??;

    let mut result = output.commit(result)?;
    result.duration_ms = started.elapsed().as_millis() as u64;
    let completed_stage = if format == ArchiveFormat::Gzip {
        "GZIP file created".into()
    } else {
        format!("{} archive created", format.label())
    };
    emit_progress(&app, &event_job_id, 100, &completed_stage);
    Ok(result)
}

pub async fn extract_archive(
    app: AppHandle,
    input_path: String,
    password: Option<ArchivePassword>,
    job_id: Option<String>,
    output_options: Option<OutputOptions>,
) -> Result<ConversionResult, ConversionError> {
    let (input, format) = validate_archive_input_with_password(&input_path, password.as_ref())?;
    if format == ExtractArchiveFormat::Gzip {
        return extract_gzip(app, input, job_id, output_options).await;
    }
    let output = PreparedDirectory::new(&input, format, "-extracted", output_options)?;
    let working_path = output.working_path.clone();
    let active_job = super::jobs::ActiveJobGuard::register(app.clone(), job_id)?;
    let cancel_token = active_job.cancel_token();
    let event_job_id = active_job.job_id().to_owned();
    let started = Instant::now();
    let worker_app = app.clone();
    let worker_job_id = event_job_id.clone();
    let input_for_worker = input.clone();

    let extracted_bytes = tokio::task::spawn_blocking(move || match format {
        ExtractArchiveFormat::Zip => extract_zip_archive(
            &input_for_worker,
            &working_path,
            Some(ArchiveProgress {
                app: &worker_app,
                job_id: &worker_job_id,
            }),
            &cancel_token,
        ),
        ExtractArchiveFormat::Tar => extract_tar_archive(
            &input_for_worker,
            &working_path,
            Some(ArchiveProgress {
                app: &worker_app,
                job_id: &worker_job_id,
            }),
            &cancel_token,
        ),
        ExtractArchiveFormat::TarGz => extract_tar_gz_archive(
            &input_for_worker,
            &working_path,
            Some(ArchiveProgress {
                app: &worker_app,
                job_id: &worker_job_id,
            }),
            &cancel_token,
        ),
        ExtractArchiveFormat::Gzip => unreachable!("GZIP uses single-file extraction"),
        ExtractArchiveFormat::SevenZ => extract_7z_archive(
            &input_for_worker,
            &working_path,
            password.as_ref(),
            Some(ArchiveProgress {
                app: &worker_app,
                job_id: &worker_job_id,
            }),
            &cancel_token,
        ),
    })
    .await
    .map_err(|error| {
        process_error(format!(
            "{} extraction stopped unexpectedly: {error}",
            format.label()
        ))
    })??;

    let mut result = output.commit(extracted_bytes)?;
    result.duration_ms = started.elapsed().as_millis() as u64;
    emit_progress(&app, &event_job_id, 100, "Archive extracted");
    Ok(result)
}

pub(super) fn validate_create_entries_for_format(
    entries: Vec<ArchiveEntryRequest>,
    format: ArchiveFormat,
) -> Result<Vec<(PathBuf, String, u64, u32)>, ConversionError> {
    if format == ArchiveFormat::Gzip {
        if entries.len() != 1 {
            return Err(invalid_archive(
                "GZIP compresses exactly one regular file at a time.",
            ));
        }
        let entry = &entries[0];
        if entry.folder_derived
            || Path::new(&entry.archive_path)
                .components()
                .filter(|component| matches!(component, Component::Normal(_)))
                .count()
                != 1
        {
            return Err(invalid_archive(
                "GZIP cannot compress a folder. Choose one regular file directly.",
            ));
        }
    }
    validate_create_entries(entries)
}

pub(super) fn validate_create_entries(
    entries: Vec<ArchiveEntryRequest>,
) -> Result<Vec<(PathBuf, String, u64, u32)>, ConversionError> {
    if entries.is_empty() || entries.len() > MAX_ARCHIVE_INPUTS {
        return Err(invalid_archive("Add between 1 and 100 files."));
    }

    let mut seen_inputs = HashSet::new();
    let mut used_names = HashSet::new();
    let mut validated = Vec::with_capacity(entries.len());
    for entry in entries {
        let input = PathBuf::from(&entry.input_path);
        let link_metadata = fs::symlink_metadata(&input).map_err(|error| {
            if error.kind() == io::ErrorKind::NotFound {
                ConversionError::InputNotFound {
                    path: entry.input_path.clone(),
                }
            } else {
                process_error(format!("Could not inspect {}: {error}", input.display()))
            }
        })?;
        if link_metadata.file_type().is_symlink() {
            return Err(invalid_archive(
                "Symbolic links cannot be added to an archive.",
            ));
        }
        if !link_metadata.file_type().is_file() {
            return Err(invalid_archive(
                "Only regular files can be added to an archive.",
            ));
        }
        let normalized = input.canonicalize().unwrap_or_else(|_| input.clone());
        if !seen_inputs.insert(normalized) {
            return Err(invalid_archive(
                "Remove duplicate files before creating the archive.",
            ));
        }

        let requested_name = sanitize_archive_path(&entry.archive_path)
            .ok_or_else(|| invalid_archive("One of the archive paths is invalid or unsafe."))?;
        let archive_name = unique_archive_path(&requested_name, &mut used_names);
        let metadata = input.metadata().map_err(|error| {
            process_error(format!("Could not read {}: {error}", input.display()))
        })?;
        #[cfg(unix)]
        let permissions = metadata.permissions().mode() & 0o777;
        #[cfg(not(unix))]
        let permissions = 0o644;
        validated.push((input, archive_name, metadata.len(), permissions));
    }
    Ok(validated)
}

#[cfg(test)]
pub(super) fn validate_archive_input(
    input_path: &str,
) -> Result<(PathBuf, ExtractArchiveFormat), ConversionError> {
    validate_archive_input_with_password(input_path, None)
}

pub(super) fn validate_archive_input_with_password(
    input_path: &str,
    password: Option<&ArchivePassword>,
) -> Result<(PathBuf, ExtractArchiveFormat), ConversionError> {
    let (input, format) = resolve_archive_input(input_path)?;
    validate_password_for_extraction(format, password)?;
    match format {
        ExtractArchiveFormat::Zip => {
            let file = File::open(&input)
                .map_err(|error| process_error(format!("Could not open the ZIP file: {error}")))?;
            zip::ZipArchive::new(file).map_err(|error| {
                process_error(format!("This ZIP file is invalid or damaged: {error}"))
            })?;
        }
        ExtractArchiveFormat::Tar => validate_tar_file(&input)?,
        ExtractArchiveFormat::TarGz => validate_tar_gz_file(&input)?,
        ExtractArchiveFormat::Gzip => validate_gzip_file(&input)?,
        ExtractArchiveFormat::SevenZ => validate_7z_file(&input, password)?,
    }
    Ok((input, format))
}

fn resolve_archive_input(
    input_path: &str,
) -> Result<(PathBuf, ExtractArchiveFormat), ConversionError> {
    let input = PathBuf::from(input_path);
    if !input.is_file() {
        return Err(ConversionError::InputNotFound {
            path: input_path.into(),
        });
    }
    let format = archive_format_for_path(&input).ok_or_else(|| {
        invalid_archive("Archive extraction supports ZIP, TAR, TAR.GZ, TGZ, GZIP, and 7Z files.")
    })?;
    Ok((input, format))
}

pub(super) fn is_supported_archive_path(path: &Path) -> bool {
    archive_format_for_path(path).is_some()
}

fn archive_format_for_path(path: &Path) -> Option<ExtractArchiveFormat> {
    let name = path.file_name()?.to_str()?.to_ascii_lowercase();
    if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
        return Some(ExtractArchiveFormat::TarGz);
    }
    match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "zip" => Some(ExtractArchiveFormat::Zip),
        "tar" => Some(ExtractArchiveFormat::Tar),
        "gz" => Some(ExtractArchiveFormat::Gzip),
        "7z" => Some(ExtractArchiveFormat::SevenZ),
        _ => None,
    }
}

fn validate_gzip_file(input: &Path) -> Result<(), ConversionError> {
    let file = File::open(input)
        .map_err(|error| process_error(format!("Could not open the GZIP file: {error}")))?;
    let mut decoder = MultiGzDecoder::new(file);
    if decoder.header().is_none() {
        return Err(invalid_archive("This GZIP file is invalid or damaged."));
    }
    let mut first_byte = [0u8; 1];
    decoder
        .read(&mut first_byte)
        .map_err(|error| process_error(format!("This GZIP file is invalid or damaged: {error}")))?;
    Ok(())
}

fn gzip_output_name(input: &Path) -> Result<(String, String), ConversionError> {
    let file_name = input
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| invalid_archive("This GZIP filename is unsupported."))?;
    if !file_name.to_ascii_lowercase().ends_with(".gz") {
        return Err(invalid_archive("This GZIP filename is unsupported."));
    }
    let inner_name = &file_name[..file_name.len() - 3];
    if inner_name.is_empty() {
        return Err(invalid_archive("This GZIP filename has no output name."));
    }
    let inner_path = Path::new(inner_name);
    let stem = inner_path
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or(inner_name)
        .to_owned();
    let extension = inner_path
        .extension()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or_default()
        .to_owned();
    Ok((stem, extension))
}

async fn extract_gzip(
    app: AppHandle,
    input: PathBuf,
    job_id: Option<String>,
    output_options: Option<OutputOptions>,
) -> Result<ConversionResult, ConversionError> {
    let (stem, extension) = gzip_output_name(&input)?;
    let output = prepare_output_with_stem(&input, &stem, &extension, "-extracted", output_options)?;
    let working_path = output.working_path().to_path_buf();
    let active_job = super::jobs::ActiveJobGuard::register(app.clone(), job_id)?;
    let cancel_token = active_job.cancel_token();
    let event_job_id = active_job.job_id().to_owned();
    let started = Instant::now();
    let worker_app = app.clone();
    let worker_job_id = event_job_id.clone();

    let output_size = tokio::task::spawn_blocking(move || {
        extract_gzip_file_with_limit(
            &input,
            &working_path,
            Some(ArchiveProgress {
                app: &worker_app,
                job_id: &worker_job_id,
            }),
            &cancel_token,
            MAX_EXTRACTED_BYTES,
        )
    })
    .await
    .map_err(|error| process_error(format!("GZIP extraction stopped unexpectedly: {error}")))??;

    let mut result = output.commit_regular_file(ConversionResult {
        output_path: String::new(),
        output_paths: Vec::new(),
        output_size,
        duration_ms: 0,
        undo_manifest: None,
    })?;
    result.duration_ms = started.elapsed().as_millis() as u64;
    emit_progress(&app, &event_job_id, 100, "GZIP decompressed");
    Ok(result)
}

fn extract_gzip_file_with_limit(
    input: &Path,
    output: &Path,
    progress: Option<ArchiveProgress<'_>>,
    cancel_token: &CancellationToken,
    max_extracted_bytes: u64,
) -> Result<u64, ConversionError> {
    check_cancelled(cancel_token)?;
    let file = File::open(input)
        .map_err(|error| process_error(format!("Could not open the GZIP file: {error}")))?;
    let mut decoder = MultiGzDecoder::new(file);
    if decoder.header().is_none() {
        return Err(invalid_archive("This GZIP file is invalid or damaged."));
    }
    let mut destination = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)
        .map_err(|error| {
            process_error(format!("Could not create the decompressed file: {error}"))
        })?;
    let mut extracted_bytes = 0u64;
    let mut buffer = vec![0u8; BUFFER_SIZE];
    emit_worker_progress(progress, -1, "Decompressing GZIP");

    loop {
        check_cancelled(cancel_token)?;
        let read = decoder.read(&mut buffer).map_err(|error| {
            process_error(format!("This GZIP file is invalid or damaged: {error}"))
        })?;
        if read == 0 {
            break;
        }
        extracted_bytes = extracted_bytes
            .checked_add(read as u64)
            .filter(|size| *size <= max_extracted_bytes)
            .ok_or_else(|| {
                invalid_archive("GZIP decompression exceeded the 100 GB safety limit.")
            })?;
        destination.write_all(&buffer[..read]).map_err(|error| {
            process_error(format!("Could not write the decompressed file: {error}"))
        })?;
    }
    check_cancelled(cancel_token)?;
    destination.sync_all().map_err(|error| {
        process_error(format!("Could not finish the decompressed file: {error}"))
    })?;
    Ok(extracted_bytes)
}

fn validate_tar_file(input: &Path) -> Result<(), ConversionError> {
    let file = File::open(input)
        .map_err(|error| process_error(format!("Could not open the TAR file: {error}")))?;
    if file
        .metadata()
        .map_err(|error| process_error(format!("Could not inspect the TAR file: {error}")))?
        .len()
        < 1_024
    {
        return Err(process_error(
            "This TAR file is invalid or damaged: the archive is incomplete".into(),
        ));
    }
    validate_tar_reader(file, "TAR")
}

fn validate_tar_gz_file(input: &Path) -> Result<(), ConversionError> {
    let file = File::open(input)
        .map_err(|error| process_error(format!("Could not open the TAR.GZ file: {error}")))?;
    let mut decoder = GzDecoder::new(file);
    let mut prefix = [0u8; 1_024];
    decoder.read_exact(&mut prefix).map_err(|error| {
        process_error(format!(
            "This TAR.GZ file is invalid or damaged: the archive is incomplete ({error})"
        ))
    })?;

    let file = File::open(input)
        .map_err(|error| process_error(format!("Could not open the TAR.GZ file: {error}")))?;
    validate_tar_reader(GzDecoder::new(file), "TAR.GZ")
}

fn validate_tar_reader<R: Read>(reader: R, label: &str) -> Result<(), ConversionError> {
    let mut archive = tar::Archive::new(reader);
    let mut entries = archive.entries().map_err(|error| {
        process_error(format!("This {label} file is invalid or damaged: {error}"))
    })?;
    if let Some(entry) = entries.next() {
        entry.map_err(|error| {
            process_error(format!("This {label} file is invalid or damaged: {error}"))
        })?;
    }
    Ok(())
}

#[derive(Debug, Clone)]
struct SevenZEntryPlan {
    relative: String,
    normalized: String,
    is_directory: bool,
    size: u64,
}

#[derive(Debug)]
struct SevenZExtractionPlan {
    entries: Vec<SevenZEntryPlan>,
    total_bytes: u64,
}

fn validate_password_value(password: &ArchivePassword) -> Result<(), ConversionError> {
    let value = password.as_str();
    if value.is_empty() {
        return Err(invalid_archive("Enter an archive password."));
    }
    if value.len() > MAX_ARCHIVE_PASSWORD_BYTES
        || value.chars().count() > MAX_ARCHIVE_PASSWORD_CHARS
    {
        return Err(invalid_archive(
            "Archive passwords can contain up to 256 characters or 1,024 UTF-8 bytes.",
        ));
    }
    if value.contains('\0') {
        return Err(invalid_archive(
            "Archive passwords cannot contain a null character.",
        ));
    }
    Ok(())
}

pub(super) fn validate_password_for_creation(
    format: ArchiveFormat,
    password: Option<&ArchivePassword>,
) -> Result<(), ConversionError> {
    if let Some(password) = password {
        if format != ArchiveFormat::SevenZ {
            return Err(invalid_archive(
                "Password protection is available only for 7Z archives.",
            ));
        }
        validate_password_value(password)?;
    }
    Ok(())
}

pub(super) fn validate_password_for_extraction(
    format: ExtractArchiveFormat,
    password: Option<&ArchivePassword>,
) -> Result<(), ConversionError> {
    if let Some(password) = password {
        if format != ExtractArchiveFormat::SevenZ {
            return Err(invalid_archive(
                "Passwords are accepted only when extracting a 7Z archive.",
            ));
        }
        validate_password_value(password)?;
    }
    Ok(())
}

fn seven_z_password(password: Option<&ArchivePassword>) -> Password {
    password
        .map(|value| Password::from(value.as_str()))
        .unwrap_or_else(Password::empty)
}

fn validate_7z_file(
    input: &Path,
    password: Option<&ArchivePassword>,
) -> Result<(), ConversionError> {
    let reader = open_7z_reader(input, password)?;
    validate_7z_archive(
        reader.archive(),
        MAX_ARCHIVE_ENTRIES,
        MAX_EXTRACTED_BYTES,
        MAX_7Z_DECODER_MEMORY_BYTES,
    )?;
    Ok(())
}

fn open_7z_reader(
    input: &Path,
    password: Option<&ArchivePassword>,
) -> Result<ArchiveReader<File>, ConversionError> {
    let file = File::open(input)
        .map_err(|error| process_error(format!("Could not open the 7Z file: {error}")))?;
    ArchiveReader::new(file, seven_z_password(password))
        .map_err(|error| map_7z_error(error, password.is_some()))
}

fn map_7z_error(error: sevenz_rust2::Error, password_supplied: bool) -> ConversionError {
    match error {
        sevenz_rust2::Error::PasswordRequired if !password_supplied => {
            ConversionError::ArchivePasswordRequired
        }
        sevenz_rust2::Error::PasswordRequired | sevenz_rust2::Error::MaybeBadPassword(_)
            if password_supplied =>
        {
            ConversionError::IncorrectArchivePassword
        }
        sevenz_rust2::Error::MaybeBadPassword(_) => ConversionError::ArchivePasswordRequired,
        sevenz_rust2::Error::UnsupportedCompressionMethod(method) => invalid_archive(&format!(
            "This 7Z archive uses an unsupported compression method ({method})."
        )),
        error => process_error(format!("This 7Z file is invalid or damaged: {error}")),
    }
}

fn validate_7z_archive(
    archive: &sevenz_rust2::Archive,
    max_entries: usize,
    max_extracted_bytes: u64,
    max_decoder_memory_bytes: u64,
) -> Result<SevenZExtractionPlan, ConversionError> {
    if archive.files.len() > max_entries {
        return Err(invalid_archive(
            "This 7Z archive contains too many entries to extract safely.",
        ));
    }
    validate_7z_decoder_memory(archive, max_decoder_memory_bytes)?;

    let mut total_bytes = 0u64;
    let mut entries = Vec::with_capacity(archive.files.len());
    let mut paths = HashMap::with_capacity(archive.files.len());
    for entry in &archive.files {
        if entry.is_anti_item() {
            return Err(invalid_archive(
                "7Z anti-items are not extracted for safety.",
            ));
        }
        validate_7z_entry_type(entry)?;
        let normalized_separators = entry.name().replace('\\', "/");
        if entry.is_directory() && matches!(normalized_separators.as_str(), "." | "./") {
            continue;
        }
        let relative = sanitize_archive_path(&normalized_separators).ok_or_else(|| {
            invalid_archive("The 7Z archive contains an unsafe path and was not extracted.")
        })?;
        let normalized = relative.to_lowercase();
        if paths
            .insert(normalized.clone(), entry.is_directory())
            .is_some()
        {
            return Err(invalid_archive(
                "The 7Z archive contains duplicate paths and was not extracted.",
            ));
        }
        if !entry.is_directory() {
            total_bytes = total_bytes
                .checked_add(entry.size())
                .filter(|size| *size <= max_extracted_bytes)
                .ok_or_else(|| {
                    invalid_archive("The extracted archive would exceed the 100 GB limit.")
                })?;
        }
        entries.push(SevenZEntryPlan {
            relative,
            normalized,
            is_directory: entry.is_directory(),
            size: entry.size(),
        });
    }

    for entry in &entries {
        let mut ancestor = entry.normalized.as_str();
        while let Some(index) = ancestor.rfind('/') {
            ancestor = &ancestor[..index];
            if paths
                .get(ancestor)
                .is_some_and(|is_directory| !is_directory)
            {
                return Err(invalid_archive(
                    "The 7Z archive contains a file whose path is also used as a folder.",
                ));
            }
        }
    }

    Ok(SevenZExtractionPlan {
        entries,
        total_bytes,
    })
}

fn validate_7z_entry_type(entry: &ArchiveEntry) -> Result<(), ConversionError> {
    if entry.is_directory() && (entry.has_stream() || entry.size() != 0) {
        return Err(invalid_archive(
            "The 7Z archive contains a malformed directory entry.",
        ));
    }
    if !entry.is_directory() && !entry.has_stream() && entry.size() != 0 {
        return Err(invalid_archive(
            "The 7Z archive contains a malformed file entry.",
        ));
    }
    if entry.has_windows_attributes && entry.windows_attributes() & 0x400 != 0 {
        return Err(invalid_archive(
            "7Z links and reparse points are not extracted for safety.",
        ));
    }
    if entry.has_windows_attributes {
        let unix_kind = (entry.windows_attributes() >> 16) & 0o170000;
        let expected_kind = if entry.is_directory() {
            0o040000
        } else {
            0o100000
        };
        if unix_kind != 0 && unix_kind != expected_kind {
            return Err(invalid_archive(
                "7Z links and special files are not extracted for safety.",
            ));
        }
    }
    Ok(())
}

fn validate_7z_decoder_memory(
    archive: &sevenz_rust2::Archive,
    max_decoder_memory_bytes: u64,
) -> Result<(), ConversionError> {
    for block in &archive.blocks {
        let mut block_memory = 0u64;
        for coder in &block.coders {
            let method = coder.encoder_method_id();
            if !is_supported_7z_method(method) {
                let label = EncoderMethod::by_id(method)
                    .map(|value| value.name().to_owned())
                    .unwrap_or_else(|| format!("{:02x?}", method));
                return Err(invalid_archive(&format!(
                    "This 7Z archive uses an unsupported compression method ({label})."
                )));
            }
            block_memory = block_memory
                .checked_add(seven_z_coder_memory_bytes(method, coder.properties())?)
                .ok_or_else(|| invalid_archive("The 7Z decoder memory requirement is invalid."))?;
            if block_memory > max_decoder_memory_bytes {
                let limit_mb = max_decoder_memory_bytes / (1024 * 1024);
                return Err(invalid_archive(&format!(
                    "This 7Z archive requires more than {limit_mb} MB of decoder memory."
                )));
            }
        }
    }
    Ok(())
}

fn is_supported_7z_method(method: &[u8]) -> bool {
    matches!(
        method,
        EncoderMethod::ID_AES256_SHA256
            | EncoderMethod::ID_COPY
            | EncoderMethod::ID_LZMA
            | EncoderMethod::ID_LZMA2
            | EncoderMethod::ID_PPMD
            | EncoderMethod::ID_BZIP2
            | EncoderMethod::ID_BCJ_X86
            | EncoderMethod::ID_BCJ_PPC
            | EncoderMethod::ID_BCJ_IA64
            | EncoderMethod::ID_BCJ_ARM
            | EncoderMethod::ID_BCJ_ARM64
            | EncoderMethod::ID_BCJ_ARM_THUMB
            | EncoderMethod::ID_BCJ_SPARC
            | EncoderMethod::ID_BCJ_RISCV
            | EncoderMethod::ID_DELTA
            | EncoderMethod::ID_BCJ2
    )
}

fn seven_z_coder_memory_bytes(method: &[u8], properties: &[u8]) -> Result<u64, ConversionError> {
    if method == EncoderMethod::ID_LZMA {
        let bytes = properties.get(1..5).ok_or_else(|| {
            invalid_archive("The 7Z archive has invalid LZMA decoder properties.")
        })?;
        return Ok(u32::from_le_bytes(bytes.try_into().expect("four bytes")) as u64);
    }
    if method == EncoderMethod::ID_LZMA2 {
        let property = *properties.first().ok_or_else(|| {
            invalid_archive("The 7Z archive has invalid LZMA2 decoder properties.")
        })? as u32;
        if property > 40 {
            return Err(invalid_archive(
                "The 7Z archive has invalid LZMA2 decoder properties.",
            ));
        }
        if property == 40 {
            return Ok(u32::MAX as u64);
        }
        return Ok((2u64 | u64::from(property & 1)) << (property / 2 + 11));
    }
    if method == EncoderMethod::ID_PPMD {
        let bytes = properties.get(1..5).ok_or_else(|| {
            invalid_archive("The 7Z archive has invalid PPMd decoder properties.")
        })?;
        return Ok(u32::from_le_bytes(bytes.try_into().expect("four bytes")) as u64);
    }
    Ok(0)
}

fn write_zip_archive(
    output: &Path,
    entries: &[(PathBuf, String, u64, u32)],
    progress: Option<ArchiveProgress<'_>>,
    cancel_token: &CancellationToken,
) -> Result<ConversionResult, ConversionError> {
    let file = File::create(output)
        .map_err(|error| process_error(format!("Could not create the ZIP file: {error}")))?;
    let mut writer = zip::ZipWriter::new(file);
    let total_bytes = entries.iter().map(|entry| entry.2).sum::<u64>().max(1);
    let mut completed_bytes = 0u64;
    let mut buffer = vec![0u8; BUFFER_SIZE];

    for (index, (path, archive_name, _, permissions)) in entries.iter().enumerate() {
        check_cancelled(cancel_token)?;
        emit_worker_progress(
            progress,
            progress_percent(completed_bytes, total_bytes),
            &format!("Adding {} of {}", index + 1, entries.len()),
        );
        writer
            .start_file(
                archive_name,
                SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Deflated)
                    .unix_permissions(*permissions),
            )
            .map_err(|error| process_error(format!("Could not add {archive_name}: {error}")))?;
        let mut input = File::open(path).map_err(|error| {
            process_error(format!("Could not read {}: {error}", path.display()))
        })?;
        loop {
            check_cancelled(cancel_token)?;
            let read = input.read(&mut buffer).map_err(|error| {
                process_error(format!("Could not read {}: {error}", path.display()))
            })?;
            if read == 0 {
                break;
            }
            writer
                .write_all(&buffer[..read])
                .map_err(|error| process_error(format!("Could not write the ZIP file: {error}")))?;
            completed_bytes = completed_bytes.saturating_add(read as u64);
        }
    }
    writer
        .finish()
        .map_err(|error| process_error(format!("Could not finish the ZIP file: {error}")))?;

    Ok(ConversionResult {
        output_path: String::new(),
        output_paths: Vec::new(),
        output_size: 0,
        duration_ms: 0,
        undo_manifest: None,
    })
}

fn write_tar_archive(
    output: &Path,
    entries: &[(PathBuf, String, u64, u32)],
    progress: Option<ArchiveProgress<'_>>,
    cancel_token: &CancellationToken,
) -> Result<ConversionResult, ConversionError> {
    let file = File::create(output)
        .map_err(|error| process_error(format!("Could not create the TAR file: {error}")))?;
    let file = write_tar_entries(file, entries, progress, cancel_token, "TAR")?;
    file.sync_all()
        .map_err(|error| process_error(format!("Could not finish the TAR file: {error}")))?;
    Ok(empty_conversion_result())
}

fn write_tar_gz_archive(
    output: &Path,
    entries: &[(PathBuf, String, u64, u32)],
    progress: Option<ArchiveProgress<'_>>,
    cancel_token: &CancellationToken,
) -> Result<ConversionResult, ConversionError> {
    let file = File::create(output)
        .map_err(|error| process_error(format!("Could not create the TAR.GZ file: {error}")))?;
    let encoder = GzEncoder::new(file, Compression::default());
    let encoder = write_tar_entries(encoder, entries, progress, cancel_token, "TAR.GZ")?;
    let file = encoder
        .finish()
        .map_err(|error| process_error(format!("Could not finish the TAR.GZ file: {error}")))?;
    file.sync_all()
        .map_err(|error| process_error(format!("Could not finish the TAR.GZ file: {error}")))?;
    Ok(empty_conversion_result())
}

fn write_7z_archive(
    output: &Path,
    entries: &[(PathBuf, String, u64, u32)],
    password: Option<&ArchivePassword>,
    progress: Option<ArchiveProgress<'_>>,
    cancel_token: &CancellationToken,
) -> Result<ConversionResult, ConversionError> {
    check_cancelled(cancel_token)?;
    let file = File::create(output)
        .map_err(|error| process_error(format!("Could not create the 7Z file: {error}")))?;
    let mut writer = ArchiveWriter::new(file)
        .map_err(|error| process_error(format!("Could not create the 7Z file: {error}")))?;
    if let Some(password) = password {
        writer.set_content_methods(vec![
            AesEncoderOptions::new(seven_z_password(Some(password))).into(),
            EncoderMethod::LZMA2.into(),
        ]);
        writer.set_encrypt_header(true);
    } else {
        writer.set_encrypt_header(false);
    }

    let total_bytes = entries.iter().map(|entry| entry.2).sum::<u64>().max(1);
    let mut completed_bytes = 0u64;
    for (index, (path, archive_name, _, _)) in entries.iter().enumerate() {
        check_cancelled(cancel_token)?;
        emit_worker_progress(
            progress,
            progress_percent(completed_bytes, total_bytes),
            &format!("Adding {} of {}", index + 1, entries.len()),
        );
        let input = File::open(path).map_err(|error| {
            process_error(format!("Could not read {}: {error}", path.display()))
        })?;
        let last_percent = progress_percent(completed_bytes, total_bytes);
        let reader = CancellableProgressReader {
            input,
            cancel_token,
            progress,
            completed_bytes: &mut completed_bytes,
            total_bytes,
            last_percent,
            stage: "7Z",
        };
        if let Err(error) =
            writer.push_archive_entry(ArchiveEntry::new_file(archive_name), Some(reader))
        {
            if cancel_token.is_cancelled() {
                return Err(ConversionError::Cancelled);
            }
            return Err(process_error(format!(
                "Could not add {archive_name} to the 7Z file: {error}"
            )));
        }
    }

    check_cancelled(cancel_token)?;
    let file = writer
        .finish()
        .map_err(|error| process_error(format!("Could not finish the 7Z file: {error}")))?;
    file.sync_all()
        .map_err(|error| process_error(format!("Could not finish the 7Z file: {error}")))?;
    drop(file);

    emit_worker_progress(progress, 96, "Verifying 7Z");
    verify_7z_matches_sources(output, entries, password, cancel_token)?;
    Ok(empty_conversion_result())
}

fn verify_7z_matches_sources(
    archive_path: &Path,
    entries: &[(PathBuf, String, u64, u32)],
    password: Option<&ArchivePassword>,
    cancel_token: &CancellationToken,
) -> Result<(), ConversionError> {
    check_cancelled(cancel_token)?;
    let reader = open_7z_reader(archive_path, password)?;
    let plan = validate_7z_archive(
        reader.archive(),
        MAX_ARCHIVE_ENTRIES,
        MAX_EXTRACTED_BYTES,
        MAX_7Z_DECODER_MEMORY_BYTES,
    )?;
    if plan.entries.len() != entries.len() || plan.entries.iter().any(|entry| entry.is_directory) {
        return Err(process_error(
            "7Z verification failed because the archive manifest differs from the selected files."
                .into(),
        ));
    }

    let expected = entries
        .iter()
        .map(|(_, archive_name, size, _)| (archive_name.as_str(), *size))
        .collect::<HashMap<_, _>>();
    for entry in &plan.entries {
        if expected.get(entry.relative.as_str()) != Some(&entry.size) {
            return Err(process_error(
                "7Z verification failed because the archive manifest differs from the selected files."
                    .into(),
            ));
        }
    }

    let verification_root = tempfile::tempdir()
        .map_err(|error| process_error(format!("Could not prepare 7Z verification: {error}")))?;
    let extracted_root = verification_root.path().join("contents");
    extract_7z_archive(archive_path, &extracted_root, password, None, cancel_token)?;
    for (source, archive_name, expected_size, _) in entries {
        check_cancelled(cancel_token)?;
        let extracted = extracted_root.join(archive_name);
        let metadata = fs::symlink_metadata(&extracted).map_err(|error| {
            process_error(format!("Could not verify {}: {error}", extracted.display()))
        })?;
        if !metadata.is_file()
            || metadata.file_type().is_symlink()
            || metadata.len() != *expected_size
        {
            return Err(process_error(
                "7Z verification failed because an archived file differs from its source.".into(),
            ));
        }
        verify_file_bytes_match(source, &extracted, cancel_token)?;
    }
    Ok(())
}

fn verify_file_bytes_match(
    source: &Path,
    extracted: &Path,
    cancel_token: &CancellationToken,
) -> Result<(), ConversionError> {
    let mut source_file = File::open(source).map_err(|error| {
        process_error(format!("Could not verify {}: {error}", source.display()))
    })?;
    let mut extracted_file = File::open(extracted).map_err(|error| {
        process_error(format!("Could not verify {}: {error}", extracted.display()))
    })?;
    let mut source_buffer = vec![0u8; BUFFER_SIZE];
    let mut extracted_buffer = vec![0u8; BUFFER_SIZE];
    loop {
        check_cancelled(cancel_token)?;
        let source_read =
            read_full_chunk(&mut source_file, &mut source_buffer).map_err(|error| {
                process_error(format!("Could not verify {}: {error}", source.display()))
            })?;
        let extracted_read =
            read_full_chunk(&mut extracted_file, &mut extracted_buffer).map_err(|error| {
                process_error(format!("Could not verify {}: {error}", extracted.display()))
            })?;
        if source_read != extracted_read
            || source_buffer[..source_read] != extracted_buffer[..extracted_read]
        {
            return Err(process_error(
                "7Z verification failed because an archived file differs from its source.".into(),
            ));
        }
        if source_read == 0 {
            return Ok(());
        }
    }
}

fn write_gzip_file(
    output: &Path,
    entry: &(PathBuf, String, u64, u32),
    progress: Option<ArchiveProgress<'_>>,
    cancel_token: &CancellationToken,
) -> Result<ConversionResult, ConversionError> {
    let (input_path, _, input_size, _) = entry;
    check_cancelled(cancel_token)?;
    let output_file = File::create(output)
        .map_err(|error| process_error(format!("Could not create the GZIP file: {error}")))?;
    let mut encoder = GzEncoder::new(output_file, Compression::default());
    let mut input = File::open(input_path).map_err(|error| {
        process_error(format!("Could not read {}: {error}", input_path.display()))
    })?;
    let mut buffer = vec![0u8; BUFFER_SIZE];
    let mut completed_bytes = 0u64;

    loop {
        check_cancelled(cancel_token)?;
        let read = input.read(&mut buffer).map_err(|error| {
            process_error(format!("Could not read {}: {error}", input_path.display()))
        })?;
        if read == 0 {
            break;
        }
        encoder
            .write_all(&buffer[..read])
            .map_err(|error| process_error(format!("Could not write the GZIP file: {error}")))?;
        completed_bytes = completed_bytes.saturating_add(read as u64);
        emit_worker_progress(
            progress,
            progress_percent(completed_bytes, (*input_size).max(1)),
            "Compressing file",
        );
    }

    check_cancelled(cancel_token)?;
    let output_file = encoder
        .finish()
        .map_err(|error| process_error(format!("Could not finish the GZIP file: {error}")))?;
    output_file
        .sync_all()
        .map_err(|error| process_error(format!("Could not finish the GZIP file: {error}")))?;
    drop(output_file);

    emit_worker_progress(progress, 96, "Verifying GZIP");
    verify_gzip_matches_source(input_path, output, *input_size, cancel_token)?;
    Ok(empty_conversion_result())
}

fn verify_gzip_matches_source(
    source: &Path,
    gzip_path: &Path,
    expected_size: u64,
    cancel_token: &CancellationToken,
) -> Result<(), ConversionError> {
    let mut source_file = File::open(source).map_err(|error| {
        process_error(format!("Could not verify {}: {error}", source.display()))
    })?;
    let gzip_file = File::open(gzip_path)
        .map_err(|error| process_error(format!("Could not verify the GZIP file: {error}")))?;
    let mut decoder = GzDecoder::new(gzip_file);
    let mut source_buffer = vec![0u8; BUFFER_SIZE];
    let mut decoded_buffer = vec![0u8; BUFFER_SIZE];
    let mut verified_bytes = 0u64;

    loop {
        check_cancelled(cancel_token)?;
        let source_read =
            read_full_chunk(&mut source_file, &mut source_buffer).map_err(|error| {
                process_error(format!("Could not verify {}: {error}", source.display()))
            })?;
        let decoded_read = read_full_chunk(&mut decoder, &mut decoded_buffer)
            .map_err(|error| process_error(format!("GZIP verification failed: {error}")))?;
        if source_read != decoded_read
            || source_buffer[..source_read] != decoded_buffer[..decoded_read]
        {
            return Err(process_error(
                "GZIP verification failed because the decoded contents differ from the source."
                    .into(),
            ));
        }
        verified_bytes = verified_bytes.saturating_add(source_read as u64);
        if source_read == 0 {
            break;
        }
    }

    if verified_bytes != expected_size {
        return Err(process_error(
            "GZIP verification failed because the source changed during compression.".into(),
        ));
    }
    Ok(())
}

fn read_full_chunk<R: Read>(reader: &mut R, buffer: &mut [u8]) -> io::Result<usize> {
    let mut filled = 0;
    while filled < buffer.len() {
        let read = reader.read(&mut buffer[filled..])?;
        if read == 0 {
            break;
        }
        filled += read;
    }
    Ok(filled)
}

fn write_tar_entries<W: Write>(
    writer: W,
    entries: &[(PathBuf, String, u64, u32)],
    progress: Option<ArchiveProgress<'_>>,
    cancel_token: &CancellationToken,
    label: &str,
) -> Result<W, ConversionError> {
    let mut builder = tar::Builder::new(writer);
    let total_bytes = entries.iter().map(|entry| entry.2).sum::<u64>().max(1);
    let mut completed_bytes = 0u64;

    for (index, (path, archive_name, size, permissions)) in entries.iter().enumerate() {
        check_cancelled(cancel_token)?;
        emit_worker_progress(
            progress,
            progress_percent(completed_bytes, total_bytes),
            &format!("Adding {} of {}", index + 1, entries.len()),
        );

        let input = File::open(path).map_err(|error| {
            process_error(format!("Could not read {}: {error}", path.display()))
        })?;
        let mut header = tar::Header::new_gnu();
        header.set_size(*size);
        header.set_mode(*permissions);
        header.set_entry_type(tar::EntryType::file());
        let last_percent = progress_percent(completed_bytes, total_bytes);
        let reader = CancellableProgressReader {
            input,
            cancel_token,
            progress,
            completed_bytes: &mut completed_bytes,
            total_bytes,
            last_percent,
            stage: label,
        };
        if let Err(error) = builder.append_data(&mut header, archive_name, reader) {
            if cancel_token.is_cancelled() {
                return Err(ConversionError::Cancelled);
            }
            return Err(process_error(format!(
                "Could not add {archive_name} to the {label} file: {error}"
            )));
        }
    }
    builder
        .finish()
        .map_err(|error| process_error(format!("Could not finish the {label} file: {error}")))?;
    builder
        .into_inner()
        .map_err(|error| process_error(format!("Could not finish the {label} file: {error}")))
}

struct CancellableProgressReader<'a> {
    input: File,
    cancel_token: &'a CancellationToken,
    progress: Option<ArchiveProgress<'a>>,
    completed_bytes: &'a mut u64,
    total_bytes: u64,
    last_percent: i32,
    stage: &'a str,
}

impl Read for CancellableProgressReader<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if self.cancel_token.is_cancelled() {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "archive creation cancelled",
            ));
        }
        let read = self.input.read(buffer)?;
        *self.completed_bytes = self.completed_bytes.saturating_add(read as u64);
        let percent = progress_percent(*self.completed_bytes, self.total_bytes);
        if percent != self.last_percent {
            emit_worker_progress(
                self.progress,
                percent,
                &format!("Creating {} archive", self.stage),
            );
            self.last_percent = percent;
        }
        Ok(read)
    }
}

fn extract_7z_archive(
    input: &Path,
    output: &Path,
    password: Option<&ArchivePassword>,
    progress: Option<ArchiveProgress<'_>>,
    cancel_token: &CancellationToken,
) -> Result<u64, ConversionError> {
    extract_7z_archive_with_limits(
        input,
        output,
        password,
        progress,
        cancel_token,
        MAX_ARCHIVE_ENTRIES,
        MAX_EXTRACTED_BYTES,
        MAX_7Z_DECODER_MEMORY_BYTES,
    )
}

#[allow(clippy::too_many_arguments)]
fn extract_7z_archive_with_limits(
    input: &Path,
    output: &Path,
    password: Option<&ArchivePassword>,
    progress: Option<ArchiveProgress<'_>>,
    cancel_token: &CancellationToken,
    max_entries: usize,
    max_extracted_bytes: u64,
    max_decoder_memory_bytes: u64,
) -> Result<u64, ConversionError> {
    check_cancelled(cancel_token)?;
    let mut reader = open_7z_reader(input, password)?;
    let plan = validate_7z_archive(
        reader.archive(),
        max_entries,
        max_extracted_bytes,
        max_decoder_memory_bytes,
    )?;
    check_cancelled(cancel_token)?;
    reader.set_thread_count(1);

    fs::create_dir(output)
        .map_err(|error| process_error(format!("Could not prepare the output folder: {error}")))?;
    let mut expected = plan
        .entries
        .iter()
        .cloned()
        .map(|entry| (entry.normalized.clone(), entry))
        .collect::<HashMap<_, _>>();
    let mut extracted_bytes = 0u64;
    let mut completed_entries = 0usize;
    let mut failure = None;
    let mut buffer = vec![0u8; BUFFER_SIZE];

    let decode_result = reader.for_each_entries(|entry, entry_reader| {
        if cancel_token.is_cancelled() {
            return Err(abort_7z_extraction(
                &mut failure,
                ConversionError::Cancelled,
            ));
        }
        let normalized_separators = entry.name().replace('\\', "/");
        if entry.is_directory() && matches!(normalized_separators.as_str(), "." | "./") {
            return Ok(true);
        }
        let Some(relative) = sanitize_archive_path(&normalized_separators) else {
            return Err(abort_7z_extraction(
                &mut failure,
                invalid_archive("The 7Z archive contains an unsafe path and was not extracted."),
            ));
        };
        let normalized = relative.to_lowercase();
        let Some(expected_entry) = expected.remove(&normalized) else {
            return Err(abort_7z_extraction(
                &mut failure,
                invalid_archive("The 7Z archive changed while it was being extracted."),
            ));
        };
        if expected_entry.relative != relative
            || expected_entry.is_directory != entry.is_directory()
            || expected_entry.size != entry.size()
        {
            return Err(abort_7z_extraction(
                &mut failure,
                invalid_archive("The 7Z archive changed while it was being extracted."),
            ));
        }

        emit_worker_progress(
            progress,
            progress_percent(extracted_bytes, plan.total_bytes.max(1)),
            &format!(
                "Extracting {} of {}",
                completed_entries + 1,
                plan.entries.len()
            ),
        );
        let destination = output.join(&relative);
        if expected_entry.is_directory {
            if let Err(error) = fs::create_dir_all(&destination) {
                return Err(abort_7z_extraction(
                    &mut failure,
                    process_error(format!(
                        "Could not create {}: {error}",
                        destination.display()
                    )),
                ));
            }
            completed_entries += 1;
            return Ok(true);
        }

        if let Some(parent) = destination.parent() {
            if let Err(error) = fs::create_dir_all(parent) {
                return Err(abort_7z_extraction(
                    &mut failure,
                    process_error(format!("Could not create {}: {error}", parent.display())),
                ));
            }
        }
        let mut destination_file = match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&destination)
        {
            Ok(file) => file,
            Err(error) => {
                return Err(abort_7z_extraction(
                    &mut failure,
                    process_error(format!(
                        "Could not create {}: {error}",
                        destination.display()
                    )),
                ));
            }
        };
        if let Err(error) = copy_7z_entry_contents(
            entry_reader,
            &mut destination_file,
            &destination,
            cancel_token,
            expected_entry.size,
            max_extracted_bytes,
            &mut extracted_bytes,
            &mut buffer,
        ) {
            return Err(abort_7z_extraction(&mut failure, error));
        }
        if let Err(error) = destination_file.sync_all() {
            return Err(abort_7z_extraction(
                &mut failure,
                process_error(format!(
                    "Could not finish {}: {error}",
                    destination.display()
                )),
            ));
        }
        completed_entries += 1;
        Ok(true)
    });

    if let Some(error) = failure {
        return Err(error);
    }
    decode_result.map_err(|error| map_7z_error(error, password.is_some()))?;
    check_cancelled(cancel_token)?;
    if !expected.is_empty()
        || completed_entries != plan.entries.len()
        || extracted_bytes != plan.total_bytes
    {
        return Err(invalid_archive(
            "The 7Z archive ended before every declared entry was extracted.",
        ));
    }
    verify_7z_output(output, &plan)?;
    Ok(extracted_bytes)
}

#[allow(clippy::too_many_arguments)]
fn copy_7z_entry_contents<R: Read + ?Sized, W: Write>(
    reader: &mut R,
    writer: &mut W,
    destination: &Path,
    cancel_token: &CancellationToken,
    expected_size: u64,
    max_extracted_bytes: u64,
    extracted_bytes: &mut u64,
    buffer: &mut [u8],
) -> Result<(), ConversionError> {
    let mut entry_bytes = 0u64;
    loop {
        check_cancelled(cancel_token)?;
        let read = reader.read(buffer).map_err(|error| {
            process_error(format!(
                "Could not extract {}: {error}",
                destination.display()
            ))
        })?;
        if read == 0 {
            break;
        }
        entry_bytes = entry_bytes
            .checked_add(read as u64)
            .filter(|size| *size <= expected_size)
            .ok_or_else(|| invalid_archive("A 7Z entry exceeded its declared size."))?;
        *extracted_bytes = extracted_bytes
            .checked_add(read as u64)
            .filter(|size| *size <= max_extracted_bytes)
            .ok_or_else(|| invalid_archive("Extraction exceeded the 100 GB safety limit."))?;
        writer.write_all(&buffer[..read]).map_err(|error| {
            process_error(format!(
                "Could not write {}: {error}",
                destination.display()
            ))
        })?;
    }
    if entry_bytes != expected_size {
        return Err(invalid_archive(
            "A 7Z entry ended before its declared size.",
        ));
    }
    Ok(())
}

fn abort_7z_extraction(
    failure: &mut Option<ConversionError>,
    error: ConversionError,
) -> sevenz_rust2::Error {
    *failure = Some(error);
    io::Error::other("ConvertKit stopped 7Z extraction").into()
}

fn verify_7z_output(output: &Path, plan: &SevenZExtractionPlan) -> Result<(), ConversionError> {
    for entry in &plan.entries {
        let path = output.join(&entry.relative);
        let metadata = fs::symlink_metadata(&path).map_err(|error| {
            process_error(format!("Could not verify {}: {error}", path.display()))
        })?;
        if metadata.file_type().is_symlink()
            || (entry.is_directory && !metadata.is_dir())
            || (!entry.is_directory && !metadata.is_file())
            || (!entry.is_directory && metadata.len() != entry.size)
        {
            return Err(invalid_archive(
                "The extracted 7Z output did not match the archive manifest.",
            ));
        }
    }
    Ok(())
}

fn extract_zip_archive(
    input: &Path,
    output: &Path,
    progress: Option<ArchiveProgress<'_>>,
    cancel_token: &CancellationToken,
) -> Result<u64, ConversionError> {
    let file = File::open(input)
        .map_err(|error| process_error(format!("Could not open the ZIP file: {error}")))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|error| process_error(format!("This ZIP file is invalid or damaged: {error}")))?;
    let archive_len = archive.len();
    if archive_len > MAX_ARCHIVE_ENTRIES {
        return Err(invalid_archive(
            "This ZIP contains too many entries to extract safely.",
        ));
    }

    let declared_total = (0..archive_len).try_fold(0u64, |total, index| {
        let entry = archive
            .by_index(index)
            .map_err(|error| process_error(format!("Could not inspect the ZIP file: {error}")))?;
        total
            .checked_add(entry.size())
            .filter(|size| *size <= MAX_EXTRACTED_BYTES)
            .ok_or_else(|| invalid_archive("The extracted archive would exceed the 100 GB limit."))
    })?;

    fs::create_dir_all(output)
        .map_err(|error| process_error(format!("Could not prepare the output folder: {error}")))?;
    let mut extracted_bytes = 0u64;
    let mut extracted_paths = HashSet::new();
    let mut buffer = vec![0u8; BUFFER_SIZE];
    for index in 0..archive_len {
        check_cancelled(cancel_token)?;
        let mut entry = archive
            .by_index(index)
            .map_err(|error| process_error(format!("Could not read ZIP entry: {error}")))?;
        let relative = entry.enclosed_name().ok_or_else(|| {
            invalid_archive("The ZIP contains an unsafe path and was not extracted.")
        })?;
        if relative.as_os_str().is_empty() {
            continue;
        }
        let normalized_relative = relative.to_string_lossy().replace('\\', "/").to_lowercase();
        if !extracted_paths.insert(normalized_relative) {
            return Err(invalid_archive(
                "The ZIP contains duplicate paths and was not extracted.",
            ));
        }
        if entry
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err(invalid_archive(
                "ZIP symlinks are not extracted for safety.",
            ));
        }

        emit_worker_progress(
            progress,
            progress_percent(extracted_bytes, declared_total.max(1)),
            &format!("Extracting {} of {}", index + 1, archive_len),
        );
        let destination = output.join(relative);
        if entry.is_dir() {
            fs::create_dir_all(&destination).map_err(|error| {
                process_error(format!(
                    "Could not create {}: {error}",
                    destination.display()
                ))
            })?;
            continue;
        }
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                process_error(format!("Could not create {}: {error}", parent.display()))
            })?;
        }
        let mut destination_file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&destination)
            .map_err(|error| {
                if error.kind() == io::ErrorKind::AlreadyExists {
                    invalid_archive(
                        "The ZIP contains paths that collide on this filesystem and was not extracted.",
                    )
                } else {
                    process_error(format!(
                        "Could not create {}: {error}",
                        destination.display()
                    ))
                }
            })?;
        loop {
            check_cancelled(cancel_token)?;
            let read = entry.read(&mut buffer).map_err(|error| {
                process_error(format!(
                    "Could not extract {}: {error}",
                    destination.display()
                ))
            })?;
            if read == 0 {
                break;
            }
            extracted_bytes = extracted_bytes
                .checked_add(read as u64)
                .filter(|size| *size <= MAX_EXTRACTED_BYTES)
                .ok_or_else(|| invalid_archive("Extraction exceeded the 100 GB safety limit."))?;
            destination_file
                .write_all(&buffer[..read])
                .map_err(|error| {
                    process_error(format!(
                        "Could not write {}: {error}",
                        destination.display()
                    ))
                })?;
        }
    }
    Ok(extracted_bytes)
}

fn extract_tar_archive(
    input: &Path,
    output: &Path,
    progress: Option<ArchiveProgress<'_>>,
    cancel_token: &CancellationToken,
) -> Result<u64, ConversionError> {
    extract_tar_archive_with_limits(
        input,
        output,
        progress,
        cancel_token,
        MAX_ARCHIVE_ENTRIES,
        MAX_EXTRACTED_BYTES,
    )
}

fn extract_tar_archive_with_limits(
    input: &Path,
    output: &Path,
    progress: Option<ArchiveProgress<'_>>,
    cancel_token: &CancellationToken,
    max_entries: usize,
    max_extracted_bytes: u64,
) -> Result<u64, ConversionError> {
    let file = File::open(input)
        .map_err(|error| process_error(format!("Could not open the TAR file: {error}")))?;
    extract_tar_reader_with_limits(
        file,
        output,
        progress,
        cancel_token,
        max_entries,
        max_extracted_bytes,
        "TAR",
    )
}

fn extract_tar_gz_archive(
    input: &Path,
    output: &Path,
    progress: Option<ArchiveProgress<'_>>,
    cancel_token: &CancellationToken,
) -> Result<u64, ConversionError> {
    let file = File::open(input)
        .map_err(|error| process_error(format!("Could not open the TAR.GZ file: {error}")))?;
    extract_tar_reader_with_limits(
        GzDecoder::new(file),
        output,
        progress,
        cancel_token,
        MAX_ARCHIVE_ENTRIES,
        MAX_EXTRACTED_BYTES,
        "TAR.GZ",
    )
}

#[allow(clippy::too_many_arguments)]
fn extract_tar_reader_with_limits<R: Read>(
    reader: R,
    output: &Path,
    progress: Option<ArchiveProgress<'_>>,
    cancel_token: &CancellationToken,
    max_entries: usize,
    max_extracted_bytes: u64,
    label: &str,
) -> Result<u64, ConversionError> {
    let mut archive = tar::Archive::new(reader);
    let entries = archive.entries().map_err(|error| {
        process_error(format!("This {label} file is invalid or damaged: {error}"))
    })?;

    fs::create_dir_all(output)
        .map_err(|error| process_error(format!("Could not prepare the output folder: {error}")))?;
    let mut extracted_bytes = 0u64;
    let mut extracted_paths = HashSet::new();
    let mut buffer = vec![0u8; BUFFER_SIZE];

    for (index, entry) in entries.enumerate() {
        check_cancelled(cancel_token)?;
        if index >= max_entries {
            return Err(invalid_archive(&format!(
                "This {label} contains too many entries to extract safely."
            )));
        }
        let mut entry = entry
            .map_err(|error| process_error(format!("Could not read {label} entry: {error}")))?;
        let entry_type = entry.header().entry_type();
        if !entry_type.is_file() && !entry_type.is_dir() {
            return Err(invalid_archive(&format!(
                "{label} links and special files are not extracted for safety."
            )));
        }

        let raw_path = entry
            .path()
            .map_err(|error| process_error(format!("Could not read a {label} path: {error}")))?;
        let raw_path = raw_path.to_str().ok_or_else(|| {
            invalid_archive(&format!("The {label} contains an unsupported filename."))
        })?;
        if entry_type.is_dir() && matches!(raw_path, "." | "./") {
            continue;
        }
        let relative = sanitize_archive_path(raw_path).ok_or_else(|| {
            invalid_archive(&format!(
                "The {label} contains an unsafe path and was not extracted."
            ))
        })?;
        let normalized_relative = relative.to_lowercase();
        if !extracted_paths.insert(normalized_relative) {
            return Err(invalid_archive(&format!(
                "The {label} contains duplicate paths and was not extracted."
            )));
        }

        let declared_size = entry.size();
        let projected_size = extracted_bytes.checked_add(declared_size).ok_or_else(|| {
            invalid_archive("The extracted archive would exceed the safety limit.")
        })?;
        if projected_size > max_extracted_bytes {
            return Err(invalid_archive(
                "The extracted archive would exceed the 100 GB limit.",
            ));
        }
        emit_worker_progress(progress, -1, &format!("Extracting item {}", index + 1));

        let destination = output.join(&relative);
        if entry_type.is_dir() {
            fs::create_dir_all(&destination).map_err(|error| {
                process_error(format!(
                    "Could not create {}: {error}",
                    destination.display()
                ))
            })?;
            continue;
        }
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                process_error(format!("Could not create {}: {error}", parent.display()))
            })?;
        }
        let mut destination_file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&destination)
            .map_err(|error| {
                process_error(format!(
                    "Could not create {}: {error}",
                    destination.display()
                ))
            })?;
        let mut entry_bytes = 0u64;
        loop {
            check_cancelled(cancel_token)?;
            let read = entry.read(&mut buffer).map_err(|error| {
                process_error(format!(
                    "Could not extract {}: {error}",
                    destination.display()
                ))
            })?;
            if read == 0 {
                break;
            }
            entry_bytes = entry_bytes.checked_add(read as u64).ok_or_else(|| {
                invalid_archive("The extracted archive would exceed the safety limit.")
            })?;
            extracted_bytes = extracted_bytes
                .checked_add(read as u64)
                .filter(|size| *size <= max_extracted_bytes)
                .ok_or_else(|| invalid_archive("Extraction exceeded the 100 GB safety limit."))?;
            destination_file
                .write_all(&buffer[..read])
                .map_err(|error| {
                    process_error(format!(
                        "Could not write {}: {error}",
                        destination.display()
                    ))
                })?;
        }
        if entry_bytes != declared_size {
            return Err(invalid_archive(&format!(
                "A {label} entry ended before its declared size and was not extracted."
            )));
        }
    }
    Ok(extracted_bytes)
}

fn empty_conversion_result() -> ConversionResult {
    ConversionResult {
        output_path: String::new(),
        output_paths: Vec::new(),
        output_size: 0,
        duration_ms: 0,
        undo_manifest: None,
    }
}

fn sanitize_archive_path(value: &str) -> Option<String> {
    let normalized = value.replace('\\', "/");
    let path = Path::new(&normalized);
    let parts = path
        .components()
        .filter_map(|component| match component {
            Component::Normal(value) => value.to_str(),
            Component::CurDir => None,
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => Some(""),
        })
        .collect::<Vec<_>>();
    if parts.is_empty() || parts.iter().any(|part| part.is_empty()) {
        return None;
    }
    Some(parts.join("/"))
}

fn unique_archive_path(requested: &str, used: &mut HashSet<String>) -> String {
    if used.insert(requested.to_lowercase()) {
        return requested.to_owned();
    }
    let path = Path::new(requested);
    let parent = path.parent().filter(|value| !value.as_os_str().is_empty());
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("file");
    let extension = path.extension().and_then(|value| value.to_str());
    for index in 2u32.. {
        let name = match extension {
            Some(extension) => format!("{stem} ({index}).{extension}"),
            None => format!("{stem} ({index})"),
        };
        let candidate = parent
            .map(|value| value.join(&name).to_string_lossy().replace('\\', "/"))
            .unwrap_or(name);
        if used.insert(candidate.to_lowercase()) {
            return candidate;
        }
    }
    unreachable!()
}

fn progress_percent(completed: u64, total: u64) -> i32 {
    ((completed.saturating_mul(95) / total.max(1)).min(95)) as i32
}

fn emit_progress(app: &AppHandle, job_id: &str, percent: i32, stage: &str) {
    let _ = app.emit(
        "conversion-progress",
        ProgressPayload {
            job_id: job_id.to_owned(),
            percent,
            stage: stage.into(),
        },
    );
}

fn emit_worker_progress(progress: Option<ArchiveProgress<'_>>, percent: i32, stage: &str) {
    if let Some(progress) = progress {
        emit_progress(progress.app, progress.job_id, percent, stage);
    }
}

fn check_cancelled(cancel_token: &CancellationToken) -> Result<(), ConversionError> {
    if cancel_token.is_cancelled() {
        Err(ConversionError::Cancelled)
    } else {
        Ok(())
    }
}

fn invalid_archive(message: &str) -> ConversionError {
    ConversionError::UnsupportedConversion {
        input: message.into(),
        output: "archive".into(),
    }
}

fn process_error(message: String) -> ConversionError {
    ConversionError::ProcessFailed {
        message,
        stderr: String::new(),
        exit_code: None,
    }
}

struct PreparedDirectory {
    parent: PathBuf,
    base_name: String,
    final_path: PathBuf,
    working_path: PathBuf,
    committed: bool,
}

impl PreparedDirectory {
    fn new(
        input: &Path,
        format: ExtractArchiveFormat,
        default_suffix: &str,
        options: Option<OutputOptions>,
    ) -> Result<Self, ConversionError> {
        let options = options.unwrap_or_else(|| OutputOptions {
            suffix: default_suffix.into(),
            ..OutputOptions::default()
        });
        validate_suffix(&options.suffix)?;
        let has_custom_directory = options
            .directory
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .is_some();
        let requested_parent = options
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
        let parent = resolve_writable_directory(requested_parent, has_custom_directory)?;
        let stem = archive_stem(input, format);
        let base_name = format!("{stem}{}", options.suffix);
        let final_path = deduplicate_directory(&parent, &base_name);
        let working_path = parent.join(format!(".convertkit-{}", Uuid::new_v4()));
        Ok(Self {
            parent,
            base_name,
            final_path,
            working_path,
            committed: false,
        })
    }

    fn commit(mut self, output_size: u64) -> Result<ConversionResult, ConversionError> {
        if !self.working_path.is_dir() {
            return Err(ConversionError::OutputMissing);
        }
        loop {
            match atomic_rename_no_replace(&self.working_path, &self.final_path) {
                Ok(()) => break,
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                    self.final_path = deduplicate_directory(&self.parent, &self.base_name);
                }
                Err(error) => {
                    let _ = fs::remove_dir_all(&self.working_path);
                    return Err(process_error(format!(
                        "Could not finalize the extracted folder: {error}"
                    )));
                }
            }
        }
        self.committed = true;
        let output_path = self.final_path.to_string_lossy().into_owned();
        Ok(ConversionResult {
            output_path: output_path.clone(),
            output_paths: vec![output_path],
            output_size,
            duration_ms: 0,
            undo_manifest: None,
        })
    }
}

fn archive_stem(input: &Path, format: ExtractArchiveFormat) -> String {
    let file_name = input
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("archive");
    if format == ExtractArchiveFormat::TarGz {
        let lowercase = file_name.to_ascii_lowercase();
        let suffix_length = if lowercase.ends_with(".tar.gz") {
            7
        } else if lowercase.ends_with(".tgz") {
            4
        } else {
            0
        };
        if suffix_length > 0 && file_name.len() > suffix_length {
            return file_name[..file_name.len() - suffix_length].to_owned();
        }
    }
    input
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("archive")
        .to_owned()
}

impl Drop for PreparedDirectory {
    fn drop(&mut self) {
        if !self.committed {
            let _ = fs::remove_dir_all(&self.working_path);
        }
    }
}

fn deduplicate_directory(parent: &Path, base_name: &str) -> PathBuf {
    let base = parent.join(base_name);
    if !base.exists() {
        return base;
    }
    for index in 1u32.. {
        let candidate = parent.join(format!("{base_name} ({index})"));
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!()
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    fn write_7z_fixture(path: &Path, entries: Vec<(ArchiveEntry, Option<Vec<u8>>)>) {
        let mut writer = ArchiveWriter::create(path).expect("create 7Z fixture");
        for (entry, contents) in entries {
            match contents {
                Some(contents) => {
                    writer
                        .push_archive_entry(entry, Some(Cursor::new(contents)))
                        .expect("write 7Z file entry");
                }
                None => {
                    writer
                        .push_archive_entry::<Cursor<Vec<u8>>>(entry, None)
                        .expect("write 7Z empty entry");
                }
            }
        }
        writer.finish().expect("finish 7Z fixture");
    }

    fn gzip_bytes(contents: &[u8]) -> Vec<u8> {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(contents).expect("write GZIP fixture");
        encoder.finish().expect("finish GZIP fixture")
    }

    struct CancelAfterFirstRead<'a> {
        contents: Cursor<Vec<u8>>,
        token: &'a CancellationToken,
        did_cancel: bool,
    }

    impl Read for CancelAfterFirstRead<'_> {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            let read = self.contents.read(buffer)?;
            if read > 0 && !self.did_cancel {
                self.token.cancel();
                self.did_cancel = true;
            }
            Ok(read)
        }
    }

    #[test]
    fn rejects_unsafe_archive_paths() {
        assert_eq!(
            sanitize_archive_path("folder/photo.jpg").as_deref(),
            Some("folder/photo.jpg")
        );
        assert!(sanitize_archive_path("../secret.txt").is_none());
        assert!(sanitize_archive_path("/absolute.txt").is_none());
    }

    #[test]
    fn duplicate_archive_names_are_renamed() {
        let mut used = HashSet::new();
        assert_eq!(unique_archive_path("photo.jpg", &mut used), "photo.jpg");
        assert_eq!(unique_archive_path("photo.jpg", &mut used), "photo (2).jpg");
        assert_eq!(unique_archive_path("photo.jpg", &mut used), "photo (3).jpg");
        assert_eq!(unique_archive_path("PHOTO.JPG", &mut used), "PHOTO (4).JPG");
    }

    #[test]
    fn gzip_creation_preserves_the_source_extension_in_its_output_extension() {
        assert_eq!(
            ArchiveFormat::Gzip.output_extension(Path::new("report.csv")),
            "csv.gz"
        );
        assert_eq!(
            ArchiveFormat::Gzip.output_extension(Path::new("payload")),
            "gz"
        );
        assert_eq!(
            ArchiveFormat::TarGz.output_extension(Path::new("report.csv")),
            "tar.gz"
        );
        assert_eq!(
            ArchiveFormat::SevenZ.output_extension(Path::new("report.csv")),
            "7z"
        );
    }

    #[test]
    fn gzip_creation_accepts_one_direct_regular_file_only() {
        let directory = tempfile::tempdir().expect("tempdir");
        let input = directory.path().join("report.csv");
        fs::write(&input, b"name,value\nalpha,1\n").expect("fixture");
        let direct = ArchiveEntryRequest {
            input_path: input.to_string_lossy().into_owned(),
            archive_path: "report.csv".into(),
            folder_derived: false,
        };

        assert!(
            validate_create_entries_for_format(vec![direct.clone()], ArchiveFormat::Gzip).is_ok()
        );
        assert!(matches!(
            validate_create_entries_for_format(
                vec![direct.clone(), direct.clone()],
                ArchiveFormat::Gzip
            ),
            Err(ConversionError::UnsupportedConversion { input, .. })
                if input.contains("exactly one regular file")
        ));
        assert!(matches!(
            validate_create_entries_for_format(
                vec![ArchiveEntryRequest {
                    folder_derived: true,
                    ..direct
                }],
                ArchiveFormat::Gzip
            ),
            Err(ConversionError::UnsupportedConversion { input, .. })
                if input.contains("cannot compress a folder")
        ));
    }

    #[test]
    fn creates_and_verifies_a_single_file_gzip_without_changing_the_source() {
        let directory = tempfile::tempdir().expect("tempdir");
        let input = directory.path().join("report.csv");
        let contents = b"name,value\nalpha,1\nbeta,2\n";
        fs::write(&input, contents).expect("fixture");
        let output = directory.path().join("report-archive.csv.gz");
        let entry = (
            input.clone(),
            "report.csv".into(),
            contents.len() as u64,
            0o644,
        );

        write_gzip_file(&output, &entry, None, &CancellationToken::new()).expect("create GZIP");

        let mut decoded = Vec::new();
        GzDecoder::new(File::open(&output).expect("GZIP output"))
            .read_to_end(&mut decoded)
            .expect("decode GZIP");
        assert_eq!(decoded, contents);
        assert_eq!(fs::read(&input).expect("source after"), contents);
    }

    #[test]
    fn gzip_creation_commits_with_preserved_extension_and_keep_both_naming() {
        let directory = tempfile::tempdir().expect("tempdir");
        let input = directory.path().join("report.csv");
        let contents = b"name,value\nalpha,1\n";
        fs::write(&input, contents).expect("fixture");
        let entry = (
            input.clone(),
            "report.csv".into(),
            contents.len() as u64,
            0o644,
        );
        let create = || {
            let extension = ArchiveFormat::Gzip.output_extension(&input);
            let prepared = prepare_output(
                &input,
                &extension,
                "-archive",
                Some(OutputOptions {
                    directory: Some(directory.path().to_string_lossy().into_owned()),
                    suffix: "-archive".into(),
                }),
            )
            .expect("prepare output");
            write_gzip_file(
                prepared.working_path(),
                &entry,
                None,
                &CancellationToken::new(),
            )
            .expect("create GZIP");
            prepared
                .commit(empty_conversion_result())
                .expect("commit GZIP")
        };

        let first = create();
        let second = create();

        assert_eq!(
            first.output_path,
            directory
                .path()
                .join("report-archive.csv.gz")
                .to_string_lossy()
        );
        assert_eq!(
            second.output_path,
            directory
                .path()
                .join("report-archive (1).csv.gz")
                .to_string_lossy()
        );
        assert_eq!(fs::read(&input).expect("source after"), contents);
    }

    #[test]
    fn gzip_creation_and_verification_honor_cancellation() {
        let directory = tempfile::tempdir().expect("tempdir");
        let input = directory.path().join("input.bin");
        fs::write(&input, vec![7u8; BUFFER_SIZE * 2]).expect("fixture");
        let entry = (input, "input.bin".into(), (BUFFER_SIZE * 2) as u64, 0o644);
        let token = CancellationToken::new();
        token.cancel();

        assert!(matches!(
            write_gzip_file(
                &directory.path().join("cancelled.bin.gz"),
                &entry,
                None,
                &token
            ),
            Err(ConversionError::Cancelled)
        ));
    }

    #[test]
    fn creates_and_verifies_a_7z_with_nested_files_without_changing_sources() {
        let directory = tempfile::tempdir().expect("tempdir");
        let first = directory.path().join("first.txt");
        let second = directory.path().join("second.bin");
        let first_contents = b"first file\n";
        let second_contents = [0, 1, 2, 3, 255, 128, 64];
        fs::write(&first, first_contents).expect("first fixture");
        fs::write(&second, second_contents).expect("second fixture");
        let output = directory.path().join("files.7z");
        let entries = vec![
            (
                first.clone(),
                "folder/first.txt".into(),
                first_contents.len() as u64,
                0o644,
            ),
            (
                second.clone(),
                "second.bin".into(),
                second_contents.len() as u64,
                0o644,
            ),
        ];

        write_7z_archive(&output, &entries, None, None, &CancellationToken::new())
            .expect("create verified 7Z");

        let reader = open_7z_reader(&output, None).expect("open created 7Z");
        let plan = validate_7z_archive(
            reader.archive(),
            MAX_ARCHIVE_ENTRIES,
            MAX_EXTRACTED_BYTES,
            MAX_7Z_DECODER_MEMORY_BYTES,
        )
        .expect("validate created 7Z");
        assert_eq!(plan.entries.len(), 2);
        assert_eq!(
            plan.total_bytes,
            (first_contents.len() + second_contents.len()) as u64
        );
        assert_eq!(fs::read(first).expect("first source after"), first_contents);
        assert_eq!(
            fs::read(second).expect("second source after"),
            second_contents
        );
    }

    #[test]
    fn creates_header_and_content_encrypted_7z_and_requires_the_correct_password() {
        let directory = tempfile::tempdir().expect("tempdir");
        let source = directory.path().join("private.txt");
        let contents = b"private archive contents\n";
        fs::write(&source, contents).expect("fixture");
        let archive = directory.path().join("private.7z");
        let entries = vec![(
            source.clone(),
            "nested/private.txt".into(),
            contents.len() as u64,
            0o600,
        )];
        let password = ArchivePassword("correct horse battery staple".into());

        write_7z_archive(
            &archive,
            &entries,
            Some(&password),
            None,
            &CancellationToken::new(),
        )
        .expect("create encrypted 7Z");

        let reader = open_7z_reader(&archive, Some(&password)).expect("read encrypted archive");
        assert!(reader.archive().blocks.iter().any(|block| {
            block
                .coders
                .iter()
                .any(|coder| coder.encoder_method_id() == EncoderMethod::ID_AES256_SHA256)
        }));

        assert!(matches!(
            validate_archive_input_with_password(archive.to_str().expect("archive path"), None),
            Err(ConversionError::ArchivePasswordRequired)
        ));
        let wrong = ArchivePassword("wrong password".into());
        assert!(matches!(
            validate_archive_input_with_password(
                archive.to_str().expect("archive path"),
                Some(&wrong)
            ),
            Err(ConversionError::IncorrectArchivePassword)
        ));
        validate_archive_input_with_password(
            archive.to_str().expect("archive path"),
            Some(&password),
        )
        .expect("correct password validates");

        let extracted = directory.path().join("extracted");
        let extracted_bytes = extract_7z_archive(
            &archive,
            &extracted,
            Some(&password),
            None,
            &CancellationToken::new(),
        )
        .expect("extract encrypted 7Z");
        assert_eq!(extracted_bytes, contents.len() as u64);
        assert_eq!(
            fs::read(extracted.join("nested/private.txt")).expect("decoded contents"),
            contents
        );
        assert_eq!(fs::read(source).expect("source after"), contents);
    }

    #[test]
    fn archive_passwords_are_bounded_redacted_and_7z_only() {
        let password = ArchivePassword("never-print-this-secret".into());
        assert_eq!(format!("{password:?}"), "ArchivePassword([REDACTED])");
        assert!(validate_password_for_creation(ArchiveFormat::SevenZ, Some(&password)).is_ok());
        assert!(matches!(
            validate_password_for_creation(ArchiveFormat::Zip, Some(&password)),
            Err(ConversionError::UnsupportedConversion { .. })
        ));

        let empty = ArchivePassword(String::new());
        assert!(validate_password_value(&empty).is_err());
        let too_long = ArchivePassword("x".repeat(MAX_ARCHIVE_PASSWORD_CHARS + 1));
        assert!(validate_password_value(&too_long).is_err());
        let oversized_unicode = ArchivePassword("🗄".repeat(MAX_ARCHIVE_PASSWORD_CHARS + 1));
        assert!(validate_password_value(&oversized_unicode).is_err());
        let null = ArchivePassword("before\0after".into());
        assert!(validate_password_value(&null).is_err());
    }

    #[test]
    fn rejects_a_7z_whose_decoded_bytes_do_not_match_the_sources() {
        let directory = tempfile::tempdir().expect("tempdir");
        let source = directory.path().join("report.txt");
        fs::write(&source, b"source").expect("source fixture");
        let archive = directory.path().join("report.7z");
        write_7z_fixture(
            &archive,
            vec![(
                ArchiveEntry::new_file("report.txt"),
                Some(b"broken".to_vec()),
            )],
        );
        let entries = vec![(source, "report.txt".into(), 6, 0o644)];

        let result = verify_7z_matches_sources(&archive, &entries, None, &CancellationToken::new());

        assert!(
            matches!(result, Err(ConversionError::ProcessFailed { message, .. }) if message.contains("differs from its source"))
        );
    }

    #[test]
    fn seven_z_creation_honors_cancellation_before_creating_a_partial() {
        let directory = tempfile::tempdir().expect("tempdir");
        let input = directory.path().join("input.bin");
        fs::write(&input, vec![7u8; BUFFER_SIZE * 2]).expect("fixture");
        let output = directory.path().join("cancelled.7z");
        let entries = vec![(input, "input.bin".into(), (BUFFER_SIZE * 2) as u64, 0o644)];
        let token = CancellationToken::new();
        token.cancel();

        assert!(matches!(
            write_7z_archive(&output, &entries, None, None, &token),
            Err(ConversionError::Cancelled)
        ));
        assert!(!output.exists());
    }

    #[test]
    fn seven_z_creation_commits_with_keep_both_naming() {
        let directory = tempfile::tempdir().expect("tempdir");
        let input = directory.path().join("report.csv");
        let contents = b"name,value\nalpha,1\n";
        fs::write(&input, contents).expect("fixture");
        let entries = vec![(
            input.clone(),
            "report.csv".into(),
            contents.len() as u64,
            0o644,
        )];
        let create = || {
            let prepared = prepare_output(
                &input,
                &ArchiveFormat::SevenZ.output_extension(&input),
                "-archive",
                Some(OutputOptions {
                    directory: Some(directory.path().to_string_lossy().into_owned()),
                    suffix: "-archive".into(),
                }),
            )
            .expect("prepare output");
            write_7z_archive(
                prepared.working_path(),
                &entries,
                None,
                None,
                &CancellationToken::new(),
            )
            .expect("create 7Z");
            prepared
                .commit(empty_conversion_result())
                .expect("commit 7Z")
        };

        let first = create();
        let second = create();

        assert_eq!(
            first.output_path,
            directory.path().join("report-archive.7z").to_string_lossy()
        );
        assert_eq!(
            second.output_path,
            directory
                .path()
                .join("report-archive (1).7z")
                .to_string_lossy()
        );
        assert_eq!(fs::read(input).expect("source after"), contents);
    }

    #[test]
    fn seven_z_creation_rejects_duplicate_inputs() {
        let directory = tempfile::tempdir().expect("tempdir");
        let input = directory.path().join("report.txt");
        fs::write(&input, b"report").expect("fixture");
        let request = |archive_path: &str| ArchiveEntryRequest {
            input_path: input.to_string_lossy().into_owned(),
            archive_path: archive_path.into(),
            folder_derived: false,
        };

        let result = validate_create_entries_for_format(
            vec![request("first.txt"), request("second.txt")],
            ArchiveFormat::SevenZ,
        );

        assert!(
            matches!(result, Err(ConversionError::UnsupportedConversion { input, .. }) if input.contains("duplicate files"))
        );
    }

    #[cfg(unix)]
    #[test]
    fn seven_z_creation_rejects_special_inputs() {
        use std::os::unix::net::UnixListener;

        let directory = tempfile::tempdir().expect("tempdir");
        let socket = directory.path().join("input.socket");
        let _listener = UnixListener::bind(&socket).expect("socket fixture");
        let result = validate_create_entries_for_format(
            vec![ArchiveEntryRequest {
                input_path: socket.to_string_lossy().into_owned(),
                archive_path: "input.socket".into(),
                folder_derived: false,
            }],
            ArchiveFormat::SevenZ,
        );

        assert!(
            matches!(result, Err(ConversionError::UnsupportedConversion { input, .. }) if input.contains("regular files"))
        );
    }

    #[test]
    fn creates_and_extracts_a_zip_without_traversal() {
        let source = tempfile::tempdir().expect("source");
        let output = tempfile::tempdir().expect("output");
        let first = source.path().join("first.txt");
        let second = source.path().join("second.txt");
        fs::write(&first, b"first file").expect("first fixture");
        fs::write(&second, b"second file").expect("second fixture");
        let archive_path = output.path().join("files.zip");
        let entries = vec![
            (first, "folder/first.txt".into(), 10, 0o644),
            (second, "second.txt".into(), 11, 0o644),
        ];

        write_zip_archive(&archive_path, &entries, None, &CancellationToken::new())
            .expect("create archive");

        let extracted = output.path().join("extracted");
        let extracted_bytes =
            extract_zip_archive(&archive_path, &extracted, None, &CancellationToken::new())
                .expect("extract archive");

        assert_eq!(extracted_bytes, 21);
        assert_eq!(
            fs::read(extracted.join("folder/first.txt")).expect("first"),
            b"first file"
        );
        assert_eq!(
            fs::read(extracted.join("second.txt")).expect("second"),
            b"second file"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn refuses_zip_unicode_alias_collisions_without_overwriting_first_entry() {
        let directory = tempfile::tempdir().expect("tempdir");
        let archive_path = directory.path().join("unicode-collision.zip");
        let file = File::create(&archive_path).expect("archive");
        let mut writer = zip::ZipWriter::new(file);
        let composed_name = "\u{00e9}.txt";
        let decomposed_name = "e\u{0301}.txt";
        writer
            .start_file(composed_name, SimpleFileOptions::default())
            .expect("composed entry");
        writer.write_all(b"first payload").expect("first payload");
        writer
            .start_file(decomposed_name, SimpleFileOptions::default())
            .expect("decomposed entry");
        writer.write_all(b"second payload").expect("second payload");
        writer.finish().expect("finish");

        let output = directory.path().join("output");
        let result = extract_zip_archive(&archive_path, &output, None, &CancellationToken::new());

        let Err(ConversionError::UnsupportedConversion {
            input: message,
            output: kind,
        }) = result
        else {
            panic!("filesystem alias collision should be rejected as an unsafe archive");
        };
        assert!(message.contains("collide on this filesystem"));
        assert_eq!(kind, "archive");
        assert_eq!(
            fs::read(output.join(composed_name)).expect("first entry survives"),
            b"first payload"
        );
        assert_eq!(
            fs::read(output.join(decomposed_name)).expect("filesystem alias resolves"),
            b"first payload",
            "the colliding entry must never truncate the first entry"
        );
    }

    #[test]
    fn creates_and_extracts_a_tar_without_links() {
        let source = tempfile::tempdir().expect("source");
        let output = tempfile::tempdir().expect("output");
        let first = source.path().join("first.txt");
        let second = source.path().join("second.txt");
        fs::write(&first, b"first file").expect("first fixture");
        fs::write(&second, b"second file").expect("second fixture");
        let archive_path = output.path().join("files.tar");
        let entries = vec![
            (first, "folder/first.txt".into(), 10, 0o644),
            (second, "second.txt".into(), 11, 0o644),
        ];

        write_tar_archive(&archive_path, &entries, None, &CancellationToken::new())
            .expect("create TAR archive");

        let extracted = output.path().join("extracted");
        extract_tar_archive(&archive_path, &extracted, None, &CancellationToken::new())
            .expect("extract TAR archive");

        assert_eq!(
            fs::read(extracted.join("folder/first.txt")).expect("first"),
            b"first file"
        );
        assert_eq!(
            fs::read(extracted.join("second.txt")).expect("second"),
            b"second file"
        );
    }

    #[test]
    fn creates_and_extracts_a_tar_gz_with_identical_contents() {
        let source = tempfile::tempdir().expect("source");
        let output = tempfile::tempdir().expect("output");
        let first = source.path().join("first.txt");
        let second = source.path().join("second.bin");
        fs::write(&first, b"first compressed file").expect("first fixture");
        fs::write(&second, [0, 1, 2, 3, 250, 251, 252, 253]).expect("second fixture");
        let archive_path = output.path().join("files.tar.gz");
        let entries = vec![
            (first, "folder/first.txt".into(), 21, 0o644),
            (second, "second.bin".into(), 8, 0o600),
        ];

        write_tar_gz_archive(&archive_path, &entries, None, &CancellationToken::new())
            .expect("create TAR.GZ archive");
        validate_archive_input(archive_path.to_str().expect("archive path"))
            .expect("validate TAR.GZ archive");

        let extracted = output.path().join("extracted");
        extract_tar_gz_archive(&archive_path, &extracted, None, &CancellationToken::new())
            .expect("extract TAR.GZ archive");

        assert_eq!(
            fs::read(extracted.join("folder/first.txt")).expect("first"),
            b"first compressed file"
        );
        assert_eq!(
            fs::read(extracted.join("second.bin")).expect("second"),
            [0, 1, 2, 3, 250, 251, 252, 253]
        );
    }

    #[test]
    fn extracts_a_7z_with_nested_and_empty_entries_without_changing_source() {
        let directory = tempfile::tempdir().expect("tempdir");
        let archive_path = directory.path().join("sample.7z");
        write_7z_fixture(
            &archive_path,
            vec![
                (ArchiveEntry::new_directory("folder"), None),
                (
                    ArchiveEntry::new_file("folder/first.txt"),
                    Some(b"first file".to_vec()),
                ),
                (
                    ArchiveEntry::new_file("second.bin"),
                    Some(vec![0, 1, 2, 3, 250, 251, 252, 253]),
                ),
                (ArchiveEntry::new_file("empty.txt"), Some(Vec::new())),
            ],
        );
        let source_before = fs::read(&archive_path).expect("source before");
        let (validated_path, format) =
            validate_archive_input(archive_path.to_str().expect("archive path"))
                .expect("validate 7Z archive");
        assert_eq!(validated_path, archive_path);
        assert_eq!(format, ExtractArchiveFormat::SevenZ);

        let output = directory.path().join("output");
        let extracted = extract_7z_archive(
            &archive_path,
            &output,
            None,
            None,
            &CancellationToken::new(),
        )
        .expect("extract 7Z archive");

        assert_eq!(extracted, 18);
        assert_eq!(
            fs::read(output.join("folder/first.txt")).expect("first"),
            b"first file"
        );
        assert_eq!(
            fs::read(output.join("second.bin")).expect("second"),
            [0, 1, 2, 3, 250, 251, 252, 253]
        );
        assert_eq!(
            fs::metadata(output.join("empty.txt"))
                .expect("empty file")
                .len(),
            0
        );
        assert_eq!(
            fs::read(&archive_path).expect("source after"),
            source_before
        );
    }

    #[test]
    fn refuses_7z_path_traversal_before_creating_output() {
        let directory = tempfile::tempdir().expect("tempdir");
        let archive_path = directory.path().join("unsafe.7z");
        write_7z_fixture(
            &archive_path,
            vec![(
                ArchiveEntry::new_file("../escape.txt"),
                Some(b"blocked".to_vec()),
            )],
        );

        let output = directory.path().join("output");
        let result = extract_7z_archive(
            &archive_path,
            &output,
            None,
            None,
            &CancellationToken::new(),
        );
        assert!(matches!(
            result,
            Err(ConversionError::UnsupportedConversion { .. })
        ));
        assert!(!directory.path().join("escape.txt").exists());
        assert!(!output.exists());
    }

    #[test]
    fn rejects_7z_duplicate_and_file_directory_collisions() {
        let mut duplicate = sevenz_rust2::Archive::default();
        duplicate.files = vec![
            ArchiveEntry::new_file("Photo.txt"),
            ArchiveEntry::new_file("photo.TXT"),
        ];
        assert!(matches!(
            validate_7z_archive(&duplicate, 10, 100, MAX_7Z_DECODER_MEMORY_BYTES),
            Err(ConversionError::UnsupportedConversion { .. })
        ));

        let mut collision = sevenz_rust2::Archive::default();
        collision.files = vec![
            ArchiveEntry::new_file("folder"),
            ArchiveEntry::new_file("folder/file.txt"),
        ];
        assert!(matches!(
            validate_7z_archive(&collision, 10, 100, MAX_7Z_DECODER_MEMORY_BYTES),
            Err(ConversionError::UnsupportedConversion { .. })
        ));
    }

    #[test]
    fn rejects_7z_anti_items_links_and_special_files() {
        let mut anti = ArchiveEntry::new_file("deleted.txt");
        anti.is_anti_item = true;
        let mut archive = sevenz_rust2::Archive::default();
        archive.files = vec![anti];
        assert!(matches!(
            validate_7z_archive(&archive, 10, 100, MAX_7Z_DECODER_MEMORY_BYTES),
            Err(ConversionError::UnsupportedConversion { .. })
        ));

        let mut symlink = ArchiveEntry::new_file("link.txt");
        symlink.has_windows_attributes = true;
        symlink.windows_attributes = 0o120777 << 16;
        archive.files = vec![symlink];
        assert!(matches!(
            validate_7z_archive(&archive, 10, 100, MAX_7Z_DECODER_MEMORY_BYTES),
            Err(ConversionError::UnsupportedConversion { .. })
        ));

        let mut reparse = ArchiveEntry::new_file("reparse.txt");
        reparse.has_windows_attributes = true;
        reparse.windows_attributes = 0x400;
        archive.files = vec![reparse];
        assert!(matches!(
            validate_7z_archive(&archive, 10, 100, MAX_7Z_DECODER_MEMORY_BYTES),
            Err(ConversionError::UnsupportedConversion { .. })
        ));
    }

    #[test]
    fn enforces_7z_entry_size_and_decoder_memory_limits() {
        let mut archive = sevenz_rust2::Archive::default();
        let mut first = ArchiveEntry::new_file("first.bin");
        first.size = 4;
        let mut second = ArchiveEntry::new_file("second.bin");
        second.size = 5;
        archive.files = vec![first, second];
        assert!(matches!(
            validate_7z_archive(&archive, 1, 100, MAX_7Z_DECODER_MEMORY_BYTES),
            Err(ConversionError::UnsupportedConversion { .. })
        ));
        assert!(matches!(
            validate_7z_archive(&archive, 10, 8, MAX_7Z_DECODER_MEMORY_BYTES),
            Err(ConversionError::UnsupportedConversion { .. })
        ));

        assert_eq!(
            seven_z_coder_memory_bytes(EncoderMethod::ID_LZMA, &[0, 0, 0, 0, 32])
                .expect("LZMA dictionary"),
            512 * 1024 * 1024
        );
        assert_eq!(
            seven_z_coder_memory_bytes(EncoderMethod::ID_LZMA2, &[40]).expect("LZMA2 dictionary"),
            u32::MAX as u64
        );
        assert_eq!(
            seven_z_coder_memory_bytes(EncoderMethod::ID_PPMD, &[6, 0, 0, 0, 32])
                .expect("PPMd memory"),
            512 * 1024 * 1024
        );
        assert!(seven_z_coder_memory_bytes(EncoderMethod::ID_LZMA2, &[41]).is_err());
    }

    #[test]
    fn seven_z_extraction_honors_precancellation_without_output() {
        let directory = tempfile::tempdir().expect("tempdir");
        let archive_path = directory.path().join("cancelled.7z");
        write_7z_fixture(
            &archive_path,
            vec![(
                ArchiveEntry::new_file("large.bin"),
                Some(vec![7; BUFFER_SIZE * 2]),
            )],
        );
        let token = CancellationToken::new();
        token.cancel();
        let output = directory.path().join("output");

        let result = extract_7z_archive(&archive_path, &output, None, None, &token);

        assert!(matches!(result, Err(ConversionError::Cancelled)));
        assert!(!output.exists());
    }

    #[test]
    fn seven_z_limits_are_enforced_before_output_creation() {
        let directory = tempfile::tempdir().expect("tempdir");
        let archive_path = directory.path().join("limited.7z");
        write_7z_fixture(
            &archive_path,
            vec![(ArchiveEntry::new_file("payload.bin"), Some(vec![5; 64]))],
        );
        let output = directory.path().join("size-output");
        let result = extract_7z_archive_with_limits(
            &archive_path,
            &output,
            None,
            None,
            &CancellationToken::new(),
            10,
            63,
            MAX_7Z_DECODER_MEMORY_BYTES,
        );
        assert!(matches!(
            result,
            Err(ConversionError::UnsupportedConversion { .. })
        ));
        assert!(!output.exists());

        let output = directory.path().join("memory-output");
        let result = extract_7z_archive_with_limits(
            &archive_path,
            &output,
            None,
            None,
            &CancellationToken::new(),
            10,
            100,
            1,
        );
        assert!(matches!(
            result,
            Err(ConversionError::UnsupportedConversion { .. })
        ));
        assert!(!output.exists());
    }

    #[test]
    fn rejects_a_corrupt_7z_header() {
        let directory = tempfile::tempdir().expect("tempdir");
        let archive_path = directory.path().join("corrupt.7z");
        fs::write(&archive_path, b"not a seven zip archive").expect("corrupt fixture");

        let result = validate_archive_input(archive_path.to_str().expect("archive path"));

        assert!(matches!(result, Err(ConversionError::ProcessFailed { .. })));
    }

    #[test]
    fn seven_z_stream_copy_honors_midstream_cancellation() {
        let token = CancellationToken::new();
        let mut reader = CancelAfterFirstRead {
            contents: Cursor::new(vec![9; BUFFER_SIZE * 2]),
            token: &token,
            did_cancel: false,
        };
        let mut output = Vec::new();
        let mut extracted_bytes = 0;
        let mut buffer = vec![0; BUFFER_SIZE];

        let result = copy_7z_entry_contents(
            &mut reader,
            &mut output,
            Path::new("partial.bin"),
            &token,
            (BUFFER_SIZE * 2) as u64,
            (BUFFER_SIZE * 2) as u64,
            &mut extracted_bytes,
            &mut buffer,
        );

        assert!(matches!(result, Err(ConversionError::Cancelled)));
        assert_eq!(output.len(), BUFFER_SIZE);
        assert_eq!(extracted_bytes, BUFFER_SIZE as u64);
    }

    #[test]
    fn recognizes_only_supported_archive_names() {
        assert_eq!(
            archive_format_for_path(Path::new("bundle.tar.gz")),
            Some(ExtractArchiveFormat::TarGz)
        );
        assert_eq!(
            archive_format_for_path(Path::new("bundle.TGZ")),
            Some(ExtractArchiveFormat::TarGz)
        );
        assert_eq!(
            archive_format_for_path(Path::new("bundle.7Z")),
            Some(ExtractArchiveFormat::SevenZ)
        );
        assert_eq!(
            archive_format_for_path(Path::new("bundle.gz")),
            Some(ExtractArchiveFormat::Gzip)
        );
        assert_eq!(
            archive_stem(Path::new("bundle.tar.gz"), ExtractArchiveFormat::TarGz),
            "bundle"
        );
        assert_eq!(
            archive_stem(Path::new("bundle.tgz"), ExtractArchiveFormat::TarGz),
            "bundle"
        );
    }

    #[test]
    fn derives_single_file_gzip_output_names() {
        assert_eq!(
            gzip_output_name(Path::new("report.csv.gz")).expect("name"),
            ("report".into(), "csv".into())
        );
        assert_eq!(
            gzip_output_name(Path::new("archive.GZ")).expect("name"),
            ("archive".into(), String::new())
        );
        assert_eq!(
            gzip_output_name(Path::new("notes.txt.Gz")).expect("name"),
            ("notes".into(), "txt".into())
        );
        assert!(gzip_output_name(Path::new(".gz")).is_err());
    }

    #[test]
    fn extracts_standalone_and_multimember_gzip_files() {
        let directory = tempfile::tempdir().expect("tempdir");
        let archive_path = directory.path().join("report.csv.gz");
        let output_path = directory.path().join("report-extracted.csv");
        let mut fixture = gzip_bytes(b"name,value\nalpha,1\n");
        fixture.extend(gzip_bytes(b"beta,2\n"));
        fs::write(&archive_path, fixture).expect("write GZIP fixture");

        validate_gzip_file(&archive_path).expect("valid GZIP");
        let size = extract_gzip_file_with_limit(
            &archive_path,
            &output_path,
            None,
            &CancellationToken::new(),
            1_024,
        )
        .expect("extract GZIP");

        let expected = b"name,value\nalpha,1\nbeta,2\n";
        assert_eq!(size, expected.len() as u64);
        assert_eq!(fs::read(output_path).expect("read output"), expected);
    }

    #[test]
    fn accepts_a_valid_empty_gzip_file() {
        let directory = tempfile::tempdir().expect("tempdir");
        let archive_path = directory.path().join("empty.txt.gz");
        let output_path = directory.path().join("empty-extracted.txt");
        fs::write(&archive_path, gzip_bytes(b"")).expect("write GZIP fixture");

        let size = extract_gzip_file_with_limit(
            &archive_path,
            &output_path,
            None,
            &CancellationToken::new(),
            1_024,
        )
        .expect("extract empty GZIP");

        assert_eq!(size, 0);
        assert_eq!(fs::metadata(output_path).expect("output metadata").len(), 0);
    }

    #[test]
    fn rejects_damaged_and_oversized_gzip_output() {
        let directory = tempfile::tempdir().expect("tempdir");
        let damaged_path = directory.path().join("damaged.txt.gz");
        let damaged_output = directory.path().join("damaged.txt");
        let mut damaged = gzip_bytes(b"damaged contents");
        let crc_index = damaged.len() - 8;
        damaged[crc_index] ^= 0xff;
        fs::write(&damaged_path, damaged).expect("write damaged GZIP");
        let damaged_result = extract_gzip_file_with_limit(
            &damaged_path,
            &damaged_output,
            None,
            &CancellationToken::new(),
            1_024,
        );
        assert!(damaged_result.is_err());

        let large_path = directory.path().join("large.txt.gz");
        let large_output = directory.path().join("large.txt");
        fs::write(&large_path, gzip_bytes(b"five!")).expect("write large GZIP");
        let large_result = extract_gzip_file_with_limit(
            &large_path,
            &large_output,
            None,
            &CancellationToken::new(),
            4,
        );
        assert!(matches!(
            large_result,
            Err(ConversionError::UnsupportedConversion { .. })
        ));
    }

    #[test]
    fn cancels_gzip_before_creating_output() {
        let directory = tempfile::tempdir().expect("tempdir");
        let archive_path = directory.path().join("cancel.txt.gz");
        let output_path = directory.path().join("cancel.txt");
        fs::write(&archive_path, gzip_bytes(b"cancel me")).expect("write GZIP fixture");
        let token = CancellationToken::new();
        token.cancel();

        let result = extract_gzip_file_with_limit(&archive_path, &output_path, None, &token, 1_024);

        assert!(matches!(result, Err(ConversionError::Cancelled)));
        assert!(!output_path.exists());
    }

    #[test]
    fn refuses_tar_gz_path_traversal() {
        let directory = tempfile::tempdir().expect("tempdir");
        let archive_path = directory.path().join("unsafe.tar.gz");
        let file = File::create(&archive_path).expect("archive");
        let encoder = GzEncoder::new(file, Compression::default());
        let mut writer = tar::Builder::new(encoder);
        let mut header = tar::Header::new_gnu();
        header.set_path("safe.txt").expect("initial safe path");
        header.set_size(7);
        header.set_mode(0o644);
        header.set_entry_type(tar::EntryType::file());
        let bytes = header.as_mut_bytes();
        bytes[..100].fill(0);
        bytes[..13].copy_from_slice(b"../escape.txt");
        header.set_cksum();
        writer.append(&header, &b"blocked"[..]).expect("entry");
        let encoder = writer.into_inner().expect("finish TAR");
        encoder.finish().expect("finish gzip");

        let output = directory.path().join("output");
        let result =
            extract_tar_gz_archive(&archive_path, &output, None, &CancellationToken::new());
        assert!(matches!(
            result,
            Err(ConversionError::UnsupportedConversion { .. })
        ));
        assert!(!directory.path().join("escape.txt").exists());
    }

    #[test]
    fn refuses_tar_path_traversal() {
        let directory = tempfile::tempdir().expect("tempdir");
        let archive_path = directory.path().join("unsafe.tar");
        let file = File::create(&archive_path).expect("archive");
        let mut writer = tar::Builder::new(file);
        let mut header = tar::Header::new_gnu();
        header.set_path("safe.txt").expect("initial safe path");
        header.set_size(7);
        header.set_mode(0o644);
        header.set_entry_type(tar::EntryType::file());
        let bytes = header.as_mut_bytes();
        bytes[..100].fill(0);
        bytes[..13].copy_from_slice(b"../escape.txt");
        header.set_cksum();
        writer.append(&header, &b"blocked"[..]).expect("entry");
        writer.finish().expect("finish");

        let output = directory.path().join("output");
        let result = extract_tar_archive(&archive_path, &output, None, &CancellationToken::new());
        assert!(matches!(
            result,
            Err(ConversionError::UnsupportedConversion { .. })
        ));
        assert!(!directory.path().join("escape.txt").exists());
    }

    #[test]
    fn refuses_tar_links_and_special_entries() {
        let directory = tempfile::tempdir().expect("tempdir");
        let archive_path = directory.path().join("linked.tar");
        let file = File::create(&archive_path).expect("archive");
        let mut writer = tar::Builder::new(file);
        let mut header = tar::Header::new_gnu();
        header.set_size(0);
        header.set_mode(0o777);
        header.set_entry_type(tar::EntryType::symlink());
        header.set_link_name("target.txt").expect("link target");
        writer
            .append_data(&mut header, "link.txt", &b""[..])
            .expect("link entry");
        writer.finish().expect("finish");

        let output = directory.path().join("output");
        let result = extract_tar_archive(&archive_path, &output, None, &CancellationToken::new());
        assert!(matches!(
            result,
            Err(ConversionError::UnsupportedConversion { .. })
        ));
        assert!(!output.join("link.txt").exists());
    }

    #[test]
    fn enforces_tar_expansion_limits_before_writing() {
        let directory = tempfile::tempdir().expect("tempdir");
        let archive_path = directory.path().join("large.tar");
        let file = File::create(&archive_path).expect("archive");
        let mut writer = tar::Builder::new(file);
        let mut header = tar::Header::new_gnu();
        header.set_size(5);
        header.set_mode(0o644);
        writer
            .append_data(&mut header, "large.txt", &b"large"[..])
            .expect("file entry");
        writer.finish().expect("finish");

        let output = directory.path().join("output");
        let result = extract_tar_archive_with_limits(
            &archive_path,
            &output,
            None,
            &CancellationToken::new(),
            10,
            4,
        );
        assert!(matches!(
            result,
            Err(ConversionError::UnsupportedConversion { .. })
        ));
        assert!(!output.join("large.txt").exists());
    }

    #[test]
    fn tar_creation_honors_cancellation() {
        let directory = tempfile::tempdir().expect("tempdir");
        let input = directory.path().join("input.txt");
        fs::write(&input, b"contents").expect("fixture");
        let entries = vec![(input, "input.txt".into(), 8, 0o644)];
        let token = CancellationToken::new();
        token.cancel();

        let result = write_tar_archive(
            &directory.path().join("cancelled.tar"),
            &entries,
            None,
            &token,
        );
        assert!(matches!(result, Err(ConversionError::Cancelled)));
    }

    #[test]
    fn tar_gz_creation_honors_cancellation() {
        let directory = tempfile::tempdir().expect("tempdir");
        let input = directory.path().join("input.txt");
        fs::write(&input, b"contents").expect("fixture");
        let entries = vec![(input, "input.txt".into(), 8, 0o644)];
        let token = CancellationToken::new();
        token.cancel();

        let result = write_tar_gz_archive(
            &directory.path().join("cancelled.tar.gz"),
            &entries,
            None,
            &token,
        );
        assert!(matches!(result, Err(ConversionError::Cancelled)));
    }

    #[test]
    fn extracted_directory_never_replaces_a_directory_created_after_prepare() {
        let directory = tempfile::tempdir().expect("tempdir");
        let input = directory.path().join("documents.zip");
        fs::write(&input, b"archive fixture").expect("source archive");
        let prepared = PreparedDirectory::new(
            &input,
            ExtractArchiveFormat::Zip,
            "-extracted",
            Some(OutputOptions {
                directory: Some(directory.path().to_string_lossy().into_owned()),
                suffix: "-extracted".into(),
            }),
        )
        .expect("prepared directory");

        let raced = directory.path().join("documents-extracted");
        let numbered = directory.path().join("documents-extracted (1)");
        fs::create_dir(&raced).expect("racing directory");
        fs::write(raced.join("keep.txt"), b"existing").expect("racing contents");
        fs::create_dir(&prepared.working_path).expect("working directory");
        fs::write(prepared.working_path.join("result.txt"), b"extracted")
            .expect("working contents");

        let result = prepared.commit(9).expect("atomic directory commit");

        assert_eq!(
            fs::read(raced.join("keep.txt")).expect("racing directory survives"),
            b"existing"
        );
        assert_eq!(
            fs::read(numbered.join("result.txt")).expect("numbered extraction"),
            b"extracted"
        );
        assert_eq!(result.output_path, numbered.to_string_lossy());
    }

    #[cfg(unix)]
    #[test]
    fn rejects_direct_symbolic_link_inputs() {
        use std::os::unix::fs::symlink;

        let directory = tempfile::tempdir().expect("tempdir");
        let target = directory.path().join("target.txt");
        let link = directory.path().join("link.txt");
        fs::write(&target, b"target").expect("target");
        symlink(&target, &link).expect("symlink");

        let result = validate_create_entries(vec![ArchiveEntryRequest {
            input_path: link.to_string_lossy().into_owned(),
            archive_path: "link.txt".into(),
            folder_derived: false,
        }]);
        assert!(matches!(
            result,
            Err(ConversionError::UnsupportedConversion { .. })
        ));
    }

    #[test]
    fn refuses_zip_path_traversal() {
        let directory = tempfile::tempdir().expect("tempdir");
        let archive_path = directory.path().join("unsafe.zip");
        let file = File::create(&archive_path).expect("archive");
        let mut writer = zip::ZipWriter::new(file);
        writer
            .start_file("../escape.txt", SimpleFileOptions::default())
            .expect("entry");
        writer.write_all(b"blocked").expect("contents");
        writer.finish().expect("finish");

        let output = directory.path().join("output");
        let result = extract_zip_archive(&archive_path, &output, None, &CancellationToken::new());
        assert!(matches!(
            result,
            Err(ConversionError::UnsupportedConversion { .. })
        ));
        assert!(!directory.path().join("escape.txt").exists());
    }
}
