#!/bin/sh

set -eu

APP_PATH=${1:-"src-tauri/target/debug/bundle/macos/ConvertKit.app"}
MINIMAL_PATH=/usr/bin:/bin:/usr/sbin:/sbin

fail() {
  printf '7Z creation smoke failed: %s\n' "$1" >&2
  exit 1
}

[ -d "$APP_PATH" ] || fail "app bundle not found at $APP_PATH"
APP_PATH=$(cd "$(dirname "$APP_PATH")" && pwd -P)/$(basename "$APP_PATH")
APP_EXECUTABLE="$APP_PATH/Contents/MacOS/convertkit"
[ -x "$APP_EXECUTABLE" ] || fail "app executable is missing"

SMOKE_ROOT=${TMPDIR:-/tmp}
SMOKE_ROOT=${SMOKE_ROOT%/}
SMOKE_DIRECTORY=$(mktemp -d "$SMOKE_ROOT/convertkit-seven-create-smoke.XXXXXX")
case "$SMOKE_DIRECTORY" in
  "$SMOKE_ROOT"/convertkit-seven-create-smoke.*) ;;
  *) fail "temporary directory was created outside the expected location" ;;
esac

cleanup() {
  case "$SMOKE_DIRECTORY" in
    "$SMOKE_ROOT"/convertkit-seven-create-smoke.*)
      [ ! -d "$SMOKE_DIRECTORY" ] || find "$SMOKE_DIRECTORY" -depth -delete
      ;;
    *) printf 'Refusing to clean unexpected directory: %s\n' "$SMOKE_DIRECTORY" >&2 ;;
  esac
}
trap cleanup EXIT HUP INT TERM

SOURCE_DIRECTORY="$SMOKE_DIRECTORY/sources"
OUTPUT_DIRECTORY="$SMOKE_DIRECTORY/output"
EXTRACT_DIRECTORY="$SMOKE_DIRECTORY/extracted"
CANCEL_DIRECTORY="$SMOKE_DIRECTORY/cancel-output"
RUNTIME_DIRECTORY="$SMOKE_DIRECTORY/runtime"
ARCHIVE_PASSWORD='correct horse battery staple'
mkdir \
  "$SOURCE_DIRECTORY" \
  "$OUTPUT_DIRECTORY" \
  "$EXTRACT_DIRECTORY" \
  "$CANCEL_DIRECTORY" \
  "$RUNTIME_DIRECTORY"

FIRST_SOURCE="$SOURCE_DIRECTORY/first.txt"
SECOND_SOURCE="$SOURCE_DIRECTORY/second.bin"
CANCEL_SOURCE="$SOURCE_DIRECTORY/cancel.bin"
printf 'ConvertKit built-in 7Z creation smoke\n' > "$FIRST_SOURCE"
printf '%s' 'AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJygpKissLS4vMDEyMzQ1Njc4Pz4=' |
  /usr/bin/base64 -D > "$SECOND_SOURCE"
/bin/dd if=/dev/urandom of="$CANCEL_SOURCE" bs=1048576 count=64 2>/dev/null

FIRST_HASH=$(/usr/bin/shasum -a 256 "$FIRST_SOURCE" | /usr/bin/awk '{print $1}')
SECOND_HASH=$(/usr/bin/shasum -a 256 "$SECOND_SOURCE" | /usr/bin/awk '{print $1}')
CANCEL_HASH=$(/usr/bin/shasum -a 256 "$CANCEL_SOURCE" | /usr/bin/awk '{print $1}')

