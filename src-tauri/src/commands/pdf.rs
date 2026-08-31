use std::collections::HashSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

use serde::Deserialize;
use tauri::{AppHandle, Emitter};
use tokio_util::sync::CancellationToken;

use crate::engines::process::{capture_output, finish_output};
use crate::engines::{self, tool_command, ConversionResult};
use crate::error::ConversionError;
use crate::formats::Format;
use crate::progress::ProgressPayload;

use super::output::{prepare_output, OutputOptions};

const MAX_MERGE_INPUTS: usize = 100;
const MAX_SPLIT_PAGES: u32 = 5_000;
const MAX_PAGE_IMAGE_EXPORTS: u32 = 1_000;
const MERGE_TIMEOUT: Duration = Duration::from_secs(5 * 60);
const SPLIT_TIMEOUT: Duration = Duration::from_secs(10 * 60);
const COMPRESSION_TIMEOUT: Duration = Duration::from_secs(10 * 60);
const MAX_PDF_COMPRESSION_PAGES: u32 = 10_000;
pub(super) const PDF_TARGET_SIZE_MIN_BYTES: u64 = 1024 * 1024;
pub(super) const PDF_TARGET_SIZE_MAX_BYTES: u64 = 100 * 1024 * 1024 * 1024;

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum PdfSplitMode {
    EveryPage,
    Extract,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PdfPageImageFormat {
    Png,
    Jpeg,
}

impl PdfPageImageFormat {
    fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
        }
    }

    fn argument(self) -> &'static str {
        match self {
            Self::Png => "-png",
            Self::Jpeg => "-jpeg",
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PdfPageImageResolution {
    Screen,
    Print,
}

impl PdfPageImageResolution {
    fn dpi(self) -> u32 {
        match self {
            Self::Screen => 144,
            Self::Print => 300,
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PdfCompressionPreset {
    High,
    Balanced,
    Smallest,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum PdfCompressionGoal {
    #[default]
    Quality,
    FileSize,
}

#[derive(Debug, Clone, Copy)]
struct PdfTargetCandidate {
    resolution: u16,
    jpeg_quality: u8,
}

const PDF_TARGET_CANDIDATES: [PdfTargetCandidate; 10] = [
    PdfTargetCandidate {
        resolution: 300,
        jpeg_quality: 92,
    },
    PdfTargetCandidate {
        resolution: 240,
        jpeg_quality: 88,
    },
    PdfTargetCandidate {
        resolution: 200,
        jpeg_quality: 84,
    },
    PdfTargetCandidate {
        resolution: 170,
        jpeg_quality: 80,
    },
    PdfTargetCandidate {
        resolution: 144,
        jpeg_quality: 76,
    },
    PdfTargetCandidate {
        resolution: 120,
        jpeg_quality: 70,
    },
    PdfTargetCandidate {
        resolution: 96,
        jpeg_quality: 62,
    },
    PdfTargetCandidate {
        resolution: 72,
        jpeg_quality: 52,
    },
    PdfTargetCandidate {
        resolution: 50,
        jpeg_quality: 40,
    },
    PdfTargetCandidate {
        resolution: 36,
        jpeg_quality: 30,
    },
];

impl PdfCompressionPreset {
    fn ghostscript_setting(self) -> &'static str {
        match self {
            Self::High => "/prepress",
            Self::Balanced => "/ebook",
            Self::Smallest => "/screen",
        }
    }
}

pub async fn merge_pdfs(
    app: AppHandle,
    input_paths: Vec<String>,
    job_id: Option<String>,
    output_options: Option<OutputOptions>,
) -> Result<ConversionResult, ConversionError> {
    let inputs = validate_combine_inputs(&input_paths)?;
    require_tool("pdfunite", "Install Poppler to combine files into a PDF")?;
    require_tool("pdfinfo", "Install Poppler to verify the combined PDF")?;
    if inputs.iter().any(|input| is_combine_image(input)) {
        require_tool(
            "convertkit-image-pdf",
            "Reinstall ConvertKit to restore its image to PDF component",
        )?;
    }

    let prepared_output = prepare_output(&inputs[0], "pdf", "-combined", output_options)?;
    let active_job = super::jobs::ActiveJobGuard::register(app.clone(), job_id)?;
    let cancel_token = active_job.cancel_token();
    let event_job_id = active_job.job_id().to_owned();

    let started = Instant::now();
    let temporary_directory =
        tempfile::tempdir().map_err(|error| ConversionError::ProcessFailed {
            message: format!("Could not create a temporary PDF workspace: {error}"),
            stderr: String::new(),
            exit_code: None,
        })?;
    let (prepared_inputs, expected_pages) = prepare_combine_inputs(
        &inputs,
        temporary_directory.path(),
        &app,
        &event_job_id,
        cancel_token.clone(),
    )
    .await?;

    let _ = app.emit(
        "conversion-progress",
        ProgressPayload {
            job_id: event_job_id.clone(),
            percent: 60,
            stage: format!("Combining {} files", inputs.len()),
        },
    );

    let result = run_pdf_merge(
        &prepared_inputs,
        prepared_output.working_path(),
        expected_pages,
        cancel_token,
    )
    .await;

    match result {
        Ok(result) => {
            let mut result = prepared_output.commit(result)?;
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
        Err(error) => Err(error),
    }
}

pub async fn split_pdf(
    app: AppHandle,
    input_path: String,
    mode: PdfSplitMode,
    page_selection: String,
    job_id: Option<String>,
    output_options: Option<OutputOptions>,
) -> Result<ConversionResult, ConversionError> {
    let input = validate_pdf_input(&input_path)?;
    require_tool("pdfinfo", "Install Poppler to inspect PDF pages")?;
    require_tool("pdfseparate", "Install Poppler to split PDF files")?;

    let selected_pages = if mode == PdfSplitMode::Extract {
        let pages = parse_page_selection(&page_selection)?;
        if pages.len() > 1 {
            require_tool("pdfunite", "Install Poppler to combine extracted PDF pages")?;
        }
        Some(pages)
    } else {
        None
    };

    let active_job = super::jobs::ActiveJobGuard::register(app.clone(), job_id)?;
    let cancel_token = active_job.cancel_token();
    let event_job_id = active_job.job_id().to_owned();
    let started = Instant::now();
    let page_count = pdf_page_count(&input, cancel_token.clone()).await?;

    let mut result = match mode {
        PdfSplitMode::EveryPage => {
            split_every_page(
                &input,
                page_count,
                output_options,
                &app,
                &event_job_id,
                cancel_token,
            )
            .await?
        }
        PdfSplitMode::Extract => {
            let pages = selected_pages.expect("extract mode validates a page selection");
            validate_page_bounds(&pages, page_count)?;
            extract_pages(
                &input,
                &pages,
                output_options,
                &app,
                &event_job_id,
                cancel_token,
            )
            .await?
        }
    };

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

#[allow(clippy::too_many_arguments)]
pub async fn export_pdf_pages(
    app: AppHandle,
    input_path: String,
    mode: PdfSplitMode,
    page_selection: String,
    output_format: PdfPageImageFormat,
    resolution: PdfPageImageResolution,
    job_id: Option<String>,
    output_options: Option<OutputOptions>,
) -> Result<ConversionResult, ConversionError> {
    let input = validate_pdf_input(&input_path)?;
    require_tool("pdfinfo", "Install Poppler to inspect PDF pages")?;
    require_tool("pdftoppm", "Install Poppler to export PDF pages as images")?;

    let selected_pages = if mode == PdfSplitMode::Extract {
        Some(parse_page_selection(&page_selection)?)
    } else {
        None
    };
    let active_job = super::jobs::ActiveJobGuard::register(app.clone(), job_id)?;
    let cancel_token = active_job.cancel_token();
    let event_job_id = active_job.job_id().to_owned();
    let started = Instant::now();
    let page_count = pdf_page_count(&input, cancel_token.clone()).await?;
    let pages = match selected_pages {
        Some(pages) => {
            validate_page_bounds(&pages, page_count)?;
            pages
        }
        None => (1..=page_count).collect(),
    };
    if pages.len() > MAX_PAGE_IMAGE_EXPORTS as usize {
        return Err(invalid_page_selection(&format!(
            "Choose no more than {MAX_PAGE_IMAGE_EXPORTS} pages per PDF."
        )));
    }

    let temp = tempfile::tempdir().map_err(|error| ConversionError::ProcessFailed {
        message: format!("Could not create a temporary PDF export workspace: {error}"),
        stderr: String::new(),
        exit_code: None,
    })?;
    let base_options = output_options.unwrap_or_else(|| OutputOptions {
        suffix: "-page".into(),
        ..OutputOptions::default()
    });
    let page_width = page_count.to_string().len().max(3);
    let mut prepared = Vec::with_capacity(pages.len());

    for (index, page) in pages.iter().copied().enumerate() {
        if cancel_token.is_cancelled() {
            return Err(ConversionError::Cancelled);
        }
        let _ = app.emit(
            "conversion-progress",
            ProgressPayload {
                job_id: event_job_id.clone(),
                percent: ((index * 100) / pages.len()) as i32,
                stage: format!("Exporting page {page}"),
            },
        );

        let prefix = temp.path().join(format!("page-{index}"));
        let arguments = page_image_arguments(&input, &prefix, page, output_format, resolution);
        run_pdf_tool(
            "pdftoppm",
            &arguments,
            "PDF page export",
            cancel_token.clone(),
            SPLIT_TIMEOUT,
        )
        .await?;
        let rendered = prefix.with_extension(output_format.extension());
        verify_page_image(&rendered, output_format)?;

        let mut page_options = base_options.clone();
        page_options.suffix = format!("{}-{page:0page_width$}", base_options.suffix);
        let output = prepare_output(&input, output_format.extension(), "", Some(page_options))?;
        std::fs::copy(&rendered, output.working_path()).map_err(|error| {
            ConversionError::ProcessFailed {
                message: format!("Could not prepare page {page}: {error}"),
                stderr: String::new(),
                exit_code: None,
            }
        })?;
        prepared.push(output);
    }

    let mut result = commit_split_outputs(prepared)?;
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

fn page_image_arguments(
    input: &Path,
    prefix: &Path,
    page: u32,
    format: PdfPageImageFormat,
    resolution: PdfPageImageResolution,
) -> Vec<OsString> {
    let mut arguments = vec![
        OsString::from("-f"),
        OsString::from(page.to_string()),
        OsString::from("-l"),
        OsString::from(page.to_string()),
        OsString::from("-singlefile"),
        OsString::from("-cropbox"),
        OsString::from("-r"),
        OsString::from(resolution.dpi().to_string()),
        OsString::from(format.argument()),
    ];
    if format == PdfPageImageFormat::Jpeg {
        arguments.extend([OsString::from("-jpegopt"), OsString::from("quality=90")]);
    }
    arguments.extend([input.as_os_str().to_owned(), prefix.as_os_str().to_owned()]);
    arguments
}

fn verify_page_image(output: &Path, format: PdfPageImageFormat) -> Result<(), ConversionError> {
    let signature: &[u8] = match format {
        PdfPageImageFormat::Png => b"\x89PNG\r\n\x1a\n",
        PdfPageImageFormat::Jpeg => b"\xff\xd8\xff",
    };
    engines::verification::file_with_signature(output, signature)?;
    Ok(())
}

pub async fn compress_pdf(
    app: AppHandle,
    input_path: String,
    preset: PdfCompressionPreset,
    compression_goal: PdfCompressionGoal,
    target_size_bytes: Option<u64>,
    job_id: Option<String>,
    output_options: Option<OutputOptions>,
) -> Result<ConversionResult, ConversionError> {
    let input = validate_pdf_input(&input_path)?;
    require_tool("gs", "Install Ghostscript to compress PDF files")?;
    require_tool("pdfinfo", "Install Poppler to verify PDF page layout")?;

    let source_size = std::fs::metadata(&input)
        .map_err(|_| ConversionError::InputNotFound {
            path: input.to_string_lossy().into_owned(),
        })?
        .len();
    let target_size =
        validate_pdf_compression_settings(&input, compression_goal, target_size_bytes)?;

    let prepared_output = prepare_output(&input, "pdf", "-compressed", output_options)?;
    let active_job = super::jobs::ActiveJobGuard::register(app.clone(), job_id)?;
    let cancel_token = active_job.cancel_token();
    let event_job_id = active_job.job_id().to_owned();
    let started = Instant::now();
    let source_topology = inspect_pdf_topology(&input, cancel_token.clone()).await?;

    let _ = app.emit(
        "conversion-progress",
        ProgressPayload {
            job_id: event_job_id.clone(),
            percent: -1,
            stage: "Compressing PDF".into(),
        },
    );

    match target_size {
        Some(target_size) => {
            compress_pdf_to_target(
                &app,
                &event_job_id,
                &input,
                prepared_output.working_path(),
                target_size,
                &source_topology,
                cancel_token,
            )
            .await?;
        }
        None => {
            let arguments = ghostscript_arguments(&input, prepared_output.working_path(), preset);
            run_pdf_tool(
                "gs",
                &arguments,
                "PDF compression",
                cancel_token.clone(),
                COMPRESSION_TIMEOUT,
            )
            .await?;

            let compressed_size = engines::verification::file_with_signature(
                prepared_output.working_path(),
                b"%PDF-",
            )?;

            // Some already-optimized PDFs grow after recompression. Keep the source bytes
            // in that case so the promised compressed copy is never larger than its input.
            if compressed_size >= source_size {
                std::fs::copy(&input, prepared_output.working_path()).map_err(|error| {
                    ConversionError::ProcessFailed {
                        message: format!("Could not preserve the smaller PDF: {error}"),
                        stderr: String::new(),
                        exit_code: None,
                    }
                })?;
            }
            verify_compressed_pdf_output(
                prepared_output.working_path(),
                &source_topology,
                cancel_token,
            )
            .await?;
        }
    }

    let mut result = prepared_output.commit(ConversionResult {
        output_path: String::new(),
        output_paths: Vec::new(),
        output_size: 0,
        duration_ms: 0,
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

pub(super) fn validate_pdf_compression_settings(
    input: &Path,
    compression_goal: PdfCompressionGoal,
    target_size_bytes: Option<u64>,
) -> Result<Option<u64>, ConversionError> {
    if compression_goal == PdfCompressionGoal::Quality {
        return Ok(None);
    }
    let target = target_size_bytes
        .filter(|target| {
            (PDF_TARGET_SIZE_MIN_BYTES..=PDF_TARGET_SIZE_MAX_BYTES).contains(target)
                && target % (1024 * 1024) == 0
        })
        .ok_or_else(|| invalid_pdf_target("Choose a whole-megabyte target from 1 MB to 100 GB."))?;
    let source_size = std::fs::metadata(input)
        .map_err(|_| ConversionError::InputNotFound {
            path: input.to_string_lossy().into_owned(),
        })?
        .len();
    if target >= source_size {
        return Err(invalid_pdf_target(
            "Choose a target smaller than the source PDF.",
        ));
    }
    Ok(Some(target))
}

fn invalid_pdf_target(message: &str) -> ConversionError {
    ConversionError::UnsupportedConversion {
        input: message.into(),
        output: "target-size PDF".into(),
    }
}

async fn compress_pdf_to_target(
    app: &AppHandle,
    job_id: &str,
    input: &Path,
    output: &Path,
    target_size: u64,
    source_topology: &PdfTopology,
    cancel_token: CancellationToken,
) -> Result<(), ConversionError> {
    for (index, candidate) in PDF_TARGET_CANDIDATES.iter().enumerate() {
        if cancel_token.is_cancelled() {
            engines::cleanup_partial(output);
            return Err(ConversionError::Cancelled);
        }
        engines::cleanup_partial(output);
        let _ = app.emit(
            "conversion-progress",
            ProgressPayload {
                job_id: job_id.to_owned(),
                percent: 5 + ((index * 80) / PDF_TARGET_CANDIDATES.len()) as i32,
                stage: format!(
                    "Trying PDF size {} of {}",
                    index + 1,
                    PDF_TARGET_CANDIDATES.len()
                ),
            },
        );
        let arguments = target_ghostscript_arguments(input, output, *candidate);
        run_pdf_tool(
            "gs",
            &arguments,
            "Target-size PDF compression",
            cancel_token.clone(),
            COMPRESSION_TIMEOUT,
        )
        .await?;
        let output_size = engines::verification::file_with_signature(output, b"%PDF-")?;
        if output_size > target_size {
            continue;
        }

        verify_compressed_pdf_output(output, source_topology, cancel_token.clone()).await?;
        return Ok(());
    }

    engines::cleanup_partial(output);
    Err(ConversionError::ProcessFailed {
        message: format!(
            "This PDF could not reach the {} MB target without changing its page layout.",
            target_size / (1024 * 1024)
        ),
        stderr: String::new(),
        exit_code: None,
    })
}

async fn verify_compressed_pdf_output(
    output: &Path,
    source_topology: &PdfTopology,
    cancel_token: CancellationToken,
) -> Result<u64, ConversionError> {
    let output_size = engines::verification::file_with_signature(output, b"%PDF-")?;
    let output_topology = match inspect_pdf_topology(output, cancel_token).await {
        Ok(topology) => topology,
        Err(error) => {
            engines::cleanup_partial(output);
            return Err(error);
        }
    };
    if !pdf_topologies_match(source_topology, &output_topology) {
        engines::cleanup_partial(output);
        return Err(ConversionError::ProcessFailed {
            message:
                "The compressed PDF changed its page count or page boxes, so no output was saved."
                    .into(),
            stderr: String::new(),
            exit_code: None,
        });
    }
    Ok(output_size)
}

fn ghostscript_arguments(
    input: &Path,
    output: &Path,
    preset: PdfCompressionPreset,
) -> Vec<OsString> {
    let mut output_argument = OsString::from("-sOutputFile=");
    output_argument.push(output.as_os_str());
    vec![
        OsString::from("-dSAFER"),
        OsString::from("-dBATCH"),
        OsString::from("-dNOPAUSE"),
        OsString::from("-dQUIET"),
        OsString::from("-sDEVICE=pdfwrite"),
        OsString::from("-dCompatibilityLevel=1.7"),
        OsString::from("-dDetectDuplicateImages=true"),
        OsString::from("-dCompressFonts=true"),
        OsString::from("-dSubsetFonts=true"),
        OsString::from("-dAutoRotatePages=/None"),
        OsString::from(format!("-dPDFSETTINGS={}", preset.ghostscript_setting())),
        output_argument,
        input.as_os_str().to_owned(),
    ]
}

fn target_ghostscript_arguments(
    input: &Path,
    output: &Path,
    candidate: PdfTargetCandidate,
) -> Vec<OsString> {
    let mut arguments = ghostscript_arguments(input, output, PdfCompressionPreset::Balanced);
    let input_argument = arguments.pop().expect("Ghostscript input argument");
    let output_argument = arguments.pop().expect("Ghostscript output argument");
    arguments.extend([
        OsString::from("-dAutoFilterColorImages=false"),
        OsString::from("-dAutoFilterGrayImages=false"),
        OsString::from("-dColorImageFilter=/DCTEncode"),
        OsString::from("-dGrayImageFilter=/DCTEncode"),
        OsString::from("-dDownsampleColorImages=true"),
        OsString::from("-dDownsampleGrayImages=true"),
        OsString::from("-dDownsampleMonoImages=true"),
        OsString::from("-dColorImageDownsampleType=/Bicubic"),
        OsString::from("-dGrayImageDownsampleType=/Bicubic"),
        OsString::from("-dMonoImageDownsampleType=/Subsample"),
        OsString::from(format!("-dColorImageResolution={}", candidate.resolution)),
        OsString::from(format!("-dGrayImageResolution={}", candidate.resolution)),
        OsString::from(format!(
            "-dMonoImageResolution={}",
            candidate.resolution.max(72)
        )),
        OsString::from(format!("-dJPEGQ={}", candidate.jpeg_quality)),
        output_argument,
        input_argument,
    ]);
    arguments
}

#[derive(Debug, Clone)]
struct PdfTopology {
    pages: Vec<PdfPageTopology>,
}

#[derive(Debug, Clone, Default)]
struct PdfPageTopology {
    rotation: Option<i32>,
    media_box: Option<[f64; 4]>,
    crop_box: Option<[f64; 4]>,
    bleed_box: Option<[f64; 4]>,
    trim_box: Option<[f64; 4]>,
    art_box: Option<[f64; 4]>,
}

async fn inspect_pdf_topology(
    input: &Path,
    cancel_token: CancellationToken,
) -> Result<PdfTopology, ConversionError> {
    let page_count = pdf_page_count(input, cancel_token.clone()).await?;
    if page_count > MAX_PDF_COMPRESSION_PAGES {
        return Err(ConversionError::UnsupportedConversion {
            input: format!("This PDF contains {page_count} pages"),
            output: format!("compression for PDFs with at most {MAX_PDF_COMPRESSION_PAGES} pages"),
        });
    }
    let arguments = vec![
        OsString::from("-f"),
        OsString::from("1"),
        OsString::from("-l"),
        OsString::from(page_count.to_string()),
        OsString::from("-box"),
        input.as_os_str().to_owned(),
    ];
    let output = run_pdf_tool(
        "pdfinfo",
        &arguments,
        "PDF page-layout inspection",
        cancel_token,
        Duration::from_secs(30),
    )
    .await?;
    parse_pdf_topology(&output, page_count)
}

fn parse_pdf_topology(output: &str, page_count: u32) -> Result<PdfTopology, ConversionError> {
    let mut pages = vec![PdfPageTopology::default(); page_count as usize];
    for line in output.lines() {
        let fields = line.split_whitespace().collect::<Vec<_>>();
        if fields.len() < 4 || fields.first() != Some(&"Page") {
            continue;
        }
        let Some(index) = fields
            .get(1)
            .and_then(|value| value.parse::<usize>().ok())
            .and_then(|page| page.checked_sub(1))
            .filter(|index| *index < pages.len())
        else {
            continue;
        };
        match fields[2].trim_end_matches(':') {
            "rot" => {
                pages[index].rotation = fields[3]
                    .parse::<i32>()
                    .ok()
                    .filter(|rotation| matches!(rotation, 0 | 90 | 180 | 270))
            }
            "MediaBox" => pages[index].media_box = parse_pdf_box(&fields[3..]),
            "CropBox" => pages[index].crop_box = parse_pdf_box(&fields[3..]),
            "BleedBox" => pages[index].bleed_box = parse_pdf_box(&fields[3..]),
            "TrimBox" => pages[index].trim_box = parse_pdf_box(&fields[3..]),
            "ArtBox" => pages[index].art_box = parse_pdf_box(&fields[3..]),
            _ => {}
        }
    }
    if pages.iter().any(|page| {
        page.rotation.is_none()
            || page.media_box.is_none()
            || page.crop_box.is_none()
            || page.bleed_box.is_none()
            || page.trim_box.is_none()
            || page.art_box.is_none()
    }) {
        return Err(ConversionError::ProcessFailed {
            message: "Could not verify every PDF page box.".into(),
            stderr: output.into(),
            exit_code: None,
        });
    }
    Ok(PdfTopology { pages })
}

fn parse_pdf_box(fields: &[&str]) -> Option<[f64; 4]> {
    if fields.len() < 4 {
        return None;
    }
    let parsed = [
        fields[0].parse().ok()?,
        fields[1].parse().ok()?,
        fields[2].parse().ok()?,
        fields[3].parse().ok()?,
    ];
    if parsed.iter().all(|value: &f64| value.is_finite())
        && parsed[2] > parsed[0]
        && parsed[3] > parsed[1]
    {
        Some(parsed)
    } else {
        None
    }
}

fn pdf_topologies_match(source: &PdfTopology, output: &PdfTopology) -> bool {
    source.pages.len() == output.pages.len()
        && source
            .pages
            .iter()
            .zip(&output.pages)
            .all(|(source, output)| {
                source.rotation == output.rotation
                    && pdf_boxes_match(source.media_box, output.media_box)
                    && pdf_boxes_match(source.crop_box, output.crop_box)
                    && pdf_boxes_match(source.bleed_box, output.bleed_box)
                    && pdf_boxes_match(source.trim_box, output.trim_box)
                    && pdf_boxes_match(source.art_box, output.art_box)
            })
}

fn pdf_boxes_match(source: Option<[f64; 4]>, output: Option<[f64; 4]>) -> bool {
    match (source, output) {
        (Some(source), Some(output)) => source
            .iter()
            .zip(output)
            .all(|(source, output)| (source - output).abs() <= 0.01),
        (None, None) => true,
        _ => false,
    }
}

fn validate_pdf_input(input_path: &str) -> Result<PathBuf, ConversionError> {
    let path = PathBuf::from(input_path);
    if !path.is_file() {
        return Err(ConversionError::InputNotFound {
            path: input_path.into(),
        });
    }
    let format = path
        .extension()
        .and_then(|extension| extension.to_str())
        .and_then(Format::from_extension);
    if format != Some(Format::Pdf) {
        return Err(ConversionError::UnsupportedConversion {
            input: input_path.into(),
            output: "PDF pages".into(),
        });
    }
    Ok(path)
}

fn require_tool(tool: &str, install_hint: &str) -> Result<(), ConversionError> {
    if engines::resolve_tool(tool).is_some() {
        Ok(())
    } else {
        Err(ConversionError::MissingDependency {
            tool: tool.into(),
            install_hint: install_hint.into(),
        })
    }
}

pub(super) fn parse_page_selection(value: &str) -> Result<Vec<u32>, ConversionError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(invalid_page_selection("Enter pages such as 1-3, 5, 8-10."));
    }

    let mut pages = Vec::new();
    let mut seen = HashSet::new();
    for part in value.split(',') {
        let part = part.trim();
        if part.is_empty() {
            return Err(invalid_page_selection("Remove the empty page range."));
        }

        let (start, end) = if let Some((start, end)) = part.split_once('-') {
            if end.contains('-') {
                return Err(invalid_page_selection("Use page ranges such as 3-7."));
            }
            (parse_page_number(start)?, parse_page_number(end)?)
        } else {
            let page = parse_page_number(part)?;
            (page, page)
        };

        if start > end {
            return Err(invalid_page_selection(
                "Page ranges must run from low to high.",
            ));
        }
        for page in start..=end {
            if !seen.insert(page) {
                return Err(invalid_page_selection("Each page can appear only once."));
            }
            pages.push(page);
            if pages.len() > MAX_SPLIT_PAGES as usize {
                return Err(invalid_page_selection("The selection is too large."));
            }
        }
    }
    Ok(pages)
}

fn parse_page_number(value: &str) -> Result<u32, ConversionError> {
    value
        .trim()
        .parse::<u32>()
        .ok()
        .filter(|page| *page > 0)
        .ok_or_else(|| invalid_page_selection("Page numbers must be positive whole numbers."))
}

fn invalid_page_selection(message: &str) -> ConversionError {
    ConversionError::UnsupportedConversion {
        input: message.into(),
        output: "selected PDF pages".into(),
    }
}

fn validate_page_bounds(pages: &[u32], page_count: u32) -> Result<(), ConversionError> {
    if pages.iter().any(|page| *page > page_count) {
        return Err(invalid_page_selection(&format!(
            "This PDF has {page_count} pages. Choose pages within that range."
        )));
    }
    Ok(())
}

pub(super) async fn pdf_page_count(
    input: &Path,
    cancel_token: CancellationToken,
) -> Result<u32, ConversionError> {
    let arguments = vec![input.as_os_str().to_owned()];
    let stdout = run_pdf_tool(
        "pdfinfo",
        &arguments,
        "PDF inspection",
        cancel_token,
        Duration::from_secs(30),
    )
    .await?;
    stdout
        .lines()
        .find_map(|line| {
            line.strip_prefix("Pages:")
                .and_then(|value| value.trim().parse::<u32>().ok())
        })
        .filter(|pages| *pages > 0)
        .ok_or_else(|| ConversionError::ProcessFailed {
            message: "Could not read the PDF page count".into(),
            stderr: stdout,
            exit_code: None,
        })
}

async fn split_every_page(
    input: &Path,
    page_count: u32,
    output_options: Option<OutputOptions>,
    app: &AppHandle,
    job_id: &str,
    cancel_token: CancellationToken,
) -> Result<ConversionResult, ConversionError> {
    if page_count > MAX_SPLIT_PAGES {
        return Err(invalid_page_selection(&format!(
            "This PDF has {page_count} pages; the split limit is {MAX_SPLIT_PAGES}."
        )));
    }

    let _ = app.emit(
        "conversion-progress",
        ProgressPayload {
            job_id: job_id.to_owned(),
            percent: -1,
            stage: format!("Splitting {page_count} pages"),
        },
    );
    let temp = tempfile::tempdir().map_err(|error| ConversionError::ProcessFailed {
        message: format!("Could not create temporary PDF workspace: {error}"),
        stderr: String::new(),
        exit_code: None,
    })?;
    let pattern = temp.path().join("page-%d.pdf");
    let arguments = vec![
        OsString::from("-f"),
        OsString::from("1"),
        OsString::from("-l"),
        OsString::from(page_count.to_string()),
        input.as_os_str().to_owned(),
        pattern.as_os_str().to_owned(),
    ];
    run_pdf_tool(
        "pdfseparate",
        &arguments,
        "PDF split",
        cancel_token.clone(),
        SPLIT_TIMEOUT,
    )
    .await?;

    let width = page_count.to_string().len().max(3);
    let base_options = output_options.unwrap_or_else(|| OutputOptions {
        suffix: "-pages".into(),
        ..OutputOptions::default()
    });
    let mut prepared = Vec::with_capacity(page_count as usize);
    for page in 1..=page_count {
        if cancel_token.is_cancelled() {
            return Err(ConversionError::Cancelled);
        }
        let mut page_options = base_options.clone();
        page_options.suffix = format!("{}-page-{page:0width$}", base_options.suffix);
        let output = prepare_output(input, "pdf", "", Some(page_options))?;
        let source = temp.path().join(format!("page-{page}.pdf"));
        std::fs::copy(&source, output.working_path()).map_err(|error| {
            ConversionError::ProcessFailed {
                message: format!("Could not prepare split page {page}: {error}"),
                stderr: String::new(),
                exit_code: None,
            }
        })?;
        prepared.push(output);
    }

    commit_split_outputs(prepared)
}

async fn extract_pages(
    input: &Path,
    pages: &[u32],
    output_options: Option<OutputOptions>,
    app: &AppHandle,
    job_id: &str,
    cancel_token: CancellationToken,
) -> Result<ConversionResult, ConversionError> {
    let _ = app.emit(
        "conversion-progress",
        ProgressPayload {
            job_id: job_id.to_owned(),
            percent: -1,
            stage: format!("Extracting {} pages", pages.len()),
        },
    );
    let temp = tempfile::tempdir().map_err(|error| ConversionError::ProcessFailed {
        message: format!("Could not create temporary PDF workspace: {error}"),
        stderr: String::new(),
        exit_code: None,
    })?;
    let mut extracted = Vec::with_capacity(pages.len());
    for (index, (start, end)) in contiguous_ranges(pages).into_iter().enumerate() {
        let pattern = temp.path().join(format!("segment-{index}-%d.pdf"));
        let arguments = vec![
            OsString::from("-f"),
            OsString::from(start.to_string()),
            OsString::from("-l"),
            OsString::from(end.to_string()),
            input.as_os_str().to_owned(),
            pattern.as_os_str().to_owned(),
        ];
        run_pdf_tool(
            "pdfseparate",
            &arguments,
            "PDF page extraction",
            cancel_token.clone(),
            SPLIT_TIMEOUT,
        )
        .await?;
        for page in start..=end {
            extracted.push(temp.path().join(format!("segment-{index}-{page}.pdf")));
        }
    }

    let output = prepare_output(input, "pdf", "-pages", output_options)?;
    if extracted.len() == 1 {
        std::fs::copy(&extracted[0], output.working_path()).map_err(|error| {
            ConversionError::ProcessFailed {
                message: format!("Could not save the extracted PDF page: {error}"),
                stderr: String::new(),
                exit_code: None,
            }
        })?;
    } else {
        let mut arguments = extracted
            .iter()
            .map(|path| path.as_os_str().to_owned())
            .collect::<Vec<_>>();
        arguments.push(output.working_path().as_os_str().to_owned());
        run_pdf_tool(
            "pdfunite",
            &arguments,
            "PDF page extraction",
            cancel_token,
            SPLIT_TIMEOUT,
        )
        .await?;
    }

    let result = output.commit(ConversionResult {
        output_path: String::new(),
        output_paths: Vec::new(),
        output_size: 0,
        duration_ms: 0,
        undo_manifest: None,
    })?;
    let output_path = result.output_path.clone();
    Ok(ConversionResult {
        output_path,
        output_paths: vec![result.output_path],
        output_size: result.output_size,
        duration_ms: 0,
        undo_manifest: None,
    })
}

fn contiguous_ranges(pages: &[u32]) -> Vec<(u32, u32)> {
    let mut ranges = Vec::new();
    let Some((&first, rest)) = pages.split_first() else {
        return ranges;
    };
    let mut start = first;
    let mut end = first;
    for &page in rest {
        if page == end + 1 {
            end = page;
        } else {
            ranges.push((start, end));
            start = page;
            end = page;
        }
    }
    ranges.push((start, end));
    ranges
}

fn commit_split_outputs(
    prepared: Vec<super::output::PreparedOutput>,
) -> Result<ConversionResult, ConversionError> {
    let mut output_paths = Vec::with_capacity(prepared.len());
    let mut output_size = 0u64;
    for output in prepared {
        match output.commit(ConversionResult {
            output_path: String::new(),
            output_paths: Vec::new(),
            output_size: 0,
            duration_ms: 0,
            undo_manifest: None,
        }) {
            Ok(result) => {
                output_size += result.output_size;
                output_paths.push(result.output_path);
            }
            Err(error) => {
                for path in &output_paths {
                    let _ = std::fs::remove_file(path);
                }
                return Err(error);
            }
        }
    }
    let output_path = output_paths
        .first()
        .cloned()
        .ok_or(ConversionError::OutputMissing)?;
    Ok(ConversionResult {
        output_path,
        output_paths,
        output_size,
        duration_ms: 0,
        undo_manifest: None,
    })
}

pub(super) async fn run_pdf_tool(
    tool: &str,
    arguments: &[OsString],
    label: &str,
    cancel_token: CancellationToken,
    timeout: Duration,
) -> Result<String, ConversionError> {
    let mut command = tool_command(tool);
    command
        .args(arguments)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = command
        .spawn()
        .map_err(|error| ConversionError::ProcessFailed {
            message: format!("Failed to start {label}: {error}"),
            stderr: String::new(),
            exit_code: None,
        })?;
    let stdout_task = capture_output(child.stdout.take());
    let stderr_task = capture_output(child.stderr.take());

    enum ProcessExit {
        Finished(std::process::ExitStatus),
        Cancelled,
        TimedOut,
    }
    let exit = tokio::select! {
        result = child.wait() => ProcessExit::Finished(result.map_err(|error| {
            ConversionError::ProcessFailed {
                message: format!("{label} process failed: {error}"),
                stderr: String::new(),
                exit_code: None,
            }
        })?),
        _ = cancel_token.cancelled() => ProcessExit::Cancelled,
        _ = tokio::time::sleep(timeout) => ProcessExit::TimedOut,
    };

    match exit {
        ProcessExit::Cancelled => {
            let _ = child.kill().await;
            let _ = finish_output(stdout_task).await;
            let _ = finish_output(stderr_task).await;
            Err(ConversionError::Cancelled)
        }
        ProcessExit::TimedOut => {
            let _ = child.kill().await;
            let _ = finish_output(stdout_task).await;
            let _ = finish_output(stderr_task).await;
            Err(ConversionError::Timeout {
                seconds: timeout.as_secs(),
            })
        }
        ProcessExit::Finished(status) => {
            let stdout = finish_output(stdout_task).await;
            let stderr = finish_output(stderr_task).await;
            if status.success() {
                Ok(stdout)
            } else {
                Err(ConversionError::ProcessFailed {
                    message: format!("{label} failed"),
                    stderr,
                    exit_code: status.code(),
                })
            }
        }
    }
}

fn validate_combine_inputs(input_paths: &[String]) -> Result<Vec<PathBuf>, ConversionError> {
    if !(2..=MAX_MERGE_INPUTS).contains(&input_paths.len()) {
        return Err(ConversionError::UnsupportedConversion {
            input: format!("Combine to PDF requires 2 to {MAX_MERGE_INPUTS} files"),
            output: "combined PDF".into(),
        });
    }

    let mut seen = HashSet::with_capacity(input_paths.len());
    let mut inputs = Vec::with_capacity(input_paths.len());
    for input_path in input_paths {
        let path = PathBuf::from(input_path);
        if !path.is_file() {
            return Err(ConversionError::InputNotFound {
                path: input_path.clone(),
            });
        }
        let format = path
            .extension()
            .and_then(|extension| extension.to_str())
            .and_then(Format::from_extension);
        if !matches!(format, Some(Format::Pdf | Format::Png | Format::Jpg)) {
            return Err(ConversionError::UnsupportedConversion {
                input: input_path.clone(),
                output: "combined PDF from PDF, PNG, or JPEG files".into(),
            });
        }

        let normalized = path.canonicalize().unwrap_or_else(|_| path.clone());
        if !seen.insert(normalized) {
            return Err(ConversionError::UnsupportedConversion {
                input: "Duplicate input".into(),
                output: "combined PDF".into(),
            });
        }
        inputs.push(path);
    }
    Ok(inputs)
}

fn is_combine_image(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|extension| extension.to_str())
            .and_then(Format::from_extension),
        Some(Format::Png | Format::Jpg)
    )
}

async fn prepare_combine_inputs(
    inputs: &[PathBuf],
    temporary_directory: &Path,
    app: &AppHandle,
    job_id: &str,
    cancel_token: CancellationToken,
) -> Result<(Vec<PathBuf>, u32), ConversionError> {
    let mut prepared = Vec::with_capacity(inputs.len());
    let mut expected_pages = 0_u32;

    for (index, input) in inputs.iter().enumerate() {
        let _ = app.emit(
            "conversion-progress",
            ProgressPayload {
                job_id: job_id.to_owned(),
                percent: ((index * 50) / inputs.len()) as i32,
                stage: format!("Preparing {} of {}", index + 1, inputs.len()),
            },
        );

        let pages = if is_combine_image(input) {
            let output = temporary_directory.join(format!("image-{index}.pdf"));
            engines::image_pdf::create_single_page_pdf(input, &output, cancel_token.clone())
                .await?;
            prepared.push(output);
            1
        } else {
            let pages = pdf_page_count(input, cancel_token.clone()).await?;
            prepared.push(input.clone());
            pages
        };
        expected_pages =
            expected_pages
                .checked_add(pages)
                .ok_or_else(|| ConversionError::ProcessFailed {
                    message: "The combined PDF would contain too many pages".into(),
                    stderr: String::new(),
                    exit_code: None,
                })?;
    }

    Ok((prepared, expected_pages))
}

async fn run_pdf_merge(
    inputs: &[PathBuf],
    output: &Path,
    expected_pages: u32,
    cancel_token: CancellationToken,
) -> Result<ConversionResult, ConversionError> {
    let mut command = tool_command("pdfunite");
    command
        .args(pdfunite_arguments(inputs, output))
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let mut child = command
        .spawn()
        .map_err(|error| ConversionError::ProcessFailed {
            message: format!("Failed to start PDF merge: {error}"),
            stderr: String::new(),
            exit_code: None,
        })?;
    let stderr_task = capture_output(child.stderr.take());

    enum ProcessExit {
        Finished(std::process::ExitStatus),
        Cancelled,
        TimedOut,
    }

    let exit = tokio::select! {
        result = child.wait() => ProcessExit::Finished(result.map_err(|error| {
            ConversionError::ProcessFailed {
                message: format!("PDF merge process failed: {error}"),
                stderr: String::new(),
                exit_code: None,
            }
        })?),
        _ = cancel_token.cancelled() => ProcessExit::Cancelled,
        _ = tokio::time::sleep(MERGE_TIMEOUT) => ProcessExit::TimedOut,
    };

    match exit {
        ProcessExit::Cancelled => {
            let _ = child.kill().await;
            let _ = finish_output(stderr_task).await;
            engines::cleanup_partial(output);
            return Err(ConversionError::Cancelled);
        }
        ProcessExit::TimedOut => {
            let _ = child.kill().await;
            let _ = finish_output(stderr_task).await;
            engines::cleanup_partial(output);
            return Err(ConversionError::Timeout {
                seconds: MERGE_TIMEOUT.as_secs(),
            });
        }
        ProcessExit::Finished(status) => {
            let stderr = finish_output(stderr_task).await;
            if !status.success() {
                engines::cleanup_partial(output);
                return Err(ConversionError::ProcessFailed {
                    message: "PDF merge failed".into(),
                    stderr,
                    exit_code: status.code(),
                });
            }
        }
    }

    let output_size = engines::verification::file_with_signature(output, b"%PDF-")?;
    let actual_pages = match pdf_page_count(output, cancel_token).await {
        Ok(pages) => pages,
        Err(error) => {
            engines::cleanup_partial(output);
            return Err(error);
        }
    };
    if actual_pages != expected_pages {
        engines::cleanup_partial(output);
        return Err(ConversionError::ProcessFailed {
            message: format!(
                "Combined PDF verification failed: expected {expected_pages} pages, found {actual_pages}"
            ),
            stderr: String::new(),
            exit_code: None,
        });
    }

    Ok(ConversionResult {
        output_path: output.to_string_lossy().into(),
        output_paths: Vec::new(),
        output_size,
        duration_ms: 0,
        undo_manifest: None,
    })
}

fn pdfunite_arguments(inputs: &[PathBuf], output: &Path) -> Vec<PathBuf> {
    inputs
        .iter()
        .cloned()
        .chain(std::iter::once(output.to_path_buf()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{dictionary, Document, Object, Stream};

    fn write_pdf_fixture(path: &Path, media_boxes: &[[i64; 4]]) {
        let mut document = Document::with_version("1.7");
        let pages_id = document.new_object_id();
        let mut page_ids = Vec::with_capacity(media_boxes.len());
        for media_box in media_boxes {
            let content_id = document.add_object(Stream::new(dictionary! {}, Vec::new()));
            let page_id = document.add_object(dictionary! {
                "Type" => "Page",
                "Parent" => Object::Reference(pages_id),
                "MediaBox" => media_box.iter().copied().map(Object::from).collect::<Vec<_>>(),
                "Resources" => dictionary! {},
                "Contents" => Object::Reference(content_id),
            });
            page_ids.push(page_id);
        }
        document.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => page_ids.iter().copied().map(Object::Reference).collect::<Vec<_>>(),
                "Count" => page_ids.len() as i64,
            }),
        );
        let catalog_id = document.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => Object::Reference(pages_id),
        });
        document.trailer.set("Root", Object::Reference(catalog_id));
        document.compress();
        document.save(path).expect("save PDF fixture");
    }

    #[test]
    fn validates_combine_count_types_and_duplicates() {
        let directory = tempfile::tempdir().expect("tempdir");
        let first = directory.path().join("first.pdf");
        let second = directory.path().join("second.pdf");
        let image = directory.path().join("image.png");
        let jpeg = directory.path().join("photo.jpeg");
        let unsupported = directory.path().join("notes.txt");
        std::fs::write(&first, b"%PDF-1.4").expect("first PDF");
        std::fs::write(&second, b"%PDF-1.4").expect("second PDF");
        std::fs::write(&image, b"fixture").expect("image");
        std::fs::write(&jpeg, b"fixture").expect("JPEG");
        std::fs::write(&unsupported, b"fixture").expect("text");

        assert!(validate_combine_inputs(&[
            first.to_string_lossy().into(),
            second.to_string_lossy().into(),
        ])
        .is_ok());
        assert!(validate_combine_inputs(&[first.to_string_lossy().into()]).is_err());
        assert!(validate_combine_inputs(&[
            first.to_string_lossy().into(),
            first.to_string_lossy().into(),
        ])
        .is_err());
        assert!(validate_combine_inputs(&[
            first.to_string_lossy().into(),
            image.to_string_lossy().into(),
        ])
        .is_ok());
        assert!(validate_combine_inputs(&[
            image.to_string_lossy().into(),
            jpeg.to_string_lossy().into(),
        ])
        .is_ok());
        assert!(validate_combine_inputs(&[
            first.to_string_lossy().into(),
            unsupported.to_string_lossy().into(),
        ])
        .is_err());
    }

    #[test]
    fn places_output_after_all_inputs() {
        let inputs = vec![PathBuf::from("a.pdf"), PathBuf::from("b.pdf")];
        assert_eq!(
            pdfunite_arguments(&inputs, Path::new("merged.pdf")),
            ["a.pdf", "b.pdf", "merged.pdf"]
                .into_iter()
                .map(PathBuf::from)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn split_rollback_only_removes_transaction_owned_outputs() {
        let directory = tempfile::tempdir().expect("tempdir");
        let input = directory.path().join("source.pdf");
        std::fs::write(&input, b"%PDF-source").expect("source PDF");
        let options = |suffix: &str| OutputOptions {
            directory: Some(directory.path().to_string_lossy().into_owned()),
            suffix: suffix.into(),
        };

        let first = prepare_output(&input, "pdf", "", Some(options("-pages-page-001")))
            .expect("first output");
        let second = prepare_output(&input, "pdf", "", Some(options("-pages-page-002")))
            .expect("second output");
        let raced = directory.path().join("source-pages-page-001.pdf");
        let transaction_output = directory.path().join("source-pages-page-001 (1).pdf");
        std::fs::write(&raced, b"other process").expect("racing output");
        std::fs::write(first.working_path(), b"%PDF-page-one").expect("first working page");
        // Leave the second working path missing to force rollback after the
        // first output has committed to its atomically selected numbered path.

        let result = commit_split_outputs(vec![first, second]);

        assert!(matches!(result, Err(ConversionError::OutputMissing)));
        assert_eq!(
            std::fs::read(&raced).expect("racing output survives"),
            b"other process"
        );
        assert!(!transaction_output.exists());
    }

    #[test]
    fn parses_page_lists_and_ranges_in_order() {
        assert_eq!(
            parse_page_selection("1-3, 7, 9-10").expect("selection"),
            vec![1, 2, 3, 7, 9, 10]
        );
        assert_eq!(
            contiguous_ranges(&[1, 2, 3, 7, 9, 10]),
            vec![(1, 3), (7, 7), (9, 10)]
        );
    }

    #[test]
    fn rejects_invalid_or_duplicate_page_selections() {
        for selection in ["", "0", "3-1", "1,,2", "1-2-3", "1, 1", "1-3, 3"] {
            assert!(
                parse_page_selection(selection).is_err(),
                "accepted {selection}"
            );
        }
    }

    #[test]
    fn validates_page_bounds() {
        assert!(validate_page_bounds(&[1, 3], 3).is_ok());
        assert!(validate_page_bounds(&[1, 4], 3).is_err());
    }

    #[test]
    fn maps_compression_presets_to_ghostscript_settings() {
        assert_eq!(
            PdfCompressionPreset::High.ghostscript_setting(),
            "/prepress"
        );
        assert_eq!(
            PdfCompressionPreset::Balanced.ghostscript_setting(),
            "/ebook"
        );
        assert_eq!(
            PdfCompressionPreset::Smallest.ghostscript_setting(),
            "/screen"
        );
    }

    #[test]
    fn builds_safe_ghostscript_compression_arguments() {
        let arguments = ghostscript_arguments(
            Path::new("source file.pdf"),
            Path::new("output file.pdf"),
            PdfCompressionPreset::Balanced,
        );
        let arguments = arguments
            .iter()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect::<Vec<_>>();

        assert!(arguments.contains(&"-dSAFER".into()));
        assert!(arguments.contains(&"-dAutoRotatePages=/None".into()));
        assert!(arguments.contains(&"-dPDFSETTINGS=/ebook".into()));
        assert!(arguments.contains(&"-sOutputFile=output file.pdf".into()));
        assert_eq!(
            arguments.last().map(String::as_str),
            Some("source file.pdf")
        );
    }

    #[test]
    fn validates_whole_megabyte_targets_below_the_source() {
        let directory = tempfile::tempdir().expect("tempdir");
        let input = directory.path().join("source.pdf");
        std::fs::write(&input, vec![0_u8; 3 * 1024 * 1024]).expect("fixture");

        assert_eq!(
            validate_pdf_compression_settings(
                &input,
                PdfCompressionGoal::Quality,
                Some(PDF_TARGET_SIZE_MIN_BYTES),
            )
            .expect("quality ignores the target"),
            None
        );
        assert_eq!(
            validate_pdf_compression_settings(
                &input,
                PdfCompressionGoal::FileSize,
                Some(2 * 1024 * 1024),
            )
            .expect("valid target"),
            Some(2 * 1024 * 1024)
        );
        for target in [
            None,
            Some(PDF_TARGET_SIZE_MIN_BYTES - 1),
            Some(PDF_TARGET_SIZE_MIN_BYTES + 1),
            Some(3 * 1024 * 1024),
            Some(PDF_TARGET_SIZE_MAX_BYTES + 1024 * 1024),
        ] {
            assert!(validate_pdf_compression_settings(
                &input,
                PdfCompressionGoal::FileSize,
                target,
            )
            .is_err());
        }
    }

    #[test]
    fn builds_deterministic_target_size_arguments_after_the_base_preset() {
        let arguments = target_ghostscript_arguments(
            Path::new("source.pdf"),
            Path::new("output.pdf"),
            PDF_TARGET_CANDIDATES[3],
        )
        .into_iter()
        .map(|argument| argument.to_string_lossy().into_owned())
        .collect::<Vec<_>>();

        let preset = arguments
            .iter()
            .position(|argument| argument == "-dPDFSETTINGS=/ebook")
            .expect("base preset");
        let resolution = arguments
            .iter()
            .position(|argument| argument == "-dColorImageResolution=170")
            .expect("resolution override");
        assert!(resolution > preset);
        assert!(arguments.contains(&"-dJPEGQ=80".into()));
        assert_eq!(
            &arguments[arguments.len() - 2..],
            ["-sOutputFile=output.pdf", "source.pdf"]
        );
    }

    #[test]
    fn parses_and_compares_effective_page_boxes() {
        let info = "Pages: 2\n\
Page    1 rot:   0\n\
Page    2 rot:   90\n\
Page    1 MediaBox: 0.00 0.00 612.00 792.00\n\
Page    1 CropBox: 10.00 20.00 600.00 780.00\n\
Page    1 BleedBox: 10.00 20.00 600.00 780.00\n\
Page    1 TrimBox: 10.00 20.00 600.00 780.00\n\
Page    1 ArtBox: 10.00 20.00 600.00 780.00\n\
Page    2 MediaBox: 0.00 0.00 500.00 700.00\n\
Page    2 CropBox: 0.00 0.00 500.00 700.00\n\
Page    2 BleedBox: 0.00 0.00 500.00 700.00\n\
Page    2 TrimBox: 0.00 0.00 500.00 700.00\n\
Page    2 ArtBox: 0.00 0.00 500.00 700.00\n";
        let source = parse_pdf_topology(info, 2).expect("topology");
        let same = parse_pdf_topology(&info.replace("612.00", "612.005"), 2).expect("topology");
        let changed = parse_pdf_topology(&info.replace("612.00", "611.00"), 2).expect("topology");

        assert!(pdf_topologies_match(&source, &same));
        assert!(!pdf_topologies_match(&source, &changed));
        assert!(parse_pdf_topology("Page 1 rot: 0", 1).is_err());
        assert!(parse_pdf_topology(&info.replace("612.00", "NaN"), 2).is_err());
        assert!(parse_pdf_topology(
            &info.replace("Page    1 rot:   0", "Page    1 rot:   45"),
            2
        )
        .is_err());
    }

    #[tokio::test]
    async fn rejects_corrupt_pdf_compression_output_without_touching_source() {
        if engines::resolve_tool("pdfinfo").is_none() {
            return;
        }
        let directory = tempfile::tempdir().expect("tempdir");
        let source = directory.path().join("source.pdf");
        let output = directory.path().join("compressed.pdf");
        write_pdf_fixture(&source, &[[0, 0, 612, 792]]);
        let source_bytes = std::fs::read(&source).expect("source bytes");
        std::fs::write(&output, b"%PDF-1.7\nnot a valid PDF body").expect("corrupt output");
        let source_topology = inspect_pdf_topology(&source, CancellationToken::new())
            .await
            .expect("source topology");

        let result =
            verify_compressed_pdf_output(&output, &source_topology, CancellationToken::new()).await;

        assert!(result.is_err());
        assert!(!output.exists());
        assert_eq!(
            std::fs::read(&source).expect("source survives"),
            source_bytes
        );
    }

    #[tokio::test]
    async fn rejects_changed_pdf_page_count_and_geometry() {
        if engines::resolve_tool("pdfinfo").is_none() {
            return;
        }
        let directory = tempfile::tempdir().expect("tempdir");
        let source = directory.path().join("source.pdf");
        let output = directory.path().join("compressed.pdf");
        write_pdf_fixture(&source, &[[0, 0, 612, 792]]);
        let source_bytes = std::fs::read(&source).expect("source bytes");
        let source_topology = inspect_pdf_topology(&source, CancellationToken::new())
            .await
            .expect("source topology");

        for boxes in [
            vec![[0, 0, 600, 792]],
            vec![[0, 0, 612, 792], [0, 0, 612, 792]],
        ] {
            write_pdf_fixture(&output, &boxes);
            let result =
                verify_compressed_pdf_output(&output, &source_topology, CancellationToken::new())
                    .await;
            assert!(result.is_err(), "accepted changed topology: {boxes:?}");
            assert!(!output.exists());
        }
        assert_eq!(
            std::fs::read(&source).expect("source survives"),
            source_bytes
        );
    }

    #[tokio::test]
    async fn accepts_matching_pdf_compression_output() {
        if engines::resolve_tool("pdfinfo").is_none() {
            return;
        }
        let directory = tempfile::tempdir().expect("tempdir");
        let source = directory.path().join("source.pdf");
        let output = directory.path().join("compressed.pdf");
        write_pdf_fixture(&source, &[[0, 0, 612, 792], [10, 20, 400, 500]]);
        write_pdf_fixture(&output, &[[0, 0, 612, 792], [10, 20, 400, 500]]);
        let source_topology = inspect_pdf_topology(&source, CancellationToken::new())
            .await
            .expect("source topology");

        let size =
            verify_compressed_pdf_output(&output, &source_topology, CancellationToken::new())
                .await
                .expect("matching topology");

        assert_eq!(size, std::fs::metadata(&output).expect("output").len());
    }

    #[test]
    fn builds_page_image_arguments_with_explicit_page_and_resolution() {
        let arguments = page_image_arguments(
            Path::new("source file.pdf"),
            Path::new("page output"),
            7,
            PdfPageImageFormat::Jpeg,
            PdfPageImageResolution::Print,
        )
        .into_iter()
        .map(|argument| argument.to_string_lossy().into_owned())
        .collect::<Vec<_>>();

        assert!(arguments.windows(2).any(|pair| pair == ["-f", "7"]));
        assert!(arguments.windows(2).any(|pair| pair == ["-l", "7"]));
        assert!(arguments.windows(2).any(|pair| pair == ["-r", "300"]));
        assert!(arguments.contains(&"-singlefile".into()));
        assert!(arguments.contains(&"-cropbox".into()));
        assert!(arguments.contains(&"-jpeg".into()));
        assert!(arguments.contains(&"quality=90".into()));
        assert_eq!(
            &arguments[arguments.len() - 2..],
            ["source file.pdf", "page output"]
        );
    }

    #[test]
    fn verifies_png_and_jpeg_signatures() {
        let directory = tempfile::tempdir().expect("tempdir");
        let png = directory.path().join("page.png");
        let jpeg = directory.path().join("page.jpg");
        let invalid = directory.path().join("invalid.png");
        std::fs::write(&png, b"\x89PNG\r\n\x1a\nfixture").expect("PNG fixture");
        std::fs::write(&jpeg, b"\xff\xd8\xfffixture").expect("JPEG fixture");
        std::fs::write(&invalid, b"not an image").expect("invalid fixture");

        assert!(verify_page_image(&png, PdfPageImageFormat::Png).is_ok());
        assert!(verify_page_image(&jpeg, PdfPageImageFormat::Jpeg).is_ok());
        assert!(verify_page_image(&invalid, PdfPageImageFormat::Png).is_err());
        assert!(!invalid.exists());
    }
}
