#!/bin/sh

set -eu

APP_PATH=${1:-"src-tauri/target/debug/bundle/macos/ConvertKit.app"}
MINIMAL_PATH=/usr/bin:/bin:/usr/sbin:/sbin

fail() {
  printf 'GZIP extraction smoke failed: %s\n' "$1" >&2
  exit 1
}

[ -d "$APP_PATH" ] || fail "app bundle not found at $APP_PATH"
APP_PATH=$(cd "$(dirname "$APP_PATH")" && pwd -P)/$(basename "$APP_PATH")
APP_EXECUTABLE="$APP_PATH/Contents/MacOS/convertkit"
[ -x "$APP_EXECUTABLE" ] || fail "app executable is missing"

SMOKE_ROOT=${TMPDIR:-/tmp}
SMOKE_ROOT=${SMOKE_ROOT%/}
SMOKE_DIRECTORY=$(mktemp -d "$SMOKE_ROOT/convertkit-gzip-smoke.XXXXXX")
case "$SMOKE_DIRECTORY" in
  "$SMOKE_ROOT"/convertkit-gzip-smoke.*) ;;
  *) fail "temporary directory was created outside the expected location" ;;
esac

cleanup() {
  case "$SMOKE_DIRECTORY" in
    "$SMOKE_ROOT"/convertkit-gzip-smoke.*)
      [ ! -d "$SMOKE_DIRECTORY" ] || find "$SMOKE_DIRECTORY" -depth -delete
      ;;
    *) printf 'Refusing to clean unexpected directory: %s\n' "$SMOKE_DIRECTORY" >&2 ;;
  esac
}
trap cleanup EXIT HUP INT TERM

SOURCE_DIRECTORY="$SMOKE_DIRECTORY/sources"
OUTPUT_DIRECTORY="$SMOKE_DIRECTORY/output"
TRUNCATED_OUTPUT_DIRECTORY="$SMOKE_DIRECTORY/truncated-output"
CORRUPT_OUTPUT_DIRECTORY="$SMOKE_DIRECTORY/corrupt-output"
CANCEL_OUTPUT_DIRECTORY="$SMOKE_DIRECTORY/cancel-output"
RUNTIME_DIRECTORY="$SMOKE_DIRECTORY/runtime"
mkdir \
  "$SOURCE_DIRECTORY" \
  "$OUTPUT_DIRECTORY" \
  "$TRUNCATED_OUTPUT_DIRECTORY" \
  "$CORRUPT_OUTPUT_DIRECTORY" \
  "$CANCEL_OUTPUT_DIRECTORY" \
  "$RUNTIME_DIRECTORY"

REPORT_ARCHIVE="$SOURCE_DIRECTORY/report.csv.gz"
TRUNCATED_ARCHIVE="$SOURCE_DIRECTORY/truncated.csv.gz"
CORRUPT_ARCHIVE="$SOURCE_DIRECTORY/corrupt-crc.csv.gz"
CANCEL_ARCHIVE="$SOURCE_DIRECTORY/large.bin.gz"
EXPECTED_REPORT="$SOURCE_DIRECTORY/expected-report.csv"

# These mtime-zero fixtures contain `name,count\nalpha,3\nbeta,7\n`. One has
# a truncated footer and the other has a deliberately incorrect CRC32. Keeping
# the streams inline proves the packaged extractor does not use a system gzip.
REPORT_FIXTURE_BASE64='H4sIAAAAAAAC/8tLzE3VSc4vzSvhSswpyEjUMeZKSi1J1DHnAgB25pkiGgAAAA=='
TRUNCATED_FIXTURE_BASE64='H4sIAAAAAAAC/8tLzE3VSc4vzSvhSswpyEjUMeZKSi1J1DHnAgB25pki'
CORRUPT_FIXTURE_BASE64='H4sIAAAAAAAC/8tLzE3VSc4vzSvhSswpyEjUMeZKSi1J1DHnAgB35pkiGgAAAA=='

decode_fixture() {
  encoded=$1
  destination=$2
  printf '%s' "$encoded" | /usr/bin/base64 -D > "$destination"
  [ -s "$destination" ] || fail "could not decode $(basename "$destination")"
}

