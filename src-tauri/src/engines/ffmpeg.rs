use std::path::Path;

use log::{debug, info};
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio_util::sync::CancellationToken;

use crate::engines::process::{capture_output, finish_output, run_process, ProcessMessages};
use crate::engines::{
    resolve_tool, tool_command, ConversionEngine, ConversionRequest, ConversionResult,
};
use crate::error::ConversionError;
use crate::formats::{FileCategory, Format};
use crate::progress::{parse_ffmpeg_progress, ProgressPayload};

pub struct FfmpegEngine;

const FFMPEG_CONVERSION_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(4 * 60 * 60);

impl ConversionEngine for FfmpegEngine {
    fn required_tool(&self) -> &'static str {
        "ffmpeg"
    }

    fn supports(&self, input: Format, output: Format) -> bool {
        let ic = input.category();
        let oc = output.category();

        let video_to_video = ic == FileCategory::Video && oc == FileCategory::Video;
        let audio_to_audio = ic == FileCategory::Audio && oc == FileCategory::Audio;
        let video_to_audio = ic == FileCategory::Video && oc == FileCategory::Audio;
        let video_to_gif = ic == FileCategory::Video && output == Format::Gif;

        (video_to_video || audio_to_audio || video_to_audio || video_to_gif) && input != output
    }

    async fn convert(
        &self,
        request: ConversionRequest,
        app: AppHandle,
        cancel_token: CancellationToken,
    ) -> Result<ConversionResult, ConversionError> {
        let input = &request.input_path;
        let output = &request.output_path;

        // Special path for GIF (two-pass palette optimisation).
        if request.output_format == Format::Gif {
            return convert_to_gif(input, output, &request.job_id, &app, cancel_token).await;
        }

        // Probe input duration for progress reporting.
        let duration_ms = probe_duration_ms(input).await.unwrap_or(0);

        // Check if we can do a codec-copy (container swap, instant).
        let codec_copy = can_copy_streams(input, request.output_format).await;

        let mut args: Vec<String> = vec!["-i".into(), input.to_string_lossy().into()];

        if codec_copy {
            let _ = app.emit(
                "conversion-progress",
                ProgressPayload {
                    job_id: request.job_id.clone(),
                    percent: -1,
                    stage: "Remuxing (no re-encode)…".into(),
                },
            );
            args.extend(["-c".into(), "copy".into()]);
        } else if request.output_format.category() == FileCategory::Audio
            || request.input_format.category() == FileCategory::Video
                && request.output_format.category() == FileCategory::Audio
        {
            // Audio-only output.
            let _ = app.emit(
                "conversion-progress",
                ProgressPayload {
                    job_id: request.job_id.clone(),
                    percent: if duration_ms > 0 { 0 } else { -1 },
                    stage: "Extracting audio…".into(),
                },
            );
            args.push("-vn".into()); // strip video
            args.extend(audio_codec_args(request.output_format));
        } else {
            // Full video re-encode.
            let _ = app.emit(
                "conversion-progress",
                ProgressPayload {
                    job_id: request.job_id.clone(),
                    percent: if duration_ms > 0 { 0 } else { -1 },
                    stage: "Encoding…".into(),
                },
            );
            args.extend(video_codec_args(request.output_format));
        }

        // Machine-readable progress on stdout.
        args.extend(["-progress".into(), "pipe:1".into()]);
        args.extend(["-y".into(), output.to_string_lossy().into()]);

        debug!("ffmpeg {}", args.join(" "));

        let ffmpeg = resolve_tool("ffmpeg").ok_or_else(|| ConversionError::MissingDependency {
            tool: "ffmpeg".into(),
            install_hint: "Install ffmpeg with Homebrew".into(),
        })?;
        let mut command = tokio::process::Command::new(ffmpeg);
        command
            .args(&args)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);
        let mut child = command
            .spawn()
            .map_err(|e| ConversionError::ProcessFailed {
                message: format!("Failed to spawn ffmpeg: {e}"),
                stderr: String::new(),
                exit_code: None,
            })?;

        // Stream progress from stdout.
        let stdout = child.stdout.take();
        let app2 = app.clone();
        let progress_job_id = request.job_id.clone();
        let progress_handle = tokio::spawn(async move {
            if let Some(stdout) = stdout {
                let reader = BufReader::new(stdout);
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    if let Some(pct) = parse_ffmpeg_progress(&line, duration_ms) {
                        let _ = app2.emit(
                            "conversion-progress",
                            ProgressPayload {
                                job_id: progress_job_id.clone(),
                                percent: pct,
                                stage: "Encoding…".into(),
                            },
                        );
                    }
                }
            }
        });

        let stderr_handle = capture_output(child.stderr.take());

        enum ProcessExit {
            Finished(Result<std::process::ExitStatus, std::io::Error>),
            Cancelled,
            TimedOut,
        }
        let exit = tokio::select! {
            result = child.wait() => ProcessExit::Finished(result),
            _ = cancel_token.cancelled() => ProcessExit::Cancelled,
            _ = tokio::time::sleep(FFMPEG_CONVERSION_TIMEOUT) => ProcessExit::TimedOut,
        };

        match exit {
            ProcessExit::Cancelled => {
                let _ = child.kill().await;
                let _ = progress_handle.await;
                let _ = finish_output(stderr_handle).await;
                super::cleanup_partial(output);
                return Err(ConversionError::Cancelled);
            }
            ProcessExit::TimedOut => {
                let _ = child.kill().await;
                let _ = progress_handle.await;
                let _ = finish_output(stderr_handle).await;
                super::cleanup_partial(output);
                return Err(ConversionError::Timeout {
                    seconds: FFMPEG_CONVERSION_TIMEOUT.as_secs(),
                });
            }
            ProcessExit::Finished(Err(error)) => {
                let _ = child.kill().await;
                let _ = progress_handle.await;
                let stderr = finish_output(stderr_handle).await;
                super::cleanup_partial(output);
                return Err(ConversionError::ProcessFailed {
                    message: format!("FFmpeg process could not be awaited: {error}"),
                    stderr,
                    exit_code: None,
                });
            }
            ProcessExit::Finished(Ok(status)) => {
                let _ = progress_handle.await;
                let stderr = finish_output(stderr_handle).await;
                if !status.success() {
                    super::cleanup_partial(output);
                    return Err(ConversionError::ProcessFailed {
                        message: "FFmpeg conversion failed".into(),
                        stderr,
                        exit_code: status.code(),
                    });
                }
            }
        }

        let output_size = super::verification::nonempty_file(output)?;
        info!("FFmpeg conversion complete: {output_size} bytes");

        Ok(ConversionResult {
            output_path: output.to_string_lossy().into(),
            output_paths: Vec::new(),
            output_size,
            duration_ms: 0,
            undo_manifest: None,
        })
    }
}

