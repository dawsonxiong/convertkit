use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter};
use tokio_util::sync::CancellationToken;

use crate::engines::process::{capture_output, finish_output};
use crate::engines::{self, tool_command, ConversionResult};
use crate::error::ConversionError;
use crate::formats::Format;
use crate::progress::ProgressPayload;

use super::output::{prepare_output, OutputOptions};

const EXTRACTION_TIMEOUT: Duration = Duration::from_secs(10 * 60);

pub(super) fn supports_text_extraction(format: Format) -> bool {
    matches!(
        format,
        Format::Pdf | Format::Docx | Format::Html | Format::Md | Format::Epub
    )
}

pub(super) fn required_tool(format: Format) -> Option<(&'static str, &'static str)> {
    match format {
        Format::Pdf => Some(("pdftotext", "Poppler")),
        Format::Docx | Format::Html | Format::Md | Format::Epub => Some(("pandoc", "Pandoc")),
        _ => None,
    }
}

pub(super) async fn extract_text(
    app: AppHandle,
    input_path: String,
    job_id: Option<String>,
    output_options: Option<OutputOptions>,
) -> Result<ConversionResult, ConversionError> {
    let input = validate_input(&input_path)?;
    let format = input_format(&input).ok_or_else(|| unsupported(&input_path))?;
    let (tool, label) = required_tool(format).ok_or_else(|| unsupported(&input_path))?;
    if engines::resolve_tool(tool).is_none() {
        return Err(ConversionError::MissingDependency {
            tool: tool.into(),
            install_hint: format!("Install {label} to extract text from this file"),
        });
    }

    let output = prepare_output(&input, "txt", "-text", output_options)?;
    let active_job = super::jobs::ActiveJobGuard::register(app.clone(), job_id)?;
    let cancel_token = active_job.cancel_token();
    let event_job_id = active_job.job_id().to_owned();
    let started = Instant::now();

    let _ = app.emit(
        "conversion-progress",
        ProgressPayload {
            job_id: event_job_id.clone(),
            percent: -1,
            stage: "Extracting text".into(),
        },
    );

    run_extractor(
        tool,
        extraction_arguments(format, &input, output.working_path()),
        cancel_token,
    )
    .await?;

    let result = output.commit(ConversionResult {
        output_path: String::new(),
        output_paths: Vec::new(),
        output_size: 0,
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

fn validate_input(input_path: &str) -> Result<PathBuf, ConversionError> {
    let input = PathBuf::from(input_path);
    if !input.is_file() {
        return Err(ConversionError::InputNotFound {
            path: input_path.into(),
        });
    }
    if !input_format(&input).is_some_and(supports_text_extraction) {
        return Err(unsupported(input_path));
    }
    Ok(input)
}

fn input_format(path: &Path) -> Option<Format> {
    path.extension()
        .and_then(|extension| extension.to_str())
        .and_then(Format::from_extension)
}

fn unsupported(input_path: &str) -> ConversionError {
    ConversionError::UnsupportedConversion {
        input: input_path.into(),
        output: "plain text from PDF, DOCX, HTML, Markdown, or EPUB".into(),
    }
}

fn extraction_arguments(format: Format, input: &Path, output: &Path) -> Vec<OsString> {
    if format == Format::Pdf {
        return vec![
            OsString::from("-enc"),
            OsString::from("UTF-8"),
            OsString::from("-eol"),
            OsString::from("unix"),
            OsString::from("-nopgbrk"),
            input.as_os_str().to_owned(),
            output.as_os_str().to_owned(),
        ];
    }

    vec![
        input.as_os_str().to_owned(),
        OsString::from("--to=plain"),
        OsString::from("--wrap=none"),
        OsString::from("--output"),
        output.as_os_str().to_owned(),
    ]
}

async fn run_extractor(
    tool: &str,
    arguments: Vec<OsString>,
    cancel_token: CancellationToken,
) -> Result<(), ConversionError> {
    let mut command = tool_command(tool);
    command
        .args(arguments)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = command
        .spawn()
        .map_err(|error| ConversionError::ProcessFailed {
            message: format!("Could not start text extraction: {error}"),
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
                message: format!("Text extraction process failed: {error}"),
                stderr: String::new(),
                exit_code: None,
            }
        })?),
        _ = cancel_token.cancelled() => ProcessExit::Cancelled,
        _ = tokio::time::sleep(EXTRACTION_TIMEOUT) => ProcessExit::TimedOut,
    };

    match exit {
        ProcessExit::Cancelled => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            let _ = finish_output(stdout_task).await;
            let _ = finish_output(stderr_task).await;
            Err(ConversionError::Cancelled)
        }
        ProcessExit::TimedOut => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            let _ = finish_output(stdout_task).await;
            let _ = finish_output(stderr_task).await;
            Err(ConversionError::Timeout {
                seconds: EXTRACTION_TIMEOUT.as_secs(),
            })
        }
        ProcessExit::Finished(status) => {
            let _ = finish_output(stdout_task).await;
            let stderr = finish_output(stderr_task).await;
            if status.success() {
                Ok(())
            } else {
                Err(ConversionError::ProcessFailed {
                    message: "Text extraction failed".into(),
                    stderr,
                    exit_code: status.code(),
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_documents_with_reliable_plain_text_extractors() {
        for format in [
            Format::Pdf,
            Format::Docx,
            Format::Html,
            Format::Md,
            Format::Epub,
        ] {
            assert!(supports_text_extraction(format));
        }
        for format in [Format::Txt, Format::Png, Format::Mp4, Format::Svg] {
            assert!(!supports_text_extraction(format));
        }
    }

    #[test]
    fn routes_pdf_to_poppler_and_documents_to_pandoc() {
        assert_eq!(required_tool(Format::Pdf), Some(("pdftotext", "Poppler")));
        assert_eq!(required_tool(Format::Docx), Some(("pandoc", "Pandoc")));
        assert_eq!(required_tool(Format::Png), None);
    }

    #[test]
    fn builds_non_shell_pdf_and_document_arguments() {
        let input = Path::new("/tmp/source with spaces.pdf");
        let output = Path::new("/tmp/output with spaces.txt");
        let pdf = extraction_arguments(Format::Pdf, input, output);
        assert_eq!(pdf.last(), Some(&output.as_os_str().to_owned()));
        assert!(pdf.contains(&OsString::from("-nopgbrk")));

        let document = extraction_arguments(Format::Docx, input, output);
        assert_eq!(document.first(), Some(&input.as_os_str().to_owned()));
        assert!(document.contains(&OsString::from("--to=plain")));
        assert_eq!(document.last(), Some(&output.as_os_str().to_owned()));
    }

    #[tokio::test]
    async fn cancellation_stops_a_running_extractor() {
        let cancel_token = CancellationToken::new();
        cancel_token.cancel();
        let result = run_extractor("sleep", vec![OsString::from("30")], cancel_token).await;
        assert!(matches!(result, Err(ConversionError::Cancelled)));
    }
}
