use std::collections::BTreeSet;
use std::path::Path;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::engines::{self, ConversionResult};
use crate::error::ConversionError;
use crate::formats::{FileCategory, Format};
use crate::ActiveJobs;

use super::output::OutputOptions;

#[derive(Debug, Deserialize)]
#[serde(tag = "operation")]
pub enum JobRequest {
    #[serde(rename = "convert")]
    Convert {
        #[serde(rename = "inputPath")]
        input_path: String,
        #[serde(rename = "outputFormat")]
        output_format: String,
        #[serde(rename = "jobId")]
        job_id: Option<String>,
        #[serde(rename = "outputOptions")]
        output_options: Option<OutputOptions>,
    },
    #[serde(rename = "resize")]
    Resize {
        #[serde(rename = "inputPath")]
        input_path: String,
        width: u32,
        height: u32,
        #[serde(rename = "preserveAspect")]
        preserve_aspect: bool,
        #[serde(rename = "jobId")]
        job_id: Option<String>,
        #[serde(rename = "outputOptions")]
        output_options: Option<OutputOptions>,
    },
    #[serde(rename = "optimize")]
    Optimize {
        #[serde(rename = "inputPath")]
        input_path: String,
        #[serde(rename = "keepMetadata")]
        keep_metadata: bool,
        #[serde(default, rename = "compressionGoal")]
        compression_goal: super::optimize::ImageOptimizationGoal,
        #[serde(default, rename = "targetSizeBytes")]
        target_size_bytes: Option<u64>,
        #[serde(rename = "jobId")]
        job_id: Option<String>,
        #[serde(rename = "outputOptions")]
        output_options: Option<OutputOptions>,
    },
    #[serde(rename = "exportImages")]
    ExportImages {
        #[serde(rename = "inputPath")]
        input_path: String,
        presets: Vec<super::image_export::ImageExportPreset>,
        #[serde(rename = "jobId")]
        job_id: Option<String>,
        #[serde(rename = "outputOptions")]
        output_options: Option<OutputOptions>,
    },
    #[serde(rename = "removeMetadata")]
    RemoveMetadata {
        #[serde(rename = "inputPath")]
        input_path: String,
        #[serde(rename = "jobId")]
        job_id: Option<String>,
        #[serde(rename = "outputOptions")]
        output_options: Option<OutputOptions>,
    },
    #[serde(rename = "extractAudio")]
    ExtractAudio {
        #[serde(rename = "inputPath")]
        input_path: String,
        #[serde(rename = "streamIndex")]
        stream_index: u32,
        #[serde(rename = "streamCodec")]
        stream_codec: String,
        #[serde(rename = "outputFormat")]
        output_format: super::convert::AudioOutputFormat,
        #[serde(rename = "jobId")]
        job_id: Option<String>,
        #[serde(rename = "outputOptions")]
        output_options: Option<OutputOptions>,
    },
    #[serde(rename = "transcribe")]
    Transcribe {
        #[serde(rename = "inputPath")]
        input_path: String,
        model: super::transcribe::TranscriptionModel,
        language: String,
        #[serde(rename = "outputFormat")]
        output_format: super::transcribe::TranscriptionOutputFormat,
        #[serde(rename = "jobId")]
        job_id: Option<String>,
        #[serde(rename = "outputOptions")]
        output_options: Option<OutputOptions>,
    },
    #[serde(rename = "encodeVideo")]
    EncodeVideo {
        #[serde(rename = "inputPath")]
        input_path: String,
        preset: super::video::VideoEncodingPreset,
        #[serde(default)]
        resolution: super::video::VideoResolution,
        #[serde(default)]
        quality: super::video::VideoQuality,
        #[serde(default, rename = "compressionGoal")]
        compression_goal: super::video::VideoCompressionGoal,
        #[serde(default, rename = "targetSizeBytes")]
        target_size_bytes: Option<u64>,
        #[serde(rename = "jobId")]
        job_id: Option<String>,
        #[serde(rename = "outputOptions")]
        output_options: Option<OutputOptions>,
    },
    #[serde(rename = "removeAudio")]
    RemoveAudio {
        #[serde(rename = "inputPath")]
        input_path: String,
        #[serde(rename = "jobId")]
        job_id: Option<String>,
        #[serde(rename = "outputOptions")]
        output_options: Option<OutputOptions>,
    },
    #[serde(rename = "compressAudio")]
    CompressAudio {
        #[serde(rename = "inputPath")]
        input_path: String,
        #[serde(default)]
        preset: super::audio::AudioCompressionPreset,
        #[serde(rename = "jobId")]
        job_id: Option<String>,
        #[serde(rename = "outputOptions")]
        output_options: Option<OutputOptions>,
    },
    #[serde(rename = "extractSubtitles")]
    ExtractSubtitles {
        #[serde(rename = "inputPath")]
        input_path: String,
        #[serde(rename = "streamIndex")]
        stream_index: u32,
        #[serde(rename = "streamCodec")]
        stream_codec: String,
        #[serde(rename = "outputFormat")]
        output_format: super::subtitles::SubtitleOutputFormat,
        #[serde(rename = "jobId")]
        job_id: Option<String>,
        #[serde(rename = "outputOptions")]
        output_options: Option<OutputOptions>,
    },
    #[serde(rename = "generateThumbnails")]
    GenerateThumbnails {
        #[serde(rename = "inputPath")]
        input_path: String,
        mode: super::thumbnails::ThumbnailMode,
        #[serde(rename = "outputFormat")]
        output_format: super::thumbnails::ThumbnailOutputFormat,
        #[serde(rename = "jobId")]
        job_id: Option<String>,
        #[serde(rename = "outputOptions")]
        output_options: Option<OutputOptions>,
    },
    #[serde(rename = "extractText")]
    ExtractText {
        #[serde(rename = "inputPath")]
        input_path: String,
        #[serde(rename = "jobId")]
        job_id: Option<String>,
        #[serde(rename = "outputOptions")]
        output_options: Option<OutputOptions>,
    },
    #[serde(rename = "recognizeText")]
    RecognizeText {
        #[serde(rename = "inputPath")]
        input_path: String,
        #[serde(default, rename = "outputFormat")]
        output_format: super::ocr::OcrOutputFormat,
        #[serde(rename = "jobId")]
        job_id: Option<String>,
        #[serde(rename = "outputOptions")]
        output_options: Option<OutputOptions>,
    },
    #[serde(rename = "mergePdf")]
    MergePdf {
        #[serde(rename = "inputPaths")]
        input_paths: Vec<String>,
        #[serde(rename = "jobId")]
        job_id: Option<String>,
        #[serde(rename = "outputOptions")]
        output_options: Option<OutputOptions>,
    },
    #[serde(rename = "splitPdf")]
    SplitPdf {
        #[serde(rename = "inputPath")]
        input_path: String,
        mode: super::pdf::PdfSplitMode,
        #[serde(rename = "pageSelection")]
        page_selection: String,
        #[serde(rename = "jobId")]
        job_id: Option<String>,
        #[serde(rename = "outputOptions")]
        output_options: Option<OutputOptions>,
    },
    #[serde(rename = "exportPdfPages")]
    ExportPdfPages {
        #[serde(rename = "inputPath")]
        input_path: String,
        mode: super::pdf::PdfSplitMode,
        #[serde(rename = "pageSelection")]
        page_selection: String,
        #[serde(rename = "outputFormat")]
        output_format: super::pdf::PdfPageImageFormat,
        resolution: super::pdf::PdfPageImageResolution,
        #[serde(rename = "jobId")]
        job_id: Option<String>,
        #[serde(rename = "outputOptions")]
        output_options: Option<OutputOptions>,
    },
    #[serde(rename = "compressPdf")]
    CompressPdf {
        #[serde(rename = "inputPath")]
        input_path: String,
        preset: super::pdf::PdfCompressionPreset,
        #[serde(default, rename = "compressionGoal")]
        compression_goal: super::pdf::PdfCompressionGoal,
        #[serde(default, rename = "targetSizeBytes")]
        target_size_bytes: Option<u64>,
        #[serde(rename = "jobId")]
        job_id: Option<String>,
        #[serde(rename = "outputOptions")]
        output_options: Option<OutputOptions>,
    },
    #[serde(rename = "rename")]
    Rename {
        items: Vec<super::rename::RenameItemRequest>,
        #[serde(rename = "jobId")]
        job_id: Option<String>,
    },
    #[serde(rename = "createArchive")]
    CreateArchive {
        entries: Vec<super::archive::ArchiveEntryRequest>,
        format: super::archive::ArchiveFormat,
        #[serde(default)]
        password: Option<super::archive::ArchivePassword>,
        #[serde(rename = "jobId")]
        job_id: Option<String>,
        #[serde(rename = "outputOptions")]
        output_options: Option<OutputOptions>,
    },
    #[serde(rename = "extractArchive")]
    ExtractArchive {
        #[serde(rename = "inputPath")]
        input_path: String,
        #[serde(default)]
        password: Option<super::archive::ArchivePassword>,
        #[serde(rename = "jobId")]
        job_id: Option<String>,
        #[serde(rename = "outputOptions")]
        output_options: Option<OutputOptions>,
    },
}