decode_fixture "$REPORT_FIXTURE_BASE64" "$REPORT_ARCHIVE"
decode_fixture "$TRUNCATED_FIXTURE_BASE64" "$TRUNCATED_ARCHIVE"
decode_fixture "$CORRUPT_FIXTURE_BASE64" "$CORRUPT_ARCHIVE"
printf 'name,count\nalpha,3\nbeta,7\n' > "$EXPECTED_REPORT"

# Build a deterministic 64 MiB all-zero GZIP without a gzip executable. DEFLATE
# stored blocks make fixture construction transparent while leaving enough I/O
# for the debug harness to exercise cooperative cancellation between reads.
CANCEL_BLOCK="$SOURCE_DIRECTORY/cancel-block.bin"
CANCEL_CHUNK="$SOURCE_DIRECTORY/cancel-chunk.bin"
printf '%s' 'H4sIAAAAAAACAw==' | /usr/bin/base64 -D > "$CANCEL_ARCHIVE"
printf '%s' 'AP//AAA=' | /usr/bin/base64 -D > "$CANCEL_BLOCK"
dd if=/dev/zero bs=65535 count=1 2>/dev/null >> "$CANCEL_BLOCK"
block_index=0
while [ "$block_index" -lt 16 ]; do
  /bin/cat "$CANCEL_BLOCK" >> "$CANCEL_CHUNK"
  block_index=$((block_index + 1))
done
chunk_index=0
while [ "$chunk_index" -lt 64 ]; do
  /bin/cat "$CANCEL_CHUNK" >> "$CANCEL_ARCHIVE"
  chunk_index=$((chunk_index + 1))
done
printf '%s' 'AQAE//s=' | /usr/bin/base64 -D >> "$CANCEL_ARCHIVE"
dd if=/dev/zero bs=1024 count=1 2>/dev/null >> "$CANCEL_ARCHIVE"
printf '%s' '7TDrsgAAAAQ=' | /usr/bin/base64 -D >> "$CANCEL_ARCHIVE"
/bin/rm -f "$CANCEL_BLOCK" "$CANCEL_CHUNK"

assert_fixture_hash() {
  fixture=$1
  expected=$2
  label=$3
  actual=$(/usr/bin/shasum -a 256 "$fixture" | /usr/bin/awk '{print $1}')
  [ "$actual" = "$expected" ] || fail "$label fixture hash changed"
}

REPORT_HASH=19818952aed98d641f81fbb25870bec2bbca176220389644fc938a7a8f03ed2b
TRUNCATED_HASH=1bcce551615c170718875de254ad9b9432bf0ebe0c6e85f90c4aca97a8b412cc
CORRUPT_HASH=49e79fde7d6f79b3bad6affeb56b4eb2b3c758aa72bf1ec64e7a3a9a50b7a45c
CANCEL_HASH=9ad6362fe3adb57d97a2b2046bdd9f9ceecb6f38bbc2d4ad4b5af3b42efd197a
assert_fixture_hash "$REPORT_ARCHIVE" "$REPORT_HASH" report
assert_fixture_hash "$TRUNCATED_ARCHIVE" "$TRUNCATED_HASH" truncated
assert_fixture_hash "$CORRUPT_ARCHIVE" "$CORRUPT_HASH" corrupt
assert_fixture_hash "$CANCEL_ARCHIVE" "$CANCEL_HASH" cancellation

