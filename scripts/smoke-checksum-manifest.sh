#!/bin/sh

set -eu

APP_PATH=${1:-"src-tauri/target/debug/bundle/macos/ConvertKit.app"}
MINIMAL_PATH=/usr/bin:/bin:/usr/sbin:/sbin

fail() {
  printf 'Checksum manifest smoke failed: %s\n' "$1" >&2
  exit 1
}

[ -d "$APP_PATH" ] || fail "app bundle not found at $APP_PATH"
APP_PATH=$(cd "$(dirname "$APP_PATH")" && pwd -P)/$(basename "$APP_PATH")
APP_EXECUTABLE="$APP_PATH/Contents/MacOS/convertkit"
[ -x "$APP_EXECUTABLE" ] || fail "app executable is missing"

SMOKE_ROOT=${TMPDIR:-/tmp}
SMOKE_ROOT=${SMOKE_ROOT%/}
SMOKE_DIRECTORY=$(mktemp -d "$SMOKE_ROOT/convertkit-checksum-manifest-smoke.XXXXXX")
case "$SMOKE_DIRECTORY" in
  "$SMOKE_ROOT"/convertkit-checksum-manifest-smoke.*) ;;
  *) fail "temporary directory was created outside the expected location" ;;
esac

cleanup() {
  case "$SMOKE_DIRECTORY" in
    "$SMOKE_ROOT"/convertkit-checksum-manifest-smoke.*)
      [ ! -d "$SMOKE_DIRECTORY" ] || find "$SMOKE_DIRECTORY" -depth -delete
      ;;
    *) printf 'Refusing to clean unexpected directory: %s\n' "$SMOKE_DIRECTORY" >&2 ;;
  esac
}
trap cleanup EXIT HUP INT TERM

SOURCE_DIRECTORY="$SMOKE_DIRECTORY/source"
CANCEL_DIRECTORY="$SMOKE_DIRECTORY/cancel"
RUNTIME_DIRECTORY="$SMOKE_DIRECTORY/runtime"
HOME_DIRECTORY="$SMOKE_DIRECTORY/home"
mkdir "$SOURCE_DIRECTORY" "$CANCEL_DIRECTORY" "$RUNTIME_DIRECTORY" "$HOME_DIRECTORY"
SOURCE_DIRECTORY=$(cd "$SOURCE_DIRECTORY" && pwd -P)
CANCEL_DIRECTORY=$(cd "$CANCEL_DIRECTORY" && pwd -P)
RUNTIME_DIRECTORY=$(cd "$RUNTIME_DIRECTORY" && pwd -P)
HOME_DIRECTORY=$(cd "$HOME_DIRECTORY" && pwd -P)

MATCHING_FILE="$SOURCE_DIRECTORY/matching.txt"
CHANGED_FILE="$SOURCE_DIRECTORY/changed file.txt"
MISSING_FILE="$SOURCE_DIRECTORY/missing.txt"
printf 'matching bytes\n' > "$MATCHING_FILE"
printf 'original changed bytes\n' > "$CHANGED_FILE"
printf 'eventually missing bytes\n' > "$MISSING_FILE"

MATCHING_HASH=$(/usr/bin/shasum -a 256 "$MATCHING_FILE" | /usr/bin/awk '{print $1}')
CHANGED_HASH=$(/usr/bin/shasum -a 256 "$CHANGED_FILE" | /usr/bin/awk '{print $1}')
MISSING_HASH=$(/usr/bin/shasum -a 256 "$MISSING_FILE" | /usr/bin/awk '{print $1}')

run_app() {
  request_path=$1
  result_path=$2
  log_path=$3
  if ! PATH=$MINIMAL_PATH \
    HOME="$HOME_DIRECTORY" \
    TMPDIR="$RUNTIME_DIRECTORY" \
    CONVERTKIT_DEBUG_SMOKE_REQUEST="$request_path" \
    "$APP_EXECUTABLE" > "$log_path" 2>&1; then
    /bin/cat "$log_path" >&2
    fail "packaged app invocation failed"
  fi
  [ -f "$result_path" ] || {
    /bin/cat "$log_path" >&2
    fail "packaged app did not write $result_path"
  }
}

result_value() {
  result_path=$1
  key=$2
  /usr/bin/plutil -extract "$key" raw -o - "$result_path" 2>/dev/null || {
    /bin/cat "$result_path" >&2
    fail "result is missing $key"
  }
}