run_request() {
  request_path=$1
  result_path=$2
  job_id=$3
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

write_create_request() {
  job_id=$1
  result_path=$2
  cancel_after_ms=${3:-null}
  request_path="$SMOKE_DIRECTORY/$job_id-request.json"
  {
    printf '%s\n' '{' '  "request": {' '    "operation": "createArchive",' '    "entries": ['
    if [ "$job_id" = seven-create-cancel ]; then
      printf '      { "inputPath": "%s", "archivePath": "cancel.bin", "folderDerived": false }\n' "$CANCEL_SOURCE"
    else
      printf '      { "inputPath": "%s", "archivePath": "nested/first.txt", "folderDerived": true },\n' "$FIRST_SOURCE"
      printf '      { "inputPath": "%s", "archivePath": "second.bin", "folderDerived": false }\n' "$SECOND_SOURCE"
    fi
    printf '%s\n' '    ],' '    "format": "sevenZ",'
    printf '    "password": "%s",\n' "$ARCHIVE_PASSWORD"
    printf '    "jobId": "%s",\n' "$job_id"
    printf '%s\n' '    "outputOptions": {'
    if [ "$job_id" = seven-create-cancel ]; then
      printf '      "directory": "%s",\n' "$CANCEL_DIRECTORY"
      printf '%s\n' '      "suffix": "-cancelled"'
    else
      printf '      "directory": "%s",\n' "$OUTPUT_DIRECTORY"
      printf '%s\n' '      "suffix": "-seven"'
    fi
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
  run_request "$request_path" "$result_path" "$job_id"
}

write_extract_request() {
  archive_path=$1
  result_path=$2
  job_id=$3
  password_value=$4
  request_path="$SMOKE_DIRECTORY/$job_id-request.json"
  {
    printf '%s\n' '{' '  "request": {' '    "operation": "extractArchive",'
    printf '    "inputPath": "%s",\n' "$archive_path"
    if [ "$password_value" != __omit__ ]; then
      printf '    "password": "%s",\n' "$password_value"
    fi
    printf '    "jobId": "%s",\n' "$job_id"
    printf '%s\n' '    "outputOptions": {'
    printf '      "directory": "%s",\n' "$EXTRACT_DIRECTORY"
    printf '%s\n' '      "suffix": "-verified"' '    }' '  },'
    printf '  "resultPath": "%s",\n' "$result_path"
    printf '%s\n' '  "cancelAfterMs": null,' '  "cancelJobId": null' '}'
  } > "$request_path"
  run_request "$request_path" "$result_path" "$job_id"
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

FIRST_RESULT="$SMOKE_DIRECTORY/create-first-result.json"
write_create_request seven-create-first "$FIRST_RESULT"
FIRST_ARCHIVE=$(success_output "$FIRST_RESULT")
[ -f "$FIRST_ARCHIVE" ] || fail "the reported 7Z archive does not exist"
case "$FIRST_ARCHIVE" in
  *.7z) ;;
  *) fail "the created archive does not use the .7z extension" ;;
esac

# Repeating the same request must retain both archives with distinct names.
SECOND_RESULT="$SMOKE_DIRECTORY/create-second-result.json"
write_create_request seven-create-second "$SECOND_RESULT"
SECOND_ARCHIVE=$(success_output "$SECOND_RESULT")
[ -f "$SECOND_ARCHIVE" ] || fail "the keep-both 7Z archive does not exist"
[ "$SECOND_ARCHIVE" != "$FIRST_ARCHIVE" ] || fail "keep-both reused the first archive path"
[ -f "$FIRST_ARCHIVE" ] || fail "keep-both removed the first archive"

# Encrypted headers must reject omitted and incorrect passwords without leaving output.
MISSING_RESULT="$SMOKE_DIRECTORY/extract-missing-result.json"
write_extract_request "$FIRST_ARCHIVE" "$MISSING_RESULT" seven-create-missing __omit__
assert_error "$MISSING_RESULT" ArchivePasswordRequired

WRONG_RESULT="$SMOKE_DIRECTORY/extract-wrong-result.json"
write_extract_request "$FIRST_ARCHIVE" "$WRONG_RESULT" seven-create-wrong 'definitely wrong'
assert_error "$WRONG_RESULT" IncorrectArchivePassword
[ -z "$(find "$EXTRACT_DIRECTORY" -mindepth 1 -print -quit)" ] ||
  fail "password rejection left an output or partial"

# Use the packaged reader to independently validate the packaged writer with the correct password.
EXTRACT_RESULT="$SMOKE_DIRECTORY/extract-result.json"
write_extract_request "$FIRST_ARCHIVE" "$EXTRACT_RESULT" seven-create-extract "$ARCHIVE_PASSWORD"
EXTRACTED_PATH=$(success_output "$EXTRACT_RESULT")
[ -d "$EXTRACTED_PATH" ] || fail "the verified extraction folder does not exist"
/usr/bin/cmp -s "$FIRST_SOURCE" "$EXTRACTED_PATH/nested/first.txt" || fail "nested text contents changed"
/usr/bin/cmp -s "$SECOND_SOURCE" "$EXTRACTED_PATH/second.bin" || fail "binary contents changed"
file_count=$(find "$EXTRACTED_PATH" -type f | /usr/bin/wc -l | /usr/bin/tr -d ' ')
[ "$file_count" -eq 2 ] || fail "the created archive extracted an unexpected file count"

# A large incompressible source keeps the writer active long enough to exercise
# cancellation inside its bounded streaming reader.
CANCEL_RESULT="$SMOKE_DIRECTORY/cancel-result.json"
write_create_request seven-create-cancel "$CANCEL_RESULT" 10
assert_error "$CANCEL_RESULT" Cancelled
[ -z "$(find "$CANCEL_DIRECTORY" -mindepth 1 -print -quit)" ] || fail "cancellation left an output or partial"

[ "$FIRST_HASH" = "$(/usr/bin/shasum -a 256 "$FIRST_SOURCE" | /usr/bin/awk '{print $1}')" ] || fail "first source changed"
[ "$SECOND_HASH" = "$(/usr/bin/shasum -a 256 "$SECOND_SOURCE" | /usr/bin/awk '{print $1}')" ] || fail "second source changed"
[ "$CANCEL_HASH" = "$(/usr/bin/shasum -a 256 "$CANCEL_SOURCE" | /usr/bin/awk '{print $1}')" ] || fail "cancellation source changed"

PARTIAL_PATH=$(find "$SMOKE_DIRECTORY" -name '.convertkit-*' -print -quit)
[ -z "$PARTIAL_PATH" ] || fail "a hidden 7Z partial was left behind"

printf 'Packaged 7Z creation smoke passed\n'
printf 'Archive: %s\n' "$FIRST_ARCHIVE"
printf 'Keep-both archive: %s\n' "$SECOND_ARCHIVE"
printf 'Encrypted header, wrong-password, round-trip, cancellation, source, and partial cleanup checks passed\n'
