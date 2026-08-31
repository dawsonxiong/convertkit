use std::path::Path;
use std::process::{ExitStatus, Stdio};
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt};
use tokio_util::sync::CancellationToken;

use crate::error::ConversionError;

use super::cleanup_partial;

const MAX_CAPTURED_STDERR_BYTES: usize = 64 * 1024;

#[derive(Clone, Copy)]
pub struct ProcessMessages {
    pub start: &'static str,
    pub wait: &'static str,
    pub failure: &'static str,
}

enum ProcessExit {
    Finished(Result<ExitStatus, std::io::Error>),
    Cancelled,
    TimedOut,
}

struct BoundedBytes {
    bytes: Vec<u8>,
    truncated: bool,
}

/// Run a quiet subprocess with consistent cancellation, timeout, stderr draining,
/// and partial-output cleanup. Stderr is drained for the lifetime of the process
/// while only its bounded tail is retained for a useful error message.
pub async fn run_process(
    mut command: tokio::process::Command,
    cancel_token: CancellationToken,
    timeout: Duration,
    partial_outputs: &[&Path],
    messages: ProcessMessages,
) -> Result<(), ConversionError> {
    command
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let mut child = command
        .spawn()
        .map_err(|error| ConversionError::ProcessFailed {
            message: format!("{}: {error}", messages.start),
            stderr: String::new(),
            exit_code: None,
        })?;

    let stderr_task = capture_output(child.stderr.take());
    let exit = tokio::select! {
        result = child.wait() => ProcessExit::Finished(result),
        _ = cancel_token.cancelled() => ProcessExit::Cancelled,
        _ = tokio::time::sleep(timeout) => ProcessExit::TimedOut,
    };

    match exit {
        ProcessExit::Cancelled => {
            let _ = child.kill().await;
            finish_output(stderr_task).await;
            cleanup_outputs(partial_outputs);
            Err(ConversionError::Cancelled)
        }
        ProcessExit::TimedOut => {
            let _ = child.kill().await;
            finish_output(stderr_task).await;
            cleanup_outputs(partial_outputs);
            Err(ConversionError::Timeout {
                seconds: timeout.as_secs(),
            })
        }
        ProcessExit::Finished(Err(error)) => {
            let _ = child.kill().await;
            let stderr = finish_output(stderr_task).await;
            cleanup_outputs(partial_outputs);
            Err(ConversionError::ProcessFailed {
                message: format!("{}: {error}", messages.wait),
                stderr,
                exit_code: None,
            })
        }
        ProcessExit::Finished(Ok(status)) => {
            let stderr = finish_output(stderr_task).await;
            if status.success() {
                Ok(())
            } else {
                cleanup_outputs(partial_outputs);
                Err(ConversionError::ProcessFailed {
                    message: messages.failure.into(),
                    stderr,
                    exit_code: status.code(),
                })
            }
        }
    }
}

