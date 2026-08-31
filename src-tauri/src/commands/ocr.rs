use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

use serde::Deserialize;
use tauri::{AppHandle, Emitter};
use tokio_util::sync::CancellationToken;

use crate::engines::process::{run_process_with_output, ProcessMessages};
use crate::engines::{self, ConversionResult};
use crate::error::ConversionError;
use crate::progress::ProgressPayload;

use super::output::{prepare_output, OutputOptions};

const MAX_OCR_PDF_PAGES: u32 = 250;
const MAX_TEXT_BYTES_PER_PAGE: usize = 2 * 1024 * 1024;
const MAX_TOTAL_TEXT_BYTES: usize = 32 * 1024 * 1024;
const OCR_PAGE_TIMEOUT: Duration = Duration::from_secs(2 * 60);
const PDF_RENDER_TIMEOUT: Duration = Duration::from_secs(10 * 60);
const SEARCHABLE_PDF_TIMEOUT: Duration = Duration::from_secs(30 * 60);
const MAX_SEARCHABLE_PROGRESS_BYTES: usize = 128 * 1024;
const OCR_MESSAGES: ProcessMessages = ProcessMessages {
    start: "Could not start on-device text recognition",
    wait: "Could not wait for on-device text recognition",
    failure: "On-device text recognition failed",
};

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum OcrOutputFormat {
    #[default]
    Text,
    SearchablePdf,
}

pub(super) fn supports_ocr_input(path: &Path) -> bool {
    matches!(
        extension(path).as_deref(),
        Some("pdf" | "jpg" | "jpeg" | "png" | "gif" | "tif" | "tiff" | "bmp" | "heic" | "heif")
    )
}

pub(super) fn vision_helper_available() -> bool {
    resolve_vision_helper().is_some()
}

pub(super) async fn recognize_text(
    app: AppHandle,
    input_path: String,
    output_format: OcrOutputFormat,
    job_id: Option<String>,
    output_options: Option<OutputOptions>,
) -> Result<ConversionResult, ConversionError> {
    let input = validate_input(&input_path)?;
    let helper = resolve_vision_helper().ok_or_else(|| ConversionError::MissingDependency {
        tool: "convertkit-vision-ocr".into(),
        install_hint: "Reinstall ConvertKit to restore its on-device OCR component".into(),
    })?;
    let is_pdf = extension(&input).as_deref() == Some("pdf");
    if is_pdf && output_format == OcrOutputFormat::Text {
        require_poppler("pdfinfo", "Install Poppler to inspect scanned PDF pages")?;
        require_poppler("pdftoppm", "Install Poppler to render scanned PDF pages")?;
    }

    let extension = match output_format {
        OcrOutputFormat::Text => "txt",
        OcrOutputFormat::SearchablePdf => "pdf",
    };
    let output = prepare_output(&input, extension, "-ocr", output_options)?;
    let active_job = super::jobs::ActiveJobGuard::register(app.clone(), job_id)?;
    let cancel_token = active_job.cancel_token();
    let event_job_id = active_job.job_id().to_owned();
    let started = Instant::now();

    if output_format == OcrOutputFormat::SearchablePdf {
        emit_progress(&app, &event_job_id, -1, "Creating searchable PDF");
        create_searchable_pdf(&helper, &input, output.working_path(), cancel_token.clone()).await?;
        if cancel_token.is_cancelled() {
            return Err(ConversionError::Cancelled);
        }
        engines::verification::file_with_signature(output.working_path(), b"%PDF-")?;
        let result = output.commit(ConversionResult {
            output_path: String::new(),
            output_paths: Vec::new(),
            output_size: 0,
            duration_ms: started.elapsed().as_millis() as u64,
            undo_manifest: None,
        })?;
        emit_progress(&app, &event_job_id, 100, "Complete");
        return Ok(result);
    }

    let text = if is_pdf {
        recognize_pdf(&app, &event_job_id, &input, &helper, cancel_token).await?
    } else {
        emit_progress(&app, &event_job_id, -1, "Recognizing text");
        recognize_image(&helper, &input, cancel_token).await?
    };
    if text.trim().is_empty() {
        return Err(ConversionError::ProcessFailed {
            message: "No text was recognized in this file".into(),
            stderr: String::new(),
            exit_code: None,
        });
    }

    tokio::fs::write(output.working_path(), text.as_bytes())
        .await
        .map_err(|error| ConversionError::ProcessFailed {
            message: format!("Could not write recognized text: {error}"),
            stderr: String::new(),
            exit_code: None,
        })?;
    let result = output.commit(ConversionResult {
        output_path: String::new(),
        output_paths: Vec::new(),
        output_size: 0,
        duration_ms: started.elapsed().as_millis() as u64,
        undo_manifest: None,
    })?;
    emit_progress(&app, &event_job_id, 100, "Complete");
    Ok(result)
}

