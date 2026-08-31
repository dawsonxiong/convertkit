#!/bin/sh

set -eu

APP_PATH=${1:-"src-tauri/target/debug/bundle/macos/ConvertKit.app"}
MINIMAL_PATH=/usr/bin:/bin:/usr/sbin:/sbin

fail() {
  printf 'Quality video smoke failed: %s\n' "$1" >&2
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
SMOKE_DIRECTORY=$(mktemp -d "$SMOKE_ROOT/convertkit-video-quality-smoke.XXXXXX")
case "$SMOKE_DIRECTORY" in
  "$SMOKE_ROOT"/convertkit-video-quality-smoke.*) ;;
  *) fail "temporary directory was created outside the expected location" ;;
esac

cleanup() {
  case "$SMOKE_DIRECTORY" in
    "$SMOKE_ROOT"/convertkit-video-quality-smoke.*)
      [ ! -d "$SMOKE_DIRECTORY" ] || find "$SMOKE_DIRECTORY" -depth -delete
      ;;
    *) printf 'Refusing to clean unexpected directory: %s\n' "$SMOKE_DIRECTORY" >&2 ;;
  esac
}
trap cleanup EXIT HUP INT TERM

SOURCE_DIRECTORY="$SMOKE_DIRECTORY/source"
OUTPUT_DIRECTORY="$SMOKE_DIRECTORY/output"
CANCEL_OUTPUT_DIRECTORY="$SMOKE_DIRECTORY/cancel-output"
RUNTIME_DIRECTORY="$SMOKE_DIRECTORY/runtime"
HOME_DIRECTORY="$SMOKE_DIRECTORY/home"
mkdir "$SOURCE_DIRECTORY" "$OUTPUT_DIRECTORY" "$CANCEL_OUTPUT_DIRECTORY" \
  "$RUNTIME_DIRECTORY" "$HOME_DIRECTORY"

SOURCE_VIDEO="$SOURCE_DIRECTORY/representative source.mkv"
"$FFMPEG" -hide_banner -loglevel error \
  -f lavfi -i 'testsrc2=size=640x360:rate=24:duration=3' \
  -f lavfi -i 'sine=frequency=997:sample_rate=48000:duration=3' \
  -map 0:v:0 -map 1:a:0 -c:v ffv1 -level 3 -c:a pcm_s16le -shortest \
  "$SOURCE_VIDEO"
[ -s "$SOURCE_VIDEO" ] || fail "the representative source was not created"

SOURCE_HASH=$(/usr/bin/shasum -a 256 "$SOURCE_VIDEO" | /usr/bin/awk '{print $1}')
SOURCE_DURATION=$("$FFPROBE" -v error -show_entries format=duration \
  -of default=noprint_wrappers=1:nokey=1 "$SOURCE_VIDEO")
[ -n "$SOURCE_DURATION" ] || fail "the source duration could not be measured"

assert_source_unchanged() {
  current_hash=$(/usr/bin/shasum -a 256 "$SOURCE_VIDEO" | /usr/bin/awk '{print $1}')
  [ "$current_hash" = "$SOURCE_HASH" ] || fail "the source video changed"
}