// ---------------------------------------------------------------------------
// GIF two-pass conversion
// ---------------------------------------------------------------------------

async fn convert_to_gif(
    input: &Path,
    output: &Path,
    job_id: &str,
    app: &AppHandle,
    cancel_token: CancellationToken,
) -> Result<ConversionResult, ConversionError> {
    let _ = app.emit(
        "conversion-progress",
        ProgressPayload {
            job_id: job_id.to_owned(),
            percent: -1,
            stage: "Generating palette…".into(),
        },
    );

    let tmp = tempfile::tempdir().map_err(|e| ConversionError::ProcessFailed {
        message: format!("Failed to create temp dir: {e}"),
        stderr: String::new(),
        exit_code: None,
    })?;
    let palette = tmp.path().join("palette.png");

    // Pass 1: generate palette.
    run_ffmpeg_simple(
        &[
            "-i",
            &input.to_string_lossy(),
            "-vf",
            "fps=15,scale=480:-1:flags=lanczos,palettegen",
            "-y",
            &palette.to_string_lossy(),
        ],
        &cancel_token,
        &palette,
        "GIF palette generation failed",
    )
    .await?;

    let _ = app.emit(
        "conversion-progress",
        ProgressPayload {
            job_id: job_id.to_owned(),
            percent: 50,
            stage: "Encoding GIF…".into(),
        },
    );

    // Pass 2: encode with palette.
    let filter = "fps=15,scale=480:-1:flags=lanczos[x];[x][1:v]paletteuse";
    run_ffmpeg_simple(
        &[
            "-i",
            &input.to_string_lossy(),
            "-i",
            &palette.to_string_lossy(),
            "-lavfi",
            filter,
            "-y",
            &output.to_string_lossy(),
        ],
        &cancel_token,
        output,
        "GIF encoding failed",
    )
    .await?;

    let output_size = super::verification::nonempty_file(output)?;
    Ok(ConversionResult {
        output_path: output.to_string_lossy().into(),
        output_paths: Vec::new(),
        output_size,
        duration_ms: 0,
        undo_manifest: None,
    })
}