impl JobRequest {
    pub(crate) fn input_path(&self) -> &str {
        match self {
            Self::Convert { input_path, .. }
            | Self::Resize { input_path, .. }
            | Self::Optimize { input_path, .. }
            | Self::ExportImages { input_path, .. }
            | Self::RemoveMetadata { input_path, .. }
            | Self::ExtractAudio { input_path, .. }
            | Self::Transcribe { input_path, .. }
            | Self::EncodeVideo { input_path, .. }
            | Self::RemoveAudio { input_path, .. }
            | Self::CompressAudio { input_path, .. }
            | Self::ExtractSubtitles { input_path, .. }
            | Self::GenerateThumbnails { input_path, .. }
            | Self::ExtractText { input_path, .. }
            | Self::RecognizeText { input_path, .. }
            | Self::SplitPdf { input_path, .. }
            | Self::ExportPdfPages { input_path, .. }
            | Self::CompressPdf { input_path, .. }
            | Self::ExtractArchive { input_path, .. } => input_path,
            Self::MergePdf { input_paths, .. } => {
                input_paths.first().map(String::as_str).unwrap_or_default()
            }
            Self::CreateArchive { entries, .. } => entries
                .first()
                .map(|entry| entry.input_path.as_str())
                .unwrap_or_default(),
            Self::Rename { items, .. } => items
                .first()
                .map(|item| item.input_path.as_str())
                .unwrap_or_default(),
        }
    }
}

/// Registers a cancellable job and guarantees cleanup on every exit path.
pub(super) struct ActiveJobGuard {
    app: AppHandle,
    job_id: String,
    cancel_token: CancellationToken,
}

impl ActiveJobGuard {
    pub(super) fn register(
        app: AppHandle,
        job_id: Option<String>,
    ) -> Result<Self, ConversionError> {
        let job_id = job_id.unwrap_or_else(|| Uuid::new_v4().to_string());
        let cancel_token = CancellationToken::new();
        {
            let jobs = app.state::<ActiveJobs>();
            let mut active = jobs.0.lock().expect("ActiveJobs lock poisoned");
            if active.contains_key(&job_id) {
                return Err(ConversionError::UnsupportedConversion {
                    input: "Duplicate job ID".into(),
                    output: job_id,
                });
            }
            active.insert(job_id.clone(), cancel_token.clone());
        }

        Ok(Self {
            app,
            job_id,
            cancel_token,
        })
    }

    pub(super) fn cancel_token(&self) -> CancellationToken {
        self.cancel_token.clone()
    }

    pub(super) fn job_id(&self) -> &str {
        &self.job_id
    }
}