/// Run a subprocess whose stdout is the result. Both pipes are drained for the
/// entire process lifetime; stdout is retained only up to the caller-provided
/// bound so a malformed helper cannot exhaust application memory.
pub async fn run_process_with_output(
    mut command: tokio::process::Command,
    cancel_token: CancellationToken,
    timeout: Duration,
    max_stdout_bytes: usize,
    partial_outputs: &[&Path],
    messages: ProcessMessages,
) -> Result<Vec<u8>, ConversionError> {
    command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let mut child = command
        .spawn()
        .map_err(|error| ConversionError::ProcessFailed {
            message: format!("{}: {error}", messages.start),
            stderr: String::new(),
            exit_code: None,
        })?;
    let stdout_task = capture_bounded_bytes(child.stdout.take(), max_stdout_bytes);
    let stderr_task = capture_output(child.stderr.take());
    let exit = tokio::select! {
        result = child.wait() => ProcessExit::Finished(result),
        _ = cancel_token.cancelled() => ProcessExit::Cancelled,
        _ = tokio::time::sleep(timeout) => ProcessExit::TimedOut,
    };

    match exit {
        ProcessExit::Cancelled => {
            let _ = child.kill().await;
            finish_bounded_bytes(stdout_task).await;
            finish_output(stderr_task).await;
            cleanup_outputs(partial_outputs);
            Err(ConversionError::Cancelled)
        }
        ProcessExit::TimedOut => {
            let _ = child.kill().await;
            finish_bounded_bytes(stdout_task).await;
            finish_output(stderr_task).await;
            cleanup_outputs(partial_outputs);
            Err(ConversionError::Timeout {
                seconds: timeout.as_secs(),
            })
        }
        ProcessExit::Finished(Err(error)) => {
            let _ = child.kill().await;
            finish_bounded_bytes(stdout_task).await;
            let stderr = finish_output(stderr_task).await;
            cleanup_outputs(partial_outputs);
            Err(ConversionError::ProcessFailed {
                message: format!("{}: {error}", messages.wait),
                stderr,
                exit_code: None,
            })
        }
        ProcessExit::Finished(Ok(status)) => {
            let stdout = finish_bounded_bytes(stdout_task).await;
            let stderr = finish_output(stderr_task).await;
            if !status.success() {
                cleanup_outputs(partial_outputs);
                return Err(ConversionError::ProcessFailed {
                    message: messages.failure.into(),
                    stderr,
                    exit_code: status.code(),
                });
            }
            if stdout.truncated {
                cleanup_outputs(partial_outputs);
                return Err(ConversionError::ProcessFailed {
                    message: format!(
                        "{} produced more than {max_stdout_bytes} bytes of output",
                        messages.failure
                    ),
                    stderr,
                    exit_code: status.code(),
                });
            }
            Ok(stdout.bytes)
        }
    }
}

pub fn capture_output<R>(stream: Option<R>) -> tokio::task::JoinHandle<String>
where
    R: AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        match stream {
            Some(stream) => drain_output(stream).await,
            None => String::new(),
        }
    })
}

async fn drain_output<R>(mut stream: R) -> String
where
    R: AsyncRead + Unpin,
{
    let mut retained = Vec::with_capacity(MAX_CAPTURED_STDERR_BYTES);
    let mut buffer = [0_u8; 8 * 1024];

    loop {
        let count = match stream.read(&mut buffer).await {
            Ok(0) | Err(_) => break,
            Ok(count) => count,
        };

        if count >= MAX_CAPTURED_STDERR_BYTES {
            retained.clear();
            retained.extend_from_slice(&buffer[count - MAX_CAPTURED_STDERR_BYTES..count]);
            continue;
        }

        let overflow = retained
            .len()
            .saturating_add(count)
            .saturating_sub(MAX_CAPTURED_STDERR_BYTES);
        if overflow > 0 {
            retained.drain(..overflow);
        }
        retained.extend_from_slice(&buffer[..count]);
    }

    String::from_utf8_lossy(&retained).into_owned()
}

pub async fn finish_output(task: tokio::task::JoinHandle<String>) -> String {
    task.await.unwrap_or_default()
}

fn capture_bounded_bytes<R>(
    stream: Option<R>,
    maximum: usize,
) -> tokio::task::JoinHandle<BoundedBytes>
where
    R: AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        let Some(mut stream) = stream else {
            return BoundedBytes {
                bytes: Vec::new(),
                truncated: false,
            };
        };
        let mut bytes = Vec::with_capacity(maximum.min(64 * 1024));
        let mut buffer = [0_u8; 8 * 1024];
        let mut truncated = false;
        loop {
            let count = match stream.read(&mut buffer).await {
                Ok(0) | Err(_) => break,
                Ok(count) => count,
            };
            let remaining = maximum.saturating_sub(bytes.len());
            let retained = remaining.min(count);
            bytes.extend_from_slice(&buffer[..retained]);
            truncated |= retained < count;
        }
        BoundedBytes { bytes, truncated }
    })
}

async fn finish_bounded_bytes(task: tokio::task::JoinHandle<BoundedBytes>) -> BoundedBytes {
    task.await.unwrap_or(BoundedBytes {
        bytes: Vec::new(),
        truncated: true,
    })
}

