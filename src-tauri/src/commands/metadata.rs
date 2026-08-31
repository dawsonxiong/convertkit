use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::ffi::OsString;
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use img_parts::jpeg::{markers, Jpeg};
use img_parts::png::Png;
use img_parts::riff::{RiffChunk, RiffContent};
use img_parts::webp::{WebP, CHUNK_EXIF, CHUNK_VP8X, CHUNK_XMP};
use img_parts::{Bytes, DynImage, ImageEXIF};
use lopdf::{Dictionary, Document, Object, ObjectId, StringFormat};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::engines::{self, tool_command, ConversionResult};
use crate::error::ConversionError;
use crate::formats::{FileCategory, Format};
use crate::progress::ProgressPayload;

use super::output::{prepare_output, OutputOptions};

const MAX_IMAGE_INPUT_BYTES: u64 = 512 * 1024 * 1024;
const MAX_PDF_INPUT_BYTES: u64 = 512 * 1024 * 1024;
const MAX_MEDIA_INPUT_BYTES: u64 = 8 * 1024 * 1024 * 1024 * 1024;
const MAX_PDF_OBJECTS: usize = 500_000;
const MAX_PDF_PAGES: usize = 25_000;
const MAX_PDF_NESTING: usize = 128;
const PROBE_TIMEOUT: Duration = Duration::from_secs(15);
const WEBP_EXIF_FLAG: u8 = 0b0000_1000;
const WEBP_XMP_FLAG: u8 = 0b0000_0100;

