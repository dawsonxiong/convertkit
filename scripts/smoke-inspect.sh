#!/bin/sh

set -eu

APP_PATH=${1:-"src-tauri/target/debug/bundle/macos/ConvertKit.app"}
MINIMAL_PATH=/usr/bin:/bin:/usr/sbin:/sbin

fail() {
  printf 'Inspect Files smoke failed: %s\n' "$1" >&2
  exit 1
}

[ -d "$APP_PATH" ] || fail "app bundle not found at $APP_PATH"
APP_PATH=$(cd "$(dirname "$APP_PATH")" && pwd -P)/$(basename "$APP_PATH")
APP_EXECUTABLE="$APP_PATH/Contents/MacOS/convertkit"
[ -x "$APP_EXECUTABLE" ] || fail "app executable is missing"

SMOKE_ROOT=${TMPDIR:-/tmp}
SMOKE_ROOT=${SMOKE_ROOT%/}
SMOKE_DIRECTORY=$(mktemp -d "$SMOKE_ROOT/convertkit-inspect-smoke.XXXXXX")
case "$SMOKE_DIRECTORY" in
  "$SMOKE_ROOT"/convertkit-inspect-smoke.*) ;;
  *) fail "temporary directory was created outside the expected location" ;;
esac

cleanup() {
  case "$SMOKE_DIRECTORY" in
    "$SMOKE_ROOT"/convertkit-inspect-smoke.*)
      [ ! -d "$SMOKE_DIRECTORY" ] || find "$SMOKE_DIRECTORY" -depth -delete
      ;;
    *) printf 'Refusing to clean unexpected directory: %s\n' "$SMOKE_DIRECTORY" >&2 ;;
  esac
}
trap cleanup EXIT HUP INT TERM

SOURCE_DIRECTORY="$SMOKE_DIRECTORY/source"
RUNTIME_DIRECTORY="$SMOKE_DIRECTORY/runtime"
HOME_DIRECTORY="$SMOKE_DIRECTORY/home"
mkdir "$SOURCE_DIRECTORY" "$RUNTIME_DIRECTORY" "$HOME_DIRECTORY"

SOURCE_FILE="$SOURCE_DIRECTORY/inspect-fixture.txt"
REPORT_FILE="$SMOKE_DIRECTORY/inspection-report.json"
RESULT_FILE="$SMOKE_DIRECTORY/inspect-result.json"
REQUEST_FILE="$SMOKE_DIRECTORY/inspect-request.json"
APP_LOG="$SMOKE_DIRECTORY/inspect.log"
printf 'ConvertKit inspect smoke\nline two,42\n' > "$SOURCE_FILE"

EXPECTED_MD5=54bc179d20f51262d64b1fd104630dec
EXPECTED_SHA1=99a5630419afe8ae8a87c728a33095aa25fee2ba
EXPECTED_SHA256=ccf23917ee7fd843550a0a6db6ba084985efdacd0254e0eb9171c397fd7040c2
SOURCE_HASH=$(/usr/bin/shasum -a 256 "$SOURCE_FILE" | /usr/bin/awk '{print $1}')
[ "$SOURCE_HASH" = "$EXPECTED_SHA256" ] || fail "fixture bytes changed"

{
  printf '%s\n' '{' '  "inspect": {'
  printf '    "inputPath": "%s",\n' "$SOURCE_FILE"
  printf '%s\n' '    "jobId": "inspect-smoke",'
  printf '    "reportPath": "%s",\n' "$REPORT_FILE"
  printf '%s\n' '    "reportFormat": "json"' '  },'
  printf '  "resultPath": "%s",\n' "$RESULT_FILE"
  printf '%s\n' \
    '  "cancelAfterMs": null,' \
    '  "cancelJobId": null' \
    '}'
} > "$REQUEST_FILE"

if ! PATH=$MINIMAL_PATH \
  HOME="$HOME_DIRECTORY" \
  TMPDIR="$RUNTIME_DIRECTORY" \
  CONVERTKIT_DEBUG_SMOKE_REQUEST="$REQUEST_FILE" \
  "$APP_EXECUTABLE" > "$APP_LOG" 2>&1; then
  /bin/cat "$APP_LOG" >&2
  fail "packaged app invocation failed"
fi
[ -f "$RESULT_FILE" ] || {
  /bin/cat "$APP_LOG" >&2
  fail "packaged app did not write a result"
}

extract_result() {
  key=$1
  /usr/bin/plutil -extract "Ok.$key" raw -o - "$RESULT_FILE" 2>/dev/null || {
    /bin/cat "$RESULT_FILE" >&2
    fail "result is missing Ok.$key"
  }
}

[ "$(extract_result file.path)" = "$SOURCE_FILE" ] || fail "reported source path changed"
[ "$(extract_result file.name)" = inspect-fixture.txt ] || fail "reported name is wrong"
[ "$(extract_result file.extension)" = txt ] || fail "reported extension is wrong"
[ "$(extract_result file.format)" = txt ] || fail "reported format is wrong"
[ "$(extract_result file.category)" = document ] || fail "reported category is wrong"
[ "$(extract_result file.size)" = 37 ] || fail "reported byte size is wrong"
[ "$(extract_result checksums.md5)" = "$EXPECTED_MD5" ] || fail "MD5 is wrong"
[ "$(extract_result checksums.sha1)" = "$EXPECTED_SHA1" ] || fail "SHA-1 is wrong"
[ "$(extract_result checksums.sha256)" = "$EXPECTED_SHA256" ] || fail "SHA-256 is wrong"
[ "$(extract_result reportPath)" = "$REPORT_FILE" ] || fail "reported report path is wrong"

[ -s "$REPORT_FILE" ] || fail "inspection report was not created"
report_value() {
  key=$1
  /usr/bin/plutil -extract "$key" raw -o - "$REPORT_FILE" 2>/dev/null ||
    fail "report is missing $key"
}
[ "$(report_value schemaVersion)" = 1 ] || fail "report schema version is wrong"
[ "$(report_value generatedBy)" = ConvertKit ] || fail "report generator is wrong"
[ "$(report_value files.0.file.path)" = "$SOURCE_FILE" ] || fail "report source path is wrong"
[ "$(report_value files.0.file.size)" = 37 ] || fail "report byte size is wrong"
[ "$(report_value files.0.checksums.md5)" = "$EXPECTED_MD5" ] || fail "report MD5 is wrong"
[ "$(report_value files.0.checksums.sha1)" = "$EXPECTED_SHA1" ] || fail "report SHA-1 is wrong"
[ "$(report_value files.0.checksums.sha256)" = "$EXPECTED_SHA256" ] ||
  fail "report SHA-256 is wrong"

AFTER_HASH=$(/usr/bin/shasum -a 256 "$SOURCE_FILE" | /usr/bin/awk '{print $1}')
[ "$AFTER_HASH" = "$SOURCE_HASH" ] || fail "inspection modified the source"
PARTIAL=$(find "$SMOKE_DIRECTORY" -name '.convertkit-*' -print -quit)
[ -z "$PARTIAL" ] || fail "inspection left a hidden partial"

printf 'Packaged Inspect Files smoke passed\n'
printf 'Metadata, three checksums, exact JSON report fields, and source preservation verified\n'
