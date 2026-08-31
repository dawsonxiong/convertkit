#!/bin/sh

set -eu

APP_PATH=${1:-"src-tauri/target/debug/bundle/macos/ConvertKit.app"}
MINIMAL_PATH=/usr/bin:/bin:/usr/sbin:/sbin

fail() {
  printf 'Audio compression smoke failed: %s\n' "$1" >&2
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
SMOKE_DIRECTORY=$(mktemp -d "$SMOKE_ROOT/convertkit-audio-compression-smoke.XXXXXX")
case "$SMOKE_DIRECTORY" in
  "$SMOKE_ROOT"/convertkit-audio-compression-smoke.*) ;;
  *) fail "temporary directory was created outside the expected location" ;;
esac

cleanup() {
  case "$SMOKE_DIRECTORY" in
    "$SMOKE_ROOT"/convertkit-audio-compression-smoke.*)
      [ ! -d "$SMOKE_DIRECTORY" ] || find "$SMOKE_DIRECTORY" -depth -delete
      ;;
    *) printf 'Refusing to clean unexpected directory: %s\n' "$SMOKE_DIRECTORY" >&2 ;;
  esac
}
trap cleanup EXIT HUP INT TERM

SOURCE_DIRECTORY="$SMOKE_DIRECTORY/source"
OUTPUT_DIRECTORY="$SMOKE_DIRECTORY/output"
CANCEL_OUTPUT_DIRECTORY="$SMOKE_DIRECTORY/cancel-output"
NOT_SMALLER_OUTPUT_DIRECTORY="$SMOKE_DIRECTORY/not-smaller-output"
RUNTIME_DIRECTORY="$SMOKE_DIRECTORY/runtime"
mkdir "$SOURCE_DIRECTORY" "$OUTPUT_DIRECTORY" "$CANCEL_OUTPUT_DIRECTORY" \
  "$NOT_SMALLER_OUTPUT_DIRECTORY" \
  "$RUNTIME_DIRECTORY"

SOURCE_AUDIO="$SOURCE_DIRECTORY/representative source.wav"
"$FFMPEG" -hide_banner -loglevel error \
  -f lavfi -i 'sine=frequency=440:sample_rate=48000:duration=20' \
  -f lavfi -i 'sine=frequency=997:sample_rate=48000:duration=20' \
  -filter_complex '[0:a][1:a]join=inputs=2:channel_layout=stereo[a]' \
  -map '[a]' -c:a pcm_s24le "$SOURCE_AUDIO"
[ -s "$SOURCE_AUDIO" ] || fail "the representative source was not created"

SOURCE_HASH=$(/usr/bin/shasum -a 256 "$SOURCE_AUDIO" | /usr/bin/awk '{print $1}')
SOURCE_SIZE=$(/usr/bin/stat -f '%z' "$SOURCE_AUDIO")
SOURCE_DURATION=$("$FFPROBE" -v error -show_entries format=duration \
  -of default=noprint_wrappers=1:nokey=1 "$SOURCE_AUDIO")
SOURCE_CHANNELS=$("$FFPROBE" -v error -select_streams a:0 -show_entries stream=channels \
  -of default=noprint_wrappers=1:nokey=1 "$SOURCE_AUDIO")
SOURCE_SAMPLE_RATE=$("$FFPROBE" -v error -select_streams a:0 -show_entries stream=sample_rate \
  -of default=noprint_wrappers=1:nokey=1 "$SOURCE_AUDIO")
[ "$SOURCE_CHANNELS" -eq 2 ] || fail "the representative source is not stereo"
[ "$SOURCE_SAMPLE_RATE" -eq 48000 ] || fail "the representative source is not 48 kHz"

assert_source_unchanged() {
  current_hash=$(/usr/bin/shasum -a 256 "$SOURCE_AUDIO" | /usr/bin/awk '{print $1}')
  [ "$current_hash" = "$SOURCE_HASH" ] || fail "the source audio changed"
}