run_request() {
  preset=$1
  suffix=$2
  job_id=$3
  result_path=$4
  output_directory=$5
  cancel_at_progress=$6
  request_path="$SMOKE_DIRECTORY/$job_id-request.json"
  app_log="$SMOKE_DIRECTORY/$job_id.log"

  if [ "$cancel_at_progress" = null ]; then
    cancel_job_id=null
  else
    cancel_job_id="\"$job_id\""
  fi
  {
    printf '%s\n' '{' '  "request": {' '    "operation": "encodeVideo",'
    printf '    "inputPath": "%s",\n' "$SOURCE_VIDEO"
    printf '    "preset": "%s",\n' "$preset"
    printf '%s\n' \
      '    "resolution": "automatic",' \
      '    "quality": "balanced",' \
      '    "compressionGoal": "quality",' \
      '    "targetSizeBytes": null,'
    printf '    "jobId": "%s",\n' "$job_id"
    printf '%s\n' '    "outputOptions": {'
    printf '      "directory": "%s",\n' "$output_directory"
    printf '      "suffix": "%s"\n' "$suffix"
    printf '%s\n' '    }' '  },'
    printf '  "resultPath": "%s",\n' "$result_path"
    printf '%s\n' '  "cancelAfterMs": null,'
    printf '  "cancelAtProgress": %s,\n' "$cancel_at_progress"
    printf '  "cancelJobId": %s\n' "$cancel_job_id"
    printf '%s\n' '}'
  } > "$request_path"

  if ! PATH=$MINIMAL_PATH \
    HOME="$HOME_DIRECTORY" \
    TMPDIR="$RUNTIME_DIRECTORY" \
    CONVERTKIT_DEBUG_SMOKE_REQUEST="$request_path" \
    "$APP_EXECUTABLE" > "$app_log" 2>&1; then
    /bin/cat "$app_log" >&2
    fail "$job_id did not complete through the packaged app"
  fi
  [ -f "$result_path" ] || {
    /bin/cat "$app_log" >&2
    fail "$job_id did not write a result"
  }
}

success_output() {
  result_path=$1
  if ! /usr/bin/plutil -extract Ok.output_path raw -o - "$result_path" 2>/dev/null; then
    /bin/cat "$result_path" >&2
    fail "the packaged app returned an error"
  fi
}

assert_duration_matches() {
  output_duration=$1
  /usr/bin/awk -v source="$SOURCE_DURATION" -v output="$output_duration" '
    BEGIN {
      difference = source - output;
      if (difference < 0) difference = -difference;
      tolerance = source * 0.02;
      if (tolerance < 1) tolerance = 1;
      exit difference <= tolerance ? 0 : 1;
    }
  ' || fail "the output duration differs from the source"
}

assert_container() {
  format_names=$1
  expected=$2
  printf '%s\n' "$format_names" | /usr/bin/awk -F, -v expected="$expected" '
    {
      for (field = 1; field <= NF; field += 1) {
        if ($field == expected) exit 0;
      }
      exit 1;
    }
  ' || fail "the output container is not $expected"
}

assert_valid_output() {
  output_path=$1
  expected_container=$2
  expected_video_codec=$3
  expected_audio_codec=$4
  output_label=$5
  [ -s "$output_path" ] || fail "$output_label does not exist"

  format_names=$("$FFPROBE" -v error -show_entries format=format_name \
    -of default=noprint_wrappers=1:nokey=1 "$output_path")
  assert_container "$format_names" "$expected_container"

  video_codec=$("$FFPROBE" -v error -select_streams v:0 \
    -show_entries stream=codec_name -of default=noprint_wrappers=1:nokey=1 "$output_path")
  audio_codec=$("$FFPROBE" -v error -select_streams a:0 \
    -show_entries stream=codec_name -of default=noprint_wrappers=1:nokey=1 "$output_path")
  width=$("$FFPROBE" -v error -select_streams v:0 \
    -show_entries stream=width -of default=noprint_wrappers=1:nokey=1 "$output_path")
  height=$("$FFPROBE" -v error -select_streams v:0 \
    -show_entries stream=height -of default=noprint_wrappers=1:nokey=1 "$output_path")
  output_duration=$("$FFPROBE" -v error -show_entries format=duration \
    -of default=noprint_wrappers=1:nokey=1 "$output_path")
  video_count=$("$FFPROBE" -v error -select_streams v \
    -show_entries stream=index -of csv=p=0 "$output_path" | \
    /usr/bin/awk 'NF { count += 1 } END { print count + 0 }')
  audio_count=$("$FFPROBE" -v error -select_streams a \
    -show_entries stream=index -of csv=p=0 "$output_path" | \
    /usr/bin/awk 'NF { count += 1 } END { print count + 0 }')

  [ "$video_count" -eq 1 ] || fail "$output_label has the wrong video stream count"
  [ "$audio_count" -eq 1 ] || fail "$output_label has the wrong audio stream count"
  [ "$video_codec" = "$expected_video_codec" ] || fail "$output_label has the wrong video codec"
  [ "$audio_codec" = "$expected_audio_codec" ] || fail "$output_label has the wrong audio codec"
  [ "$width" -eq 640 ] && [ "$height" -eq 360 ] || fail "$output_label changed dimensions"
  assert_duration_matches "$output_duration"

  "$FFMPEG" -hide_banner -loglevel error -i "$output_path" \
    -map 0:v:0 -map '0:a?' -f null /dev/null
}

