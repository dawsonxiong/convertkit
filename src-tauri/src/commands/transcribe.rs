use std::ffi::OsString;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use reqwest::header::ACCEPT_ENCODING;
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

use crate::engines::process::{run_process, ProcessMessages};
use crate::engines::{resolve_tool, tool_command, verification, ConversionResult};
use crate::error::ConversionError;
use crate::formats::{FileCategory, Format};
use crate::progress::ProgressPayload;

use super::output::{prepare_output, OutputOptions};

const PREPROCESS_TIMEOUT: Duration = Duration::from_secs(2 * 60 * 60);
const TRANSCRIPTION_TIMEOUT: Duration = Duration::from_secs(6 * 60 * 60);
const MODEL_MAGIC: &[u8] = b"lmgg";
const VERIFY_TEXT_BYTES: u64 = 1024 * 1024;

const PREPROCESS_MESSAGES: ProcessMessages = ProcessMessages {
    start: "Could not start FFmpeg",
    wait: "FFmpeg preprocessing could not be awaited",
    failure: "Audio preprocessing failed",
};

const TRANSCRIPTION_MESSAGES: ProcessMessages = ProcessMessages {
    start: "Could not start whisper-cli",
    wait: "whisper-cli could not be awaited",
    failure: "Local transcription failed",
};

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub enum TranscriptionModel {
    Tiny,
    Base,
    Small,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TranscriptionOutputFormat {
    Txt,
    Srt,
    Vtt,
}

impl TranscriptionOutputFormat {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Txt => "txt",
            Self::Srt => "srt",
            Self::Vtt => "vtt",
        }
    }

    fn output_flag(self) -> &'static str {
        match self {
            Self::Txt => "-otxt",
            Self::Srt => "-osrt",
            Self::Vtt => "-ovtt",
        }
    }
}

#[derive(Clone, Copy)]
struct ModelSpec {
    model: TranscriptionModel,
    label: &'static str,
    filename: &'static str,
    url: &'static str,
    expected_size: u64,
    sha1: &'static str,
}

const MODEL_SPECS: [ModelSpec; 3] = [
    ModelSpec {
        model: TranscriptionModel::Tiny,
        label: "Tiny",
        filename: "ggml-tiny.bin",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.bin",
        expected_size: 77_691_713,
        sha1: "bd577a113a864445d4c299885e0cb97d4ba92b5f",
    },
    ModelSpec {
        model: TranscriptionModel::Base,
        label: "Base",
        filename: "ggml-base.bin",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin",
        expected_size: 147_951_465,
        sha1: "465707469ff3a37a2b9b8d8f89f2f99de7299dac",
    },
    ModelSpec {
        model: TranscriptionModel::Small,
        label: "Small",
        filename: "ggml-small.bin",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.bin",
        expected_size: 487_601_967,
        sha1: "55356645c2b361a969dfd0ef2c5a50d530afd8d5",
    },
];