write_create_request() {
  request_path=$1
  result_path=$2
  job_id=$3
  cancel_after=${4:-null}
  cancel_job_id=${5:-null}
  {
    printf '%s\n' '{' '  "checksumManifest": {' '    "action": "create",' '    "inputs": ['
    printf '      {"inputPath":"%s","relativePath":null},\n' "$MATCHING_FILE"
    printf '      {"inputPath":"%s","relativePath":null},\n' "$CHANGED_FILE"
    printf '      {"inputPath":"%s","relativePath":null}\n' "$MISSING_FILE"
    printf '    ],\n    "jobId":"%s"\n' "$job_id"
    printf '%s\n' '  },'
    printf '  "resultPath":"%s",\n' "$result_path"
    printf '  "cancelAfterMs":%s,\n' "$cancel_after"
    if [ "$cancel_job_id" = null ]; then
      printf '%s\n' '  "cancelJobId":null,'
    else
      printf '  "cancelJobId":"%s",\n' "$cancel_job_id"
    fi
    printf '%s\n' '  "cancelAtProgress":null' '}'
  } > "$request_path"
}

CREATE_REQUEST="$SMOKE_DIRECTORY/create-request.json"
CREATE_RESULT="$SMOKE_DIRECTORY/create-result.json"
CREATE_LOG="$SMOKE_DIRECTORY/create.log"
write_create_request "$CREATE_REQUEST" "$CREATE_RESULT" checksum-create
run_app "$CREATE_REQUEST" "$CREATE_RESULT" "$CREATE_LOG"

MANIFEST_PATH=$(result_value "$CREATE_RESULT" Ok.outputPath)
[ "$MANIFEST_PATH" = "$SOURCE_DIRECTORY/checksums.sha256" ] ||
  fail "first manifest did not use the standard filename"
[ "$(result_value "$CREATE_RESULT" Ok.entryCount)" = 3 ] || fail "manifest entry count is wrong"
[ -s "$MANIFEST_PATH" ] || fail "manifest was not created"

[ "$(/usr/bin/shasum -a 256 "$MATCHING_FILE" | /usr/bin/awk '{print $1}')" = "$MATCHING_HASH" ] ||
  fail "manifest creation changed matching.txt"
[ "$(/usr/bin/shasum -a 256 "$CHANGED_FILE" | /usr/bin/awk '{print $1}')" = "$CHANGED_HASH" ] ||
  fail "manifest creation changed changed file.txt"
[ "$(/usr/bin/shasum -a 256 "$MISSING_FILE" | /usr/bin/awk '{print $1}')" = "$MISSING_HASH" ] ||
  fail "manifest creation changed missing.txt"

(cd "$SOURCE_DIRECTORY" && /usr/bin/shasum -a 256 -c checksums.sha256 >/dev/null) ||
  fail "system shasum rejected the generated standard manifest"

KEEP_REQUEST="$SMOKE_DIRECTORY/keep-request.json"
KEEP_RESULT="$SMOKE_DIRECTORY/keep-result.json"
KEEP_LOG="$SMOKE_DIRECTORY/keep.log"
write_create_request "$KEEP_REQUEST" "$KEEP_RESULT" checksum-keep
run_app "$KEEP_REQUEST" "$KEEP_RESULT" "$KEEP_LOG"
[ "$(result_value "$KEEP_RESULT" Ok.outputPath)" = "$SOURCE_DIRECTORY/checksums (1).sha256" ] ||
  fail "second manifest did not use keep-both naming"

write_verify_request() {
  request_path=$1
  result_path=$2
  manifest_path=$3
  job_id=$4
  {
    printf '%s\n' '{' '  "checksumManifest": {' '    "action":"verify",'
    printf '    "manifestPath":"%s",\n' "$manifest_path"
    printf '    "jobId":"%s"\n' "$job_id"
    printf '%s\n' '  },'
    printf '  "resultPath":"%s",\n' "$result_path"
    printf '%s\n' \
      '  "cancelAfterMs":null,' \
      '  "cancelAtProgress":null,' \
      '  "cancelJobId":null' \
      '}'
  } > "$request_path"
}

VERIFY_REQUEST="$SMOKE_DIRECTORY/verify-request.json"
VERIFY_RESULT="$SMOKE_DIRECTORY/verify-result.json"
VERIFY_LOG="$SMOKE_DIRECTORY/verify.log"
write_verify_request "$VERIFY_REQUEST" "$VERIFY_RESULT" "$MANIFEST_PATH" checksum-verify
run_app "$VERIFY_REQUEST" "$VERIFY_RESULT" "$VERIFY_LOG"
[ "$(result_value "$VERIFY_RESULT" Ok.algorithm)" = sha256 ] || fail "algorithm is wrong"
for index in 0 1 2; do
  [ "$(result_value "$VERIFY_RESULT" "Ok.entries.$index.status")" = match ] ||
    fail "an unchanged manifest entry did not match"