async fn create_searchable_pdf(
    helper: &Path,
    input: &Path,
    working_output: &Path,
    cancel_token: CancellationToken,
) -> Result<(), ConversionError> {
    let mut command = tokio::process::Command::new(helper);
    command
        .arg("--searchable-pdf")
        .arg(input)
        .arg(working_output)
        .stdin(Stdio::null());
    let output = run_process_with_output(
        command,
        cancel_token,
        SEARCHABLE_PDF_TIMEOUT,
        MAX_SEARCHABLE_PROGRESS_BYTES,
        &[working_output],
        OCR_MESSAGES,
    )
    .await?;
    validate_searchable_progress(&output)
}

fn validate_searchable_progress(output: &[u8]) -> Result<(), ConversionError> {
    let text = std::str::from_utf8(output).map_err(|error| ConversionError::ProcessFailed {
        message: format!("On-device OCR returned invalid progress: {error}"),
        stderr: String::new(),
        exit_code: None,
    })?;
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        let value: serde_json::Value =
            serde_json::from_str(line).map_err(|error| ConversionError::ProcessFailed {
                message: format!("On-device OCR returned invalid progress: {error}"),
                stderr: String::new(),
                exit_code: None,
            })?;
        let page = value.get("page").and_then(serde_json::Value::as_u64);
        let total = value.get("total").and_then(serde_json::Value::as_u64);
        if !matches!((page, total), (Some(page), Some(total)) if page > 0 && total > 0 && page <= total && total <= u64::from(MAX_OCR_PDF_PAGES))
        {
            return Err(ConversionError::ProcessFailed {
                message: "On-device OCR returned invalid page progress".into(),
                stderr: String::new(),
                exit_code: None,
            });
        }
    }
    Ok(())
}

async fn recognize_pdf(
    app: &AppHandle,
    job_id: &str,
    input: &Path,
    helper: &Path,
    cancel_token: CancellationToken,
) -> Result<String, ConversionError> {
    let page_count = super::pdf::pdf_page_count(input, cancel_token.clone()).await?;
    if page_count == 0 || page_count > MAX_OCR_PDF_PAGES {
        return Err(ConversionError::UnsupportedConversion {
            input: input.to_string_lossy().into(),
            output: format!("OCR for PDFs with 1 to {MAX_OCR_PDF_PAGES} pages"),
        });
    }
    let workspace = tempfile::Builder::new()
        .prefix("convertkit-ocr-")
        .tempdir()
        .map_err(|error| ConversionError::ProcessFailed {
            message: format!("Could not create the OCR workspace: {error}"),
            stderr: String::new(),
            exit_code: None,
        })?;
    let mut result = String::new();

    for page in 1..=page_count {
        if cancel_token.is_cancelled() {
            return Err(ConversionError::Cancelled);
        }
        let percent = (((page - 1) * 100) / page_count) as i32;
        emit_progress(
            app,
            job_id,
            percent,
            &format!("Recognizing page {page} of {page_count}"),
        );
        let prefix = workspace.path().join(format!("page-{page}"));
        let rendered = prefix.with_extension("png");
        let arguments = pdf_render_arguments(input, &prefix, page);
        super::pdf::run_pdf_tool(
            "pdftoppm",
            &arguments,
            "OCR page rendering",
            cancel_token.clone(),
            PDF_RENDER_TIMEOUT,
        )
        .await?;
        engines::verification::file_with_signature(&rendered, b"\x89PNG\r\n\x1a\n")?;
        let page_text = recognize_image(helper, &rendered, cancel_token.clone()).await?;
        if !result.is_empty() {
            result.push_str("\n\n");
        }
        result.push_str(page_text.trim());
        result.push('\n');
        if result.len() > MAX_TOTAL_TEXT_BYTES {
            return Err(ConversionError::ProcessFailed {
                message: "Recognized text exceeds the 32 MiB safety limit".into(),
                stderr: String::new(),
                exit_code: None,
            });
        }
        let _ = std::fs::remove_file(rendered);
    }

    Ok(result)
}

