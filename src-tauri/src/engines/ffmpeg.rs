use std::path::Path;

use log::{debug, info};
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio_util::sync::CancellationToken;

use crate::engines::{ConversionEngine, ConversionRequest, ConversionResult};
use crate::error::ConversionError;
use crate::formats::{FileCategory, Format};
use crate::progress::{parse_ffmpeg_progress, ProgressPayload};

pub struct FfmpegEngine;

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
            return convert_to_gif(input, output, &app, cancel_token).await;
        }

        // Probe input duration for progress reporting.
        let duration_ms = probe_duration_ms(input).await.unwrap_or(0);

        // Check if we can do a codec-copy (container swap, instant).
        let codec_copy = can_copy_streams(input, request.output_format).await;

        let mut args: Vec<String> = vec![
            "-i".into(),
            input.to_string_lossy().into(),
        ];

        if codec_copy {
            let _ = app.emit("conversion-progress", ProgressPayload {
                percent: -1,
                stage: "Remuxing (no re-encode)…".into(),
            });
            args.extend(["-c".into(), "copy".into()]);
        } else if request.output_format.category() == FileCategory::Audio
            || request.input_format.category() == FileCategory::Video
                && request.output_format.category() == FileCategory::Audio
        {
            // Audio-only output.
            let _ = app.emit("conversion-progress", ProgressPayload {
                percent: if duration_ms > 0 { 0 } else { -1 },
                stage: "Extracting audio…".into(),
            });
            args.push("-vn".into()); // strip video
            args.extend(audio_codec_args(request.output_format));
        } else {
            // Full video re-encode.
            let _ = app.emit("conversion-progress", ProgressPayload {
                percent: if duration_ms > 0 { 0 } else { -1 },
                stage: "Encoding…".into(),
            });
            args.extend(video_codec_args(request.output_format));
        }

        // Machine-readable progress on stdout.
        args.extend(["-progress".into(), "pipe:1".into()]);
        args.extend(["-y".into(), output.to_string_lossy().into()]);

        debug!("ffmpeg {}", args.join(" "));

        let mut child = tokio::process::Command::new("ffmpeg")
            .args(&args)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| ConversionError::ProcessFailed {
                message: format!("Failed to spawn ffmpeg: {e}"),
                stderr: String::new(),
                exit_code: None,
            })?;

        // Stream progress from stdout.
        let stdout = child.stdout.take();
        let app2 = app.clone();
        let progress_handle = tokio::spawn(async move {
            if let Some(stdout) = stdout {
                let reader = BufReader::new(stdout);
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    if let Some(pct) = parse_ffmpeg_progress(&line, duration_ms) {
                        let _ = app2.emit("conversion-progress", ProgressPayload {
                            percent: pct,
                            stage: "Encoding…".into(),
                        });
                    }
                }
            }
        });

        let status = tokio::select! {
            result = child.wait() => {
                result.map_err(|e| ConversionError::ProcessFailed {
                    message: format!("ffmpeg process error: {e}"),
                    stderr: String::new(),
                    exit_code: None,
                })?
            }
            _ = cancel_token.cancelled() => {
                let _ = child.kill().await;
                super::cleanup_partial(output);
                return Err(ConversionError::Cancelled);
            }
        };

        let _ = progress_handle.await;

        if !status.success() {
            let stderr = read_stderr(&mut child).await;
            super::cleanup_partial(output);
            return Err(ConversionError::ProcessFailed {
                message: "FFmpeg conversion failed".into(),
                stderr,
                exit_code: status.code(),
            });
        }

        let meta = std::fs::metadata(output).map_err(|_| ConversionError::OutputMissing)?;
        info!("FFmpeg conversion complete: {} bytes", meta.len());

        Ok(ConversionResult {
            output_path: output.to_string_lossy().into(),
            output_size: meta.len(),
            duration_ms: 0,
        })
    }
}

// ---------------------------------------------------------------------------
// GIF two-pass conversion
// ---------------------------------------------------------------------------