write_request() {
  input_path=$1
  output_directory=$2
  suffix=$3
  job_id=$4
  result_path=$5
  cancel_after_ms=${6:-null}
  request_path="$SMOKE_DIRECTORY/$job_id-request.json"

  {
    printf '%s\n' '{' '  "request": {' '    "operation": "extractArchive",'
    printf '    "inputPath": "%s",\n' "$input_path"
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
  if [ "$actual_kind" != "$expected_kind" ]; then
    /bin/cat "$result_path" >&2
    fail "expected $expected_kind, got ${actual_kind:-no error}"
  fi
}

assert_empty_directory() {
  directory=$1
  label=$2
  [ -z "$(find "$directory" -mindepth 1 -print -quit)" ] ||
    fail "$label left a finished or partial output"
}

FIRST_RESULT="$SMOKE_DIRECTORY/report-first-result.json"
write_request "$REPORT_ARCHIVE" "$OUTPUT_DIRECTORY" -extracted gzip-valid-1 "$FIRST_RESULT"
FIRST_OUTPUT=$(success_output "$FIRST_RESULT")
EXPECTED_FIRST_OUTPUT="$OUTPUT_DIRECTORY/report-extracted.csv"
[ "$FIRST_OUTPUT" = "$EXPECTED_FIRST_OUTPUT" ] ||
  fail "report.csv.gz did not produce report-extracted.csv"
[ -f "$FIRST_OUTPUT" ] || fail "the reported GZIP output does not exist"
/usr/bin/cmp -s "$EXPECTED_REPORT" "$FIRST_OUTPUT" || fail "decompressed report bytes changed"
assert_fixture_hash "$REPORT_ARCHIVE" "$REPORT_HASH" 'report source after extraction'

# Repeating the same request must preserve both regular-file outputs.
SECOND_RESULT="$SMOKE_DIRECTORY/report-second-result.json"
write_request "$REPORT_ARCHIVE" "$OUTPUT_DIRECTORY" -extracted gzip-valid-2 "$SECOND_RESULT"
SECOND_OUTPUT=$(success_output "$SECOND_RESULT")
EXPECTED_SECOND_OUTPUT="$OUTPUT_DIRECTORY/report-extracted (1).csv"
[ "$SECOND_OUTPUT" = "$EXPECTED_SECOND_OUTPUT" ] ||
  fail "keep-both did not use report-extracted (1).csv"
/usr/bin/cmp -s "$EXPECTED_REPORT" "$SECOND_OUTPUT" || fail "numbered output bytes changed"
/usr/bin/cmp -s "$EXPECTED_REPORT" "$FIRST_OUTPUT" || fail "keep-both replaced the first output"
assert_fixture_hash "$REPORT_ARCHIVE" "$REPORT_HASH" 'report source after keep-both'

TRUNCATED_RESULT="$SMOKE_DIRECTORY/truncated-result.json"
write_request "$TRUNCATED_ARCHIVE" "$TRUNCATED_OUTPUT_DIRECTORY" -extracted \
  gzip-truncated "$TRUNCATED_RESULT"
assert_error "$TRUNCATED_RESULT" ProcessFailed
assert_empty_directory "$TRUNCATED_OUTPUT_DIRECTORY" 'truncated GZIP rejection'
assert_fixture_hash "$TRUNCATED_ARCHIVE" "$TRUNCATED_HASH" 'truncated source'

CORRUPT_RESULT="$SMOKE_DIRECTORY/corrupt-result.json"
write_request "$CORRUPT_ARCHIVE" "$CORRUPT_OUTPUT_DIRECTORY" -extracted \
  gzip-corrupt "$CORRUPT_RESULT"
assert_error "$CORRUPT_RESULT" ProcessFailed
assert_empty_directory "$CORRUPT_OUTPUT_DIRECTORY" 'CRC-corrupt GZIP rejection'
assert_fixture_hash "$CORRUPT_ARCHIVE" "$CORRUPT_HASH" 'corrupt source'

CANCEL_RESULT="$SMOKE_DIRECTORY/cancel-result.json"
write_request "$CANCEL_ARCHIVE" "$CANCEL_OUTPUT_DIRECTORY" -cancelled \
  gzip-cancel "$CANCEL_RESULT" 10
assert_error "$CANCEL_RESULT" Cancelled
assert_empty_directory "$CANCEL_OUTPUT_DIRECTORY" 'GZIP cancellation'
assert_fixture_hash "$CANCEL_ARCHIVE" "$CANCEL_HASH" 'cancellation source'

PARTIAL_PATH=$(find "$SMOKE_DIRECTORY" -name '.convertkit-*' -print -quit)
[ -z "$PARTIAL_PATH" ] || fail "a hidden extraction partial was left behind"

printf 'Packaged standalone GZIP extraction smoke passed\n'
printf 'Exact output: %s\n' "$FIRST_OUTPUT"
printf 'Keep-both output: %s\n' "$SECOND_OUTPUT"
printf 'Truncation, CRC, cancellation, source, and partial cleanup checks passed\n'