const SENSITIVE_MEDIA_TAGS: [&str; 19] = [
    "album",
    "album_artist",
    "artist",
    "author",
    "comment",
    "composer",
    "copyright",
    "creation_time",
    "date",
    "description",
    "encoded_by",
    "genre",
    "location",
    "make",
    "model",
    "publisher",
    "synopsis",
    "title",
    "track",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MetadataRemovalEngine {
    BuiltInImage,
    BuiltInPdf,
    FfmpegStreamCopy,
}

pub(super) fn metadata_removal_engine(path: &Path) -> Option<MetadataRemovalEngine> {
    let format = path
        .extension()
        .and_then(|extension| extension.to_str())
        .and_then(Format::from_extension)?;
    if matches!(format, Format::Jpg | Format::Png | Format::WebP) {
        return Some(MetadataRemovalEngine::BuiltInImage);
    }
    if format == Format::Pdf {
        return Some(MetadataRemovalEngine::BuiltInPdf);
    }
    matches!(format.category(), FileCategory::Audio | FileCategory::Video)
        .then_some(MetadataRemovalEngine::FfmpegStreamCopy)
}

#[cfg(test)]
pub(super) fn supports_metadata_removal(path: &Path) -> bool {
    metadata_removal_engine(path).is_some()
}

pub(super) async fn remove_metadata(
    app: AppHandle,
    input_path: String,
    job_id: Option<String>,
    output_options: Option<OutputOptions>,
) -> Result<ConversionResult, ConversionError> {
    let input = PathBuf::from(&input_path);
    if !input.is_file() {
        return Err(ConversionError::InputNotFound { path: input_path });
    }
    let Some(engine) = metadata_removal_engine(&input) else {
        return Err(ConversionError::UnsupportedConversion {
            input: input_path,
            output: "metadata-free JPEG, PNG, WebP, PDF, audio, or video file".into(),
        });
    };

    let input_size = std::fs::metadata(&input)
        .map_err(|_| ConversionError::InputNotFound {
            path: input.to_string_lossy().into_owned(),
        })?
        .len();
    let maximum_size = match engine {
        MetadataRemovalEngine::BuiltInImage => MAX_IMAGE_INPUT_BYTES,
        MetadataRemovalEngine::BuiltInPdf => MAX_PDF_INPUT_BYTES,
        MetadataRemovalEngine::FfmpegStreamCopy => MAX_MEDIA_INPUT_BYTES,
    };
    if input_size > maximum_size {
        return Err(ConversionError::UnsupportedConversion {
            input: input.to_string_lossy().into_owned(),
            output: match engine {
                MetadataRemovalEngine::BuiltInImage => "images up to 512 MB".into(),
                MetadataRemovalEngine::BuiltInPdf => "PDF files up to 512 MB".into(),
                MetadataRemovalEngine::FfmpegStreamCopy => "media files up to 8 TiB".into(),
            },
        });
    }

    let extension = input
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("jpg")
        .to_ascii_lowercase();
    let prepared_output = prepare_output(&input, &extension, "-clean", output_options)?;
    let active_job = super::jobs::ActiveJobGuard::register(app.clone(), job_id)?;
    let cancel_token = active_job.cancel_token();
    let event_job_id = active_job.job_id().to_owned();
    let started = Instant::now();

    let _ = app.emit(
        "conversion-progress",
        ProgressPayload {
            job_id: event_job_id.clone(),
            percent: -1,
            stage: "Removing metadata".into(),
        },
    );

    let result = match engine {
        MetadataRemovalEngine::BuiltInImage => {
            let source = input.clone();
            let destination = prepared_output.working_path().to_path_buf();
            let worker_cancel = cancel_token.clone();
            tokio::task::spawn_blocking(move || {
                strip_file_metadata(&source, &destination, &worker_cancel)
            })
            .await
            .map_err(|error| ConversionError::ProcessFailed {
                message: format!("Metadata cleanup task failed: {error}"),
                stderr: String::new(),
                exit_code: None,
            })?
        }
        MetadataRemovalEngine::BuiltInPdf => {
            let source = input.clone();
            let destination = prepared_output.working_path().to_path_buf();
            let worker_cancel = cancel_token.clone();
            tokio::task::spawn_blocking(move || {
                strip_pdf_metadata(&source, &destination, &worker_cancel)
            })
            .await
            .map_err(|error| ConversionError::ProcessFailed {
                message: format!("PDF metadata cleanup task failed: {error}"),
                stderr: String::new(),
                exit_code: None,
            })?
        }
        MetadataRemovalEngine::FfmpegStreamCopy => {
            strip_media_metadata(
                app.clone(),
                &input,
                prepared_output.working_path(),
                event_job_id.clone(),
                cancel_token,
            )
            .await
        }
    };

    match result {
        Ok(output_size) => {
            let result = prepared_output.commit(ConversionResult {
                output_path: String::new(),
                output_paths: Vec::new(),
                output_size,
                duration_ms: started.elapsed().as_millis() as u64,
                undo_manifest: None,
            })?;
            let _ = app.emit(
                "conversion-progress",
                ProgressPayload {
                    job_id: event_job_id,
                    percent: 100,
                    stage: "Complete".into(),
                },
            );
            Ok(result)
        }
        Err(error) => Err(error),
    }
}

async fn strip_media_metadata(
    app: AppHandle,
    input: &Path,
    output: &Path,
    job_id: String,
    cancel_token: CancellationToken,
) -> Result<u64, ConversionError> {
    super::video::run_ffmpeg_job(
        output,
        media_cleanup_arguments(input, output),
        0,
        "Removing metadata",
        "Media metadata removal failed",
        app,
        job_id,
        cancel_token,
    )
    .await?;
    verify_clean_media(input, output).await?;
    engines::verification::nonempty_file(output)
}

fn media_cleanup_arguments(input: &Path, output: &Path) -> Vec<OsString> {
    vec![
        OsString::from("-i"),
        input.as_os_str().to_owned(),
        OsString::from("-map"),
        OsString::from("0"),
        OsString::from("-map_metadata"),
        OsString::from("-1"),
        OsString::from("-map_metadata:s"),
        OsString::from("-1"),
        OsString::from("-map_chapters"),
        OsString::from("-1"),
        OsString::from("-c"),
        OsString::from("copy"),
        OsString::from("-progress"),
        OsString::from("pipe:1"),
        OsString::from("-nostats"),
        OsString::from("-y"),
        output.as_os_str().to_owned(),
    ]
}

#[derive(Debug, Deserialize)]
struct MediaProbe {
    #[serde(default)]
    streams: Vec<MediaProbeStream>,
    #[serde(default)]
    format: MediaProbeFormat,
}

#[derive(Debug, Deserialize)]
struct MediaProbeStream {
    codec_type: String,
    codec_name: String,
    #[serde(default)]
    tags: BTreeMap<String, String>,
}

#[derive(Debug, Default, Deserialize)]
struct MediaProbeFormat {
    #[serde(default)]
    tags: BTreeMap<String, String>,
}

async fn verify_clean_media(input: &Path, output: &Path) -> Result<(), ConversionError> {
    let source = probe_media(input).await?;
    let cleaned = probe_media(output).await?;
    let source_streams = source
        .streams
        .iter()
        .map(|stream| (&stream.codec_type, &stream.codec_name))
        .collect::<Vec<_>>();
    let cleaned_streams = cleaned
        .streams
        .iter()
        .map(|stream| (&stream.codec_type, &stream.codec_name))
        .collect::<Vec<_>>();
    if source_streams.is_empty() || source_streams != cleaned_streams {
        engines::cleanup_partial(output);
        return Err(ConversionError::ProcessFailed {
            message: "The cleaned media did not preserve every encoded stream".into(),
            stderr: String::new(),
            exit_code: None,
        });
    }

    let has_sensitive_tag = cleaned
        .format
        .tags
        .keys()
        .chain(cleaned.streams.iter().flat_map(|stream| stream.tags.keys()))
        .any(|key| {
            let normalized = key.to_ascii_lowercase();
            SENSITIVE_MEDIA_TAGS.contains(&normalized.as_str())
        });
    if has_sensitive_tag {
        engines::cleanup_partial(output);
        return Err(ConversionError::ProcessFailed {
            message: "The cleaned media still contains private metadata".into(),
            stderr: String::new(),
            exit_code: None,
        });
    }
    Ok(())
}

async fn probe_media(path: &Path) -> Result<MediaProbe, ConversionError> {
    let output = tokio::time::timeout(
        PROBE_TIMEOUT,
        tool_command("ffprobe")
            .args([
                "-v",
                "error",
                "-show_entries",
                "stream=codec_type,codec_name:stream_tags:format_tags",
                "-of",
                "json",
            ])
            .arg(path)
            .output(),
    )
    .await
    .map_err(|_| ConversionError::Timeout {
        seconds: PROBE_TIMEOUT.as_secs(),
    })?
    .map_err(|error| ConversionError::ProcessFailed {
        message: format!("Could not inspect media metadata: {error}"),
        stderr: String::new(),
        exit_code: None,
    })?;
    if !output.status.success() {
        return Err(ConversionError::ProcessFailed {
            message: "Media metadata could not be verified".into(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            exit_code: output.status.code(),
        });
    }
    serde_json::from_slice(&output.stdout).map_err(|error| ConversionError::ProcessFailed {
        message: format!("Media metadata could not be read: {error}"),
        stderr: String::new(),
        exit_code: None,
    })
}

const PDF_METADATA_KEYS: [&[u8]; 3] = [b"Metadata", b"PieceInfo", b"LastModified"];
const PDF_TRAILER_VOLATILE_KEYS: [&[u8]; 11] = [
    b"Info",
    b"ID",
    b"Size",
    b"Prev",
    b"XRefStm",
    b"Type",
    b"W",
    b"Index",
    b"Length",
    b"Filter",
    b"DecodeParms",
];
const PDF_GEOMETRY_KEYS: [&[u8]; 7] = [
    b"MediaBox",
    b"CropBox",
    b"BleedBox",
    b"TrimBox",
    b"ArtBox",
    b"Rotate",
    b"UserUnit",
];

fn strip_pdf_metadata(
    input: &Path,
    output: &Path,
    cancel_token: &CancellationToken,
) -> Result<u64, ConversionError> {
    ensure_not_cancelled(cancel_token)?;
    let source_hash = hash_file(input, cancel_token)?;
    let preflight = Document::load_metadata(input)
        .map_err(|error| invalid_pdf(format!("Could not inspect the PDF: {error}")))?;
    if preflight.encrypted {
        return Err(ConversionError::UnsupportedConversion {
            input: input.to_string_lossy().into_owned(),
            output: "an unencrypted PDF".into(),
        });
    }
    let mut document = Document::load(input)
        .map_err(|error| invalid_pdf(format!("Could not read the PDF: {error}")))?;
    validate_pdf_limits(&document, input)?;
    reject_unsupported_pdf(&document, input)?;

    let source_id = document.trailer.get(b"ID").ok().cloned();
    let expected_digest = canonical_non_metadata_digest(&document, cancel_token)?;
    let expected_geometry = page_geometry_digest(&document, cancel_token)?;

    let mut removed_references = BTreeSet::new();
    if let Some(info) = document.trailer.remove(b"Info") {
        collect_pdf_references(&info, 0, &mut removed_references)?;
    }
    document.trailer.remove(b"ID");
    strip_pdf_metadata_keys(&mut document.trailer, 0, &mut removed_references)?;
    for object in document.objects.values_mut() {
        strip_pdf_metadata_object(object, 0, &mut removed_references)?;
    }

    let reachable = reachable_non_metadata_objects(&document, cancel_token)?;
    if removed_references
        .iter()
        .any(|object_id| reachable.contains(object_id))
    {
        return Err(invalid_pdf(
            "A metadata object is shared with document content, so it cannot be removed safely",
        ));
    }
    document
        .objects
        .retain(|object_id, _| reachable.contains(object_id));
    document.max_id = document
        .objects
        .keys()
        .map(|object_id| object_id.0)
        .max()
        .unwrap_or(0);
    document.trailer.remove(b"Prev");
    document.trailer.remove(b"XRefStm");
    document.trailer.set("ID", fresh_pdf_id());

    ensure_not_cancelled(cancel_token)?;
    write_pdf(&mut document, output, cancel_token)?;
    ensure_not_cancelled(cancel_token)?;

    let cleaned = Document::load(output)
        .map_err(|error| invalid_pdf(format!("The cleaned PDF could not be reopened: {error}")))?;
    reject_unsupported_pdf(&cleaned, output)?;
    ensure_pdf_metadata_removed(&cleaned, source_id.as_ref())?;
    validate_pdf_limits(&cleaned, output)?;

    if page_geometry_digest(&cleaned, cancel_token)? != expected_geometry {
        return Err(invalid_pdf(
            "The cleaned PDF did not preserve page order or page geometry",
        ));
    }
    if canonical_non_metadata_digest(&cleaned, cancel_token)? != expected_digest {
        return Err(invalid_pdf(
            "The cleaned PDF did not preserve the complete non-metadata document structure",
        ));
    }
    if hash_file(input, cancel_token)? != source_hash {
        return Err(invalid_pdf(
            "The source PDF changed while metadata was being removed",
        ));
    }
    engines::verification::nonempty_file(output)
}

fn validate_pdf_limits(document: &Document, input: &Path) -> Result<(), ConversionError> {
    if document.objects.len() > MAX_PDF_OBJECTS {
        return Err(ConversionError::UnsupportedConversion {
            input: input.to_string_lossy().into_owned(),
            output: format!("a PDF with at most {MAX_PDF_OBJECTS} objects"),
        });
    }
    let pages = document.page_iter().take(MAX_PDF_PAGES + 1).count();
    if pages == 0 || pages > MAX_PDF_PAGES {
        return Err(ConversionError::UnsupportedConversion {
            input: input.to_string_lossy().into_owned(),
            output: format!("a PDF with 1 to {MAX_PDF_PAGES} pages"),
        });
    }
    Ok(())
}

fn reject_unsupported_pdf(document: &Document, input: &Path) -> Result<(), ConversionError> {
    if document.is_encrypted()
        || document.was_encrypted()
        || document.trailer.get(b"Encrypt").is_ok()
    {
        return Err(ConversionError::UnsupportedConversion {
            input: input.to_string_lossy().into_owned(),
            output: "an unencrypted PDF".into(),
        });
    }
    if dictionary_contains_signature(&document.trailer, 0)?
        || document.objects.values().try_fold(false, |found, object| {
            if found {
                Ok(true)
            } else {
                object_contains_signature(object, 0)
            }
        })?
    {
        return Err(ConversionError::UnsupportedConversion {
            input: input.to_string_lossy().into_owned(),
            output: "an unsigned PDF, because rewriting a signed PDF invalidates its signature"
                .into(),
        });
    }
    Ok(())
}

fn dictionary_contains_signature(
    dictionary: &Dictionary,
    depth: usize,
) -> Result<bool, ConversionError> {
    check_pdf_depth(depth)?;
    let has_signature_name = [b"Type".as_slice(), b"FT".as_slice()].iter().any(|key| {
        dictionary
            .get(key)
            .and_then(Object::as_name)
            .is_ok_and(|name| name == b"Sig")
    });
    if has_signature_name || dictionary.get(b"ByteRange").is_ok() {
        return Ok(true);
    }
    for (_, object) in dictionary.iter() {
        if object_contains_signature(object, depth + 1)? {
            return Ok(true);
        }
    }
    Ok(false)
}

fn object_contains_signature(object: &Object, depth: usize) -> Result<bool, ConversionError> {
    check_pdf_depth(depth)?;
    match object {
        Object::Array(array) => {
            for item in array {
                if object_contains_signature(item, depth + 1)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        Object::Dictionary(dictionary) => dictionary_contains_signature(dictionary, depth + 1),
        Object::Stream(stream) => dictionary_contains_signature(&stream.dict, depth + 1),
        _ => Ok(false),
    }
}

fn strip_pdf_metadata_object(
    object: &mut Object,
    depth: usize,
    removed_references: &mut BTreeSet<ObjectId>,
) -> Result<(), ConversionError> {
    check_pdf_depth(depth)?;
    match object {
        Object::Array(array) => {
            for item in array {
                strip_pdf_metadata_object(item, depth + 1, removed_references)?;
            }
        }
        Object::Dictionary(dictionary) => {
            strip_pdf_metadata_keys(dictionary, depth + 1, removed_references)?;
        }
        Object::Stream(stream) => {
            strip_pdf_metadata_keys(&mut stream.dict, depth + 1, removed_references)?;
        }
        _ => {}
    }
    Ok(())
}

fn strip_pdf_metadata_keys(
    dictionary: &mut Dictionary,
    depth: usize,
    removed_references: &mut BTreeSet<ObjectId>,
) -> Result<(), ConversionError> {
    check_pdf_depth(depth)?;
    for key in PDF_METADATA_KEYS {
        if let Some(removed) = dictionary.remove(key) {
            collect_pdf_references(&removed, depth + 1, removed_references)?;
        }
    }
    for (_, object) in dictionary.iter_mut() {
        strip_pdf_metadata_object(object, depth + 1, removed_references)?;
    }
    Ok(())
}

fn collect_pdf_references(
    object: &Object,
    depth: usize,
    references: &mut BTreeSet<ObjectId>,
) -> Result<(), ConversionError> {
    check_pdf_depth(depth)?;
    match object {
        Object::Reference(object_id) => {
            references.insert(*object_id);
        }
        Object::Array(array) => {
            for item in array {
                collect_pdf_references(item, depth + 1, references)?;
            }
        }
        Object::Dictionary(dictionary) => {
            for (_, value) in dictionary.iter() {
                collect_pdf_references(value, depth + 1, references)?;
            }
        }
        Object::Stream(stream) => {
            for (_, value) in stream.dict.iter() {
                collect_pdf_references(value, depth + 1, references)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn reachable_non_metadata_objects(
    document: &Document,
    cancel_token: &CancellationToken,
) -> Result<BTreeSet<ObjectId>, ConversionError> {
    let mut pending = BTreeSet::new();
    collect_non_metadata_dictionary_references(&document.trailer, true, 0, &mut pending)?;
    let mut reachable = BTreeSet::new();
    while let Some(object_id) = pending.pop_first() {
        ensure_not_cancelled(cancel_token)?;
        if !reachable.insert(object_id) {
            continue;
        }
        let object = document.objects.get(&object_id).ok_or_else(|| {
            invalid_pdf(format!(
                "The PDF references a missing object {} {}",
                object_id.0, object_id.1
            ))
        })?;
        collect_non_metadata_object_references(object, 0, &mut pending)?;
    }
    Ok(reachable)
}

fn collect_non_metadata_object_references(
    object: &Object,
    depth: usize,
    references: &mut BTreeSet<ObjectId>,
) -> Result<(), ConversionError> {
    check_pdf_depth(depth)?;
    match object {
        Object::Reference(object_id) => {
            references.insert(*object_id);
        }
        Object::Array(array) => {
            for item in array {
                collect_non_metadata_object_references(item, depth + 1, references)?;
            }
        }
        Object::Dictionary(dictionary) => {
            collect_non_metadata_dictionary_references(dictionary, false, depth + 1, references)?;
        }
        Object::Stream(stream) => {
            collect_non_metadata_dictionary_references(&stream.dict, false, depth + 1, references)?;
        }
        _ => {}
    }
    Ok(())
}

fn collect_non_metadata_dictionary_references(
    dictionary: &Dictionary,
    is_trailer: bool,
    depth: usize,
    references: &mut BTreeSet<ObjectId>,
) -> Result<(), ConversionError> {
    check_pdf_depth(depth)?;
    for (key, value) in dictionary.iter() {
        if is_pdf_metadata_key(key) || (is_trailer && is_pdf_volatile_trailer_key(key)) {
            continue;
        }
        collect_non_metadata_object_references(value, depth + 1, references)?;
    }
    Ok(())
}

fn canonical_non_metadata_digest(
    document: &Document,
    cancel_token: &CancellationToken,
) -> Result<[u8; 32], ConversionError> {
    let reachable = reachable_non_metadata_objects(document, cancel_token)?;
    let mut hasher = Sha256::new();
    hash_bytes(&mut hasher, document.version.as_bytes());
    hash_bytes(&mut hasher, &document.binary_mark);
    hash_dictionary(&mut hasher, &document.trailer, true, 0)?;
    for object_id in reachable {
        ensure_not_cancelled(cancel_token)?;
        hasher.update(b"object");
        hasher.update(object_id.0.to_be_bytes());
        hasher.update(object_id.1.to_be_bytes());
        let object = document.objects.get(&object_id).ok_or_else(|| {
            invalid_pdf(format!(
                "The PDF references a missing object {} {}",
                object_id.0, object_id.1
            ))
        })?;
        hash_object(&mut hasher, object, 0)?;
    }
    Ok(hasher.finalize().into())
}

fn page_geometry_digest(
    document: &Document,
    cancel_token: &CancellationToken,
) -> Result<[u8; 32], ConversionError> {
    let mut hasher = Sha256::new();
    for page_id in document.page_iter() {
        ensure_not_cancelled(cancel_token)?;
        hasher.update(b"page");
        hasher.update(page_id.0.to_be_bytes());
        hasher.update(page_id.1.to_be_bytes());
        for key in PDF_GEOMETRY_KEYS {
            hash_bytes(&mut hasher, key);
            match inherited_page_value(document, page_id, key)? {
                Some(value) => hash_object(&mut hasher, value, 0)?,
                None => hasher.update(b"missing"),
            }
        }
    }
    Ok(hasher.finalize().into())
}

fn inherited_page_value<'a>(
    document: &'a Document,
    mut object_id: ObjectId,
    key: &[u8],
) -> Result<Option<&'a Object>, ConversionError> {
    let mut visited = HashSet::new();
    for _ in 0..MAX_PDF_NESTING {
        if !visited.insert(object_id) {
            return Err(invalid_pdf("The PDF page tree contains a reference cycle"));
        }
        let dictionary = document
            .get_dictionary(object_id)
            .map_err(|error| invalid_pdf(format!("Could not read PDF page geometry: {error}")))?;
        if let Ok(value) = dictionary.get(key) {
            return Ok(Some(value));
        }
        match dictionary.get(b"Parent").and_then(Object::as_reference) {
            Ok(parent_id) => object_id = parent_id,
            Err(_) => return Ok(None),
        }
    }
    Err(invalid_pdf("The PDF page tree is nested too deeply"))
}

fn hash_object(hasher: &mut Sha256, object: &Object, depth: usize) -> Result<(), ConversionError> {
    check_pdf_depth(depth)?;
    match object {
        Object::Null => hasher.update(b"null"),
        Object::Boolean(value) => {
            hasher.update(b"boolean");
            hasher.update([u8::from(*value)]);
        }
        Object::Integer(value) => {
            hasher.update(b"integer");
            hasher.update(value.to_be_bytes());
        }
        Object::Real(value) => {
            hasher.update(b"real");
            hasher.update(value.to_bits().to_be_bytes());
        }
        Object::Name(value) => {
            hasher.update(b"name");
            hash_bytes(hasher, value);
        }
        Object::String(value, format) => {
            hasher.update(b"string");
            hasher.update([match format {
                StringFormat::Literal => 0,
                StringFormat::Hexadecimal => 1,
            }]);
            hash_bytes(hasher, value);
        }
        Object::Array(array) => {
            hasher.update(b"array");
            hasher.update((array.len() as u64).to_be_bytes());
            for item in array {
                hash_object(hasher, item, depth + 1)?;
            }
        }
        Object::Dictionary(dictionary) => hash_dictionary(hasher, dictionary, false, depth + 1)?,
        Object::Stream(stream) => {
            hasher.update(b"stream");
            hash_dictionary(hasher, &stream.dict, false, depth + 1)?;
            hash_bytes(hasher, &stream.content);
        }
        Object::Reference(object_id) => {
            hasher.update(b"reference");
            hasher.update(object_id.0.to_be_bytes());
            hasher.update(object_id.1.to_be_bytes());
        }
    }
    Ok(())
}

fn hash_dictionary(
    hasher: &mut Sha256,
    dictionary: &Dictionary,
    is_trailer: bool,
    depth: usize,
) -> Result<(), ConversionError> {
    check_pdf_depth(depth)?;
    hasher.update(b"dictionary");
    let mut entries = dictionary
        .iter()
        .filter(|(key, _)| {
            !is_pdf_metadata_key(key) && !(is_trailer && is_pdf_volatile_trailer_key(key))
        })
        .collect::<Vec<_>>();
    entries.sort_by_key(|(key, _)| *key);
    hasher.update((entries.len() as u64).to_be_bytes());
    for (key, value) in entries {
        hash_bytes(hasher, key);
        hash_object(hasher, value, depth + 1)?;
    }
    Ok(())
}

fn hash_bytes(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update((bytes.len() as u64).to_be_bytes());
    hasher.update(bytes);
}

fn is_pdf_metadata_key(key: &[u8]) -> bool {
    PDF_METADATA_KEYS.contains(&key)
}

fn is_pdf_volatile_trailer_key(key: &[u8]) -> bool {
    PDF_TRAILER_VOLATILE_KEYS.contains(&key)
}

fn ensure_pdf_metadata_removed(
    document: &Document,
    source_id: Option<&Object>,
) -> Result<(), ConversionError> {
    if document.trailer.get(b"Info").is_ok() {
        return Err(invalid_pdf(
            "The cleaned PDF still contains a document information dictionary",
        ));
    }
    ensure_no_pdf_metadata_keys(&document.trailer, 0)?;
    for object in document.objects.values() {
        ensure_no_pdf_metadata_object(object, 0)?;
    }
    let cleaned_id = document
        .trailer
        .get(b"ID")
        .map_err(|_| invalid_pdf("The cleaned PDF has no fresh document ID"))?;
    if source_id.is_some_and(|old_id| old_id == cleaned_id) || !valid_fresh_pdf_id(cleaned_id) {
        return Err(invalid_pdf(
            "The cleaned PDF did not receive a fresh document ID",
        ));
    }
    Ok(())
}

fn ensure_no_pdf_metadata_object(object: &Object, depth: usize) -> Result<(), ConversionError> {
    check_pdf_depth(depth)?;
    match object {
        Object::Array(array) => {
            for item in array {
                ensure_no_pdf_metadata_object(item, depth + 1)?;
            }
        }
        Object::Dictionary(dictionary) => ensure_no_pdf_metadata_keys(dictionary, depth + 1)?,
        Object::Stream(stream) => ensure_no_pdf_metadata_keys(&stream.dict, depth + 1)?,
        _ => {}
    }
    Ok(())
}

fn ensure_no_pdf_metadata_keys(
    dictionary: &Dictionary,
    depth: usize,
) -> Result<(), ConversionError> {
    check_pdf_depth(depth)?;
    if PDF_METADATA_KEYS
        .iter()
        .any(|key| dictionary.get(key).is_ok())
    {
        return Err(invalid_pdf(
            "The cleaned PDF still contains private metadata",
        ));
    }
    for (_, object) in dictionary.iter() {
        ensure_no_pdf_metadata_object(object, depth + 1)?;
    }
    Ok(())
}

fn fresh_pdf_id() -> Object {
    Object::Array(vec![
        Object::String(
            Uuid::new_v4().as_bytes().to_vec(),
            StringFormat::Hexadecimal,
        ),
        Object::String(
            Uuid::new_v4().as_bytes().to_vec(),
            StringFormat::Hexadecimal,
        ),
    ])
}

fn valid_fresh_pdf_id(object: &Object) -> bool {
    let Ok(values) = object.as_array() else {
        return false;
    };
    values.len() == 2
        && values.iter().all(|value| {
            matches!(value, Object::String(bytes, StringFormat::Hexadecimal) if bytes.len() == 16)
        })
}

fn write_pdf(
    document: &mut Document,
    output: &Path,
    cancel_token: &CancellationToken,
) -> Result<(), ConversionError> {
    let file = File::create(output)
        .map_err(|error| invalid_pdf(format!("Could not create the cleaned PDF: {error}")))?;
    let mut writer = CancellableWriter {
        inner: BufWriter::new(file),
        cancel_token,
    };
    if let Err(error) = document.save_to(&mut writer) {
        if cancel_token.is_cancelled() {
            return Err(ConversionError::Cancelled);
        }
        return Err(invalid_pdf(format!(
            "Could not write the cleaned PDF: {error}"
        )));
    }
    writer
        .flush()
        .map_err(|error| invalid_pdf(format!("Could not finish the cleaned PDF: {error}")))
}

struct CancellableWriter<'a> {
    inner: BufWriter<File>,
    cancel_token: &'a CancellationToken,
}

impl Write for CancellableWriter<'_> {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        if self.cancel_token.is_cancelled() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "PDF metadata removal cancelled",
            ));
        }
        self.inner.write(buffer)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        if self.cancel_token.is_cancelled() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "PDF metadata removal cancelled",
            ));
        }
        self.inner.flush()
    }
}

