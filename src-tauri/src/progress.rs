use serde::{Deserialize, Serialize};

/// Payload emitted via Tauri events to report conversion progress.
///
/// `percent` is 0..=100 for determinate progress, or -1 for indeterminate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressPayload {
    pub percent: i32,
    pub stage: String,
}

/// Parse an FFmpeg `-progress pipe:1` output line and return the current
/// progress percentage based on the total duration.
///
/// FFmpeg writes key=value pairs; we look for `out_time_ms=<microseconds>`.
/// (Despite the name, the value is in **microseconds**.)
///
/// Returns `None` if the line is not an `out_time_ms` line or cannot be parsed.
/// The returned percentage is clamped to 0..=100.
pub fn parse_ffmpeg_progress(line: &str, total_duration_ms: u64) -> Option<i32> {
    let line = line.trim();
    if !line.starts_with("out_time_ms=") {
        return None;
    }

    let value_str = line.strip_prefix("out_time_ms=")?;
    let out_time_us: i64 = value_str.parse().ok()?;

    if out_time_us < 0 || total_duration_ms == 0 {
        return None;
    }

    // out_time_us is microseconds; total_duration_ms is milliseconds.
    let out_time_ms = out_time_us as u64 / 1000;
    let pct = (out_time_ms * 100 / total_duration_ms).min(100) as i32;
    Some(pct)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_out_time_ms() {
        // 5 000 000 us = 5 000 ms, total = 10 000 ms -> 50%
        assert_eq!(
            parse_ffmpeg_progress("out_time_ms=5000000", 10_000),
            Some(50)
        );
    }

    #[test]
    fn clamps_to_100() {
        assert_eq!(
            parse_ffmpeg_progress("out_time_ms=99999999999", 10_000),
            Some(100)
        );
    }

    #[test]
    fn ignores_non_matching_lines() {
        assert_eq!(parse_ffmpeg_progress("speed=1.5x", 10_000), None);
        assert_eq!(parse_ffmpeg_progress("frame=120", 10_000), None);
    }

    #[test]
    fn handles_zero_duration() {
        assert_eq!(parse_ffmpeg_progress("out_time_ms=5000000", 0), None);
    }

    #[test]
    fn handles_negative_value() {
        assert_eq!(parse_ffmpeg_progress("out_time_ms=-1", 10_000), None);
    }
}
