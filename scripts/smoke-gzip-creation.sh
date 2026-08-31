#!/bin/sh

set -eu

APP_PATH=${1:-"src-tauri/target/debug/bundle/macos/ConvertKit.app"}
MINIMAL_PATH=/usr/bin:/bin:/usr/sbin:/sbin

fail() {
  printf 'GZIP creation smoke failed: %s\n' "$1" >&2
  exit 1
}

[ -d "$APP_PATH" ] || fail "app bundle not found at $APP_PATH"
APP_PATH=$(cd "$(dirname "$APP_PATH")" && pwd -P)/$(basename "$APP_PATH")
APP_EXECUTABLE="$APP_PATH/Contents/MacOS/convertkit"
[ -x "$APP_EXECUTABLE" ] || fail "app executable is missing"

SMOKE_ROOT=${TMPDIR:-/tmp}
SMOKE_ROOT=${SMOKE_ROOT%/}
SMOKE_DIRECTORY=$(mktemp -d "$SMOKE_ROOT/convertkit-gzip-create-smoke.XXXXXX")
case "$SMOKE_DIRECTORY" in
  "$SMOKE_ROOT"/convertkit-gzip-create-smoke.*) ;;
  *) fail "temporary directory was created outside the expected location" ;;
esac

cleanup() {
  case "$SMOKE_DIRECTORY" in
    "$SMOKE_ROOT"/convertkit-gzip-create-smoke.*)
      [ ! -d "$SMOKE_DIRECTORY" ] || find "$SMOKE_DIRECTORY" -depth -delete
      ;;
    *) printf 'Refusing to clean unexpected directory: %s\n' "$SMOKE_DIRECTORY" >&2 ;;
  esac
}
trap cleanup EXIT HUP INT TERM

SOURCE_DIRECTORY="$SMOKE_DIRECTORY/sources"
OUTPUT_DIRECTORY="$SMOKE_DIRECTORY/output"
INVALID_OUTPUT_DIRECTORY="$SMOKE_DIRECTORY/invalid-output"
CANCEL_OUTPUT_DIRECTORY="$SMOKE_DIRECTORY/cancel-output"
RUNTIME_DIRECTORY="$SMOKE_DIRECTORY/runtime"
mkdir \
  "$SOURCE_DIRECTORY" \
  "$OUTPUT_DIRECTORY" \
  "$INVALID_OUTPUT_DIRECTORY" \
  "$CANCEL_OUTPUT_DIRECTORY" \
  "$RUNTIME_DIRECTORY"

REPORT_SOURCE="$SOURCE_DIRECTORY/report.csv"
SECOND_SOURCE="$SOURCE_DIRECTORY/second.txt"
CANCEL_SOURCE="$SOURCE_DIRECTORY/cancel.bin"
printf 'name,count\nalpha,3\nbeta,7\n' > "$REPORT_SOURCE"
printf 'second input\n' > "$SECOND_SOURCE"
dd if=/dev/zero of="$CANCEL_SOURCE" bs=1048576 count=64 2>/dev/null

REPORT_HASH=$(/usr/bin/shasum -a 256 "$REPORT_SOURCE" | /usr/bin/awk '{print $1}')
SECOND_HASH=$(/usr/bin/shasum -a 256 "$SECOND_SOURCE" | /usr/bin/awk '{print $1}')
CANCEL_HASH=$(/usr/bin/shasum -a 256 "$CANCEL_SOURCE" | /usr/bin/awk '{print $1}')

write_request() {
  input_path=$1
  output_directory=$2
  suffix=$3
  job_id=$4
  result_path=$5
  cancel_after_ms=${6:-null}
  second_input=${7:-}
  request_path="$SMOKE_DIRECTORY/$job_id-request.json"

  {
    printf '%s\n' '{' '  "request": {' '    "operation": "createArchive",' '    "entries": ['
    if [ -n "$second_input" ]; then
      printf '      { "inputPath": "%s", "archivePath": "%s", "folderDerived": false },\n' \
        "$input_path" "$(basename "$input_path")"
      printf '      { "inputPath": "%s", "archivePath": "%s", "folderDerived": false }\n' \
        "$second_input" "$(basename "$second_input")"
    else
      printf '      { "inputPath": "%s", "archivePath": "%s", "folderDerived": false }\n' \
        "$input_path" "$(basename "$input_path")"
    fi
    printf '%s\n' '    ],' '    "format": "gzip",'
    printf '    "jobId": "%s",\n' "$job_id"
    printf '%s\n' '    "outputOptions": {'
    printf '      "directory": "%s",\n' "$output_directory"
    printf '      "suffix": "%s"\n' "$suffix"
    printf '%s\n' '    }' '  },'
    printf '  "resultPath": "%s",\n' "$result_path"
    printf '  "cancelAfterMs": %s,\n' "$cancel_after_ms"
    if [ "$cancel_after_ms" = null ]; then
      printf '%s\n' '  "cancelJobId": null'
    else
      printf '  "cancelJobId": "%s"\n' "$job_id"
    fi
    printf '%s\n' '}'
  } > "$request_path"

  app_log="$SMOKE_DIRECTORY/$job_id.log"
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
  if /usr/bin/plutil -extract Ok.output_path raw -o - "$result_path" >/dev/null 2>&1; then
    /bin/cat "$result_path" >&2
    fail "a request expected to fail committed an output"
  fi
  actual_kind=$(/usr/bin/plutil -extract Err.kind raw -o - "$result_path" 2>/dev/null || true)
  [ "$actual_kind" = "$expected_kind" ] || {
    /bin/cat "$result_path" >&2
    fail "expected $expected_kind, got ${actual_kind:-no error}"
  }
}

