#!/bin/sh

set -eu

APP_PATH=${1:-"src-tauri/target/debug/bundle/macos/ConvertKit.app"}
MINIMAL_PATH=/usr/bin:/bin:/usr/sbin:/sbin

fail() {
  printf 'Batch Rename smoke failed: %s\n' "$1" >&2
  exit 1
}

[ -d "$APP_PATH" ] || fail "app bundle not found at $APP_PATH"
APP_PATH=$(cd "$(dirname "$APP_PATH")" && pwd -P)/$(basename "$APP_PATH")
APP_EXECUTABLE="$APP_PATH/Contents/MacOS/convertkit"
[ -x "$APP_EXECUTABLE" ] || fail "app executable is missing"

SMOKE_ROOT=${TMPDIR:-/tmp}
SMOKE_ROOT=${SMOKE_ROOT%/}
SMOKE_DIRECTORY=$(mktemp -d "$SMOKE_ROOT/convertkit-rename-smoke.XXXXXX")
case "$SMOKE_DIRECTORY" in
  "$SMOKE_ROOT"/convertkit-rename-smoke.*) ;;
  *) fail "temporary directory was created outside the expected location" ;;
esac

cleanup() {
  /bin/chmod u+rwx "$SMOKE_DIRECTORY/rollback-blocked" 2>/dev/null || true
  case "$SMOKE_DIRECTORY" in
    "$SMOKE_ROOT"/convertkit-rename-smoke.*)
      [ ! -d "$SMOKE_DIRECTORY" ] || find "$SMOKE_DIRECTORY" -depth -delete
      ;;
    *) printf 'Refusing to clean unexpected directory: %s\n' "$SMOKE_DIRECTORY" >&2 ;;
  esac
}
trap cleanup EXIT HUP INT TERM

SOURCE_DIRECTORY="$SMOKE_DIRECTORY/sources"
RUNTIME_DIRECTORY="$SMOKE_DIRECTORY/runtime"
HOME_DIRECTORY="$SMOKE_DIRECTORY/home"
mkdir "$SOURCE_DIRECTORY" "$RUNTIME_DIRECTORY" "$HOME_DIRECTORY"

FIRST="$SOURCE_DIRECTORY/first.txt"
SECOND="$SOURCE_DIRECTORY/second.md"
COLLISION_SOURCE="$SOURCE_DIRECTORY/collision-source.txt"
COLLISION_TARGET="$SOURCE_DIRECTORY/occupied.txt"
EXTENSION_SOURCE="$SOURCE_DIRECTORY/extension-source.txt"
printf 'first rename payload\n' > "$FIRST"
printf '# second rename payload\n' > "$SECOND"
printf 'collision source payload\n' > "$COLLISION_SOURCE"
printf 'occupied target payload\n' > "$COLLISION_TARGET"
printf 'extension source payload\n' > "$EXTENSION_SOURCE"

FIRST_HASH=$(/usr/bin/shasum -a 256 "$FIRST" | /usr/bin/awk '{print $1}')
SECOND_HASH=$(/usr/bin/shasum -a 256 "$SECOND" | /usr/bin/awk '{print $1}')
COLLISION_SOURCE_HASH=$(/usr/bin/shasum -a 256 "$COLLISION_SOURCE" | /usr/bin/awk '{print $1}')
COLLISION_TARGET_HASH=$(/usr/bin/shasum -a 256 "$COLLISION_TARGET" | /usr/bin/awk '{print $1}')
EXTENSION_HASH=$(/usr/bin/shasum -a 256 "$EXTENSION_SOURCE" | /usr/bin/awk '{print $1}')

