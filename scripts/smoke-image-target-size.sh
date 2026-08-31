#!/bin/sh

set -eu

APP_PATH=${1:-"src-tauri/target/debug/bundle/macos/ConvertKit.app"}
MINIMAL_PATH=/usr/bin:/bin:/usr/sbin:/sbin
TARGET_SIZE_BYTES=$((256 * 1024))
UNREACHABLE_TARGET_BYTES=$((16 * 1024))

fail() {
  printf 'Target-size image smoke failed: %s\n' "$1" >&2
  exit 1
}

[ -d "$APP_PATH" ] || fail "app bundle not found at $APP_PATH"
APP_PATH=$(cd "$(dirname "$APP_PATH")" && pwd -P)/$(basename "$APP_PATH")
APP_EXECUTABLE="$APP_PATH/Contents/MacOS/convertkit"
[ -x "$APP_EXECUTABLE" ] || fail "app executable is missing"

MAGICK=$(command -v magick || true)
[ -x "$MAGICK" ] || fail "ImageMagick is not installed"

SMOKE_ROOT=${TMPDIR:-/tmp}
SMOKE_ROOT=${SMOKE_ROOT%/}
SMOKE_DIRECTORY=$(mktemp -d "$SMOKE_ROOT/convertkit-image-target-size-smoke.XXXXXX")
case "$SMOKE_DIRECTORY" in
  "$SMOKE_ROOT"/convertkit-image-target-size-smoke.*) ;;
  *) fail "temporary directory was created outside the expected location" ;;
esac

cleanup() {
  case "$SMOKE_DIRECTORY" in
    "$SMOKE_ROOT"/convertkit-image-target-size-smoke.*)
      [ ! -d "$SMOKE_DIRECTORY" ] || find "$SMOKE_DIRECTORY" -depth -delete
      ;;
    *) printf 'Refusing to clean unexpected directory: %s\n' "$SMOKE_DIRECTORY" >&2 ;;
  esac
}
trap cleanup EXIT HUP INT TERM

SOURCE_DIRECTORY="$SMOKE_DIRECTORY/sources"
OUTPUT_DIRECTORY="$SMOKE_DIRECTORY/output"
UNREACHABLE_OUTPUT_DIRECTORY="$SMOKE_DIRECTORY/unreachable-output"
CANCEL_OUTPUT_DIRECTORY="$SMOKE_DIRECTORY/cancel-output"
RUNTIME_DIRECTORY="$SMOKE_DIRECTORY/runtime"
mkdir "$SOURCE_DIRECTORY" "$OUTPUT_DIRECTORY" "$UNREACHABLE_OUTPUT_DIRECTORY" \
  "$CANCEL_OUTPUT_DIRECTORY" "$RUNTIME_DIRECTORY"

JPEG_SOURCE="$SOURCE_DIRECTORY/representative noise.jpg"
WEBP_SOURCE="$SOURCE_DIRECTORY/representative noise.webp"
UNREACHABLE_SOURCE="$SOURCE_DIRECTORY/unreachable noise.jpg"

# Seeded random pixels make the target search exercise multiple real lossy
# encodes while keeping fixture size and compressibility deterministic.
"$MAGICK" -seed 424242 -size 1200x800 'xc:#7580aa' +noise Random \
  -quality 100 "$JPEG_SOURCE"
"$MAGICK" -seed 424242 -size 1200x800 'xc:#7580aa' +noise Random \
  -define webp:method=6 -quality 100 "$WEBP_SOURCE"
"$MAGICK" -seed 8675309 -size 2400x1600 'xc:#7580aa' +noise Random \
  -quality 100 "$UNREACHABLE_SOURCE"

for source_path in "$JPEG_SOURCE" "$WEBP_SOURCE" "$UNREACHABLE_SOURCE"; do
  [ -s "$source_path" ] || fail "could not create fixture $source_path"
done
[ "$(stat -f '%z' "$JPEG_SOURCE")" -gt "$TARGET_SIZE_BYTES" ] ||
  fail "the JPEG fixture is not larger than its target"
[ "$(stat -f '%z' "$WEBP_SOURCE")" -gt "$TARGET_SIZE_BYTES" ] ||
  fail "the WebP fixture is not larger than its target"
[ "$(stat -f '%z' "$UNREACHABLE_SOURCE")" -gt "$UNREACHABLE_TARGET_BYTES" ] ||
  fail "the unreachable fixture is not larger than its target"