// ---------------------------------------------------------------------------
// ffprobe helpers
// ---------------------------------------------------------------------------

/// Get media duration in milliseconds via ffprobe.
async fn probe_duration_ms(path: &Path) -> Option<u64> {
    let output = tool_command("ffprobe")
        .args([
            "-v",
            "quiet",
            "-show_entries",
            "format=duration",
            "-of",
            "csv=p=0",
        ])
        .arg(path)
        .output()
        .await
        .ok()?;

    let s = String::from_utf8_lossy(&output.stdout);
    let secs: f64 = s.trim().parse().ok()?;
    Some((secs * 1000.0) as u64)
}

/// Check if streams can be copied directly into the target container.
async fn can_copy_streams(path: &Path, target: Format) -> bool {
    let output = match tool_command("ffprobe")
        .args([
            "-v",
            "quiet",
            "-show_entries",
            "stream=codec_name",
            "-of",
            "csv=p=0",
        ])
        .arg(path)
        .output()
        .await
    {
        Ok(o) => o,
        Err(_) => return false,
    };

    let raw = String::from_utf8_lossy(&output.stdout).to_string();
    let codecs: Vec<&str> = raw.trim().lines().collect();

    match target {
        Format::Mp4 => {
            codecs.iter().any(|c| ["h264", "hevc", "av1"].contains(c))
                && codecs.iter().any(|c| ["aac", "mp3"].contains(c))
        }
        Format::Mov => {
            codecs.iter().any(|c| ["h264", "hevc"].contains(c)) && codecs.contains(&"aac")
        }
        Format::Mkv => true, // MKV accepts almost anything
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Codec argument builders
// ---------------------------------------------------------------------------

fn video_codec_args(format: Format) -> Vec<String> {
    match format {
        Format::Mp4 => vec![
            "-c:v".into(),
            "libx264".into(),
            "-crf".into(),
            "23".into(),
            "-preset".into(),
            "medium".into(),
            "-c:a".into(),
            "aac".into(),
        ],
        Format::WebM => vec![
            "-c:v".into(),
            "libvpx-vp9".into(),
            "-crf".into(),
            "31".into(),
            "-b:v".into(),
            "0".into(),
            "-c:a".into(),
            "libopus".into(),
        ],
        Format::Mov => vec![
            "-c:v".into(),
            "libx264".into(),
            "-crf".into(),
            "23".into(),
            "-preset".into(),
            "medium".into(),
            "-c:a".into(),
            "aac".into(),
        ],
        Format::Mkv => vec![
            "-c:v".into(),
            "libx264".into(),
            "-crf".into(),
            "23".into(),
            "-preset".into(),
            "medium".into(),
            "-c:a".into(),
            "aac".into(),
        ],
        Format::Avi => vec![
            "-c:v".into(),
            "libx264".into(),
            "-crf".into(),
            "23".into(),
            "-c:a".into(),
            "mp3".into(),
        ],
        _ => vec![],
    }
}

fn audio_codec_args(format: Format) -> Vec<String> {
    match format {
        Format::Mp3 => vec![
            "-codec:a".into(),
            "libmp3lame".into(),
            "-qscale:a".into(),
            "2".into(),
        ],
        Format::Wav => vec!["-c:a".into(), "pcm_s16le".into()],
        Format::Aac => vec!["-c:a".into(), "aac".into(), "-b:a".into(), "192k".into()],
        Format::Flac => vec!["-c:a".into(), "flac".into()],
        Format::Ogg => vec![
            "-c:a".into(),
            "libvorbis".into(),
            "-qscale:a".into(),
            "5".into(),
        ],
        Format::M4a => vec!["-c:a".into(), "aac".into(), "-b:a".into(), "192k".into()],
        _ => vec![],
    }
}

// ---------------------------------------------------------------------------
// Utilities
// ---------------------------------------------------------------------------

async fn run_ffmpeg_simple(
    args: &[&str],
    cancel_token: &CancellationToken,
    partial_output: &Path,
    failure_message: &'static str,
) -> Result<(), ConversionError> {
    let mut command = tool_command("ffmpeg");
    command.args(args);
    run_process(
        command,
        cancel_token.clone(),
        FFMPEG_CONVERSION_TIMEOUT,
        &[partial_output],
        ProcessMessages {
            start: "Failed to start FFmpeg",
            wait: "FFmpeg process could not be awaited",
            failure: failure_message,
        },
    )
    .await
}