write_request() {
  preset=$1
  suffix=$2
  job_id=$3
  result_path=$4
  output_directory=$5
  cancel_at_progress=$6
  request_path="$SMOKE_DIRECTORY/$job_id-request.json"

  if [ "$cancel_at_progress" = null ]; then
    cancel_job_id=null
  else
    cancel_job_id="\"$job_id\""
  fi

  {
    printf '%s\n' '{' '  "request": {' '    "operation": "compressAudio",'
    printf '    "inputPath": "%s",\n' "$SOURCE_AUDIO"
    printf '    "preset": "%s",\n' "$preset"
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
}

run_request() {
  preset=$1
  suffix=$2
  job_id=$3
  result_path=$4
  output_directory=$5
  cancel_at_progress=$6
  app_log="$SMOKE_DIRECTORY/$job_id.log"

  write_request "$preset" "$suffix" "$job_id" "$result_path" \
    "$output_directory" "$cancel_at_progress"
  if ! PATH=$MINIMAL_PATH \
    TMPDIR="$RUNTIME_DIRECTORY" \
    CONVERTKIT_DEBUG_SMOKE_REQUEST="$SMOKE_DIRECTORY/$job_id-request.json" \
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
      exit difference <= 0.25 ? 0 : 1;
    }
  ' || fail "the output duration differs from the source"
}

assert_valid_output() {
  output_path=$1
  output_label=$2
  [ -s "$output_path" ] || fail "$output_label does not exist"
  case "$output_path" in
    *.m4a) ;;
    *) fail "$output_label is not an M4A file" ;;
  esac

  output_size=$(/usr/bin/stat -f '%z' "$output_path")
  [ "$output_size" -lt "$SOURCE_SIZE" ] || fail "$output_label is not smaller than its source"
  format_names=$("$FFPROBE" -v error -show_entries format=format_name \
    -of default=noprint_wrappers=1:nokey=1 "$output_path")
  printf '%s\n' "$format_names" | /usr/bin/awk -F, '
    {
      for (field = 1; field <= NF; field += 1) {
        if ($field == "mov" || $field == "mp4") found = 1;
      }
      exit found ? 0 : 1;
    }
  ' || fail "$output_label is not in the M4A container family"

  audio_codec=$("$FFPROBE" -v error -select_streams a:0 -show_entries stream=codec_name \
    -of default=noprint_wrappers=1:nokey=1 "$output_path")
  channels=$("$FFPROBE" -v error -select_streams a:0 -show_entries stream=channels \
    -of default=noprint_wrappers=1:nokey=1 "$output_path")
  sample_rate=$("$FFPROBE" -v error -select_streams a:0 -show_entries stream=sample_rate \
    -of default=noprint_wrappers=1:nokey=1 "$output_path")
  output_duration=$("$FFPROBE" -v error -show_entries format=duration \
    -of default=noprint_wrappers=1:nokey=1 "$output_path")
  audio_count=$("$FFPROBE" -v error -select_streams a -show_entries stream=index \
    -of csv=p=0 "$output_path" | /usr/bin/awk 'NF { count += 1 } END { print count + 0 }')
  video_count=$("$FFPROBE" -v error -select_streams v -show_entries stream=index \
    -of csv=p=0 "$output_path" | /usr/bin/awk 'NF { count += 1 } END { print count + 0 }')

  [ "$audio_count" -eq 1 ] || fail "$output_label has the wrong audio stream count"
  [ "$video_count" -eq 0 ] || fail "$output_label unexpectedly contains video"
  [ "$audio_codec" = aac ] || fail "$output_label is not AAC"
  [ "$channels" -eq "$SOURCE_CHANNELS" ] || fail "$output_label changed the channel count"
  [ "$sample_rate" -eq "$SOURCE_SAMPLE_RATE" ] || fail "$output_label changed the sample rate"
  assert_duration_matches "$output_duration"
  "$FFMPEG" -hide_banner -loglevel error -i "$output_path" -map 0:a:0 -f null /dev/null
}

run_success() {
  preset=$1
  suffix=$2
  job_id=$3
  result_path="$SMOKE_DIRECTORY/$job_id-result.json"
  run_request "$preset" "$suffix" "$job_id" "$result_path" "$OUTPUT_DIRECTORY" null
  output_path=$(success_output "$result_path")
  assert_valid_output "$output_path" "$preset output"
  assert_source_unchanged
  printf '%s\n' "$output_path"
}

