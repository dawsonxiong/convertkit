#!/bin/sh

set -eu

APP_PATH=${1:-"src-tauri/target/debug/bundle/macos/ConvertKit.app"}
TARGET_SIZE_BYTES=2097152

fail() {
  printf 'Target-size video smoke failed: %s\n' "$1" >&2
  exit 1
}

[ -d "$APP_PATH" ] || fail "app bundle not found at $APP_PATH"
APP_PATH=$(cd "$(dirname "$APP_PATH")" && pwd -P)/$(basename "$APP_PATH")
APP_EXECUTABLE="$APP_PATH/Contents/MacOS/convertkit"
[ -x "$APP_EXECUTABLE" ] || fail "app executable is missing"

FFMPEG=$(command -v ffmpeg || true)
FFPROBE=$(command -v ffprobe || true)
[ -n "$FFMPEG" ] || fail "FFmpeg is not installed"
[ -n "$FFPROBE" ] || fail "FFprobe is not installed"

SMOKE_ROOT=${TMPDIR:-/tmp}
SMOKE_ROOT=${SMOKE_ROOT%/}
SMOKE_DIRECTORY=$(mktemp -d "$SMOKE_ROOT/convertkit-video-target-size-smoke.XXXXXX")
case "$SMOKE_DIRECTORY" in
  "$SMOKE_ROOT"/convertkit-video-target-size-smoke.*) ;;
  *) fail "temporary directory was created outside the expected location" ;;
esac

cleanup() {
  if [ -d "$SMOKE_DIRECTORY" ]; then
    find "$SMOKE_DIRECTORY" -depth -delete
  fi
}
trap cleanup EXIT HUP INT TERM

SOURCE_VIDEO="$SMOKE_DIRECTORY/representative source.mkv"
OUTPUT_DIRECTORY="$SMOKE_DIRECTORY/output"
CANCEL_OUTPUT_DIRECTORY="$SMOKE_DIRECTORY/cancel-output"
RUNTIME_DIRECTORY="$SMOKE_DIRECTORY/runtime"
REQUEST_PATH="$SMOKE_DIRECTORY/request.json"
RESULT_PATH="$SMOKE_DIRECTORY/result.json"
APP_LOG="$SMOKE_DIRECTORY/app.log"
mkdir "$OUTPUT_DIRECTORY" "$CANCEL_OUTPUT_DIRECTORY" "$RUNTIME_DIRECTORY"

# Lossless video and PCM audio keep the source comfortably above the requested
# delivery size while exercising a real two-stream input and a path with spaces.
"$FFMPEG" -hide_banner -loglevel error \
  -f lavfi -i 'testsrc2=size=960x540:rate=30:duration=8' \
  -f lavfi -i 'sine=frequency=997:sample_rate=48000:duration=8' \
  -map 0:v:0 -map 1:a:0 -c:v ffv1 -level 3 -c:a pcm_s16le -shortest \
  "$SOURCE_VIDEO"

[ -s "$SOURCE_VIDEO" ] || fail "the representative source was not created"
SOURCE_SIZE=$(stat -f '%z' "$SOURCE_VIDEO")
[ "$SOURCE_SIZE" -gt "$TARGET_SIZE_BYTES" ] || fail "the source is not larger than the target"
SOURCE_HASH=$(shasum -a 256 "$SOURCE_VIDEO" | awk '{print $1}')
SOURCE_DURATION=$("$FFPROBE" -v error -show_entries format=duration \
  -of default=noprint_wrappers=1:nokey=1 "$SOURCE_VIDEO")
SOURCE_VIDEO_CODEC=$("$FFPROBE" -v error -select_streams v:0 \
  -show_entries stream=codec_name -of default=noprint_wrappers=1:nokey=1 "$SOURCE_VIDEO")
SOURCE_AUDIO_CODEC=$("$FFPROBE" -v error -select_streams a:0 \
  -show_entries stream=codec_name -of default=noprint_wrappers=1:nokey=1 "$SOURCE_VIDEO")
[ -n "$SOURCE_VIDEO_CODEC" ] || fail "the source has no video stream"
[ -n "$SOURCE_AUDIO_CODEC" ] || fail "the source has no audio stream"

write_success_request() {
  JOB_ID=$1
  VIDEO_PRESET=$2
  OUTPUT_SUFFIX=$3
  printf '%s\n' \
    '{' \
    '  "request": {' \
    '    "operation": "encodeVideo",' \
    "    \"inputPath\": \"$SOURCE_VIDEO\"," \
    "    \"preset\": \"$VIDEO_PRESET\"," \
    '    "resolution": "hd",' \
    '    "quality": "balanced",' \
    '    "compressionGoal": "fileSize",' \
    "    \"targetSizeBytes\": $TARGET_SIZE_BYTES," \
    "    \"jobId\": \"$JOB_ID\"," \
    '    "outputOptions": {' \
    "      \"directory\": \"$OUTPUT_DIRECTORY\"," \
    "      \"suffix\": \"$OUTPUT_SUFFIX\"" \
    '    }' \
    '  },' \
    "  \"resultPath\": \"$RESULT_PATH\"," \
    '  "cancelAfterMs": null,' \
    '  "cancelJobId": null' \
    '}' > "$REQUEST_PATH"
}