fn cleanup_outputs(paths: &[&Path]) {
    for path in paths {
        cleanup_partial(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MESSAGES: ProcessMessages = ProcessMessages {
        start: "Could not start test process",
        wait: "Test process could not be awaited",
        failure: "Test process failed",
    };

    fn sleeping_command() -> tokio::process::Command {
        let mut command = tokio::process::Command::new("/bin/sh");
        command.args(["-c", "sleep 10"]);
        command
    }

    #[tokio::test]
    async fn reports_exit_code_and_stderr_for_failed_processes() {
        let mut command = tokio::process::Command::new("/bin/sh");
        command.args(["-c", "printf 'useful details' >&2; exit 7"]);

        let error = run_process(
            command,
            CancellationToken::new(),
            Duration::from_secs(2),
            &[],
            MESSAGES,
        )
        .await
        .expect_err("the command should fail");

        match error {
            ConversionError::ProcessFailed {
                message,
                stderr,
                exit_code,
            } => {
                assert_eq!(message, "Test process failed");
                assert_eq!(stderr, "useful details");
                assert_eq!(exit_code, Some(7));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[tokio::test]
    async fn drains_large_stderr_while_retaining_only_its_tail() {
        let mut command = tokio::process::Command::new("/bin/sh");
        command.args([
            "-c",
            "i=0; while [ $i -lt 7000 ]; do printf '0123456789abcdef'; i=$((i+1)); done >&2; printf 'TAIL' >&2; exit 9",
        ]);

        let error = run_process(
            command,
            CancellationToken::new(),
            Duration::from_secs(2),
            &[],
            MESSAGES,
        )
        .await
        .expect_err("the command should fail");

        match error {
            ConversionError::ProcessFailed { stderr, .. } => {
                assert!(stderr.len() <= MAX_CAPTURED_STDERR_BYTES);
                assert!(stderr.ends_with("TAIL"));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[tokio::test]
    async fn cancellation_removes_partial_outputs() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let partial = directory.path().join("partial-output");
        std::fs::write(&partial, b"partial").expect("partial output");
        let cancel_token = CancellationToken::new();
        let trigger = cancel_token.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(20)).await;
            trigger.cancel();
        });

        let error = run_process(
            sleeping_command(),
            cancel_token,
            Duration::from_secs(2),
            &[partial.as_path()],
            MESSAGES,
        )
        .await
        .expect_err("the command should be cancelled");

        assert!(matches!(error, ConversionError::Cancelled));
        assert!(!partial.exists());
    }

    #[tokio::test]
    async fn timeout_removes_partial_outputs() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let partial = directory.path().join("partial-output");
        std::fs::write(&partial, b"partial").expect("partial output");

        let error = run_process(
            sleeping_command(),
            CancellationToken::new(),
            Duration::from_millis(20),
            &[partial.as_path()],
            MESSAGES,
        )
        .await
        .expect_err("the command should time out");

        assert!(matches!(error, ConversionError::Timeout { .. }));
        assert!(!partial.exists());
    }

    #[tokio::test]
    async fn captures_bounded_stdout_without_blocking_on_excess_output() {
        let mut success = tokio::process::Command::new("/bin/sh");
        success.args(["-c", "printf 'recognized text'"]);
        let output = run_process_with_output(
            success,
            CancellationToken::new(),
            Duration::from_secs(2),
            64,
            &[],
            MESSAGES,
        )
        .await
        .expect("captured output");
        assert_eq!(output, b"recognized text");

        let mut oversized = tokio::process::Command::new("/bin/sh");
        oversized.args(["-c", "printf '0123456789'"]);
        let error = run_process_with_output(
            oversized,
            CancellationToken::new(),
            Duration::from_secs(2),
            4,
            &[],
            MESSAGES,
        )
        .await
        .expect_err("oversized output must fail");
        assert!(matches!(error, ConversionError::ProcessFailed { .. }));
    }
}