HIGH_OUTPUT=$(run_success high -compressed-high audio-compression-high-1)
HIGH_HASH=$(/usr/bin/shasum -a 256 "$HIGH_OUTPUT" | /usr/bin/awk '{print $1}')
HIGH_KEEP_BOTH_OUTPUT=$(run_success high -compressed-high audio-compression-high-2)
[ "$HIGH_KEEP_BOTH_OUTPUT" != "$HIGH_OUTPUT" ] || fail "keep-both reused the first output path"
[ "$(/usr/bin/shasum -a 256 "$HIGH_OUTPUT" | /usr/bin/awk '{print $1}')" = "$HIGH_HASH" ] ||
  fail "the keep-both run overwrote the first output"

BALANCED_OUTPUT=$(run_success balanced -compressed-balanced audio-compression-balanced)
SMALLEST_OUTPUT=$(run_success smallest -compressed-smallest audio-compression-smallest)

SMALL_SOURCE="$SOURCE_DIRECTORY/already compact.m4a"
"$FFMPEG" -hide_banner -loglevel error \
  -f lavfi -i 'sine=frequency=661:sample_rate=24000:duration=3' \
  -ac 1 -c:a aac -b:a 16k -movflags +faststart "$SMALL_SOURCE"
[ -s "$SMALL_SOURCE" ] || fail "the already-compact source was not created"
SMALL_SOURCE_HASH=$(/usr/bin/shasum -a 256 "$SMALL_SOURCE" | /usr/bin/awk '{print $1}')
NOT_SMALLER_RESULT="$SMOKE_DIRECTORY/audio-compression-not-smaller-result.json"
NOT_SMALLER_REQUEST="$SMOKE_DIRECTORY/audio-compression-not-smaller-request.json"
{
  printf '%s\n' '{' '  "request": {' '    "operation": "compressAudio",'
  printf '    "inputPath": "%s",\n' "$SMALL_SOURCE"
  printf '%s\n' \
    '    "preset": "smallest",' \
    '    "jobId": "audio-compression-not-smaller",' \
    '    "outputOptions": {'
  printf '      "directory": "%s",\n' "$NOT_SMALLER_OUTPUT_DIRECTORY"
  printf '%s\n' \
    '      "suffix": "-compressed"' \
    '    }' \
    '  },'
  printf '  "resultPath": "%s",\n' "$NOT_SMALLER_RESULT"
  printf '%s\n' \
    '  "cancelAfterMs": null,' \
    '  "cancelAtProgress": null,' \
    '  "cancelJobId": null' \
    '}'
} > "$NOT_SMALLER_REQUEST"
NOT_SMALLER_LOG="$SMOKE_DIRECTORY/audio-compression-not-smaller.log"
if ! PATH=$MINIMAL_PATH \
  TMPDIR="$RUNTIME_DIRECTORY" \
  CONVERTKIT_DEBUG_SMOKE_REQUEST="$NOT_SMALLER_REQUEST" \
  "$APP_EXECUTABLE" > "$NOT_SMALLER_LOG" 2>&1; then
  /bin/cat "$NOT_SMALLER_LOG" >&2
  fail "the already-compact request did not complete through the packaged app"
fi
not_smaller_kind=$(/usr/bin/plutil -extract Err.kind raw -o - "$NOT_SMALLER_RESULT" 2>/dev/null || true)
if [ "$not_smaller_kind" != OutputNotSmaller ]; then
  /bin/cat "$NOT_SMALLER_RESULT" >&2
  fail "the already-compact source did not return OutputNotSmaller"
fi
[ "$(/usr/bin/shasum -a 256 "$SMALL_SOURCE" | /usr/bin/awk '{print $1}')" = "$SMALL_SOURCE_HASH" ] ||
  fail "the already-compact source changed"
[ -z "$(find "$NOT_SMALLER_OUTPUT_DIRECTORY" -mindepth 1 -print -quit)" ] ||
  fail "the already-compact request left an output or hidden partial"

CANCEL_RESULT="$SMOKE_DIRECTORY/audio-compression-cancel-result.json"
run_request balanced -compressed-cancel audio-compression-cancel "$CANCEL_RESULT" \
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

printf 'Packaged audio compression smoke passed\n'
printf 'High AAC/M4A: %s\n' "$HIGH_OUTPUT"
printf 'High keep-both: %s\n' "$HIGH_KEEP_BOTH_OUTPUT"
printf 'Balanced AAC/M4A: %s\n' "$BALANCED_OUTPUT"
printf 'Smallest AAC/M4A: %s\n' "$SMALLEST_OUTPUT"
printf 'Every output was strictly smaller; already-compact input and cancellation left no output or hidden partial\n'