impl Drop for ActiveJobGuard {
    fn drop(&mut self) {
        self.app
            .state::<ActiveJobs>()
            .0
            .lock()
            .expect("ActiveJobs lock poisoned")
            .remove(&self.job_id);
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CapabilityIssue {
    MissingDependency,
    Unsupported,
    MissingInput,
    InvalidSettings,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobCapability {
    pub input_path: String,
    pub available: bool,
    pub engine: Option<String>,
    pub missing_tools: Vec<String>,
    pub issue: Option<CapabilityIssue>,
    pub message: Option<String>,
}

impl JobCapability {
    fn available(input_path: &str, engine: &str) -> Self {
        Self {
            input_path: input_path.into(),
            available: true,
            engine: Some(engine.into()),
            missing_tools: Vec::new(),
            issue: None,
            message: None,
        }
    }

    fn unavailable(
        input_path: &str,
        issue: CapabilityIssue,
        message: impl Into<String>,
        missing_tools: Vec<String>,
    ) -> Self {
        Self {
            input_path: input_path.into(),
            available: false,
            engine: None,
            missing_tools,
            issue: Some(issue),
            message: Some(message.into()),
        }
    }
}

/// Validate the exact operation and settings without starting a subprocess.
#[tauri::command]
pub fn check_job_capabilities(app: AppHandle, requests: Vec<JobRequest>) -> Vec<JobCapability> {
    requests
        .iter()
        .map(|request| job_capability_with_app(Some(&app), request))
        .collect()
}

/// Execute any supported operation through one typed command boundary.
#[tauri::command]
pub async fn run_job(
    app: AppHandle,
    request: JobRequest,
) -> Result<ConversionResult, ConversionError> {
    match request {
        JobRequest::Convert {
            input_path,
            output_format,
            job_id,
            output_options,
        } => super::convert::convert(app, input_path, output_format, job_id, output_options).await,
        JobRequest::Resize {
            input_path,
            width,
            height,
            preserve_aspect,
            job_id,
            output_options,
        } => {
            super::resize::resize_image(
                app,
                input_path,
                width,
                height,
                preserve_aspect,
                job_id,
                output_options,
            )
            .await
        }
        JobRequest::Optimize {
            input_path,
            keep_metadata,
            compression_goal,
            target_size_bytes,
            job_id,
            output_options,
        } => {
            super::optimize::optimize_image(
                app,
                input_path,
                keep_metadata,
                compression_goal,
                target_size_bytes,
                job_id,
                output_options,
            )
            .await
        }
        JobRequest::ExportImages {
            input_path,
            presets,
            job_id,
            output_options,
        } => {
            super::image_export::export_image_variants(
                app,
                input_path,
                presets,
                job_id,
                output_options,
            )
            .await
        }
        JobRequest::RemoveMetadata {
            input_path,
            job_id,
            output_options,
        } => super::metadata::remove_metadata(app, input_path, job_id, output_options).await,
        JobRequest::ExtractAudio {
            input_path,
            stream_index,
            stream_codec,
            output_format,
            job_id,
            output_options,
        } => {
            super::audio::extract_audio(
                app,
                input_path,
                stream_index,
                stream_codec,
                output_format,
                job_id,
                output_options,
            )
            .await
        }
        JobRequest::Transcribe {
            input_path,
            model,
            language,
            output_format,
            job_id,
            output_options,
        } => {
            super::transcribe::transcribe_media(
                app,
                input_path,
                model,
                language,
                output_format,
                job_id,
                output_options,
            )
            .await
        }
        JobRequest::EncodeVideo {
            input_path,
            preset,
            resolution,
            quality,
            compression_goal,
            target_size_bytes,
            job_id,
            output_options,
        } => {
            super::video::encode_video(
                app,
                input_path,
                preset,
                resolution,
                quality,
                compression_goal,
                target_size_bytes,
                job_id,
                output_options,
            )
            .await
        }
        JobRequest::RemoveAudio {
            input_path,
            job_id,
            output_options,
        } => super::video::remove_audio(app, input_path, job_id, output_options).await,
        JobRequest::CompressAudio {
            input_path,
            preset,
            job_id,
            output_options,
        } => super::audio::compress_audio(app, input_path, preset, job_id, output_options).await,
        JobRequest::ExtractSubtitles {
            input_path,
            stream_index,
            stream_codec,
            output_format,
            job_id,
            output_options,
        } => {
            super::subtitles::extract_subtitles(
                app,
                input_path,
                stream_index,
                stream_codec,
                output_format,
                job_id,
                output_options,
            )
            .await
        }
        JobRequest::GenerateThumbnails {
            input_path,
            mode,
            output_format,
            job_id,
            output_options,
        } => {
            super::thumbnails::generate_thumbnail(
                app,
                input_path,
                mode,
                output_format,
                job_id,
                output_options,
            )
            .await
        }
        JobRequest::ExtractText {
            input_path,
            job_id,
            output_options,
        } => super::text::extract_text(app, input_path, job_id, output_options).await,
        JobRequest::RecognizeText {
            input_path,
            output_format,
            job_id,
            output_options,
        } => {
            super::ocr::recognize_text(app, input_path, output_format, job_id, output_options).await
        }
        JobRequest::MergePdf {
            input_paths,
            job_id,
            output_options,
        } => super::pdf::merge_pdfs(app, input_paths, job_id, output_options).await,
        JobRequest::SplitPdf {
            input_path,
            mode,
            page_selection,
            job_id,
            output_options,
        } => {
            super::pdf::split_pdf(
                app,
                input_path,
                mode,
                page_selection,
                job_id,
                output_options,
            )
            .await
        }
        JobRequest::ExportPdfPages {
            input_path,
            mode,
            page_selection,
            output_format,
            resolution,
            job_id,
            output_options,
        } => {
            super::pdf::export_pdf_pages(
                app,
                input_path,
                mode,
                page_selection,
                output_format,
                resolution,
                job_id,
                output_options,
            )
            .await
        }
        JobRequest::CompressPdf {
            input_path,
            preset,
            compression_goal,
            target_size_bytes,
            job_id,
            output_options,
        } => {
            super::pdf::compress_pdf(
                app,
                input_path,
                preset,
                compression_goal,
                target_size_bytes,
                job_id,
                output_options,
            )
            .await
        }
        JobRequest::Rename { items, job_id } => {
            super::rename::rename_files(app, items, job_id).await
        }
        JobRequest::CreateArchive {
            entries,
            format,
            password,
            job_id,
            output_options,
        } => {
            super::archive::create_archive(app, entries, format, password, job_id, output_options)
                .await
        }
        JobRequest::ExtractArchive {
            input_path,
            password,
            job_id,
            output_options,
        } => {
            super::archive::extract_archive(app, input_path, password, job_id, output_options).await
        }
    }
}

#[cfg(test)]
fn job_capability(request: &JobRequest) -> JobCapability {
    job_capability_with_app(None, request)
}

fn job_capability_with_app(app: Option<&AppHandle>, request: &JobRequest) -> JobCapability {
    if let JobRequest::MergePdf { input_paths, .. } = request {
        return pdf_merge_capability(input_paths);
    }
    if let JobRequest::CreateArchive {
        entries,
        format,
        password,
        ..
    } = request
    {
        let primary = request.input_path();
        return match super::archive::validate_password_for_creation(*format, password.as_ref())
            .and_then(|()| {
                super::archive::validate_create_entries_for_format(entries.clone(), *format)
            }) {
            Ok(_) => JobCapability::available(
                primary,
                match format {
                    super::archive::ArchiveFormat::Zip => "Built-in ZIP",
                    super::archive::ArchiveFormat::Tar => "Built-in TAR",
                    super::archive::ArchiveFormat::TarGz => "Built-in TAR.GZ",
                    super::archive::ArchiveFormat::SevenZ => "Built-in 7Z",
                    super::archive::ArchiveFormat::Gzip => "Built-in GZIP",
                },
            ),
            Err(ConversionError::InputNotFound { path }) => JobCapability::unavailable(
                &path,
                CapabilityIssue::MissingInput,
                "One of the source files is no longer available.",
                Vec::new(),
            ),
            Err(error) => JobCapability::unavailable(
                primary,
                CapabilityIssue::InvalidSettings,
                match error {
                    ConversionError::UnsupportedConversion { input, .. } => input,
                    _ => "Check the files selected for this archive.".into(),
                },
                Vec::new(),
            ),
        };
    }
    if let JobRequest::Rename { items, .. } = request {
        let primary = request.input_path();
        return match super::rename::validate_rename_items(items) {
            Ok(_) => JobCapability::available(primary, "Built-in rename"),
            Err(ConversionError::InputNotFound { path }) => JobCapability::unavailable(
                &path,
                CapabilityIssue::MissingInput,
                "One of the source files is no longer available.",
                Vec::new(),
            ),
            Err(ConversionError::OutputConflict { .. }) => JobCapability::unavailable(
                primary,
                CapabilityIssue::InvalidSettings,
                "Resolve the conflicting filenames before renaming.",
                Vec::new(),
            ),
            Err(_) => JobCapability::unavailable(
                primary,
                CapabilityIssue::InvalidSettings,
                "Review the filename preview before renaming.",
                Vec::new(),
            ),
        };
    }
    if let JobRequest::ExtractArchive {
        input_path,
        password,
        ..
    } = request
    {
        return match super::archive::validate_archive_input_with_password(
            input_path,
            password.as_ref(),
        ) {
            Ok((_, format)) => JobCapability::available(
                input_path,
                match format {
                    super::archive::ExtractArchiveFormat::Zip => "Built-in ZIP",
                    super::archive::ExtractArchiveFormat::Tar => "Built-in TAR",
                    super::archive::ExtractArchiveFormat::TarGz => "Built-in TAR.GZ",
                    super::archive::ExtractArchiveFormat::Gzip => "Built-in GZIP",
                    super::archive::ExtractArchiveFormat::SevenZ => "Built-in 7Z",
                },
            ),
            Err(ConversionError::InputNotFound { .. }) => JobCapability::unavailable(
                input_path,
                CapabilityIssue::MissingInput,
                "The archive is no longer available.",
                Vec::new(),
            ),
            Err(ConversionError::ArchivePasswordRequired) => JobCapability::unavailable(
                input_path,
                CapabilityIssue::InvalidSettings,
                "Enter the password for this 7Z archive.",
                Vec::new(),
            ),
            Err(ConversionError::IncorrectArchivePassword) => JobCapability::unavailable(
                input_path,
                CapabilityIssue::InvalidSettings,
                "The 7Z archive password is incorrect.",
                Vec::new(),
            ),
            Err(_) => JobCapability::unavailable(
                input_path,
                CapabilityIssue::Unsupported,
                "This archive is invalid, damaged, or unsupported.",
                Vec::new(),
            ),
        };
    }

    let input_path = request.input_path();
    let path = Path::new(input_path);
    if !path.is_file() {
        return JobCapability::unavailable(
            input_path,
            CapabilityIssue::MissingInput,
            "The source file is no longer available.",
            Vec::new(),
        );
    }

    let input_format = match format_from_path(path) {
        Some(format) => format,
        None => {
            return JobCapability::unavailable(
                input_path,
                CapabilityIssue::Unsupported,
                "This file format is not supported.",
                Vec::new(),
            );
        }
    };

    match request {
        JobRequest::Convert { output_format, .. } => {
            conversion_capability(input_path, input_format, output_format)
        }
        JobRequest::Resize { width, height, .. } => {
            if input_format.category() != FileCategory::Image {
                return JobCapability::unavailable(
                    input_path,
                    CapabilityIssue::Unsupported,
                    "Resize supports raster images only.",
                    Vec::new(),
                );
            }
            if *width == 0 || *height == 0 || *width > 32_768 || *height > 32_768 {
                return JobCapability::unavailable(
                    input_path,
                    CapabilityIssue::InvalidSettings,
                    "Choose dimensions between 1 and 32,768 pixels.",
                    Vec::new(),
                );
            }
            tool_capability(input_path, "magick", "ImageMagick")
        }
        JobRequest::Optimize {
            compression_goal,
            target_size_bytes,
            ..
        } => {
            if input_format.category() != FileCategory::Image {
                return JobCapability::unavailable(
                    input_path,
                    CapabilityIssue::Unsupported,
                    "Optimize supports raster images only.",
                    Vec::new(),
                );
            }
            if let Err(error) = super::optimize::validate_image_optimization_settings(
                path,
                input_format,
                *compression_goal,
                *target_size_bytes,
            ) {
                return JobCapability::unavailable(
                    input_path,
                    CapabilityIssue::InvalidSettings,
                    match error {
                        ConversionError::UnsupportedConversion { input, .. } => input,
                        _ => "Choose a valid image optimization target.".into(),
                    },
                    Vec::new(),
                );
            }
            tool_capability(input_path, "magick", "ImageMagick")
        }
        JobRequest::ExportImages { presets, .. } => {
            if input_format.category() != FileCategory::Image {
                return JobCapability::unavailable(
                    input_path,
                    CapabilityIssue::Unsupported,
                    "Image export supports raster images only.",
                    Vec::new(),
                );
            }
            if super::image_export::validate_presets(presets).is_err() {
                return JobCapability::unavailable(
                    input_path,
                    CapabilityIssue::InvalidSettings,
                    "Choose at least one unique image variant.",
                    Vec::new(),
                );
            }
            tool_capability(input_path, "magick", "ImageMagick")
        }
        JobRequest::RemoveMetadata { .. } => match super::metadata::metadata_removal_engine(path) {
            Some(super::metadata::MetadataRemovalEngine::BuiltInImage) => {
                JobCapability::available(input_path, "Built-in metadata cleaner")
            }
            Some(super::metadata::MetadataRemovalEngine::BuiltInPdf) => {
                JobCapability::available(input_path, "Built-in PDF metadata cleaner")
            }
            Some(super::metadata::MetadataRemovalEngine::FfmpegStreamCopy) => {
                tool_capability(input_path, "ffmpeg", "FFmpeg")
            }
            None => JobCapability::unavailable(
                input_path,
                CapabilityIssue::Unsupported,
                "Metadata removal accepts JPEG, PNG, WebP, PDF, audio, and video files.",
                Vec::new(),
            ),
        },
        JobRequest::ExtractAudio {
            stream_index,
            stream_codec,
            output_format,
            ..
        } => {
            if input_format.category() != FileCategory::Video {
                JobCapability::unavailable(
                    input_path,
                    CapabilityIssue::Unsupported,
                    "Audio extraction accepts video files only.",
                    Vec::new(),
                )
            } else if *stream_index > 4_096 || stream_codec.is_empty() || stream_codec.len() > 64 {
                JobCapability::unavailable(
                    input_path,
                    CapabilityIssue::InvalidSettings,
                    "Choose a valid embedded audio track.",
                    Vec::new(),
                )
            } else {
                let tools =
                    tools_capability(input_path, &[("ffmpeg", "FFmpeg"), ("ffprobe", "FFmpeg")]);
                if !tools.available {
                    tools
                } else if let Err(missing) = super::audio::extraction_support(*output_format) {
                    JobCapability::unavailable(
                        input_path,
                        CapabilityIssue::MissingDependency,
                        format!(
                            "This FFmpeg installation is missing: {}.",
                            missing.join(", ")
                        ),
                        vec!["ffmpeg".into()],
                    )
                } else {
                    JobCapability::available(input_path, "FFmpeg")
                }
            }
        }
        JobRequest::Transcribe {
            model, language, ..
        } => {
            if !matches!(
                input_format.category(),
                FileCategory::Audio | FileCategory::Video
            ) {
                JobCapability::unavailable(
                    input_path,
                    CapabilityIssue::Unsupported,
                    "Transcription accepts audio and video files only.",
                    Vec::new(),
                )
            } else if !super::transcribe::is_supported_language(language) {
                JobCapability::unavailable(
                    input_path,
                    CapabilityIssue::InvalidSettings,
                    "Choose a supported transcription language.",
                    Vec::new(),
                )
            } else if app.is_some_and(|app| !super::transcribe::model_available(app, *model)) {
                JobCapability::unavailable(
                    input_path,
                    CapabilityIssue::MissingDependency,
                    format!(
                        "Download the {} transcription model to continue.",
                        super::transcribe::model_label(*model)
                    ),
                    Vec::new(),
                )
            } else {
                tools_capability(
                    input_path,
                    &[("ffmpeg", "FFmpeg"), ("whisper-cli", "Whisper CLI")],
                )
            }
        }
        JobRequest::EncodeVideo {
            preset,
            compression_goal,
            target_size_bytes,
            ..
        } => {
            if input_format.category() != FileCategory::Video {
                JobCapability::unavailable(
                    input_path,
                    CapabilityIssue::Unsupported,
                    "Video encoding accepts video files only.",
                    Vec::new(),
                )
            } else if let Err(message) = super::video::validate_compression_goal(
                *preset,
                *compression_goal,
                *target_size_bytes,
            ) {
                JobCapability::unavailable(
                    input_path,
                    CapabilityIssue::InvalidSettings,
                    message,
                    Vec::new(),
                )
            } else {
                let tools =
                    tools_capability(input_path, &[("ffmpeg", "FFmpeg"), ("ffprobe", "FFmpeg")]);
                if !tools.available {
                    tools
                } else if let Err(missing) = super::video::encoder_support(*preset) {
                    JobCapability::unavailable(
                        input_path,
                        CapabilityIssue::MissingDependency,
                        format!(
                            "This FFmpeg installation is missing: {}.",
                            missing.join(", ")
                        ),
                        vec!["ffmpeg".into()],
                    )
                } else {
                    JobCapability::available(input_path, "FFmpeg")
                }
            }
        }
        JobRequest::RemoveAudio { .. } => {
            if input_format.category() != FileCategory::Video {
                JobCapability::unavailable(
                    input_path,
                    CapabilityIssue::Unsupported,
                    "Audio removal accepts video files only.",
                    Vec::new(),
                )
            } else {
                tools_capability(input_path, &[("ffmpeg", "FFmpeg"), ("ffprobe", "FFmpeg")])
            }
        }
        JobRequest::CompressAudio { .. } => {
            if input_format.category() != FileCategory::Audio {
                JobCapability::unavailable(
                    input_path,
                    CapabilityIssue::Unsupported,
                    "Audio compression accepts MP3, WAV, AAC, FLAC, OGG, and M4A files only.",
                    Vec::new(),
                )
            } else {
                let tools =
                    tools_capability(input_path, &[("ffmpeg", "FFmpeg"), ("ffprobe", "FFmpeg")]);
                if !tools.available {
                    tools
                } else if let Err(missing) = super::audio::compression_support(input_format) {
                    JobCapability::unavailable(
                        input_path,
                        CapabilityIssue::MissingDependency,
                        format!(
                            "This FFmpeg installation is missing: {}.",
                            missing.join(", ")
                        ),
                        vec!["ffmpeg".into()],
                    )
                } else {
                    JobCapability::available(input_path, "FFmpeg")
                }
            }
        }
        JobRequest::ExtractSubtitles {
            stream_index,
            stream_codec,
            output_format,
            ..
        } => {
            if input_format.category() != FileCategory::Video {
                JobCapability::unavailable(
                    input_path,
                    CapabilityIssue::Unsupported,
                    "Subtitle extraction accepts video files only.",
                    Vec::new(),
                )
            } else if *stream_index > 4_096 || stream_codec.len() > 64 {
                JobCapability::unavailable(
                    input_path,
                    CapabilityIssue::InvalidSettings,
                    "Choose a valid subtitle track.",
                    Vec::new(),
                )
            } else {
                let tools =
                    tools_capability(input_path, &[("ffmpeg", "FFmpeg"), ("ffprobe", "FFmpeg")]);
                if !tools.available {
                    tools
                } else if let Err(missing) =
                    super::subtitles::extraction_support(stream_codec, *output_format)
                {
                    JobCapability::unavailable(
                        input_path,
                        CapabilityIssue::Unsupported,
                        format!(
                            "This subtitle track cannot be exported: {}.",
                            missing.join(", ")
                        ),
                        Vec::new(),
                    )
                } else {
                    JobCapability::available(input_path, "FFmpeg")
                }
            }
        }
        JobRequest::GenerateThumbnails { output_format, .. } => {
            if input_format.category() != FileCategory::Video {
                JobCapability::unavailable(
                    input_path,
                    CapabilityIssue::Unsupported,
                    "Thumbnail generation accepts video files only.",
                    Vec::new(),
                )
            } else {
                let tools =
                    tools_capability(input_path, &[("ffmpeg", "FFmpeg"), ("ffprobe", "FFmpeg")]);
                if !tools.available {
                    tools
                } else if let Err(missing) = super::thumbnails::thumbnail_support(*output_format) {
                    JobCapability::unavailable(
                        input_path,
                        CapabilityIssue::MissingDependency,
                        format!(
                            "This FFmpeg installation is missing: {}.",
                            missing.join(", ")
                        ),
                        vec!["ffmpeg".into()],
                    )
                } else {
                    JobCapability::available(input_path, "FFmpeg")
                }
            }
        }
        JobRequest::ExtractText { .. } => {
            let Some((tool, label)) = super::text::required_tool(input_format) else {
                return JobCapability::unavailable(
                    input_path,
                    CapabilityIssue::Unsupported,
                    "Text extraction accepts PDF, DOCX, HTML, Markdown, and EPUB files only.",
                    Vec::new(),
                );
            };
            tool_capability(input_path, tool, label)
        }
        JobRequest::RecognizeText { output_format, .. } => {
            if !super::ocr::supports_ocr_input(path) {
                return JobCapability::unavailable(
                    input_path,
                    CapabilityIssue::Unsupported,
                    "Text recognition accepts common raster images and PDF files only.",
                    Vec::new(),
                );
            }
            if !super::ocr::vision_helper_available() {
                return JobCapability::unavailable(
                    input_path,
                    CapabilityIssue::MissingDependency,
                    "The bundled on-device OCR component is missing. Reinstall ConvertKit.",
                    vec!["convertkit-vision-ocr".into()],
                );
            }
            if input_format == Format::Pdf && *output_format == super::ocr::OcrOutputFormat::Text {
                tools_capability(
                    input_path,
                    &[("pdfinfo", "Poppler"), ("pdftoppm", "Poppler")],
                )
            } else {
                JobCapability::available(input_path, "macOS Vision")
            }
        }
        JobRequest::MergePdf { .. } => unreachable!("handled before single-input validation"),
        JobRequest::SplitPdf {
            mode,
            page_selection,
            ..
        } => pdf_split_capability(input_path, input_format, *mode, page_selection),
        JobRequest::ExportPdfPages {
            mode,
            page_selection,
            ..
        } => pdf_page_export_capability(input_path, input_format, *mode, page_selection),
        JobRequest::CompressPdf {
            compression_goal,
            target_size_bytes,
            ..
        } => {
            if input_format != Format::Pdf {
                JobCapability::unavailable(
                    input_path,
                    CapabilityIssue::Unsupported,
                    "PDF compression accepts PDF files only.",
                    Vec::new(),
                )
            } else if let Err(error) = super::pdf::validate_pdf_compression_settings(
                path,
                *compression_goal,
                *target_size_bytes,
            ) {
                JobCapability::unavailable(
                    input_path,
                    CapabilityIssue::InvalidSettings,
                    match error {
                        ConversionError::UnsupportedConversion { input, .. } => input,
                        _ => "Choose a valid PDF compression target.".into(),
                    },
                    Vec::new(),
                )
            } else if *compression_goal == super::pdf::PdfCompressionGoal::FileSize {
                tools_capability(input_path, &[("gs", "Ghostscript"), ("pdfinfo", "Poppler")])
            } else {
                tool_capability(input_path, "gs", "Ghostscript")
            }
        }
        JobRequest::CreateArchive { .. }
        | JobRequest::ExtractArchive { .. }
        | JobRequest::Rename { .. } => {
            unreachable!("archive jobs are handled before format validation")
        }
    }
}

fn pdf_page_export_capability(
    input_path: &str,
    input_format: Format,
    mode: super::pdf::PdfSplitMode,
    page_selection: &str,
) -> JobCapability {
    if input_format != Format::Pdf {
        return JobCapability::unavailable(
            input_path,
            CapabilityIssue::Unsupported,
            "PDF page export accepts PDF files only.",
            Vec::new(),
        );
    }
    if mode == super::pdf::PdfSplitMode::Extract
        && super::pdf::parse_page_selection(page_selection).is_err()
    {
        return JobCapability::unavailable(
            input_path,
            CapabilityIssue::InvalidSettings,
            "Enter pages such as 1-3, 5, 8-10.",
            Vec::new(),
        );
    }
    tools_capability(
        input_path,
        &[("pdfinfo", "Poppler"), ("pdftoppm", "Poppler")],
    )
}

fn pdf_split_capability(
    input_path: &str,
    input_format: Format,
    mode: super::pdf::PdfSplitMode,
    page_selection: &str,
) -> JobCapability {
    if input_format != Format::Pdf {
        return JobCapability::unavailable(
            input_path,
            CapabilityIssue::Unsupported,
            "PDF Split accepts PDF files only.",
            Vec::new(),
        );
    }

    let mut tools = vec![("pdfinfo", "Poppler"), ("pdfseparate", "Poppler")];
    if mode == super::pdf::PdfSplitMode::Extract {
        let pages = match super::pdf::parse_page_selection(page_selection) {
            Ok(pages) => pages,
            Err(_) => {
                return JobCapability::unavailable(
                    input_path,
                    CapabilityIssue::InvalidSettings,
                    "Enter pages such as 1-3, 5, 8-10.",
                    Vec::new(),
                );
            }
        };
        if pages.len() > 1 {
            tools.push(("pdfunite", "Poppler"));
        }
    }
    tools_capability(input_path, &tools)
}

fn pdf_merge_capability(input_paths: &[String]) -> JobCapability {
    let primary = input_paths.first().map(String::as_str).unwrap_or_default();
    if input_paths.len() < 2 || input_paths.len() > 100 {
        return JobCapability::unavailable(
            primary,
            CapabilityIssue::InvalidSettings,
            "Add between 2 and 100 PDF, PNG, or JPEG files.",
            Vec::new(),
        );
    }

    let mut seen = BTreeSet::new();
    let mut includes_images = false;
    for input_path in input_paths {
        let path = Path::new(input_path);
        if !path.is_file() {
            return JobCapability::unavailable(
                input_path,
                CapabilityIssue::MissingInput,
                "One of the source files is no longer available.",
                Vec::new(),
            );
        }
        let format = format_from_path(path);
        if !matches!(format, Some(Format::Pdf | Format::Png | Format::Jpg)) {
            return JobCapability::unavailable(
                input_path,
                CapabilityIssue::Unsupported,
                "Combine to PDF accepts PDF, PNG, and JPEG files only.",
                Vec::new(),
            );
        }
        includes_images |= matches!(format, Some(Format::Png | Format::Jpg));
        let normalized = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        if !seen.insert(normalized) {
            return JobCapability::unavailable(
                input_path,
                CapabilityIssue::InvalidSettings,
                "Remove duplicate files before combining.",
                Vec::new(),
            );
        }
    }

    let required_tools = combine_pdf_required_tools(includes_images);
    let mut missing_tools = Vec::new();
    for tool in required_tools {
        if engines::resolve_tool(tool).is_none() {
            missing_tools.push(tool.to_string());
        }
    }
    if missing_tools.is_empty() {
        JobCapability::available(
            primary,
            if includes_images {
                "Poppler + built-in image PDF"
            } else {
                "Poppler"
            },
        )
    } else {
        let helper_missing = missing_tools
            .iter()
            .any(|tool| tool == "convertkit-image-pdf");
        let poppler_missing = missing_tools
            .iter()
            .any(|tool| tool == "pdfunite" || tool == "pdfinfo");
        let message = match (poppler_missing, helper_missing) {
            (true, true) => {
                "Install Poppler and reinstall ConvertKit to restore PDF combining support."
            }
            (true, false) => "Install Poppler to combine files into a PDF.",
            (false, true) => "Reinstall ConvertKit to restore its image to PDF component.",
            (false, false) => "PDF combining is unavailable.",
        };
        JobCapability::unavailable(
            primary,
            CapabilityIssue::MissingDependency,
            message,
            missing_tools,
        )
    }
}

fn combine_pdf_required_tools(includes_images: bool) -> Vec<&'static str> {
    let mut tools = vec!["pdfunite", "pdfinfo"];
    if includes_images {
        tools.push("convertkit-image-pdf");
    }
    tools
}

fn conversion_capability(
    input_path: &str,
    input_format: Format,
    output_format: &str,
) -> JobCapability {
    let Some(output_format) = Format::from_extension(output_format) else {
        return JobCapability::unavailable(
            input_path,
            CapabilityIssue::Unsupported,
            "Choose a supported output format.",
            Vec::new(),
        );
    };

    let supported_engines = engines::get_engines(input_format, output_format);
    if supported_engines.is_empty() {
        return JobCapability::unavailable(
            input_path,
            CapabilityIssue::Unsupported,
            format!(
                "{} to {} is not supported.",
                input_format.label(),
                output_format.label()
            ),
            Vec::new(),
        );
    }
    if let Some(engine) = engines::get_engine(input_format, output_format) {
        return JobCapability::available(input_path, engine.required_tool());
    }

    let groups = engines::missing_tool_groups(input_format, output_format);
    let requirements = groups
        .iter()
        .map(|group| group.join(" + "))
        .collect::<Vec<_>>()
        .join(" or ");
    let missing_tools = groups
        .into_iter()
        .flatten()
        .map(str::to_string)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    JobCapability::unavailable(
        input_path,
        CapabilityIssue::MissingDependency,
        format!("Install {requirements} for this conversion."),
        missing_tools,
    )
}

fn tool_capability(input_path: &str, tool: &str, label: &str) -> JobCapability {
    if engines::resolve_tool(tool).is_some() {
        JobCapability::available(input_path, tool)
    } else {
        JobCapability::unavailable(
            input_path,
            CapabilityIssue::MissingDependency,
            format!("Install {label} to use this tool."),
            vec![tool.into()],
        )
    }
}

fn tools_capability(input_path: &str, tools: &[(&str, &str)]) -> JobCapability {
    let missing = tools
        .iter()
        .filter_map(|(tool, _)| engines::resolve_tool(tool).is_none().then_some(*tool))
        .collect::<Vec<_>>();
    let label = tools
        .first()
        .map(|(_, label)| *label)
        .unwrap_or("required tools");
    if missing.is_empty() {
        JobCapability::available(input_path, label)
    } else {
        JobCapability::unavailable(
            input_path,
            CapabilityIssue::MissingDependency,
            format!("Install {label} to use this tool."),
            missing.into_iter().map(str::to_string).collect(),
        )
    }
}

fn format_from_path(path: &Path) -> Option<Format> {
    path.extension()
        .and_then(|extension| extension.to_str())
        .and_then(Format::from_extension)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn output_options() -> Option<OutputOptions> {
        Some(OutputOptions::default())
    }

    #[test]
    fn deserializes_frontend_job_shape() {
        let request: JobRequest = serde_json::from_value(serde_json::json!({
            "operation": "resize",
            "inputPath": "/tmp/photo.png",
            "width": 800,
            "height": 600,
            "preserveAspect": true,
            "jobId": null,
            "outputOptions": {
                "directory": null,
                "suffix": "-resized",
            }
        }))
        .expect("job request");

        assert!(matches!(
            request,
            JobRequest::Resize {
                width: 800,
                height: 600,
                preserve_aspect: true,
                ..
            }
        ));
    }

    #[test]
    fn deserializes_legacy_and_target_size_image_optimization_jobs() {
        let legacy: JobRequest = serde_json::from_value(serde_json::json!({
            "operation": "optimize",
            "inputPath": "/tmp/photo.jpg",
            "keepMetadata": false,
            "jobId": null,
            "outputOptions": null
        }))
        .expect("legacy optimize request");
        assert!(matches!(
            legacy,
            JobRequest::Optimize {
                compression_goal: crate::commands::optimize::ImageOptimizationGoal::Quality,
                target_size_bytes: None,
                ..
            }
        ));

        let target: JobRequest = serde_json::from_value(serde_json::json!({
            "operation": "optimize",
            "inputPath": "/tmp/photo.webp",
            "keepMetadata": true,
            "compressionGoal": "fileSize",
            "targetSizeBytes": 512000,
            "jobId": "image-target",
            "outputOptions": null
        }))
        .expect("target-size optimize request");
        assert!(matches!(
            target,
            JobRequest::Optimize {
                compression_goal: crate::commands::optimize::ImageOptimizationGoal::FileSize,
                target_size_bytes: Some(512_000),
                ..
            }
        ));
    }

    #[test]
    fn image_target_capability_rejects_lossless_and_invalid_targets_first() {
        let directory = tempfile::tempdir().expect("tempdir");
        let png = directory.path().join("source.png");
        let jpg = directory.path().join("source.jpg");
        std::fs::write(&png, vec![0_u8; 64 * 1024]).expect("PNG fixture");
        std::fs::write(&jpg, vec![0_u8; 64 * 1024]).expect("JPEG fixture");

        for (input_path, target_size_bytes) in [
            (png.to_string_lossy().into_owned(), Some(16 * 1024)),
            (jpg.to_string_lossy().into_owned(), Some(16 * 1024 + 1)),
            (jpg.to_string_lossy().into_owned(), Some(64 * 1024)),
        ] {
            let capability = job_capability(&JobRequest::Optimize {
                input_path,
                keep_metadata: false,
                compression_goal: crate::commands::optimize::ImageOptimizationGoal::FileSize,
                target_size_bytes,
                job_id: None,
                output_options: output_options(),
            });
            assert!(!capability.available);
            assert!(matches!(
                capability.issue,
                Some(CapabilityIssue::InvalidSettings)
            ));
        }
    }

    #[test]
    fn deserializes_image_export_job_shape() {
        let request: JobRequest = serde_json::from_value(serde_json::json!({
            "operation": "exportImages",
            "inputPath": "/tmp/photo.png",
            "presets": ["web", "email", "preview"],
            "jobId": null,
            "outputOptions": {
                "directory": null,
                "suffix": "-campaign",
            }
        }))
        .expect("image export request");

        assert!(matches!(
            request,
            JobRequest::ExportImages { ref presets, .. }
                if presets == &[
                    super::super::image_export::ImageExportPreset::Web,
                    super::super::image_export::ImageExportPreset::Email,
                    super::super::image_export::ImageExportPreset::Preview,
                ]
        ));
    }

    #[test]
    fn deserializes_extract_audio_job_shape() {
        let request: JobRequest = serde_json::from_value(serde_json::json!({
            "operation": "extractAudio",
            "inputPath": "/tmp/video.mp4",
            "streamIndex": 2,
            "streamCodec": "aac",
            "outputFormat": "m4a",
            "jobId": null,
            "outputOptions": {
                "directory": null,
                "suffix": "-audio",
            }
        }))
        .expect("audio extraction request");

        assert!(matches!(
            request,
            JobRequest::ExtractAudio {
                stream_index: 2,
                ref stream_codec,
                output_format: crate::commands::convert::AudioOutputFormat::M4a,
                ..
            } if stream_codec == "aac"
        ));
    }

    #[test]
    fn deserializes_video_encoding_job_shape() {
        let request: JobRequest = serde_json::from_value(serde_json::json!({
            "operation": "encodeVideo",
            "inputPath": "/tmp/video.mov",
            "preset": "smaller",
            "resolution": "hd",
            "quality": "smallest",
            "compressionGoal": "fileSize",
            "targetSizeBytes": 26214400,
            "jobId": null,
            "outputOptions": {
                "directory": null,
                "suffix": "-encoded",
            }
        }))
        .expect("video encoding request");

        assert!(matches!(
            request,
            JobRequest::EncodeVideo {
                preset: crate::commands::video::VideoEncodingPreset::Smaller,
                resolution: crate::commands::video::VideoResolution::Hd,
                quality: crate::commands::video::VideoQuality::Smallest,
                compression_goal: crate::commands::video::VideoCompressionGoal::FileSize,
                target_size_bytes: Some(26_214_400),
                ..
            }
        ));
    }

    #[test]
    fn defaults_video_delivery_controls_for_older_clients() {
        let request: JobRequest = serde_json::from_value(serde_json::json!({
            "operation": "encodeVideo",
            "inputPath": "/tmp/video.mov",
            "preset": "compatible",
            "jobId": null,
            "outputOptions": null
        }))
        .expect("legacy video encoding request");

        assert!(matches!(
            request,
            JobRequest::EncodeVideo {
                resolution: crate::commands::video::VideoResolution::Automatic,
                quality: crate::commands::video::VideoQuality::Balanced,
                compression_goal: crate::commands::video::VideoCompressionGoal::Quality,
                target_size_bytes: None,
                ..
            }
        ));
    }

    #[test]
    fn deserializes_remove_audio_job_shape() {
        let request: JobRequest = serde_json::from_value(serde_json::json!({
            "operation": "removeAudio",
            "inputPath": "/tmp/video.mov",
            "jobId": null,
            "outputOptions": {
                "directory": null,
                "suffix": "-silent",
            }
        }))
        .expect("audio removal request");

        assert!(matches!(request, JobRequest::RemoveAudio { .. }));
    }

    #[test]
    fn deserializes_compress_audio_job_shape_and_balanced_default() {
        let high: JobRequest = serde_json::from_value(serde_json::json!({
            "operation": "compressAudio",
            "inputPath": "/tmp/recording.flac",
            "preset": "high",
            "jobId": "audio-compression",
            "outputOptions": {
                "directory": null,
                "suffix": "-compressed",
            }
        }))
        .expect("audio compression request");
        assert!(matches!(
            high,
            JobRequest::CompressAudio {
                preset: crate::commands::audio::AudioCompressionPreset::High,
                ..
            }
        ));

        let legacy: JobRequest = serde_json::from_value(serde_json::json!({
            "operation": "compressAudio",
            "inputPath": "/tmp/recording.wav",
            "jobId": null,
            "outputOptions": null
        }))
        .expect("default audio compression request");
        assert!(matches!(
            legacy,
            JobRequest::CompressAudio {
                preset: crate::commands::audio::AudioCompressionPreset::Balanced,
                ..
            }
        ));
    }

    #[test]
    fn deserializes_subtitle_extraction_job_shape() {
        let request: JobRequest = serde_json::from_value(serde_json::json!({
            "operation": "extractSubtitles",
            "inputPath": "/tmp/movie.mkv",
            "streamIndex": 3,
            "streamCodec": "subrip",
            "outputFormat": "vtt",
            "jobId": null,
            "outputOptions": {
                "directory": null,
                "suffix": "-subtitles",
            }
        }))
        .expect("subtitle extraction request");

        assert!(matches!(
            request,
            JobRequest::ExtractSubtitles {
                stream_index: 3,
                output_format: crate::commands::subtitles::SubtitleOutputFormat::Vtt,
                ..
            }
        ));
    }

    #[test]
    fn deserializes_transcription_job_shape() {
        let request: JobRequest = serde_json::from_value(serde_json::json!({
            "operation": "transcribe",
            "inputPath": "/tmp/interview.m4a",
            "model": "base",
            "language": "auto",
            "outputFormat": "srt",
            "jobId": null,
            "outputOptions": {
                "directory": null,
                "suffix": "-transcript",
            }
        }))
        .expect("transcription request");

        assert!(matches!(
            request,
            JobRequest::Transcribe {
                model: crate::commands::transcribe::TranscriptionModel::Base,
                output_format: crate::commands::transcribe::TranscriptionOutputFormat::Srt,
                ref language,
                ..
            } if language == "auto"
        ));
    }

    #[test]
    fn deserializes_thumbnail_generation_job_shape() {
        let request: JobRequest = serde_json::from_value(serde_json::json!({
            "operation": "generateThumbnails",
            "inputPath": "/tmp/movie.mkv",
            "mode": "contactSheet",
            "outputFormat": "png",
            "jobId": null,
            "outputOptions": {
                "directory": null,
                "suffix": "-thumbnail",
            }
        }))
        .expect("thumbnail generation request");

        assert!(matches!(
            request,
            JobRequest::GenerateThumbnails {
                mode: crate::commands::thumbnails::ThumbnailMode::Frame,
                output_format: crate::commands::thumbnails::ThumbnailOutputFormat::Png,
                ..
            }
        ));
    }

    #[test]
    fn deserializes_extract_text_job_shape() {
        let request: JobRequest = serde_json::from_value(serde_json::json!({
            "operation": "extractText",
            "inputPath": "/tmp/report.pdf",
            "jobId": null,
            "outputOptions": {
                "directory": null,
                "suffix": "-text",
            }
        }))
        .expect("text extraction request");

        assert!(matches!(request, JobRequest::ExtractText { .. }));
    }

    #[test]
    fn deserializes_text_recognition_job_shape() {
        let request: JobRequest = serde_json::from_value(serde_json::json!({
            "operation": "recognizeText",
            "inputPath": "/tmp/scan.png",
            "jobId": null,
            "outputOptions": {
                "directory": null,
                "suffix": "-ocr",
            }
        }))
        .expect("text recognition request");

        assert!(matches!(
            request,
            JobRequest::RecognizeText {
                output_format: crate::commands::ocr::OcrOutputFormat::Text,
                ..
            }
        ));
    }

    #[test]
    fn deserializes_searchable_pdf_text_recognition_job_shape() {
        let request: JobRequest = serde_json::from_value(serde_json::json!({
            "operation": "recognizeText",
            "inputPath": "/tmp/scan.png",
            "outputFormat": "searchablePdf",
            "jobId": null,
            "outputOptions": null
        }))
        .expect("searchable PDF recognition request");

        assert!(matches!(
            request,
            JobRequest::RecognizeText {
                output_format: crate::commands::ocr::OcrOutputFormat::SearchablePdf,
                ..
            }
        ));
    }

    #[test]
    fn deserializes_remove_metadata_job_shape() {
        let request: JobRequest = serde_json::from_value(serde_json::json!({
            "operation": "removeMetadata",
            "inputPath": "/tmp/photo.jpg",
            "jobId": null,
            "outputOptions": {
                "directory": null,
                "suffix": "-clean",
            }
        }))
        .expect("metadata removal request");

        assert!(matches!(request, JobRequest::RemoveMetadata { .. }));
    }

    #[test]
    fn deserializes_pdf_merge_job_shape() {
        let request: JobRequest = serde_json::from_value(serde_json::json!({
            "operation": "mergePdf",
            "inputPaths": ["/tmp/first.pdf", "/tmp/second.pdf"],
            "jobId": null,
            "outputOptions": {
                "directory": null,
                "suffix": "-merged",
            }
        }))
        .expect("PDF merge request");

        assert!(matches!(
            request,
            JobRequest::MergePdf { input_paths, .. } if input_paths.len() == 2
        ));
    }

    #[test]
    fn pdf_merge_requires_two_inputs() {
        let result = pdf_merge_capability(&["/tmp/only.pdf".into()]);
        assert!(!result.available);
        assert!(matches!(
            result.issue,
            Some(CapabilityIssue::InvalidSettings)
        ));
    }

    #[test]
    fn combine_pdf_requires_poppler_and_only_needs_the_image_helper_for_images() {
        assert_eq!(
            combine_pdf_required_tools(false),
            vec!["pdfunite", "pdfinfo"]
        );
        assert_eq!(
            combine_pdf_required_tools(true),
            vec!["pdfunite", "pdfinfo", "convertkit-image-pdf"]
        );
    }

    #[test]
    fn combine_pdf_rejects_unsupported_existing_inputs() {
        let directory = tempfile::tempdir().expect("tempdir");
        let pdf = directory.path().join("first.pdf");
        let webp = directory.path().join("second.webp");
        std::fs::write(&pdf, b"%PDF-1.4").expect("PDF fixture");
        std::fs::write(&webp, b"RIFFfixtureWEBP").expect("WebP fixture");

        let result =
            pdf_merge_capability(&[pdf.to_string_lossy().into(), webp.to_string_lossy().into()]);
        assert!(!result.available);
        assert!(matches!(result.issue, Some(CapabilityIssue::Unsupported)));
    }

    #[test]
    fn deserializes_pdf_split_job_shape() {
        let request: JobRequest = serde_json::from_value(serde_json::json!({
            "operation": "splitPdf",
            "inputPath": "/tmp/source.pdf",
            "mode": "extract",
            "pageSelection": "1-3, 5",
            "jobId": null,
            "outputOptions": {
                "directory": null,
                "suffix": "-pages",
            }
        }))
        .expect("PDF split request");

        assert!(matches!(
            request,
            JobRequest::SplitPdf {
                mode: crate::commands::pdf::PdfSplitMode::Extract,
                page_selection,
                ..
            } if page_selection == "1-3, 5"
        ));
    }

    #[test]
    fn deserializes_pdf_compression_job_shape() {
        let request: JobRequest = serde_json::from_value(serde_json::json!({
            "operation": "compressPdf",
            "inputPath": "/tmp/source.pdf",
            "preset": "balanced",
            "jobId": null,
            "outputOptions": {
                "directory": null,
                "suffix": "-compressed",
            }
        }))
        .expect("PDF compression request");

        assert!(matches!(
            request,
            JobRequest::CompressPdf {
                preset: crate::commands::pdf::PdfCompressionPreset::Balanced,
                compression_goal: crate::commands::pdf::PdfCompressionGoal::Quality,
                target_size_bytes: None,
                ..
            }
        ));

        let target_request: JobRequest = serde_json::from_value(serde_json::json!({
            "operation": "compressPdf",
            "inputPath": "/tmp/source.pdf",
            "preset": "high",
            "compressionGoal": "fileSize",
            "targetSizeBytes": 10485760,
            "jobId": null,
            "outputOptions": {
                "directory": null,
                "suffix": "-compressed",
            }
        }))
        .expect("target-size PDF compression request");

        assert!(matches!(
            target_request,
            JobRequest::CompressPdf {
                compression_goal: crate::commands::pdf::PdfCompressionGoal::FileSize,
                target_size_bytes: Some(10_485_760),
                ..
            }
        ));
    }

    #[test]
    fn rejects_invalid_pdf_target_size_before_engine_preflight() {
        let directory = tempfile::tempdir().expect("tempdir");
        let input = directory.path().join("source.pdf");
        std::fs::write(&input, vec![0_u8; 2 * 1024 * 1024]).expect("PDF fixture");

        for target_size_bytes in [None, Some(512), Some(2 * 1024 * 1024)] {
            let result = job_capability(&JobRequest::CompressPdf {
                input_path: input.to_string_lossy().into_owned(),
                preset: crate::commands::pdf::PdfCompressionPreset::Balanced,
                compression_goal: crate::commands::pdf::PdfCompressionGoal::FileSize,
                target_size_bytes,
                job_id: None,
                output_options: output_options(),
            });
            assert!(!result.available);
            assert!(matches!(
                result.issue,
                Some(CapabilityIssue::InvalidSettings)
            ));
        }
    }

    #[test]
    fn deserializes_pdf_page_export_job_shape() {
        let request: JobRequest = serde_json::from_value(serde_json::json!({
            "operation": "exportPdfPages",
            "inputPath": "/tmp/source.pdf",
            "mode": "extract",
            "pageSelection": "1-3, 5",
            "outputFormat": "png",
            "resolution": "print",
            "jobId": null,
            "outputOptions": {
                "directory": null,
                "suffix": "-page",
            }
        }))
        .expect("PDF page export request");

        assert!(matches!(
            request,
            JobRequest::ExportPdfPages {
                mode: crate::commands::pdf::PdfSplitMode::Extract,
                output_format: crate::commands::pdf::PdfPageImageFormat::Png,
                resolution: crate::commands::pdf::PdfPageImageResolution::Print,
                ..
            }
        ));
    }

    #[test]
    fn deserializes_archive_job_shapes() {
        let create: JobRequest = serde_json::from_value(serde_json::json!({
            "operation": "createArchive",
            "entries": [{ "inputPath": "/tmp/photo.png", "archivePath": "photo.png" }],
            "format": "tar",
            "jobId": null,
            "outputOptions": {
                "directory": null,
                "suffix": "-archive",
            }
        }))
        .expect("create archive request");
        assert!(matches!(
            create,
            JobRequest::CreateArchive {
                entries,
                format: crate::commands::archive::ArchiveFormat::Tar,
                password: None,
                ..
            } if entries.len() == 1
        ));

        let extract: JobRequest = serde_json::from_value(serde_json::json!({
            "operation": "extractArchive",
            "inputPath": "/tmp/archive.zip",
            "jobId": null,
            "outputOptions": {
                "directory": null,
                "suffix": "-extracted",
            }
        }))
        .expect("extract archive request");
        assert!(matches!(
            extract,
            JobRequest::ExtractArchive { password: None, .. }
        ));

        let encrypted: JobRequest = serde_json::from_value(serde_json::json!({
            "operation": "extractArchive",
            "inputPath": "/tmp/private.7z",
            "password": "request-only-secret",
            "jobId": null,
            "outputOptions": null
        }))
        .expect("encrypted archive request");
        let debug = format!("{encrypted:?}");
        assert!(debug.contains("[REDACTED]"));
        assert!(!debug.contains("request-only-secret"));

        let compressed: JobRequest = serde_json::from_value(serde_json::json!({
            "operation": "createArchive",
            "entries": [{ "inputPath": "/tmp/photo.png", "archivePath": "photo.png" }],
            "format": "tarGz",
            "jobId": null,
            "outputOptions": {
                "directory": null,
                "suffix": "-archive",
            }
        }))
        .expect("create TAR.GZ request");
        assert!(matches!(
            compressed,
            JobRequest::CreateArchive {
                format: crate::commands::archive::ArchiveFormat::TarGz,
                ..
            }
        ));

        let gzip: JobRequest = serde_json::from_value(serde_json::json!({
            "operation": "createArchive",
            "entries": [{
                "inputPath": "/tmp/report.csv",
                "archivePath": "report.csv",
                "folderDerived": false
            }],
            "format": "gzip",
            "jobId": null,
            "outputOptions": {
                "directory": null,
                "suffix": "-archive",
            }
        }))
        .expect("create GZIP request");
        assert!(matches!(
            gzip,
            JobRequest::CreateArchive {
                entries,
                format: crate::commands::archive::ArchiveFormat::Gzip,
                ..
            } if entries.len() == 1 && !entries[0].folder_derived
        ));

        let seven_z: JobRequest = serde_json::from_value(serde_json::json!({
            "operation": "createArchive",
            "entries": [
                { "inputPath": "/tmp/photo.png", "archivePath": "images/photo.png" },
                { "inputPath": "/tmp/report.pdf", "archivePath": "report.pdf" }
            ],
            "format": "sevenZ",
            "jobId": null,
            "outputOptions": {
                "directory": null,
                "suffix": "-archive",
            }
        }))
        .expect("create 7Z request");
        assert!(matches!(
            seven_z,
            JobRequest::CreateArchive {
                entries,
                format: crate::commands::archive::ArchiveFormat::SevenZ,
                ..
            } if entries.len() == 2
        ));
    }

    #[test]
    fn deserializes_batch_rename_job_shape() {
        let request: JobRequest = serde_json::from_value(serde_json::json!({
            "operation": "rename",
            "items": [
                { "inputPath": "/tmp/first.txt", "outputName": "01-first.txt" },
                { "inputPath": "/tmp/second.txt", "outputName": "02-second.txt" }
            ],
            "jobId": null
        }))
        .expect("batch rename request");

        assert!(matches!(request, JobRequest::Rename { items, .. } if items.len() == 2));
    }

    #[test]
    fn rejects_missing_input_before_engine_lookup() {
        let request = JobRequest::Convert {
            input_path: "/definitely/missing/file.png".into(),
            output_format: "jpg".into(),
            job_id: None,
            output_options: output_options(),
        };
        let result = job_capability(&request);
        assert!(!result.available);
        assert!(matches!(result.issue, Some(CapabilityIssue::MissingInput)));
    }

    #[test]
    fn gzip_capability_explains_single_file_and_folder_restrictions() {
        let directory = tempfile::tempdir().expect("tempdir");
        let input = directory.path().join("report.csv");
        std::fs::write(&input, b"name,value\nalpha,1\n").expect("fixture");
        let entry = crate::commands::archive::ArchiveEntryRequest {
            input_path: input.to_string_lossy().into_owned(),
            archive_path: "report.csv".into(),
            folder_derived: false,
        };
        let request = |entries| JobRequest::CreateArchive {
            entries,
            format: crate::commands::archive::ArchiveFormat::Gzip,
            password: None,
            job_id: None,
            output_options: output_options(),
        };

        let available = job_capability(&request(vec![entry.clone()]));
        assert!(available.available);
        assert_eq!(available.engine.as_deref(), Some("Built-in GZIP"));

        let multiple = job_capability(&request(vec![entry.clone(), entry.clone()]));
        assert!(!multiple.available);
        assert!(multiple
            .message
            .as_deref()
            .is_some_and(|message| message.contains("exactly one regular file")));

        let folder = job_capability(&request(vec![
            crate::commands::archive::ArchiveEntryRequest {
                folder_derived: true,
                ..entry
            },
        ]));
        assert!(!folder.available);
        assert!(folder
            .message
            .as_deref()
            .is_some_and(|message| message.contains("cannot compress a folder")));
    }

    #[test]
    fn seven_z_creation_is_available_through_the_builtin_engine() {
        let directory = tempfile::tempdir().expect("tempdir");
        let input = directory.path().join("report.csv");
        std::fs::write(&input, b"name,value\nalpha,1\n").expect("fixture");
        let request = JobRequest::CreateArchive {
            entries: vec![crate::commands::archive::ArchiveEntryRequest {
                input_path: input.to_string_lossy().into_owned(),
                archive_path: "reports/report.csv".into(),
                folder_derived: true,
            }],
            format: crate::commands::archive::ArchiveFormat::SevenZ,
            password: None,
            job_id: None,
            output_options: output_options(),
        };

        let capability = job_capability(&request);

        assert!(capability.available);
        assert_eq!(capability.engine.as_deref(), Some("Built-in 7Z"));
    }

    #[test]
    fn rejects_invalid_resize_dimensions() {
        let directory = tempfile::tempdir().expect("tempdir");
        let input = directory.path().join("image.png");
        std::fs::write(&input, b"fixture").expect("image fixture");
        let request = JobRequest::Resize {
            input_path: input.to_string_lossy().into_owned(),
            width: 0,
            height: 100,
            preserve_aspect: true,
            job_id: None,
            output_options: output_options(),
        };
        let result = job_capability(&request);
        assert!(!result.available);
        assert!(matches!(
            result.issue,
            Some(CapabilityIssue::InvalidSettings)
        ));
    }

    #[test]
    fn rejects_unsupported_conversion_pair() {
        let directory = tempfile::tempdir().expect("tempdir");
        let input = directory.path().join("notes.txt");
        std::fs::write(&input, b"fixture").expect("text fixture");
        let request = JobRequest::Convert {
            input_path: input.to_string_lossy().into_owned(),
            output_format: "png".into(),
            job_id: None,
            output_options: output_options(),
        };
        let result = job_capability(&request);
        assert!(!result.available);
        assert!(matches!(result.issue, Some(CapabilityIssue::Unsupported)));
    }

    #[test]
    fn rejects_audio_extraction_from_non_video_files() {
        let directory = tempfile::tempdir().expect("tempdir");
        let input = directory.path().join("recording.mp3");
        std::fs::write(&input, b"fixture").expect("audio fixture");
        let request = JobRequest::ExtractAudio {
            input_path: input.to_string_lossy().into_owned(),
            stream_index: 1,
            stream_codec: "aac".into(),
            output_format: crate::commands::convert::AudioOutputFormat::Wav,
            job_id: None,
            output_options: output_options(),
        };
        let result = job_capability(&request);
        assert!(!result.available);
        assert!(matches!(result.issue, Some(CapabilityIssue::Unsupported)));
    }

    #[test]
    fn rejects_audio_extraction_without_a_valid_track_selection() {
        let directory = tempfile::tempdir().expect("tempdir");
        let input = directory.path().join("movie.mp4");
        std::fs::write(&input, b"fixture").expect("video fixture");
        let request = JobRequest::ExtractAudio {
            input_path: input.to_string_lossy().into_owned(),
            stream_index: 1,
            stream_codec: String::new(),
            output_format: crate::commands::convert::AudioOutputFormat::Wav,
            job_id: None,
            output_options: output_options(),
        };
        let result = job_capability(&request);
        assert!(!result.available);
        assert!(matches!(
            result.issue,
            Some(CapabilityIssue::InvalidSettings)
        ));
    }

    #[test]
    fn rejects_audio_removal_from_non_video_files() {
        let directory = tempfile::tempdir().expect("tempdir");
        let input = directory.path().join("recording.mp3");
        std::fs::write(&input, b"fixture").expect("audio fixture");
        let request = JobRequest::RemoveAudio {
            input_path: input.to_string_lossy().into_owned(),
            job_id: None,
            output_options: output_options(),
        };
        let result = job_capability(&request);
        assert!(!result.available);
        assert!(matches!(result.issue, Some(CapabilityIssue::Unsupported)));
    }

    #[test]
    fn rejects_audio_compression_from_non_audio_files() {
        let directory = tempfile::tempdir().expect("tempdir");
        let input = directory.path().join("clip.mp4");
        std::fs::write(&input, b"fixture").expect("video fixture");
        let request = JobRequest::CompressAudio {
            input_path: input.to_string_lossy().into_owned(),
            preset: crate::commands::audio::AudioCompressionPreset::Balanced,
            job_id: None,
            output_options: output_options(),
        };
        let result = job_capability(&request);
        assert!(!result.available);
        assert!(matches!(result.issue, Some(CapabilityIssue::Unsupported)));
    }

    #[test]
    fn rejects_subtitle_extraction_from_non_video_files() {
        let directory = tempfile::tempdir().expect("tempdir");
        let input = directory.path().join("captions.srt");
        std::fs::write(&input, b"fixture").expect("subtitle fixture");
        let request = JobRequest::ExtractSubtitles {
            input_path: input.to_string_lossy().into_owned(),
            stream_index: 0,
            stream_codec: "subrip".into(),
            output_format: crate::commands::subtitles::SubtitleOutputFormat::Srt,
            job_id: None,
            output_options: output_options(),
        };
        let result = job_capability(&request);
        assert!(!result.available);
        assert!(matches!(result.issue, Some(CapabilityIssue::Unsupported)));
    }

    #[test]
    fn rejects_transcription_from_non_media_and_invalid_language() {
        let directory = tempfile::tempdir().expect("tempdir");
        let document = directory.path().join("notes.txt");
        let recording = directory.path().join("recording.wav");
        std::fs::write(&document, b"fixture").expect("text fixture");
        std::fs::write(&recording, b"fixture").expect("audio fixture");

        let unsupported = job_capability(&JobRequest::Transcribe {
            input_path: document.to_string_lossy().into_owned(),
            model: crate::commands::transcribe::TranscriptionModel::Tiny,
            language: "auto".into(),
            output_format: crate::commands::transcribe::TranscriptionOutputFormat::Txt,
            job_id: None,
            output_options: output_options(),
        });
        assert!(matches!(
            unsupported.issue,
            Some(CapabilityIssue::Unsupported)
        ));

        let invalid_language = job_capability(&JobRequest::Transcribe {
            input_path: recording.to_string_lossy().into_owned(),
            model: crate::commands::transcribe::TranscriptionModel::Tiny,
            language: "not-a-language".into(),
            output_format: crate::commands::transcribe::TranscriptionOutputFormat::Txt,
            job_id: None,
            output_options: output_options(),
        });
        assert!(matches!(
            invalid_language.issue,
            Some(CapabilityIssue::InvalidSettings)
        ));
    }

    #[test]
    fn rejects_thumbnail_generation_from_non_video_files() {
        let directory = tempfile::tempdir().expect("tempdir");
        let input = directory.path().join("photo.png");
        std::fs::write(&input, b"fixture").expect("image fixture");
        let request = JobRequest::GenerateThumbnails {
            input_path: input.to_string_lossy().into_owned(),
            mode: crate::commands::thumbnails::ThumbnailMode::Frame,
            output_format: crate::commands::thumbnails::ThumbnailOutputFormat::Jpeg,
            job_id: None,
            output_options: output_options(),
        };
        let result = job_capability(&request);
        assert!(!result.available);
        assert!(matches!(result.issue, Some(CapabilityIssue::Unsupported)));
    }

    #[test]
    fn metadata_removal_capability_selects_a_safe_format_specific_engine() {
        let directory = tempfile::tempdir().expect("tempdir");
        let supported_image = directory.path().join("photo.jpg");
        let supported_pdf = directory.path().join("document.pdf");
        let supported_media = directory.path().join("recording.mp3");
        let unsupported = directory.path().join("photo.heic");
        std::fs::write(&supported_image, b"fixture").expect("JPEG fixture");
        std::fs::write(&supported_pdf, b"fixture").expect("PDF fixture");
        std::fs::write(&supported_media, b"fixture").expect("MP3 fixture");
        std::fs::write(&unsupported, b"fixture").expect("HEIC fixture");

        let supported_result = job_capability(&JobRequest::RemoveMetadata {
            input_path: supported_image.to_string_lossy().into_owned(),
            job_id: None,
            output_options: output_options(),
        });
        assert!(supported_result.available);
        assert_eq!(
            supported_result.engine.as_deref(),
            Some("Built-in metadata cleaner")
        );

        let pdf_result = job_capability(&JobRequest::RemoveMetadata {
            input_path: supported_pdf.to_string_lossy().into_owned(),
            job_id: None,
            output_options: output_options(),
        });
        assert!(pdf_result.available);
        assert_eq!(
            pdf_result.engine.as_deref(),
            Some("Built-in PDF metadata cleaner")
        );

        let media_result = job_capability(&JobRequest::RemoveMetadata {
            input_path: supported_media.to_string_lossy().into_owned(),
            job_id: None,
            output_options: output_options(),
        });
        assert_eq!(
            media_result.available,
            engines::resolve_tool("ffmpeg").is_some()
        );
        if media_result.available {
            assert_eq!(media_result.engine.as_deref(), Some("ffmpeg"));
        } else {
            assert_eq!(media_result.missing_tools, vec!["ffmpeg"]);
        }

        let unsupported_result = job_capability(&JobRequest::RemoveMetadata {
            input_path: unsupported.to_string_lossy().into_owned(),
            job_id: None,
            output_options: output_options(),
        });
        assert!(!unsupported_result.available);
        assert!(matches!(
            unsupported_result.issue,
            Some(CapabilityIssue::Unsupported)
        ));
    }
}
