use std::path::{Path, PathBuf};
use std::time::Instant;

use tauri::{AppHandle, Emitter, Manager};
use tokio::io::AsyncReadExt;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::engines::{self, tool_command, ConversionResult};
use crate::error::ConversionError;
use crate::formats::FileCategory;
use crate::progress::ProgressPayload;
use crate::ActiveJobs;

use super::output::{prepare_output, OutputOptions};

#[tauri::command]
pub async fn optimize_image(
    app: AppHandle,
    input_path: String,
    keep_metadata: bool,
    job_id: Option<String>,
    output_options: Option<OutputOptions>,
) -> Result<ConversionResult, ConversionError> {
    let input = PathBuf::from(&input_path);
    if !input.is_file() {
        return Err(ConversionError::InputNotFound { path: input_path });
    }

    let format = crate::formats::Format::from_extension(
        input
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or_default(),
    )
    .ok_or_else(|| ConversionError::UnsupportedConversion {
        input: input_path.clone(),
        output: "optimized image".into(),
    })?;

    if format.category() != FileCategory::Image {
        return Err(ConversionError::UnsupportedConversion {
            input: format.label().into(),
            output: "optimized image".into(),
        });
    }

    if engines::resolve_tool("magick").is_none() {
        return Err(ConversionError::MissingDependency {
            tool: "magick".into(),
            install_hint: "Install ImageMagick to optimize images".into(),
        });
    }

    let extension = format.extension();
    let prepared_output = prepare_output(&input, extension, "-optimized", output_options)?;
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

    let _ = app.emit(
        "conversion-progress",
        ProgressPayload {
            percent: -1,
            stage: "Optimizing image".into(),
        },
    );

    let started = Instant::now();
    let result = run_optimization(
        &input,
        prepared_output.working_path(),
        keep_metadata,
        cancel_token,
    )
    .await;
    app.state::<ActiveJobs>()
        .0
        .lock()
        .expect("ActiveJobs lock poisoned")
        .remove(&job_id);

    match result {
        Ok(result) => {
            let mut result = prepared_output.commit(result)?;
            result.duration_ms = started.elapsed().as_millis() as u64;
            let _ = app.emit(
                "conversion-progress",
                ProgressPayload {
                    percent: 100,
                    stage: "Complete".into(),
                },
            );
            Ok(result)
        }
        Err(error) => Err(error),
    }
}

async fn run_optimization(
    input: &Path,
    output: &Path,
    keep_metadata: bool,
    cancel_token: CancellationToken,
) -> Result<ConversionResult, ConversionError> {
    let extension = input
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("png")
        .to_ascii_lowercase();
    let temp = tempfile::tempdir().map_err(|error| ConversionError::ProcessFailed {
        message: format!("Could not create optimization workspace: {error}"),
        stderr: String::new(),
        exit_code: None,
    })?;

    let strategies = optimization_strategies(&extension);
    let mut best: Option<(PathBuf, u64)> = None;
    let mut last_error = None;

    for (index, arguments) in strategies.iter().enumerate() {
        let candidate = temp.path().join(format!("candidate-{index}.{extension}"));
        match run_candidate(
            input,
            &candidate,
            arguments,
            keep_metadata,
            cancel_token.clone(),
        )
        .await
        {
            Ok(()) => {
                let size = std::fs::metadata(&candidate)
                    .map_err(|_| ConversionError::OutputMissing)?
                    .len();
                if best.as_ref().is_none_or(|(_, best_size)| size < *best_size) {
                    best = Some((candidate, size));
                }
            }
            Err(ConversionError::Cancelled) => return Err(ConversionError::Cancelled),
            Err(error) => last_error = Some(error),
        }
    }

    let specialized = temp
        .path()
        .join(format!("candidate-specialized.{extension}"));
    match run_specialized_candidate(input, &specialized, &extension, keep_metadata, cancel_token)
        .await
    {
        Ok(true) => {
            let size = std::fs::metadata(&specialized)
                .map_err(|_| ConversionError::OutputMissing)?
                .len();
            if best.as_ref().is_none_or(|(_, best_size)| size < *best_size) {
                best = Some((specialized, size));
            }
        }
        Ok(false) => {}
        Err(ConversionError::Cancelled) => return Err(ConversionError::Cancelled),
        Err(error) => last_error = Some(error),
    }

    let (best_path, output_size) = match best {
        Some(candidate) => candidate,
        None => {
            return Err(last_error.unwrap_or(ConversionError::OutputMissing));
        }
    };

    std::fs::copy(best_path, output).map_err(|error| ConversionError::ProcessFailed {
        message: format!("Could not save optimized image: {error}"),
        stderr: String::new(),
        exit_code: None,
    })?;

    Ok(ConversionResult {
        output_path: output.to_string_lossy().into(),
        output_size,
        duration_ms: 0,
    })
}