run_request() {
  request_path=$1
  result_path=$2
  label=$3
  app_log="$SMOKE_DIRECTORY/$label.log"
  if ! PATH=$MINIMAL_PATH \
    HOME="$HOME_DIRECTORY" \
    TMPDIR="$RUNTIME_DIRECTORY" \
    CONVERTKIT_DEBUG_SMOKE_REQUEST="$request_path" \
    "$APP_EXECUTABLE" > "$app_log" 2>&1; then
    /bin/cat "$app_log" >&2
    fail "$label did not complete through the packaged app"
  fi
  [ -f "$result_path" ] || {
    /bin/cat "$app_log" >&2
    fail "$label did not write a result"
  }
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

SUCCESS_REQUEST="$SMOKE_DIRECTORY/success-request.json"
SUCCESS_RESULT="$SMOKE_DIRECTORY/success-result.json"
FIRST_RENAMED="$SOURCE_DIRECTORY/project-alpha.txt"
SECOND_RENAMED="$SOURCE_DIRECTORY/project-beta.md"
{
  printf '%s\n' '{' '  "request": {' '    "operation": "rename",' '    "items": ['
  printf '      { "inputPath": "%s", "outputName": "project-alpha.txt" },\n' "$FIRST"
  printf '      { "inputPath": "%s", "outputName": "project-beta.md" }\n' "$SECOND"
  printf '%s\n' '    ],' '    "jobId": "rename-success"' '  },'
  printf '  "resultPath": "%s",\n' "$SUCCESS_RESULT"
  printf '%s\n' \
    '  "cancelAfterMs": null,' \
    '  "cancelJobId": null' \
    '}'
} > "$SUCCESS_REQUEST"
run_request "$SUCCESS_REQUEST" "$SUCCESS_RESULT" rename-success

SUCCESS_FIRST=$(/usr/bin/plutil -extract Ok.output_paths.0 raw -o - "$SUCCESS_RESULT" 2>/dev/null || true)
SUCCESS_SECOND=$(/usr/bin/plutil -extract Ok.output_paths.1 raw -o - "$SUCCESS_RESULT" 2>/dev/null || true)
[ "$SUCCESS_FIRST" = "$FIRST_RENAMED" ] || fail "first reported rename path is wrong"
[ "$SUCCESS_SECOND" = "$SECOND_RENAMED" ] || fail "second reported rename path is wrong"
[ ! -e "$FIRST" ] && [ ! -e "$SECOND" ] || fail "original names remained after commit"
[ -f "$FIRST_RENAMED" ] && [ -f "$SECOND_RENAMED" ] || fail "renamed files are missing"
[ "${FIRST_RENAMED##*.}" = txt ] && [ "${SECOND_RENAMED##*.}" = md ] ||
  fail "extensions were not preserved"
[ "$(/usr/bin/shasum -a 256 "$FIRST_RENAMED" | /usr/bin/awk '{print $1}')" = "$FIRST_HASH" ] ||
  fail "first bytes changed during rename"
[ "$(/usr/bin/shasum -a 256 "$SECOND_RENAMED" | /usr/bin/awk '{print $1}')" = "$SECOND_HASH" ] ||
  fail "second bytes changed during rename"

MANIFEST=$(/usr/bin/plutil -extract Ok.undo_manifest raw -o - "$SUCCESS_RESULT" 2>/dev/null || true)
[ -n "$MANIFEST" ] && [ -f "$MANIFEST" ] || fail "rename did not create a managed undo record"
MANIFEST_DIRECTORY=$(dirname "$MANIFEST")

UNDO_REQUEST="$SMOKE_DIRECTORY/undo-request.json"
UNDO_RESULT="$SMOKE_DIRECTORY/undo-result.json"
{
  printf '%s\n' '{'
  printf '  "undoRenameManifestPath": "%s",\n' "$MANIFEST"
  printf '  "resultPath": "%s",\n' "$UNDO_RESULT"
  printf '%s\n' \
    '  "cancelAfterMs": null,' \
    '  "cancelJobId": null' \
    '}'
} > "$UNDO_REQUEST"
run_request "$UNDO_REQUEST" "$UNDO_RESULT" rename-undo
[ "$(/usr/bin/plutil -extract Ok.0 raw -o - "$UNDO_RESULT" 2>/dev/null || true)" = "$FIRST" ] ||
  fail "undo did not report the first original path"
[ "$(/usr/bin/plutil -extract Ok.1 raw -o - "$UNDO_RESULT" 2>/dev/null || true)" = "$SECOND" ] ||
  fail "undo did not report the second original path"
[ -f "$FIRST" ] && [ -f "$SECOND" ] || fail "undo did not restore original names"
[ ! -e "$FIRST_RENAMED" ] && [ ! -e "$SECOND_RENAMED" ] || fail "undo left renamed paths"
[ ! -e "$MANIFEST" ] || fail "used undo record was not removed"
[ "$(/usr/bin/shasum -a 256 "$FIRST" | /usr/bin/awk '{print $1}')" = "$FIRST_HASH" ] ||
  fail "first bytes changed after undo"
[ "$(/usr/bin/shasum -a 256 "$SECOND" | /usr/bin/awk '{print $1}')" = "$SECOND_HASH" ] ||
  fail "second bytes changed after undo"

COLLISION_REQUEST="$SMOKE_DIRECTORY/collision-request.json"
COLLISION_RESULT="$SMOKE_DIRECTORY/collision-result.json"
{
  printf '%s\n' '{' '  "request": {' '    "operation": "rename",' '    "items": ['
  printf '      { "inputPath": "%s", "outputName": "occupied.txt" }\n' "$COLLISION_SOURCE"
  printf '%s\n' '    ],' '    "jobId": "rename-collision"' '  },'
  printf '  "resultPath": "%s",\n' "$COLLISION_RESULT"
  printf '%s\n' '  "cancelAfterMs": null,' '  "cancelJobId": null' '}'
} > "$COLLISION_REQUEST"
run_request "$COLLISION_REQUEST" "$COLLISION_RESULT" rename-collision
assert_error "$COLLISION_RESULT" OutputConflict
[ "$(/usr/bin/shasum -a 256 "$COLLISION_SOURCE" | /usr/bin/awk '{print $1}')" = "$COLLISION_SOURCE_HASH" ] ||
  fail "collision changed the source"
[ "$(/usr/bin/shasum -a 256 "$COLLISION_TARGET" | /usr/bin/awk '{print $1}')" = "$COLLISION_TARGET_HASH" ] ||
  fail "collision replaced the existing target"

EXTENSION_REQUEST="$SMOKE_DIRECTORY/extension-request.json"
EXTENSION_RESULT="$SMOKE_DIRECTORY/extension-result.json"
{
  printf '%s\n' '{' '  "request": {' '    "operation": "rename",' '    "items": ['
  printf '      { "inputPath": "%s", "outputName": "extension-source.pdf" }\n' "$EXTENSION_SOURCE"
  printf '%s\n' '    ],' '    "jobId": "rename-extension"' '  },'
  printf '  "resultPath": "%s",\n' "$EXTENSION_RESULT"
  printf '%s\n' '  "cancelAfterMs": null,' '  "cancelJobId": null' '}'
} > "$EXTENSION_REQUEST"
run_request "$EXTENSION_REQUEST" "$EXTENSION_RESULT" rename-extension
assert_error "$EXTENSION_RESULT" UnsupportedConversion
[ "$(/usr/bin/shasum -a 256 "$EXTENSION_SOURCE" | /usr/bin/awk '{print $1}')" = "$EXTENSION_HASH" ] ||
  fail "extension rejection changed the source"

# Cancel deterministically after both files have been staged (20 percent) but
# before either target is committed. The debug-only listener cancels the exact
# active job synchronously from its job-scoped progress event.
CANCEL_DIRECTORY="$SMOKE_DIRECTORY/cancel"
mkdir "$CANCEL_DIRECTORY"
CANCEL_FIRST="$CANCEL_DIRECTORY/first.txt"
CANCEL_SECOND="$CANCEL_DIRECTORY/second.txt"
printf 'cancel first bytes\n' > "$CANCEL_FIRST"
printf 'cancel second bytes\n' > "$CANCEL_SECOND"
CANCEL_FIRST_HASH=$(/usr/bin/shasum -a 256 "$CANCEL_FIRST" | /usr/bin/awk '{print $1}')
CANCEL_SECOND_HASH=$(/usr/bin/shasum -a 256 "$CANCEL_SECOND" | /usr/bin/awk '{print $1}')
CANCEL_REQUEST="$SMOKE_DIRECTORY/cancel-request.json"
CANCEL_RESULT="$SMOKE_DIRECTORY/cancel-result.json"
{
  printf '%s\n' '{' '  "request": {' '    "operation": "rename",' '    "items": ['
  printf '      { "inputPath": "%s", "outputName": "cancelled-first.txt" },\n' "$CANCEL_FIRST"
  printf '      { "inputPath": "%s", "outputName": "cancelled-second.txt" }\n' "$CANCEL_SECOND"
  printf '%s\n' '    ],' '    "jobId": "rename-cancel"' '  },'
  printf '  "resultPath": "%s",\n' "$CANCEL_RESULT"
  printf '%s\n' \
    '  "cancelAfterMs": null,' \
    '  "cancelAtProgress": 20,' \
    '  "cancelJobId": "rename-cancel"' \
    '}'
} > "$CANCEL_REQUEST"
run_request "$CANCEL_REQUEST" "$CANCEL_RESULT" rename-cancel
assert_error "$CANCEL_RESULT" Cancelled
[ -f "$CANCEL_FIRST" ] && [ -f "$CANCEL_SECOND" ] || fail "cancellation did not restore sources"
[ ! -e "$CANCEL_DIRECTORY/cancelled-first.txt" ] || fail "cancellation committed first target"
[ ! -e "$CANCEL_DIRECTORY/cancelled-second.txt" ] || fail "cancellation committed second target"
[ "$(/usr/bin/shasum -a 256 "$CANCEL_FIRST" | /usr/bin/awk '{print $1}')" = "$CANCEL_FIRST_HASH" ] ||
  fail "cancellation changed first bytes"
[ "$(/usr/bin/shasum -a 256 "$CANCEL_SECOND" | /usr/bin/awk '{print $1}')" = "$CANCEL_SECOND_HASH" ] ||
  fail "cancellation changed second bytes"

# Force the second staging rename to fail after the first item has already been
# moved to its hidden transaction path. This proves the packaged rollback puts
# the first item back and removes the failed transaction's undo record.
ROLLBACK_FIRST_DIRECTORY="$SMOKE_DIRECTORY/rollback-first"
ROLLBACK_BLOCKED_DIRECTORY="$SMOKE_DIRECTORY/rollback-blocked"
mkdir "$ROLLBACK_FIRST_DIRECTORY" "$ROLLBACK_BLOCKED_DIRECTORY"
ROLLBACK_FIRST="$ROLLBACK_FIRST_DIRECTORY/first.txt"
ROLLBACK_SECOND="$ROLLBACK_BLOCKED_DIRECTORY/second.txt"
printf 'rollback first bytes\n' > "$ROLLBACK_FIRST"
printf 'rollback second bytes\n' > "$ROLLBACK_SECOND"
ROLLBACK_FIRST_HASH=$(/usr/bin/shasum -a 256 "$ROLLBACK_FIRST" | /usr/bin/awk '{print $1}')
ROLLBACK_SECOND_HASH=$(/usr/bin/shasum -a 256 "$ROLLBACK_SECOND" | /usr/bin/awk '{print $1}')
/bin/chmod u-w "$ROLLBACK_BLOCKED_DIRECTORY"

ROLLBACK_REQUEST="$SMOKE_DIRECTORY/rollback-request.json"
ROLLBACK_RESULT="$SMOKE_DIRECTORY/rollback-result.json"
{
  printf '%s\n' '{' '  "request": {' '    "operation": "rename",' '    "items": ['
  printf '      { "inputPath": "%s", "outputName": "renamed-first.txt" },\n' "$ROLLBACK_FIRST"
  printf '      { "inputPath": "%s", "outputName": "renamed-second.txt" }\n' "$ROLLBACK_SECOND"
  printf '%s\n' '    ],' '    "jobId": "rename-rollback"' '  },'
  printf '  "resultPath": "%s",\n' "$ROLLBACK_RESULT"
  printf '%s\n' '  "cancelAfterMs": null,' '  "cancelJobId": null' '}'
} > "$ROLLBACK_REQUEST"
run_request "$ROLLBACK_REQUEST" "$ROLLBACK_RESULT" rename-rollback
/bin/chmod u+w "$ROLLBACK_BLOCKED_DIRECTORY"
assert_error "$ROLLBACK_RESULT" ProcessFailed
[ -f "$ROLLBACK_FIRST" ] && [ -f "$ROLLBACK_SECOND" ] || fail "rollback did not restore sources"
[ ! -e "$ROLLBACK_FIRST_DIRECTORY/renamed-first.txt" ] || fail "rollback left first target"
[ ! -e "$ROLLBACK_BLOCKED_DIRECTORY/renamed-second.txt" ] || fail "rollback left second target"
[ "$(/usr/bin/shasum -a 256 "$ROLLBACK_FIRST" | /usr/bin/awk '{print $1}')" = "$ROLLBACK_FIRST_HASH" ] ||
  fail "rollback changed first bytes"
[ "$(/usr/bin/shasum -a 256 "$ROLLBACK_SECOND" | /usr/bin/awk '{print $1}')" = "$ROLLBACK_SECOND_HASH" ] ||
  fail "rollback changed second bytes"

PARTIAL=$(find "$SMOKE_DIRECTORY" -name '.convertkit-rename-*' -print -quit)
[ -z "$PARTIAL" ] || fail "a hidden rename transaction path remains"
STALE_MANIFEST=$(find "$MANIFEST_DIRECTORY" -type f \( -name '*.json' -o -name '*.tmp' \) -print -quit)
[ -z "$STALE_MANIFEST" ] || fail "a managed rename manifest or partial remains"

printf 'Packaged Batch Rename smoke passed\n'
printf 'Transaction, extensions, collisions, cancellation, rollback, undo, source bytes, and cleanup verified\n'