async fn convert_to_gif(
    input: &Path,
    output: &Path,
    app: &AppHandle,
    cancel_token: CancellationToken,
) -> Result<ConversionResult, ConversionError> {
    let _ = app.emit("conversion-progress", ProgressPayload {
        percent: -1,
        stage: "Generating palette…".into(),
    });

    let tmp = tempfile::tempdir().map_err(|e| ConversionError::ProcessFailed {
        message: format!("Failed to create temp dir: {e}"),
        stderr: String::new(),
        exit_code: None,
    })?;
    let palette = tmp.path().join("palette.png");

    // Pass 1: generate palette.
    let status = run_ffmpeg_simple(
        &[
            "-i", &input.to_string_lossy(),
            "-vf", "fps=15,scale=480:-1:flags=lanczos,palettegen",
            "-y", &palette.to_string_lossy(),
        ],
        &cancel_token,
    ).await?;

    if !status.success() {
        return Err(ConversionError::ProcessFailed {
            message: "GIF palette generation failed".into(),
            stderr: String::new(),
            exit_code: status.code(),
        });
    }

    let _ = app.emit("conversion-progress", ProgressPayload {
        percent: 50,
        stage: "Encoding GIF…".into(),
    });

    // Pass 2: encode with palette.
    let filter = format!(
        "fps=15,scale=480:-1:flags=lanczos[x];[x][1:v]paletteuse"
    );
    let status = run_ffmpeg_simple(
        &[
            "-i", &input.to_string_lossy(),
            "-i", &palette.to_string_lossy(),
            "-lavfi", &filter,
            "-y", &output.to_string_lossy(),
        ],
        &cancel_token,
    ).await?;

    if !status.success() {
        super::cleanup_partial(output);
        return Err(ConversionError::ProcessFailed {
            message: "GIF encoding failed".into(),
            stderr: String::new(),
            exit_code: status.code(),
        });
    }

    let meta = std::fs::metadata(output).map_err(|_| ConversionError::OutputMissing)?;
    Ok(ConversionResult {
        output_path: output.to_string_lossy().into(),
        output_size: meta.len(),
        duration_ms: 0,
    })
}

// ---------------------------------------------------------------------------
// ffprobe helpers
// ---------------------------------------------------------------------------

/// Get media duration in milliseconds via ffprobe.
async fn probe_duration_ms(path: &Path) -> Option<u64> {
    let output = tokio::process::Command::new("ffprobe")
        .args([
            "-v", "quiet",
            "-show_entries", "format=duration",
            "-of", "csv=p=0",
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
    let output = match tokio::process::Command::new("ffprobe")
        .args([
            "-v", "quiet",
            "-show_entries", "stream=codec_name",
            "-of", "csv=p=0",
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
            codecs.iter().any(|c| ["h264", "hevc"].contains(c))
                && codecs.iter().any(|c| *c == "aac")
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
            "-c:v".into(), "libx264".into(),
            "-crf".into(), "23".into(),
            "-preset".into(), "medium".into(),
            "-c:a".into(), "aac".into(),
        ],
        Format::WebM => vec![
            "-c:v".into(), "libvpx-vp9".into(),
            "-crf".into(), "31".into(),
            "-b:v".into(), "0".into(),
            "-c:a".into(), "libopus".into(),
        ],
        Format::Mov => vec![
            "-c:v".into(), "libx264".into(),
            "-crf".into(), "23".into(),
            "-preset".into(), "medium".into(),
            "-c:a".into(), "aac".into(),
        ],
        Format::Mkv => vec![
            "-c:v".into(), "libx264".into(),
            "-crf".into(), "23".into(),
            "-preset".into(), "medium".into(),
            "-c:a".into(), "aac".into(),
        ],
        Format::Avi => vec![
            "-c:v".into(), "libx264".into(),
            "-crf".into(), "23".into(),
            "-c:a".into(), "mp3".into(),
        ],
        _ => vec![],
    }
}

fn audio_codec_args(format: Format) -> Vec<String> {
    match format {
        Format::Mp3 => vec![
            "-codec:a".into(), "libmp3lame".into(),
            "-qscale:a".into(), "2".into(),
        ],
        Format::Wav => vec![
            "-c:a".into(), "pcm_s16le".into(),
        ],
        Format::Aac => vec![
            "-c:a".into(), "aac".into(),
            "-b:a".into(), "192k".into(),
        ],
        Format::Flac => vec![
            "-c:a".into(), "flac".into(),
        ],
        Format::Ogg => vec![
            "-c:a".into(), "libvorbis".into(),
            "-qscale:a".into(), "5".into(),
        ],
        Format::M4a => vec![
            "-c:a".into(), "aac".into(),
            "-b:a".into(), "192k".into(),
        ],
        _ => vec![],
    }
}

// ---------------------------------------------------------------------------
// Utilities
// ---------------------------------------------------------------------------

async fn run_ffmpeg_simple(
    args: &[&str],
    cancel_token: &CancellationToken,
) -> Result<std::process::ExitStatus, ConversionError> {
    let mut child = tokio::process::Command::new("ffmpeg")
        .args(args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| ConversionError::ProcessFailed {
            message: format!("Failed to spawn ffmpeg: {e}"),
            stderr: String::new(),
            exit_code: None,
        })?;

    tokio::select! {
        result = child.wait() => {
            result.map_err(|e| ConversionError::ProcessFailed {
                message: format!("ffmpeg error: {e}"),
                stderr: String::new(),
                exit_code: None,
            })
        }
        _ = cancel_token.cancelled() => {
            let _ = child.kill().await;
            Err(ConversionError::Cancelled)
        }
    }
}

async fn read_stderr(child: &mut tokio::process::Child) -> String {
    match child.stderr.take() {
        Some(mut s) => {
            use tokio::io::AsyncReadExt;
            let mut buf = String::new();
            let _ = s.read_to_string(&mut buf).await;
            buf
        }
        None => String::new(),
    }
}