JPEG_HASH=$(/usr/bin/shasum -a 256 "$JPEG_SOURCE" | /usr/bin/awk '{print $1}')
WEBP_HASH=$(/usr/bin/shasum -a 256 "$WEBP_SOURCE" | /usr/bin/awk '{print $1}')
UNREACHABLE_HASH=$(/usr/bin/shasum -a 256 "$UNREACHABLE_SOURCE" | /usr/bin/awk '{print $1}')

image_topology() {
  "$MAGICK" identify -format '%m %w %h\n' "$1"
}

[ "$(image_topology "$JPEG_SOURCE")" = 'JPEG 1200 800' ] ||
  fail "the JPEG fixture has unexpected format, dimensions, or frame count"
[ "$(image_topology "$WEBP_SOURCE")" = 'WEBP 1200 800' ] ||
  fail "the WebP fixture has unexpected format, dimensions, or frame count"
[ "$(image_topology "$UNREACHABLE_SOURCE")" = 'JPEG 2400 1600' ] ||
  fail "the unreachable fixture has unexpected format, dimensions, or frame count"

assert_sources_unchanged() {
  [ "$(/usr/bin/shasum -a 256 "$JPEG_SOURCE" | /usr/bin/awk '{print $1}')" = "$JPEG_HASH" ] ||
    fail "the JPEG source changed"
  [ "$(/usr/bin/shasum -a 256 "$WEBP_SOURCE" | /usr/bin/awk '{print $1}')" = "$WEBP_HASH" ] ||
    fail "the WebP source changed"
  [ "$(/usr/bin/shasum -a 256 "$UNREACHABLE_SOURCE" | /usr/bin/awk '{print $1}')" = "$UNREACHABLE_HASH" ] ||
    fail "the unreachable source changed"
}