async fn recognize_image(
    helper: &Path,
    input: &Path,
    cancel_token: CancellationToken,
) -> Result<String, ConversionError> {
    let mut command = tokio::process::Command::new(helper);
    command.arg(input).stdin(Stdio::null());
    let output = run_process_with_output(
        command,
        cancel_token,
        OCR_PAGE_TIMEOUT,
        MAX_TEXT_BYTES_PER_PAGE,
        &[],
        OCR_MESSAGES,
    )
    .await?;
    String::from_utf8(output).map_err(|error| ConversionError::ProcessFailed {
        message: format!("On-device OCR returned invalid text: {error}"),
        stderr: String::new(),
        exit_code: None,
    })
}

fn validate_input(input_path: &str) -> Result<PathBuf, ConversionError> {
    let input = PathBuf::from(input_path);
    if !input.is_file() {
        return Err(ConversionError::InputNotFound {
            path: input_path.into(),
        });
    }
    if !supports_ocr_input(&input) {
        return Err(ConversionError::UnsupportedConversion {
            input: input_path.into(),
            output: "recognized text from an image or scanned PDF".into(),
        });
    }
    Ok(input)
}

fn extension(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
}

fn require_poppler(tool: &str, install_hint: &str) -> Result<(), ConversionError> {
    if engines::resolve_tool(tool).is_some() {
        Ok(())
    } else {
        Err(ConversionError::MissingDependency {
            tool: tool.into(),
            install_hint: install_hint.into(),
        })
    }
}

fn resolve_vision_helper() -> Option<PathBuf> {
    engines::resolve_tool("convertkit-vision-ocr")
}

fn pdf_render_arguments(input: &Path, prefix: &Path, page: u32) -> Vec<OsString> {
    vec![
        OsString::from("-f"),
        OsString::from(page.to_string()),
        OsString::from("-l"),
        OsString::from(page.to_string()),
        OsString::from("-singlefile"),
        OsString::from("-cropbox"),
        OsString::from("-r"),
        OsString::from("220"),
        OsString::from("-png"),
        input.as_os_str().to_owned(),
        prefix.as_os_str().to_owned(),
    ]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_common_images_and_pdf_only() {
        for path in [
            "scan.pdf",
            "scan.PNG",
            "scan.jpeg",
            "scan.tiff",
            "scan.heic",
            "scan.bmp",
        ] {
            assert!(supports_ocr_input(Path::new(path)), "{path}");
        }
        for path in ["notes.docx", "clip.mp4", "page.svg", "image.webp"] {
            assert!(!supports_ocr_input(Path::new(path)), "{path}");
        }
    }

    #[test]
    fn renders_exactly_one_bounded_pdf_page() {
        let arguments = pdf_render_arguments(
            Path::new("/tmp/source with spaces.pdf"),
            Path::new("/tmp/page output"),
            7,
        );
        assert!(arguments.windows(2).any(|pair| pair == ["-f", "7"]));
        assert!(arguments.windows(2).any(|pair| pair == ["-l", "7"]));
        assert!(arguments.contains(&OsString::from("-singlefile")));
        assert!(arguments.contains(&OsString::from("-png")));
    }

    #[test]
    fn searchable_pdf_progress_is_bounded_and_structured() {
        assert!(validate_searchable_progress(
            b"{\"page\":1,\"total\":2}\n{\"page\":2,\"total\":2}\n"
        )
        .is_ok());
        assert!(validate_searchable_progress(b"{\"page\":3,\"total\":2}\n").is_err());
        assert!(validate_searchable_progress(b"not-json\n").is_err());
    }
}