async fn run_specialized_candidate(
    input: &Path,
    output: &Path,
    extension: &str,
    keep_metadata: bool,
    cancel_token: CancellationToken,
) -> Result<bool, ConversionError> {
    let (tool, mut arguments): (&str, Vec<&str>) = match extension {
        "png" if engines::resolve_tool("oxipng").is_some() => {
            ("oxipng", vec!["-o", "4", "--quiet"])
        }
        "jpg" | "jpeg" if engines::resolve_tool("jpegoptim").is_some() => {
            ("jpegoptim", vec!["--quiet", "--auto-mode"])
        }
        "gif" if engines::resolve_tool("gifsicle").is_some() => {
            let mut command = tool_command("gifsicle");
            command.args(["-O3", "--output"]).arg(output).arg(input);
            run_optimizer_process(command, output, cancel_token).await?;
            return Ok(true);
        }
        _ => return Ok(false),
    };

    std::fs::copy(input, output).map_err(|error| ConversionError::ProcessFailed {
        message: format!("Could not prepare specialized optimization: {error}"),
        stderr: String::new(),
        exit_code: None,
    })?;

    match tool {
        "oxipng" if !keep_metadata => arguments.extend(["--strip", "safe"]),
        "jpegoptim" if keep_metadata => arguments.push("--keep-all"),
        "jpegoptim" => arguments.push("--strip-all"),
        _ => {}
    }

    let mut command = tool_command(tool);
    command.args(arguments).arg(output);
    run_optimizer_process(command, output, cancel_token).await?;
    Ok(true)
}

async fn run_candidate(
    input: &Path,
    output: &Path,
    arguments: &[&str],
    keep_metadata: bool,
    cancel_token: CancellationToken,
) -> Result<(), ConversionError> {
    let mut command = tool_command("magick");
    command.arg(input);
    if !keep_metadata {
        command.args(["-auto-orient", "-strip"]);
    }
    command.args(arguments).arg(output);
    run_optimizer_process(command, output, cancel_token).await
}

async fn run_optimizer_process(
    mut command: tokio::process::Command,
    output: &Path,
    cancel_token: CancellationToken,
) -> Result<(), ConversionError> {
    command
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped());

    let mut child = command
        .spawn()
        .map_err(|error| ConversionError::ProcessFailed {
            message: format!("Failed to start image optimization: {error}"),
            stderr: String::new(),
            exit_code: None,
        })?;

    let stderr = child.stderr.take();
    let stderr_task = tokio::spawn(async move {
        let mut output = String::new();
        if let Some(mut stderr) = stderr {
            let _ = stderr.read_to_string(&mut output).await;
        }
        output
    });

    let status = tokio::select! {
        result = child.wait() => result.map_err(|error| ConversionError::ProcessFailed {
            message: format!("Image optimization process failed: {error}"),
            stderr: String::new(),
            exit_code: None,
        })?,
        _ = cancel_token.cancelled() => {
            let _ = child.kill().await;
            engines::cleanup_partial(output);
            return Err(ConversionError::Cancelled);
        }
    };
    let stderr = stderr_task.await.unwrap_or_default();

    if !status.success() {
        engines::cleanup_partial(output);
        return Err(ConversionError::ProcessFailed {
            message: "Image optimization failed".into(),
            stderr,
            exit_code: status.code(),
        });
    }

    Ok(())
}

fn optimization_strategies(extension: &str) -> Vec<Vec<&'static str>> {
    match extension {
        "png" => vec![
            vec![
                "-define",
                "png:compression-level=9",
                "-define",
                "png:compression-strategy=1",
            ],
            vec![
                "-define",
                "png:compression-level=9",
                "-define",
                "png:compression-strategy=2",
            ],
        ],
        "jpg" | "jpeg" => vec![
            vec![
                "-sampling-factor",
                "4:2:0",
                "-interlace",
                "Plane",
                "-quality",
                "92",
            ],
            vec![
                "-sampling-factor",
                "4:2:0",
                "-interlace",
                "None",
                "-quality",
                "92",
            ],
        ],
        "webp" => vec![
            vec!["-quality", "90", "-define", "webp:method=4"],
            vec!["-quality", "90", "-define", "webp:method=6"],
        ],
        "gif" => vec![vec!["-layers", "Optimize"]],
        "tif" | "tiff" => vec![vec!["-compress", "Zip"]],
        "avif" | "heic" | "heif" => vec![vec!["-quality", "90"]],
        _ => vec![vec![]],
    }
}