const LANGUAGE_CODES: &[&str] = &[
    "af", "am", "ar", "as", "az", "ba", "be", "bg", "bn", "bo", "br", "bs", "ca", "cs", "cy", "da",
    "de", "el", "en", "es", "et", "eu", "fa", "fi", "fo", "fr", "gl", "gu", "ha", "haw", "he",
    "hi", "hr", "ht", "hu", "hy", "id", "is", "it", "ja", "jw", "ka", "kk", "km", "kn", "ko", "la",
    "lb", "ln", "lo", "lt", "lv", "mg", "mi", "mk", "ml", "mn", "mr", "ms", "mt", "my", "ne", "nl",
    "nn", "no", "oc", "pa", "pl", "ps", "pt", "ro", "ru", "sa", "sd", "si", "sk", "sl", "sn", "so",
    "sq", "sr", "su", "sv", "sw", "ta", "te", "tg", "th", "tk", "tl", "tr", "tt", "uk", "ur", "uz",
    "vi", "yi", "yo", "yue", "zh",
];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptionModelStatus {
    pub model: TranscriptionModel,
    pub label: String,
    pub expected_size: u64,
    pub local_size: u64,
    pub downloaded: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ModelDownloadProgress {
    model: TranscriptionModel,
    percent: i32,
    downloaded_bytes: u64,
    total_bytes: u64,
}

struct PartialDownload {
    path: PathBuf,
    committed: bool,
}

impl PartialDownload {
    fn new(path: PathBuf) -> Self {
        Self {
            path,
            committed: false,
        }
    }

    fn commit(mut self, destination: &Path) -> Result<(), ConversionError> {
        std::fs::rename(&self.path, destination)
            .map_err(|error| process_error(format!("Could not install the model: {error}")))?;
        self.committed = true;
        Ok(())
    }
}

impl Drop for PartialDownload {
    fn drop(&mut self) {
        if !self.committed {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

fn spec(model: TranscriptionModel) -> &'static ModelSpec {
    MODEL_SPECS
        .iter()
        .find(|candidate| candidate.model == model)
        .expect("every transcription model has a spec")
}

pub(super) fn model_label(model: TranscriptionModel) -> &'static str {
    spec(model).label
}

fn process_error(message: impl Into<String>) -> ConversionError {
    ConversionError::ProcessFailed {
        message: message.into(),
        stderr: String::new(),
        exit_code: None,
    }
}

fn io_error(message: &str, error: std::io::Error) -> ConversionError {
    if error.raw_os_error() == Some(28) {
        ConversionError::DiskFull
    } else {
        process_error(format!("{message}: {error}"))
    }
}

pub(super) fn models_directory(app: &AppHandle) -> Result<PathBuf, ConversionError> {
    app.path()
        .app_data_dir()
        .map(|path| path.join("models").join("whisper"))
        .map_err(|error| process_error(format!("Could not locate app storage: {error}")))
}

fn model_path(app: &AppHandle, model: TranscriptionModel) -> Result<PathBuf, ConversionError> {
    Ok(models_directory(app)?.join(spec(model).filename))
}

fn verify_model_file(path: &Path, expected_size: u64) -> bool {
    let Ok(metadata) = std::fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() || metadata.len() != expected_size {
        return false;
    }
    let Ok(mut file) = std::fs::File::open(path) else {
        return false;
    };
    let mut magic = [0_u8; 4];
    file.read_exact(&mut magic).is_ok() && magic == MODEL_MAGIC
}

pub(super) fn model_available(app: &AppHandle, model: TranscriptionModel) -> bool {
    model_path(app, model).is_ok_and(|path| verify_model_file(&path, spec(model).expected_size))
}

fn status(app: &AppHandle, model: TranscriptionModel) -> TranscriptionModelStatus {
    let model_spec = spec(model);
    let path = model_path(app, model).ok();
    let local_size = path
        .as_ref()
        .and_then(|path| std::fs::metadata(path).ok())
        .filter(|metadata| metadata.is_file())
        .map_or(0, |metadata| metadata.len());
    TranscriptionModelStatus {
        model,
        label: model_spec.label.into(),
        expected_size: model_spec.expected_size,
        local_size,
        downloaded: path
            .as_ref()
            .is_some_and(|path| verify_model_file(path, model_spec.expected_size)),
    }
}

#[tauri::command]
pub fn get_transcription_models(app: AppHandle) -> Vec<TranscriptionModelStatus> {
    MODEL_SPECS
        .iter()
        .map(|model_spec| status(&app, model_spec.model))
        .collect()
}

#[tauri::command]
pub async fn download_transcription_model(
    app: AppHandle,
    model: TranscriptionModel,
    job_id: Option<String>,
) -> Result<TranscriptionModelStatus, ConversionError> {
    let model_spec = spec(model);
    let directory = models_directory(&app)?;
    tokio::fs::create_dir_all(&directory)
        .await
        .map_err(|error| io_error("Could not create model storage", error))?;
    let destination = directory.join(model_spec.filename);
    if verify_model_file(&destination, model_spec.expected_size) {
        return Ok(status(&app, model));
    }
    if destination.exists() {
        std::fs::remove_file(&destination)
            .map_err(|error| io_error("Could not replace the invalid model", error))?;
    }

    let active_job = super::jobs::ActiveJobGuard::register(app.clone(), job_id)?;
    let cancel_token = active_job.cancel_token();
    let partial = PartialDownload::new(directory.join(format!(
        ".{}-{}.download",
        model_spec.filename,
        Uuid::new_v4()
    )));
    let response = reqwest::Client::new()
        .get(model_spec.url)
        .header(ACCEPT_ENCODING, "identity")
        .send()
        .await
        .map_err(|error| process_error(format!("Could not download the model: {error}")))?;
    if !response.status().is_success() {
        return Err(process_error(format!(
            "Model download failed with HTTP {}",
            response.status()
        )));
    }
    if response
        .content_length()
        .is_some_and(|length| length != model_spec.expected_size)
    {
        return Err(process_error(
            "The model download size did not match its manifest",
        ));
    }

    let mut file = tokio::fs::File::create(&partial.path)
        .await
        .map_err(|error| io_error("Could not create the model download", error))?;
    let mut stream = response.bytes_stream();
    let mut downloaded = 0_u64;
    let mut hasher = Sha1::new();

    loop {
        let next = tokio::select! {
            value = stream.next() => value,
            _ = cancel_token.cancelled() => return Err(ConversionError::Cancelled),
        };
        let Some(chunk) = next else {
            break;
        };
        let chunk = chunk.map_err(|error| {
            process_error(format!("The model download was interrupted: {error}"))
        })?;
        downloaded = downloaded.saturating_add(chunk.len() as u64);
        if downloaded > model_spec.expected_size {
            return Err(process_error(
                "The model download exceeded its manifest size",
            ));
        }
        file.write_all(&chunk)
            .await
            .map_err(|error| io_error("Could not write the model download", error))?;
        hasher.update(&chunk);
        let _ = app.emit(
            "model-download-progress",
            ModelDownloadProgress {
                model,
                percent: ((downloaded.saturating_mul(100) / model_spec.expected_size).min(100))
                    as i32,
                downloaded_bytes: downloaded,
                total_bytes: model_spec.expected_size,
            },
        );
    }

    file.flush()
        .await
        .map_err(|error| io_error("Could not finish the model download", error))?;
    file.sync_all()
        .await
        .map_err(|error| io_error("Could not save the model download", error))?;
    drop(file);

    let digest = format!("{:x}", hasher.finalize());
    if downloaded != model_spec.expected_size || digest != model_spec.sha1 {
        return Err(process_error(
            "The model download failed integrity verification",
        ));
    }
    if !verify_model_file(&partial.path, model_spec.expected_size) {
        return Err(process_error(
            "The downloaded model is not a valid GGML file",
        ));
    }
    partial.commit(&destination)?;
    Ok(status(&app, model))
}

#[tauri::command]
pub fn delete_transcription_model(
    app: AppHandle,
    model: TranscriptionModel,
) -> Result<Vec<TranscriptionModelStatus>, ConversionError> {
    let path = model_path(&app, model)?;
    if path.exists() {
        std::fs::remove_file(&path)
            .map_err(|error| io_error("Could not delete the model", error))?;
    }
    Ok(get_transcription_models(app))
}

pub(super) fn is_supported_language(language: &str) -> bool {
    language == "auto" || LANGUAGE_CODES.contains(&language)
}

pub(super) fn validate_input(input_path: &str) -> Result<(PathBuf, Format), ConversionError> {
    let input = PathBuf::from(input_path);
    if !input.is_file() {
        return Err(ConversionError::InputNotFound {
            path: input_path.into(),
        });
    }
    let format = input
        .extension()
        .and_then(|extension| extension.to_str())
        .and_then(Format::from_extension)
        .filter(|format| matches!(format.category(), FileCategory::Audio | FileCategory::Video))
        .ok_or_else(|| ConversionError::UnsupportedConversion {
            input: input_path.into(),
            output: "local transcript from audio or video".into(),
        })?;
    Ok((input, format))
}

fn preprocessing_arguments(input: &Path, output: &Path) -> Vec<OsString> {
    vec![
        OsString::from("-hide_banner"),
        OsString::from("-loglevel"),
        OsString::from("error"),
        OsString::from("-i"),
        input.as_os_str().to_os_string(),
        OsString::from("-vn"),
        OsString::from("-sn"),
        OsString::from("-dn"),
        OsString::from("-map"),
        OsString::from("0:a:0"),
        OsString::from("-acodec"),
        OsString::from("pcm_s16le"),
        OsString::from("-ar"),
        OsString::from("16000"),
        OsString::from("-ac"),
        OsString::from("1"),
        OsString::from("-y"),
        output.as_os_str().to_os_string(),
    ]
}

fn transcription_arguments(
    model: &Path,
    input: &Path,
    output_base: &Path,
    language: &str,
    output_format: TranscriptionOutputFormat,
) -> Vec<OsString> {
    vec![
        OsString::from("-m"),
        model.as_os_str().to_os_string(),
        OsString::from("-f"),
        input.as_os_str().to_os_string(),
        OsString::from("-l"),
        OsString::from(language),
        OsString::from("-of"),
        output_base.as_os_str().to_os_string(),
        OsString::from(output_format.output_flag()),
        OsString::from("-np"),
    ]
}

fn verify_plain_text(path: &Path) -> Result<(), ConversionError> {
    verification::nonempty_file(path)?;
    let file = std::fs::File::open(path).map_err(|_| ConversionError::OutputMissing)?;
    let mut prefix = Vec::new();
    file.take(VERIFY_TEXT_BYTES)
        .read_to_end(&mut prefix)
        .map_err(|error| process_error(format!("Could not verify the transcript: {error}")))?;
    let text = std::str::from_utf8(&prefix)
        .map_err(|_| process_error("The transcript is not valid UTF-8 text"))?;
    if text.trim().is_empty() {
        return Err(process_error("No transcript text was written"));
    }
    Ok(())
}

pub(super) async fn transcribe_media(
    app: AppHandle,
    input_path: String,
    model: TranscriptionModel,
    language: String,
    output_format: TranscriptionOutputFormat,
    job_id: Option<String>,
    output_options: Option<OutputOptions>,
) -> Result<ConversionResult, ConversionError> {
    let started = Instant::now();
    let (input, _) = validate_input(&input_path)?;
    if !is_supported_language(&language) {
        return Err(ConversionError::UnsupportedConversion {
            input: language,
            output: "a supported transcription language".into(),
        });
    }
    for (tool, hint) in [
        ("ffmpeg", "Install FFmpeg with Homebrew"),
        (
            "whisper-cli",
            "Reinstall ConvertKit or install whisper-cpp with Homebrew",
        ),
    ] {
        if resolve_tool(tool).is_none() {
            return Err(ConversionError::MissingDependency {
                tool: tool.into(),
                install_hint: hint.into(),
            });
        }
    }
    let model_path = model_path(&app, model)?;
    if !verify_model_file(&model_path, spec(model).expected_size) {
        return Err(ConversionError::MissingDependency {
            tool: format!("{} transcription model", spec(model).label),
            install_hint: "Download the selected model in ConvertKit".into(),
        });
    }

    let output = prepare_output(
        &input,
        output_format.extension(),
        "-transcript",
        output_options,
    )?;
    let active_job = super::jobs::ActiveJobGuard::register(app.clone(), job_id)?;
    let cancel_token = active_job.cancel_token();
    let event_job_id = active_job.job_id().to_owned();
    let temporary = tempfile::Builder::new()
        .prefix("convertkit-transcription-")
        .tempdir()
        .map_err(|error| io_error("Could not prepare temporary audio", error))?;
    let wave = temporary.path().join("input.wav");

    let _ = app.emit(
        "conversion-progress",
        ProgressPayload {
            job_id: event_job_id.clone(),
            percent: -1,
            stage: "Preparing audio".into(),
        },
    );
    let mut ffmpeg = tool_command("ffmpeg");
    ffmpeg.args(preprocessing_arguments(&input, &wave));
    run_process(
        ffmpeg,
        cancel_token.clone(),
        PREPROCESS_TIMEOUT,
        &[wave.as_path()],
        PREPROCESS_MESSAGES,
    )
    .await?;
    verification::file_with_signature(&wave, b"RIFF")?;

    let _ = app.emit(
        "conversion-progress",
        ProgressPayload {
            job_id: event_job_id.clone(),
            percent: -1,
            stage: "Transcribing locally".into(),
        },
    );
    let output_base = output.working_path().with_extension("");
    let mut whisper = tool_command("whisper-cli");
    whisper.args(transcription_arguments(
        &model_path,
        &wave,
        &output_base,
        &language,
        output_format,
    ));
    run_process(
        whisper,
        cancel_token.clone(),
        TRANSCRIPTION_TIMEOUT,
        &[output.working_path()],
        TRANSCRIPTION_MESSAGES,
    )
    .await?;
    if cancel_token.is_cancelled() {
        return Err(ConversionError::Cancelled);
    }

    match output_format {
        TranscriptionOutputFormat::Txt => verify_plain_text(output.working_path())?,
        TranscriptionOutputFormat::Srt => super::subtitles::verify_output(
            output.working_path(),
            super::subtitles::SubtitleOutputFormat::Srt,
        )?,
        TranscriptionOutputFormat::Vtt => super::subtitles::verify_output(
            output.working_path(),
            super::subtitles::SubtitleOutputFormat::Vtt,
        )?,
    }

    let mut result = output.commit(ConversionResult {
        output_path: String::new(),
        output_paths: Vec::new(),
        output_size: 0,
        duration_ms: started.elapsed().as_millis() as u64,
        undo_manifest: None,
    })?;
    result.duration_ms = started.elapsed().as_millis() as u64;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(values: Vec<OsString>) -> Vec<String> {
        values
            .into_iter()
            .map(|value| value.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn model_manifest_uses_fixed_https_urls_sizes_and_sha1_fingerprints() {
        assert_eq!(MODEL_SPECS.len(), 3);
        for model_spec in MODEL_SPECS {
            assert!(model_spec
                .url
                .starts_with("https://huggingface.co/ggerganov/whisper.cpp/"));
            assert!(model_spec.expected_size > 70_000_000);
            assert_eq!(model_spec.sha1.len(), 40);
            assert!(model_spec.sha1.bytes().all(|byte| byte.is_ascii_hexdigit()));
        }
    }

    #[test]
    fn model_validation_requires_exact_size_and_ggml_magic() {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("model.bin");
        std::fs::write(&path, b"lmggdata").expect("fixture");
        assert!(verify_model_file(&path, 8));
        assert!(!verify_model_file(&path, 9));
        std::fs::write(&path, b"bad!data").expect("fixture");
        assert!(!verify_model_file(&path, 8));
    }

    #[test]
    fn preprocessing_maps_only_primary_audio_to_whisper_wav_contract() {
        let arguments = strings(preprocessing_arguments(
            Path::new("input video.mp4"),
            Path::new("prepared audio.wav"),
        ));
        assert!(arguments.windows(2).any(|pair| pair == ["-map", "0:a:0"]));
        assert!(arguments
            .windows(2)
            .any(|pair| pair == ["-acodec", "pcm_s16le"]));
        assert!(arguments.windows(2).any(|pair| pair == ["-ar", "16000"]));
        assert!(arguments.windows(2).any(|pair| pair == ["-ac", "1"]));
        assert!(arguments.contains(&"-vn".into()));
    }

    #[test]
    fn transcription_arguments_are_path_safe_and_format_specific() {
        let arguments = strings(transcription_arguments(
            Path::new("model path/model.bin"),
            Path::new("input path/audio.wav"),
            Path::new("output path/transcript"),
            "fr",
            TranscriptionOutputFormat::Vtt,
        ));
        assert!(arguments
            .windows(2)
            .any(|pair| pair == ["-m", "model path/model.bin"]));
        assert!(arguments
            .windows(2)
            .any(|pair| pair == ["-f", "input path/audio.wav"]));
        assert!(arguments.windows(2).any(|pair| pair == ["-l", "fr"]));
        assert!(arguments
            .windows(2)
            .any(|pair| pair == ["-of", "output path/transcript"]));
        assert!(arguments.contains(&"-ovtt".into()));
        assert!(!arguments.contains(&"-otxt".into()));
    }

    #[test]
    fn validates_auto_and_whisper_language_codes() {
        assert!(is_supported_language("auto"));
        assert!(is_supported_language("en"));
        assert!(is_supported_language("haw"));
        assert!(is_supported_language("yue"));
        assert!(!is_supported_language("EN"));
        assert!(!is_supported_language("unknown"));
    }

    #[test]
    fn plain_text_verification_rejects_empty_and_binary_results() {
        let directory = tempfile::tempdir().expect("tempdir");
        let valid = directory.path().join("valid.txt");
        let empty = directory.path().join("empty.txt");
        let binary = directory.path().join("binary.txt");
        std::fs::write(&valid, "A local transcript.\n").expect("fixture");
        std::fs::write(&empty, "  \n").expect("fixture");
        std::fs::write(&binary, [0xff, 0xfe]).expect("fixture");
        verify_plain_text(&valid).expect("valid transcript");
        assert!(verify_plain_text(&empty).is_err());
        assert!(verify_plain_text(&binary).is_err());
    }
}