assert_empty_directory() {
  directory=$1
  label=$2
  [ -z "$(find "$directory" -mindepth 1 -print -quit)" ] ||
    fail "$label left a finished or partial output"
}

FIRST_RESULT="$SMOKE_DIRECTORY/first-result.json"
write_request "$REPORT_SOURCE" "$OUTPUT_DIRECTORY" -compressed gzip-create-1 "$FIRST_RESULT"
FIRST_OUTPUT=$(success_output "$FIRST_RESULT")
EXPECTED_FIRST_OUTPUT="$OUTPUT_DIRECTORY/report-compressed.csv.gz"
[ "$FIRST_OUTPUT" = "$EXPECTED_FIRST_OUTPUT" ] ||
  fail "report.csv did not produce report-compressed.csv.gz"
[ -f "$FIRST_OUTPUT" ] || fail "the reported GZIP output does not exist"
/usr/bin/gzip -dc "$FIRST_OUTPUT" | /usr/bin/cmp -s "$REPORT_SOURCE" - ||
  fail "independent GZIP decode did not match the source"
[ "$REPORT_HASH" = "$(/usr/bin/shasum -a 256 "$REPORT_SOURCE" | /usr/bin/awk '{print $1}')" ] ||
  fail "the report source changed"

SECOND_RESULT="$SMOKE_DIRECTORY/second-result.json"
write_request "$REPORT_SOURCE" "$OUTPUT_DIRECTORY" -compressed gzip-create-2 "$SECOND_RESULT"
SECOND_OUTPUT=$(success_output "$SECOND_RESULT")
EXPECTED_SECOND_OUTPUT="$OUTPUT_DIRECTORY/report-compressed (1).csv.gz"
[ "$SECOND_OUTPUT" = "$EXPECTED_SECOND_OUTPUT" ] ||
  fail "keep-both did not use report-compressed (1).csv.gz"
/usr/bin/gzip -dc "$SECOND_OUTPUT" | /usr/bin/cmp -s "$REPORT_SOURCE" - ||
  fail "numbered GZIP output did not match the source"
/usr/bin/gzip -dc "$FIRST_OUTPUT" | /usr/bin/cmp -s "$REPORT_SOURCE" - ||
  fail "keep-both replaced the first output"

INVALID_RESULT="$SMOKE_DIRECTORY/invalid-result.json"
write_request "$REPORT_SOURCE" "$INVALID_OUTPUT_DIRECTORY" -invalid gzip-create-invalid \
  "$INVALID_RESULT" null "$SECOND_SOURCE"
assert_error "$INVALID_RESULT" UnsupportedConversion
assert_empty_directory "$INVALID_OUTPUT_DIRECTORY" 'multi-file GZIP rejection'
[ "$SECOND_HASH" = "$(/usr/bin/shasum -a 256 "$SECOND_SOURCE" | /usr/bin/awk '{print $1}')" ] ||
  fail "the second source changed"

CANCEL_RESULT="$SMOKE_DIRECTORY/cancel-result.json"
write_request "$CANCEL_SOURCE" "$CANCEL_OUTPUT_DIRECTORY" -cancelled gzip-create-cancel \
  "$CANCEL_RESULT" 1
assert_error "$CANCEL_RESULT" Cancelled
assert_empty_directory "$CANCEL_OUTPUT_DIRECTORY" 'GZIP creation cancellation'
[ "$CANCEL_HASH" = "$(/usr/bin/shasum -a 256 "$CANCEL_SOURCE" | /usr/bin/awk '{print $1}')" ] ||
  fail "the cancellation source changed"

PARTIAL_PATH=$(find "$SMOKE_DIRECTORY" -name '.convertkit-*' -print -quit)
[ -z "$PARTIAL_PATH" ] || fail "a hidden GZIP creation partial was left behind"

printf 'Packaged standalone GZIP creation smoke passed\n'
printf 'Exact output: %s\n' "$FIRST_OUTPUT"
printf 'Keep-both output: %s\n' "$SECOND_OUTPUT"
printf 'Round-trip, invalid input, cancellation, source, and partial cleanup checks passed\n'