fn hash_file(path: &Path, cancel_token: &CancellationToken) -> Result<[u8; 32], ConversionError> {
    let file = File::open(path)
        .map_err(|error| invalid_pdf(format!("Could not verify the source PDF: {error}")))?;
    let mut reader = BufReader::new(file);
    let mut buffer = [0_u8; 64 * 1024];
    let mut hasher = Sha256::new();
    loop {
        ensure_not_cancelled(cancel_token)?;
        let read = reader
            .read(&mut buffer)
            .map_err(|error| invalid_pdf(format!("Could not verify the source PDF: {error}")))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher.finalize().into())
}

fn check_pdf_depth(depth: usize) -> Result<(), ConversionError> {
    if depth > MAX_PDF_NESTING {
        return Err(invalid_pdf("The PDF object graph is nested too deeply"));
    }
    Ok(())
}

fn ensure_not_cancelled(cancel_token: &CancellationToken) -> Result<(), ConversionError> {
    if cancel_token.is_cancelled() {
        Err(ConversionError::Cancelled)
    } else {
        Ok(())
    }
}

fn invalid_pdf(message: impl Into<String>) -> ConversionError {
    ConversionError::ProcessFailed {
        message: format!("Could not remove PDF metadata: {}", message.into()),
        stderr: String::new(),
        exit_code: None,
    }
}