run_request() {
  input_path=$1
  output_directory=$2
  suffix=$3
  target_size=$4
  job_id=$5
  result_path=$6
  cancel_at_progress=$7
  request_path="$SMOKE_DIRECTORY/$job_id-request.json"
  app_log="$SMOKE_DIRECTORY/$job_id.log"

  if [ "$cancel_at_progress" = null ]; then
    cancel_job_id=null
  else
    cancel_job_id="\"$job_id\""
  fi

  {
    printf '%s\n' '{' '  "request": {' '    "operation": "optimize",'
    printf '    "inputPath": "%s",\n' "$input_path"
    printf '%s\n' \
      '    "keepMetadata": false,' \
      '    "compressionGoal": "fileSize",'
    printf '    "targetSizeBytes": %s,\n' "$target_size"
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

assert_error() {
  result_path=$1
  expected_kind=$2
  actual_kind=$(/usr/bin/plutil -extract Err.kind raw -o - "$result_path" 2>/dev/null || true)
  if [ "$actual_kind" != "$expected_kind" ]; then
    /bin/cat "$result_path" >&2
    fail "expected $expected_kind, got ${actual_kind:-no error}"
  fi
}

assert_target_output() {
  source_path=$1
  output_path=$2
  output_label=$3

  [ -s "$output_path" ] || fail "$output_label does not exist"
  output_size=$(stat -f '%z' "$output_path")
  [ "$output_size" -le "$TARGET_SIZE_BYTES" ] ||
    fail "$output_label exceeds the exact 256 KiB target"
  [ "$(image_topology "$output_path")" = "$(image_topology "$source_path")" ] ||
    fail "$output_label changed format, dimensions, or frame count"
}

JPEG_RESULT="$SMOKE_DIRECTORY/jpeg-result.json"
run_request "$JPEG_SOURCE" "$OUTPUT_DIRECTORY" -target-size "$TARGET_SIZE_BYTES" \
  image-target-jpeg "$JPEG_RESULT" null
JPEG_OUTPUT=$(success_output "$JPEG_RESULT")
assert_target_output "$JPEG_SOURCE" "$JPEG_OUTPUT" "JPEG target output"
assert_sources_unchanged

# Repeating the same filename request must select a numbered no-clobber path and
# leave the first committed output byte-for-byte intact.
JPEG_OUTPUT_HASH=$(/usr/bin/shasum -a 256 "$JPEG_OUTPUT" | /usr/bin/awk '{print $1}')
KEEP_BOTH_RESULT="$SMOKE_DIRECTORY/jpeg-keep-both-result.json"
run_request "$JPEG_SOURCE" "$OUTPUT_DIRECTORY" -target-size "$TARGET_SIZE_BYTES" \
  image-target-jpeg-keep-both "$KEEP_BOTH_RESULT" null
KEEP_BOTH_OUTPUT=$(success_output "$KEEP_BOTH_RESULT")
[ "$KEEP_BOTH_OUTPUT" != "$JPEG_OUTPUT" ] || fail "keep-both reused the first output path"
assert_target_output "$JPEG_SOURCE" "$KEEP_BOTH_OUTPUT" "JPEG keep-both output"
[ "$(/usr/bin/shasum -a 256 "$JPEG_OUTPUT" | /usr/bin/awk '{print $1}')" = "$JPEG_OUTPUT_HASH" ] ||
  fail "keep-both overwrote the first JPEG output"
assert_sources_unchanged

WEBP_RESULT="$SMOKE_DIRECTORY/webp-result.json"
run_request "$WEBP_SOURCE" "$OUTPUT_DIRECTORY" -target-size "$TARGET_SIZE_BYTES" \
  image-target-webp "$WEBP_RESULT" null
WEBP_OUTPUT=$(success_output "$WEBP_RESULT")
assert_target_output "$WEBP_SOURCE" "$WEBP_OUTPUT" "WebP target output"
assert_sources_unchanged

# The 2400 x 1600 seeded-noise fixture remains above 16 KiB even at the lowest
# allowed quality. The API must expose that outcome as the dedicated typed error.
UNREACHABLE_RESULT="$SMOKE_DIRECTORY/unreachable-result.json"
run_request "$UNREACHABLE_SOURCE" "$UNREACHABLE_OUTPUT_DIRECTORY" -unreachable \
  "$UNREACHABLE_TARGET_BYTES" image-target-unreachable "$UNREACHABLE_RESULT" null
assert_error "$UNREACHABLE_RESULT" TargetSizeUnreachable
reported_target=$(/usr/bin/plutil -extract Err.detail.targetBytes raw -o - \
  "$UNREACHABLE_RESULT" 2>/dev/null || true)
reported_smallest=$(/usr/bin/plutil -extract Err.detail.smallestBytes raw -o - \
  "$UNREACHABLE_RESULT" 2>/dev/null || true)
[ "$reported_target" = "$UNREACHABLE_TARGET_BYTES" ] ||
  fail "the unreachable error reported the wrong byte target"
case "$reported_smallest" in
  '' | *[!0-9]*) fail "the unreachable error omitted its smallest candidate size" ;;
esac
[ "$reported_smallest" -gt "$UNREACHABLE_TARGET_BYTES" ] ||
  fail "the unreachable error reported a candidate within the target"
[ -z "$(find "$UNREACHABLE_OUTPUT_DIRECTORY" -mindepth 1 -print -quit)" ] ||
  fail "the unreachable job left an output or hidden partial"
assert_sources_unchanged

# Cancel after two real target attempts have completed but before a candidate
# can be committed. The debug listener scopes cancellation to this exact job ID.
CANCEL_RESULT="$SMOKE_DIRECTORY/cancel-result.json"
run_request "$UNREACHABLE_SOURCE" "$CANCEL_OUTPUT_DIRECTORY" -cancelled \
  "$UNREACHABLE_TARGET_BYTES" image-target-cancel "$CANCEL_RESULT" 15
assert_error "$CANCEL_RESULT" Cancelled
[ -z "$(find "$CANCEL_OUTPUT_DIRECTORY" -mindepth 1 -print -quit)" ] ||
  fail "the cancelled job left an output or hidden partial"
assert_sources_unchanged

sleep 1
if pgrep -f "[m]agick.*$(basename "$UNREACHABLE_SOURCE")" >/dev/null 2>&1; then
  fail "the cancelled job left an ImageMagick child running"
fi
PARTIAL=$(find "$SMOKE_DIRECTORY" \
  \( -name '.convertkit-*' -o -name 'convertkit-image-size-*' \) -print -quit)
[ -z "$PARTIAL" ] || fail "target-size image work left a hidden partial or workspace"

JPEG_SIZE=$(stat -f '%z' "$JPEG_OUTPUT")
KEEP_BOTH_SIZE=$(stat -f '%z' "$KEEP_BOTH_OUTPUT")
WEBP_SIZE=$(stat -f '%z' "$WEBP_OUTPUT")
printf 'Packaged target-size image smoke passed under a minimal launch PATH\n'
printf 'Exact target: %s bytes\n' "$TARGET_SIZE_BYTES"
printf 'JPEG: %s bytes; keep-both: %s bytes\n' "$JPEG_SIZE" "$KEEP_BOTH_SIZE"
printf 'WebP: %s bytes\n' "$WEBP_SIZE"
printf 'Typed unreachable target, cancellation, source preservation, and cleanup verified\n'