done

printf 'modified bytes\n' > "$CHANGED_FILE"
rm "$MISSING_FILE"
MIXED_REQUEST="$SMOKE_DIRECTORY/mixed-request.json"
MIXED_RESULT="$SMOKE_DIRECTORY/mixed-result.json"
MIXED_LOG="$SMOKE_DIRECTORY/mixed.log"
write_verify_request "$MIXED_REQUEST" "$MIXED_RESULT" "$MANIFEST_PATH" checksum-mixed
run_app "$MIXED_REQUEST" "$MIXED_RESULT" "$MIXED_LOG"
[ "$(result_value "$MIXED_RESULT" Ok.entries.0.status)" = match ] || fail "matching status is wrong"
[ "$(result_value "$MIXED_RESULT" Ok.entries.1.status)" = mismatch ] || fail "mismatch status is wrong"
[ "$(result_value "$MIXED_RESULT" Ok.entries.2.status)" = missing ] || fail "missing status is wrong"

OUTSIDE_FILE="$SMOKE_DIRECTORY/outside.txt"
printf 'outside bytes\n' > "$OUTSIDE_FILE"
OUTSIDE_HASH=$(/usr/bin/shasum -a 256 "$OUTSIDE_FILE" | /usr/bin/awk '{print $1}')
MALICIOUS_MANIFEST="$SOURCE_DIRECTORY/malicious.sha256"
printf '%s  ../outside.txt\n' "$OUTSIDE_HASH" > "$MALICIOUS_MANIFEST"
MALICIOUS_REQUEST="$SMOKE_DIRECTORY/malicious-request.json"
MALICIOUS_RESULT="$SMOKE_DIRECTORY/malicious-result.json"
MALICIOUS_LOG="$SMOKE_DIRECTORY/malicious.log"
write_verify_request "$MALICIOUS_REQUEST" "$MALICIOUS_RESULT" "$MALICIOUS_MANIFEST" checksum-malicious
run_app "$MALICIOUS_REQUEST" "$MALICIOUS_RESULT" "$MALICIOUS_LOG"
[ "$(result_value "$MALICIOUS_RESULT" Err.kind)" = ProcessFailed ] ||
  fail "traversal manifest was not rejected"

CANCEL_FILE="$CANCEL_DIRECTORY/large.bin"
/bin/dd if=/dev/zero of="$CANCEL_FILE" bs=1048576 count=64 2>/dev/null
CANCEL_HASH=$(/usr/bin/shasum -a 256 "$CANCEL_FILE" | /usr/bin/awk '{print $1}')
CANCEL_REQUEST="$SMOKE_DIRECTORY/cancel-request.json"
CANCEL_RESULT="$SMOKE_DIRECTORY/cancel-result.json"
CANCEL_LOG="$SMOKE_DIRECTORY/cancel.log"
{
  printf '%s\n' '{' '  "checksumManifest": {' '    "action":"create",' '    "inputs": ['
  printf '      {"inputPath":"%s","relativePath":null}\n' "$CANCEL_FILE"
  printf '%s\n' '    ],' '    "jobId":"checksum-cancel"' '  },'
  printf '  "resultPath":"%s",\n' "$CANCEL_RESULT"
  printf '%s\n' \
    '  "cancelAfterMs":null,' \
    '  "cancelAtProgress":5,' \
    '  "cancelJobId":"checksum-cancel"' \
    '}'
} > "$CANCEL_REQUEST"
run_app "$CANCEL_REQUEST" "$CANCEL_RESULT" "$CANCEL_LOG"
[ "$(result_value "$CANCEL_RESULT" Err.kind)" = Cancelled ] || fail "creation did not cancel"
[ ! -e "$CANCEL_DIRECTORY/checksums.sha256" ] || fail "cancelled creation committed output"
[ "$(/usr/bin/shasum -a 256 "$CANCEL_FILE" | /usr/bin/awk '{print $1}')" = "$CANCEL_HASH" ] ||
  fail "cancelled creation changed its source"

PARTIAL=$(find "$SMOKE_DIRECTORY" \( -name '.tmp*' -o -name '.convertkit-*' \) -print -quit)
[ -z "$PARTIAL" ] || fail "checksum manifest work left a hidden partial"

printf 'Packaged checksum manifest smoke passed\n'
printf 'Creation, standard compatibility, keep-both, verification states, traversal rejection, cancellation, and source preservation verified\n'