fn strip_file_metadata(
    input: &Path,
    output: &Path,
    cancel_token: &CancellationToken,
) -> Result<u64, ConversionError> {
    if cancel_token.is_cancelled() {
        return Err(ConversionError::Cancelled);
    }

    let source = std::fs::read(input).map_err(|error| ConversionError::ProcessFailed {
        message: format!("Could not read the image: {error}"),
        stderr: String::new(),
        exit_code: None,
    })?;
    let cleaned = strip_metadata_bytes(&source)?;

    if cancel_token.is_cancelled() {
        return Err(ConversionError::Cancelled);
    }
    std::fs::write(output, &cleaned).map_err(|error| ConversionError::ProcessFailed {
        message: format!("Could not write the cleaned image: {error}"),
        stderr: String::new(),
        exit_code: None,
    })?;
    if cancel_token.is_cancelled() {
        let _ = std::fs::remove_file(output);
        return Err(ConversionError::Cancelled);
    }

    Ok(cleaned.len() as u64)
}

fn strip_metadata_bytes(source: &[u8]) -> Result<Vec<u8>, ConversionError> {
    let image = DynImage::from_bytes(Bytes::copy_from_slice(source))
        .map_err(|error| invalid_image(error.to_string()))?
        .ok_or_else(|| invalid_image("Only JPEG, PNG, and WebP are supported"))?;
    let orientation = image
        .exif()
        .as_deref()
        .and_then(read_exif_orientation)
        .filter(|orientation| (2..=8).contains(orientation));

    let mut output = Vec::with_capacity(source.len());
    match image {
        DynImage::Jpeg(mut image) => {
            clean_jpeg(&mut image, orientation);
            image
                .encoder()
                .write_to(&mut output)
                .map_err(|error| invalid_image(error.to_string()))?;
        }
        DynImage::Png(mut image) => {
            clean_png(&mut image, orientation);
            image
                .encoder()
                .write_to(&mut output)
                .map_err(|error| invalid_image(error.to_string()))?;
        }
        DynImage::WebP(mut image) => {
            clean_webp(&mut image, orientation);
            image
                .encoder()
                .write_to(&mut output)
                .map_err(|error| invalid_image(error.to_string()))?;
        }
    }
    Ok(output)
}