run_success() {
  preset=$1
  suffix=$2
  job_id=$3
  expected_container=$4
  expected_video_codec=$5
  expected_audio_codec=$6
  result_path="$SMOKE_DIRECTORY/$job_id-result.json"
  run_request "$preset" "$suffix" "$job_id" "$result_path" "$OUTPUT_DIRECTORY" null
  output_path=$(success_output "$result_path")
  assert_valid_output "$output_path" "$expected_container" "$expected_video_codec" \
    "$expected_audio_codec" "$preset output"
  assert_source_unchanged
  printf '%s\n' "$output_path"
}

FIRST_OUTPUT=$(run_success compatible -quality-compatible video-quality-compatible-1 mp4 h264 aac)
FIRST_HASH=$(/usr/bin/shasum -a 256 "$FIRST_OUTPUT" | /usr/bin/awk '{print $1}')
SECOND_OUTPUT=$(run_success compatible -quality-compatible video-quality-compatible-2 mp4 h264 aac)
[ "$SECOND_OUTPUT" != "$FIRST_OUTPUT" ] || fail "keep-both reused the first output path"
[ "$(/usr/bin/shasum -a 256 "$FIRST_OUTPUT" | /usr/bin/awk '{print $1}')" = "$FIRST_HASH" ] ||
  fail "the keep-both run overwrote the first output"

SMALLER_OUTPUT=$(run_success smaller -quality-smaller video-quality-smaller mp4 hevc aac)
WEB_OUTPUT=$(run_success web -quality-web video-quality-web webm vp9 opus)
ARCHIVE_OUTPUT=$(run_success archive -quality-archive video-quality-archive matroska ffv1 flac)

CANCEL_RESULT="$SMOKE_DIRECTORY/video-quality-cancel-result.json"
run_request compatible -quality-cancel video-quality-cancel "$CANCEL_RESULT" \
  "$CANCEL_OUTPUT_DIRECTORY" 0
cancel_kind=$(/usr/bin/plutil -extract Err.kind raw -o - "$CANCEL_RESULT" 2>/dev/null || true)
if [ "$cancel_kind" != Cancelled ]; then
  /bin/cat "$CANCEL_RESULT" >&2
  fail "the cancellation did not return the Cancelled error"
fi
assert_source_unchanged

cancelled_output=$(find "$CANCEL_OUTPUT_DIRECTORY" -mindepth 1 -print -quit)
[ -z "$cancelled_output" ] || fail "the cancelled job left an output or hidden partial"
partial_output=$(find "$OUTPUT_DIRECTORY" -maxdepth 1 -name '.convertkit-*' -print -quit)
[ -z "$partial_output" ] || fail "a hidden partial output was left behind"

printf 'Packaged quality video smoke passed\n'
printf 'Compatible H.264/AAC: %s\n' "$FIRST_OUTPUT"
printf 'Compatible keep-both: %s\n' "$SECOND_OUTPUT"
printf 'Smaller H.265/AAC: %s\n' "$SMALLER_OUTPUT"
printf 'Web VP9/Opus: %s\n' "$WEB_OUTPUT"
printf 'Lossless FFV1/FLAC: %s\n' "$ARCHIVE_OUTPUT"
printf 'Cancellation left no committed or partial output\n'