write_cancel_request() {
  printf '%s\n' \
    '{' \
    '  "request": {' \
    '    "operation": "encodeVideo",' \
    "    \"inputPath\": \"$SOURCE_VIDEO\"," \
    '    "preset": "compatible",' \
    '    "resolution": "hd",' \
    '    "quality": "balanced",' \
    '    "compressionGoal": "fileSize",' \
    "    \"targetSizeBytes\": $TARGET_SIZE_BYTES," \
    '    "jobId": "video-target-size-cancel-smoke",' \
    '    "outputOptions": {' \
    "      \"directory\": \"$CANCEL_OUTPUT_DIRECTORY\"," \
    '      "suffix": "-cancelled"' \
    '    }' \
    '  },' \
    "  \"resultPath\": \"$RESULT_PATH\"," \
    '  "cancelAfterMs": 250,' \
    '  "cancelJobId": "video-target-size-cancel-smoke"' \
    '}' > "$REQUEST_PATH"
}

run_app() {
  rm -f "$RESULT_PATH"
  /usr/bin/open -F -n \
    --env 'PATH=/usr/bin:/bin:/usr/sbin:/sbin' \
    --env "TMPDIR=$RUNTIME_DIRECTORY" \
    --env "CONVERTKIT_DEBUG_SMOKE_REQUEST=$REQUEST_PATH" \
    "$APP_PATH" > "$APP_LOG" 2>&1
  attempt=0
  while [ ! -f "$RESULT_PATH" ] && [ "$attempt" -lt 360 ]; do
    sleep 0.5
    attempt=$((attempt + 1))
  done
  [ -f "$RESULT_PATH" ] || {
    cat "$APP_LOG" >&2
    fail "the app did not write a result within 180 seconds"
  }
}

read_success_output() {
  if ! /usr/bin/plutil -extract Ok.output_path raw -o - "$RESULT_PATH" 2>/dev/null; then
    cat "$RESULT_PATH" >&2
    fail "the packaged app returned an error"
  fi
}

assert_duration_matches() {
  OUTPUT_DURATION=$1
  awk -v source="$SOURCE_DURATION" -v output="$OUTPUT_DURATION" '
    BEGIN {
      difference = source - output;
      if (difference < 0) difference = -difference;
      tolerance = source * 0.02;
      if (tolerance < 0.5) tolerance = 0.5;
      exit difference <= tolerance ? 0 : 1;
    }
  ' || fail "the output duration differs from the source"
}

assert_source_unchanged() {
  CURRENT_SOURCE_HASH=$(shasum -a 256 "$SOURCE_VIDEO" | awk '{print $1}')
  [ "$CURRENT_SOURCE_HASH" = "$SOURCE_HASH" ] || fail "the source video changed"
}

assert_valid_output() {
  OUTPUT_PATH=$1
  EXPECTED_CONTAINER=$2
  EXPECTED_VIDEO_CODEC=$3
  EXPECTED_AUDIO_CODEC=$4
  OUTPUT_LABEL=$5
  [ -s "$OUTPUT_PATH" ] || fail "the reported output does not exist"

  OUTPUT_SIZE=$(stat -f '%z' "$OUTPUT_PATH")
  [ "$OUTPUT_SIZE" -le "$TARGET_SIZE_BYTES" ] || fail "$OUTPUT_LABEL exceeds the exact target size"

  FORMAT_NAME=$("$FFPROBE" -v error -show_entries format=format_name \
    -of default=noprint_wrappers=1:nokey=1 "$OUTPUT_PATH")
  case "$EXPECTED_CONTAINER:$FORMAT_NAME" in
    mp4:mp4 | mp4:mp4,* | mp4:*,mp4 | mp4:*,mp4,*) ;;
    webm:webm | webm:webm,* | webm:*,webm | webm:*,webm,*) ;;
    *) fail "$OUTPUT_LABEL is not an $EXPECTED_CONTAINER container" ;;
  esac

  VIDEO_CODEC=$("$FFPROBE" -v error -select_streams v:0 \
    -show_entries stream=codec_name -of default=noprint_wrappers=1:nokey=1 "$OUTPUT_PATH")
  AUDIO_CODEC=$("$FFPROBE" -v error -select_streams a:0 \
    -show_entries stream=codec_name -of default=noprint_wrappers=1:nokey=1 "$OUTPUT_PATH")
  WIDTH=$("$FFPROBE" -v error -select_streams v:0 \
    -show_entries stream=width -of default=noprint_wrappers=1:nokey=1 "$OUTPUT_PATH")
  HEIGHT=$("$FFPROBE" -v error -select_streams v:0 \
    -show_entries stream=height -of default=noprint_wrappers=1:nokey=1 "$OUTPUT_PATH")
  OUTPUT_DURATION=$("$FFPROBE" -v error -show_entries format=duration \
    -of default=noprint_wrappers=1:nokey=1 "$OUTPUT_PATH")

  [ "$VIDEO_CODEC" = "$EXPECTED_VIDEO_CODEC" ] || fail "$OUTPUT_LABEL has the wrong video codec"
  [ "$AUDIO_CODEC" = "$EXPECTED_AUDIO_CODEC" ] || fail "$OUTPUT_LABEL has the wrong audio codec"
  [ "$WIDTH" -eq 960 ] && [ "$HEIGHT" -eq 540 ] || fail "$OUTPUT_LABEL dimensions changed or were upscaled"
  assert_duration_matches "$OUTPUT_DURATION"

  "$FFMPEG" -hide_banner -loglevel error -i "$OUTPUT_PATH" \
    -map 0:v:0 -map 0:a:0 -f null /dev/null
}