fn clean_jpeg(image: &mut Jpeg, orientation: Option<u16>) {
    image.segments_mut().retain(|segment| {
        let marker = segment.marker();
        if marker == markers::COM {
            return false;
        }
        if !(markers::APP0..=markers::APP15).contains(&marker) {
            return true;
        }

        marker == markers::APP0
            || marker == markers::APP14
            || (marker == markers::APP2 && segment.contents().starts_with(b"ICC_PROFILE\0"))
    });
    image.set_exif(orientation.map(minimal_orientation_exif));
}

fn clean_png(image: &mut Png, orientation: Option<u16>) {
    const PRIVATE_CHUNKS: [[u8; 4]; 5] = [*b"eXIf", *b"tEXt", *b"zTXt", *b"iTXt", *b"tIME"];
    image
        .chunks_mut()
        .retain(|chunk| !PRIVATE_CHUNKS.contains(&chunk.kind()));
    image.set_exif(orientation.map(minimal_orientation_exif));
}

fn clean_webp(image: &mut WebP, orientation: Option<u16>) {
    image.remove_chunks_by_id(CHUNK_EXIF);
    image.remove_chunks_by_id(CHUNK_XMP);

    if let Some(orientation) = orientation {
        image.chunks_mut().push(RiffChunk::new(
            CHUNK_EXIF,
            RiffContent::Data(minimal_orientation_exif(orientation)),
        ));
    }

    if let Some(vp8x) = image
        .chunks_mut()
        .iter_mut()
        .find(|chunk| chunk.id() == CHUNK_VP8X)
    {
        if let RiffContent::Data(data) = vp8x.content_mut() {
            let mut updated = data.to_vec();
            if let Some(flags) = updated.first_mut() {
                *flags &= !(WEBP_EXIF_FLAG | WEBP_XMP_FLAG);
                if orientation.is_some() {
                    *flags |= WEBP_EXIF_FLAG;
                }
            }
            *data = Bytes::from(updated);
        }
    }
}

fn read_exif_orientation(exif: &[u8]) -> Option<u16> {
    if exif.len() < 8 {
        return None;
    }
    let little_endian = match &exif[0..2] {
        b"II" => true,
        b"MM" => false,
        _ => return None,
    };
    let read_u16 = |offset: usize| {
        let bytes: [u8; 2] = exif.get(offset..offset + 2)?.try_into().ok()?;
        Some(if little_endian {
            u16::from_le_bytes(bytes)
        } else {
            u16::from_be_bytes(bytes)
        })
    };
    let read_u32 = |offset: usize| {
        let bytes: [u8; 4] = exif.get(offset..offset + 4)?.try_into().ok()?;
        Some(if little_endian {
            u32::from_le_bytes(bytes)
        } else {
            u32::from_be_bytes(bytes)
        })
    };

    if read_u16(2)? != 42 {
        return None;
    }
    let ifd_offset = usize::try_from(read_u32(4)?).ok()?;
    let entry_count = usize::from(read_u16(ifd_offset)?);
    for index in 0..entry_count {
        let offset = ifd_offset.checked_add(2 + index.checked_mul(12)?)?;
        if read_u16(offset)? == 0x0112 && read_u16(offset + 2)? == 3 && read_u32(offset + 4)? == 1 {
            return read_u16(offset + 8);
        }
    }
    None
}