write_success_request 'video-target-size-compatible-smoke-1' 'compatible' '-target-size-h264'
run_app
FIRST_OUTPUT=$(read_success_output)
assert_valid_output "$FIRST_OUTPUT" 'mp4' 'h264' 'aac' 'compatible output'
assert_source_unchanged

write_success_request 'video-target-size-compatible-smoke-2' 'compatible' '-target-size-h264'
run_app
SECOND_OUTPUT=$(read_success_output)
[ "$SECOND_OUTPUT" != "$FIRST_OUTPUT" ] || fail "keep-both reused the first output path"
assert_valid_output "$SECOND_OUTPUT" 'mp4' 'h264' 'aac' 'compatible keep-both output'
assert_source_unchanged

write_success_request 'video-target-size-smaller-smoke' 'smaller' '-target-size-hevc'
run_app
SMALLER_OUTPUT=$(read_success_output)
assert_valid_output "$SMALLER_OUTPUT" 'mp4' 'hevc' 'aac' 'smaller output'
assert_source_unchanged

write_success_request 'video-target-size-web-smoke' 'web' '-target-size-vp9'
run_app
WEB_OUTPUT=$(read_success_output)
assert_valid_output "$WEB_OUTPUT" 'webm' 'vp9' 'opus' 'web output'
assert_source_unchanged

write_cancel_request
run_app
if /usr/bin/plutil -extract Ok.output_path raw -o - "$RESULT_PATH" >/dev/null 2>&1; then
  cat "$RESULT_PATH" >&2
  fail "the cancelled job committed an output"
fi
if ! CANCEL_KIND=$(/usr/bin/plutil -extract Err.kind raw -o - "$RESULT_PATH" 2>/dev/null); then
  cat "$RESULT_PATH" >&2
  fail "the cancelled job returned an unreadable result"
fi
[ "$CANCEL_KIND" = 'Cancelled' ] || {
  cat "$RESULT_PATH" >&2
  fail "the cancellation did not return the Cancelled error"
}
assert_source_unchanged

CANCELLED_OUTPUT=$(find "$CANCEL_OUTPUT_DIRECTORY" -mindepth 1 -print -quit)
[ -z "$CANCELLED_OUTPUT" ] || fail "the cancelled job left an output or hidden partial"
PARTIAL_OUTPUT=$(find "$OUTPUT_DIRECTORY" -maxdepth 1 -name '.convertkit-*' -print -quit)
[ -z "$PARTIAL_OUTPUT" ] || fail "a hidden partial output was left behind"
PASSLOG_WORKSPACE=$(find "$RUNTIME_DIRECTORY" -name 'convertkit-video-size-*' -print -quit)
[ -z "$PASSLOG_WORKSPACE" ] || fail "a two-pass encoding workspace was left behind"

FIRST_SIZE=$(stat -f '%z' "$FIRST_OUTPUT")
SECOND_SIZE=$(stat -f '%z' "$SECOND_OUTPUT")
SMALLER_SIZE=$(stat -f '%z' "$SMALLER_OUTPUT")
WEB_SIZE=$(stat -f '%z' "$WEB_OUTPUT")
printf 'Target-size packaged video smoke passed\n'
printf 'Source: %s bytes\n' "$SOURCE_SIZE"
printf 'Target: %s bytes\n' "$TARGET_SIZE_BYTES"
printf 'Compatible H.264/AAC: %s bytes at %s\n' "$FIRST_SIZE" "$FIRST_OUTPUT"
printf 'Compatible keep-both: %s bytes at %s\n' "$SECOND_SIZE" "$SECOND_OUTPUT"
printf 'Smaller H.265/AAC: %s bytes at %s\n' "$SMALLER_SIZE" "$SMALLER_OUTPUT"
printf 'Web VP9/Opus: %s bytes at %s\n' "$WEB_SIZE" "$WEB_OUTPUT"
printf 'Cancellation left no committed or partial output\n'