fn minimal_orientation_exif(orientation: u16) -> Bytes {
    let mut exif = Vec::with_capacity(26);
    exif.extend_from_slice(b"II");
    exif.extend_from_slice(&42u16.to_le_bytes());
    exif.extend_from_slice(&8u32.to_le_bytes());
    exif.extend_from_slice(&1u16.to_le_bytes());
    exif.extend_from_slice(&0x0112u16.to_le_bytes());
    exif.extend_from_slice(&3u16.to_le_bytes());
    exif.extend_from_slice(&1u32.to_le_bytes());
    exif.extend_from_slice(&orientation.to_le_bytes());
    exif.extend_from_slice(&0u16.to_le_bytes());
    exif.extend_from_slice(&0u32.to_le_bytes());
    Bytes::from(exif)
}

fn invalid_image(message: impl Into<String>) -> ConversionError {
    ConversionError::ProcessFailed {
        message: format!("Could not remove image metadata: {}", message.into()),
        stderr: String::new(),
        exit_code: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;
    use img_parts::jpeg::JpegSegment;
    use img_parts::png::PngChunk;
    use img_parts::webp::{CHUNK_ICCP, CHUNK_VP8};
    use img_parts::ImageICC;
    use lopdf::dictionary;

    const JPEG_2X2: &str = "/9j/4AAQSkZJRgABAQAAAQABAAD/2wBDAAMCAgICAgMCAgIDAwMDBAYEBAQEBAgGBgUGCQgKCgkICQkKDA8MCgsOCwkJDRENDg8QEBEQCgwSExIQEw8QEBD/2wBDAQMDAwQDBAgEBAgQCwkLEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBD/wAARCAACAAIDAREAAhEBAxEB/8QAFAABAAAAAAAAAAAAAAAAAAAAB//EABQQAQAAAAAAAAAAAAAAAAAAAAD/xAAVAQEBAAAAAAAAAAAAAAAAAAAHCP/EABQRAQAAAAAAAAAAAAAAAAAAAAD/2gAMAwEAAhEDEQA/AC9UAkf/2Q==";
    const PNG_2X2: &str = "iVBORw0KGgoAAAANSUhEUgAAAAIAAAACAQMAAABIeJ9nAAAAIGNIUk0AAHomAACAhAAA+gAAAIDoAAB1MAAA6mAAADqYAAAXcJy6UTwAAAAGUExURTNmzP///w3eOY0AAAABYktHRAH/Ai3eAAAAB3RJTUUH6ggdFxg1JMm/SAAAACV0RVh0ZGF0ZTpjcmVhdGUAMjAyNi0wOC0yOVQyMzoyNDo1MyswMDowMAqgGs8AAAAldEVYdGRhdGU6bW9kaWZ5ADIwMjYtMDgtMjlUMjM6MjQ6NTMrMDA6MDB7/aJzAAAAKHRFWHRkYXRlOnRpbWVzdGFtcAAyMDI2LTA4LTI5VDIzOjI0OjUzKzAwOjAwLOiDrAAAAAxJREFUCNdjYGBgAAAABAABJzQnCgAAAABJRU5ErkJggg==";
    const WEBP_2X2: &str =
        "UklGRjYAAABXRUJQVlA4ICoAAADQAQCdASoCAAIAAgA0JaACdLoB+AADsAD+7L2P/PTNeYP8nP+3Jl5tsAA=";

    fn fixture(encoded: &str) -> Vec<u8> {
        STANDARD.decode(encoded).expect("fixture base64")
    }

    #[test]
    fn reads_and_rebuilds_orientation_only_exif() {
        for orientation in 2..=8 {
            let exif = minimal_orientation_exif(orientation);
            assert_eq!(read_exif_orientation(&exif), Some(orientation));
        }
    }

    #[test]
    fn reads_big_endian_orientation() {
        let mut exif = Vec::new();
        exif.extend_from_slice(b"MM");
        exif.extend_from_slice(&42u16.to_be_bytes());
        exif.extend_from_slice(&8u32.to_be_bytes());
        exif.extend_from_slice(&1u16.to_be_bytes());
        exif.extend_from_slice(&0x0112u16.to_be_bytes());
        exif.extend_from_slice(&3u16.to_be_bytes());
        exif.extend_from_slice(&1u32.to_be_bytes());
        exif.extend_from_slice(&6u16.to_be_bytes());
        exif.extend_from_slice(&0u16.to_be_bytes());
        exif.extend_from_slice(&0u32.to_be_bytes());
        assert_eq!(read_exif_orientation(&exif), Some(6));
    }

    #[test]
    fn metadata_removal_routes_only_formats_with_non_reencoding_paths() {
        assert!(supports_metadata_removal(Path::new("photo.JPG")));
        assert!(supports_metadata_removal(Path::new("photo.png")));
        assert!(supports_metadata_removal(Path::new("photo.webp")));
        assert!(supports_metadata_removal(Path::new("clip.mov")));
        assert!(supports_metadata_removal(Path::new("clip.webm")));
        assert!(supports_metadata_removal(Path::new("recording.mp3")));
        assert!(supports_metadata_removal(Path::new("recording.flac")));
        assert!(!supports_metadata_removal(Path::new("photo.heic")));
        assert!(!supports_metadata_removal(Path::new("photo.tiff")));
        assert!(supports_metadata_removal(Path::new("document.pdf")));
    }

    fn tagged_pdf_document() -> Document {
        let mut document = Document::with_version("1.7");
        let pages_id = document.new_object_id();
        let xmp_id = document.add_object(lopdf::Stream::new(
            lopdf::dictionary! {
                "Type" => "Metadata",
                "Subtype" => "XML",
            },
            b"private-xmp-marker".to_vec(),
        ));
        let nested_xmp_id = document.add_object(lopdf::Stream::new(
            lopdf::dictionary! {
                "Type" => "Metadata",
                "Subtype" => "XML",
            },
            b"nested-private-xmp-marker".to_vec(),
        ));
        let attachment_id = document.add_object(lopdf::Stream::new(
            lopdf::dictionary! {
                "Type" => "EmbeddedFile",
                "Metadata" => Object::Reference(nested_xmp_id),
            },
            b"preserved-attachment-content".to_vec(),
        ));
        let file_spec_id = document.add_object(lopdf::dictionary! {
            "Type" => "Filespec",
            "F" => Object::string_literal("notes.txt"),
            "EF" => lopdf::dictionary! { "F" => Object::Reference(attachment_id) },
        });
        let annotation_id = document.add_object(lopdf::dictionary! {
            "Type" => "Annot",
            "Subtype" => "Widget",
            "FT" => "Tx",
            "T" => Object::string_literal("preserved-form-field"),
            "Rect" => vec![10.into(), 10.into(), 120.into(), 40.into()],
        });
        let content_one_id = document.add_object(lopdf::Stream::new(
            lopdf::dictionary! {},
            b"q 0 0 100 100 re S Q preserved-page-one".to_vec(),
        ));
        let content_two_id = document.add_object(lopdf::Stream::new(
            lopdf::dictionary! {},
            b"q 10 10 50 50 re f Q preserved-page-two".to_vec(),
        ));
        let page_one_id = document.add_object(lopdf::dictionary! {
            "Type" => "Page",
            "Parent" => Object::Reference(pages_id),
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "CropBox" => vec![12.into(), 12.into(), 600.into(), 780.into()],
            "Rotate" => 90,
            "Resources" => lopdf::dictionary! {},
            "Contents" => Object::Reference(content_one_id),
            "Annots" => vec![Object::Reference(annotation_id)],
            "Metadata" => Object::Reference(xmp_id),
            "PieceInfo" => lopdf::dictionary! {
                "PrivateApp" => lopdf::dictionary! {
                    "LastModified" => Object::string_literal("D:20260831010101Z"),
                    "Private" => Object::string_literal("private-piece-marker"),
                },
            },
            "LastModified" => Object::string_literal("D:20260831010101Z"),
        });
        let page_two_id = document.add_object(lopdf::dictionary! {
            "Type" => "Page",
            "Parent" => Object::Reference(pages_id),
            "MediaBox" => vec![0.into(), 0.into(), 400.into(), 300.into()],
            "Resources" => lopdf::dictionary! {},
            "Contents" => Object::Reference(content_two_id),
        });
        document.objects.insert(
            pages_id,
            Object::Dictionary(lopdf::dictionary! {
                "Type" => "Pages",
                "Kids" => vec![Object::Reference(page_one_id), Object::Reference(page_two_id)],
                "Count" => 2,
            }),
        );

        let outline_item_id = document.new_object_id();
        let outlines_id = document.add_object(lopdf::dictionary! {
            "Type" => "Outlines",
            "First" => Object::Reference(outline_item_id),
            "Last" => Object::Reference(outline_item_id),
            "Count" => 1,
        });
        document.objects.insert(
            outline_item_id,
            Object::Dictionary(lopdf::dictionary! {
                "Title" => Object::string_literal("preserved-outline"),
                "Parent" => Object::Reference(outlines_id),
                "Dest" => vec![Object::Reference(page_one_id), Object::Name(b"Fit".to_vec())],
            }),
        );
        let catalog_id = document.add_object(lopdf::dictionary! {
            "Type" => "Catalog",
            "Pages" => Object::Reference(pages_id),
            "Metadata" => Object::Reference(xmp_id),
            "Outlines" => Object::Reference(outlines_id),
            "AcroForm" => lopdf::dictionary! {
                "Fields" => vec![Object::Reference(annotation_id)],
            },
            "Names" => lopdf::dictionary! {
                "EmbeddedFiles" => lopdf::dictionary! {
                    "Names" => vec![
                        Object::string_literal("notes.txt"),
                        Object::Reference(file_spec_id),
                    ],
                },
            },
        });
        let info_id = document.add_object(lopdf::dictionary! {
            "Title" => Object::string_literal("private-info-marker"),
            "Author" => Object::string_literal("Private Person"),
            "ModDate" => Object::string_literal("D:20260831010101Z"),
        });
        document.add_object(Object::string_literal("orphan-private-marker"));
        document.trailer.set("Root", Object::Reference(catalog_id));
        document.trailer.set("Info", Object::Reference(info_id));
        document.trailer.set(
            "ID",
            Object::Array(vec![
                Object::String(vec![1_u8; 16], StringFormat::Hexadecimal),
                Object::String(vec![2_u8; 16], StringFormat::Hexadecimal),
            ]),
        );
        document
    }

    #[test]
    fn pdf_cleanup_removes_metadata_and_preserves_complete_structure() {
        let directory = tempfile::tempdir().expect("fixture directory");
        let input = directory.path().join("tagged.pdf");
        let output = directory.path().join("clean.pdf");
        let mut fixture = tagged_pdf_document();
        fixture.save(&input).expect("save PDF fixture");
        let base_bytes = std::fs::read(&input).expect("base PDF bytes");
        let base_document = Document::load(&input).expect("load base PDF");
        let mut incremental = lopdf::IncrementalDocument::create_from(base_bytes, base_document);
        let current_info_id = incremental.new_document.add_object(lopdf::dictionary! {
            "Title" => Object::string_literal("current-private-info-marker"),
            "ModDate" => Object::string_literal("D:20260831020202Z"),
        });
        incremental
            .new_document
            .trailer
            .set("Info", Object::Reference(current_info_id));
        incremental
            .save(&input)
            .expect("append incremental metadata revision");
        let source_bytes = std::fs::read(&input).expect("source bytes");

        strip_pdf_metadata(&input, &output, &CancellationToken::new()).expect("clean PDF");

        assert_eq!(std::fs::read(&input).expect("source remains"), source_bytes);
        let cleaned_bytes = std::fs::read(&output).expect("cleaned bytes");
        for removed in [
            b"private-xmp-marker".as_slice(),
            b"nested-private-xmp-marker".as_slice(),
            b"private-piece-marker".as_slice(),
            b"private-info-marker".as_slice(),
            b"current-private-info-marker".as_slice(),
            b"orphan-private-marker".as_slice(),
        ] {
            assert!(!cleaned_bytes
                .windows(removed.len())
                .any(|bytes| bytes == removed));
        }
        for preserved in [
            b"preserved-page-one".as_slice(),
            b"preserved-page-two".as_slice(),
            b"preserved-attachment-content".as_slice(),
            b"preserved-form-field".as_slice(),
            b"preserved-outline".as_slice(),
        ] {
            assert!(cleaned_bytes
                .windows(preserved.len())
                .any(|bytes| bytes == preserved));
        }

        let cleaned = Document::load(&output).expect("reload cleaned PDF");
        ensure_pdf_metadata_removed(
            &cleaned,
            Some(&Object::Array(vec![
                Object::String(vec![1_u8; 16], StringFormat::Hexadecimal),
                Object::String(vec![2_u8; 16], StringFormat::Hexadecimal),
            ])),
        )
        .expect("metadata removed");
        assert_eq!(cleaned.page_iter().count(), 2);
        assert!(cleaned.catalog().expect("catalog").get(b"Outlines").is_ok());
        assert!(cleaned.catalog().expect("catalog").get(b"AcroForm").is_ok());
        assert!(cleaned.catalog().expect("catalog").get(b"Names").is_ok());
    }

    #[test]
    fn pdf_cleanup_rejects_signed_and_encrypted_documents() {
        let mut signed = tagged_pdf_document();
        signed.add_object(lopdf::dictionary! {
            "Type" => "Sig",
            "ByteRange" => vec![0.into(), 10.into(), 20.into(), 30.into()],
            "Contents" => Object::String(vec![0_u8; 32], StringFormat::Hexadecimal),
        });
        let signed_error = reject_unsupported_pdf(&signed, Path::new("signed.pdf"))
            .expect_err("signed PDFs must be rejected");
        assert!(matches!(
            signed_error,
            ConversionError::UnsupportedConversion { .. }
        ));

        let mut encrypted = tagged_pdf_document();
        let encrypt_id = encrypted.add_object(lopdf::dictionary! {
            "Filter" => "Standard",
            "V" => 1,
            "R" => 2,
        });
        encrypted
            .trailer
            .set("Encrypt", Object::Reference(encrypt_id));
        let encrypted_error = reject_unsupported_pdf(&encrypted, Path::new("encrypted.pdf"))
            .expect_err("encrypted PDFs must be rejected");
        assert!(matches!(
            encrypted_error,
            ConversionError::UnsupportedConversion { .. }
        ));
    }

    #[test]
    fn pdf_cleanup_honors_preflight_cancellation() {
        let directory = tempfile::tempdir().expect("fixture directory");
        let input = directory.path().join("tagged.pdf");
        let output = directory.path().join("clean.pdf");
        tagged_pdf_document()
            .save(&input)
            .expect("save PDF fixture");
        let cancellation = CancellationToken::new();
        cancellation.cancel();

        assert!(matches!(
            strip_pdf_metadata(&input, &output, &cancellation),
            Err(ConversionError::Cancelled)
        ));
        assert!(!output.exists());
    }

    #[tokio::test]
    async fn media_cleanup_preserves_stream_codecs_and_removes_private_tags() {
        if engines::resolve_tool("ffmpeg").is_none() || engines::resolve_tool("ffprobe").is_none() {
            return;
        }

        let directory = tempfile::tempdir().expect("fixture directory");
        let input = directory.path().join("tagged.mp4");
        let output = directory.path().join("clean.mp4");
        let generated = tool_command("ffmpeg")
            .args([
                "-f",
                "lavfi",
                "-i",
                "color=c=blue:s=64x64:r=5",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=1000:sample_rate=44100",
                "-t",
                "0.4",
                "-map",
                "0:v",
                "-map",
                "1:a",
                "-c:v",
                "mpeg4",
                "-c:a",
                "aac",
                "-metadata",
                "title=Private title",
                "-metadata",
                "artist=Private artist",
                "-y",
            ])
            .arg(&input)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .await
            .expect("generate tagged media fixture");
        assert!(generated.success());

        let source = probe_media(&input).await.expect("probe source fixture");
        assert!(source
            .format
            .tags
            .keys()
            .any(|key| matches!(key.to_ascii_lowercase().as_str(), "title" | "artist")));

        let cleaned = tool_command("ffmpeg")
            .args(media_cleanup_arguments(&input, &output))
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .await
            .expect("strip media metadata");
        assert!(cleaned.success());
        verify_clean_media(&input, &output)
            .await
            .expect("verify clean media");

        let cleaned = probe_media(&output).await.expect("probe clean fixture");
        assert!(cleaned
            .format
            .tags
            .keys()
            .chain(cleaned.streams.iter().flat_map(|stream| stream.tags.keys()))
            .all(|key| !SENSITIVE_MEDIA_TAGS.contains(&key.to_ascii_lowercase().as_str())));
    }

    #[test]
    fn jpeg_cleanup_preserves_scan_orientation_and_structural_markers() {
        let original = Jpeg::from_bytes(Bytes::from(fixture(JPEG_2X2))).expect("JPEG fixture");
        let original_scan = original
            .segments()
            .iter()
            .find(|segment| segment.marker() == markers::SOS)
            .expect("JPEG scan")
            .clone();
        let mut tagged = original;
        tagged.set_exif(Some(minimal_orientation_exif(6)));
        tagged.segments_mut().insert(
            1,
            JpegSegment::new_with_contents(
                markers::APP1,
                Bytes::from_static(b"http://ns.adobe.com/xap/1.0/\0private XMP"),
            ),
        );
        tagged.segments_mut().insert(
            2,
            JpegSegment::new_with_contents(markers::APP13, Bytes::from_static(b"private IPTC")),
        );
        tagged.segments_mut().insert(
            3,
            JpegSegment::new_with_contents(markers::COM, Bytes::from_static(b"private comment")),
        );
        let mut source = Vec::new();
        tagged.encoder().write_to(&mut source).expect("tagged JPEG");

        let cleaned = strip_metadata_bytes(&source).expect("clean JPEG");
        let cleaned = Jpeg::from_bytes(Bytes::from(cleaned)).expect("parse clean JPEG");
        let cleaned_scan = cleaned
            .segments()
            .iter()
            .find(|segment| segment.marker() == markers::SOS)
            .expect("clean JPEG scan");

        assert_eq!(cleaned_scan, &original_scan);
        assert_eq!(
            cleaned.exif().as_deref().and_then(read_exif_orientation),
            Some(6)
        );
        assert!(cleaned
            .segments()
            .iter()
            .all(|segment| segment.marker() != markers::APP13 && segment.marker() != markers::COM));
        assert!(cleaned
            .segments()
            .iter()
            .any(|segment| segment.marker() == markers::APP0));
    }

    #[test]
    fn png_cleanup_preserves_pixels_color_profile_and_orientation() {
        let mut tagged = Png::from_bytes(Bytes::from(fixture(PNG_2X2))).expect("PNG fixture");
        let original_pixels = tagged
            .chunk_by_type(*b"IDAT")
            .expect("PNG pixels")
            .contents()
            .clone();
        tagged.set_icc_profile(Some(Bytes::from_static(b"test color profile")));
        tagged.set_exif(Some(minimal_orientation_exif(8)));
        let insert_at = tagged.chunks().len() - 1;
        tagged.chunks_mut().insert(
            insert_at,
            PngChunk::new(*b"iTXt", Bytes::from_static(b"private XMP")),
        );
        let mut source = Vec::new();
        tagged.encoder().write_to(&mut source).expect("tagged PNG");

        let cleaned = strip_metadata_bytes(&source).expect("clean PNG");
        let cleaned = Png::from_bytes(Bytes::from(cleaned)).expect("parse clean PNG");

        assert_eq!(
            cleaned.chunk_by_type(*b"IDAT").expect("pixels").contents(),
            &original_pixels
        );
        assert_eq!(
            cleaned.icc_profile().as_deref(),
            Some(b"test color profile".as_slice())
        );
        assert_eq!(
            cleaned.exif().as_deref().and_then(read_exif_orientation),
            Some(8)
        );
        assert!(cleaned
            .chunks()
            .iter()
            .all(|chunk| ![*b"tEXt", *b"zTXt", *b"iTXt", *b"tIME"].contains(&chunk.kind())));
    }

    #[test]
    fn webp_cleanup_preserves_bitstream_and_color_profile() {
        let mut tagged = WebP::from_bytes(Bytes::from(fixture(WEBP_2X2))).expect("WebP fixture");
        let original_pixels = tagged
            .chunk_by_id(CHUNK_VP8)
            .expect("WebP pixels")
            .content()
            .data()
            .expect("pixel data")
            .clone();
        tagged.set_icc_profile(Some(Bytes::from_static(b"test color profile")));
        tagged.set_exif(Some(minimal_orientation_exif(7)));
        tagged.chunks_mut().push(RiffChunk::new(
            CHUNK_XMP,
            RiffContent::Data(Bytes::from_static(b"private XMP")),
        ));
        let vp8x = tagged
            .chunks_mut()
            .iter_mut()
            .find(|chunk| chunk.id() == CHUNK_VP8X)
            .expect("VP8X");
        if let RiffContent::Data(data) = vp8x.content_mut() {
            let mut updated = data.to_vec();
            updated[0] |= WEBP_EXIF_FLAG | WEBP_XMP_FLAG;
            *data = Bytes::from(updated);
        }
        let mut source = Vec::new();
        tagged.encoder().write_to(&mut source).expect("tagged WebP");

        let cleaned = strip_metadata_bytes(&source).expect("clean WebP");
        let cleaned = WebP::from_bytes(Bytes::from(cleaned)).expect("parse clean WebP");
        let flags = cleaned
            .chunk_by_id(CHUNK_VP8X)
            .and_then(|chunk| chunk.content().data())
            .expect("clean VP8X")[0];

        assert_eq!(
            cleaned
                .chunk_by_id(CHUNK_VP8)
                .expect("pixels")
                .content()
                .data(),
            Some(&original_pixels)
        );
        assert!(cleaned.has_chunk(CHUNK_ICCP));
        assert!(!cleaned.has_chunk(CHUNK_XMP));
        assert_eq!(
            cleaned.exif().as_deref().and_then(read_exif_orientation),
            Some(7)
        );
        assert_eq!(flags & WEBP_XMP_FLAG, 0);
        assert_ne!(flags & WEBP_EXIF_FLAG, 0);
    }
}
