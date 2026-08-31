#!/bin/sh

set -eu

APP_PATH=${1:-"src-tauri/target/debug/bundle/macos/ConvertKit.app"}
MINIMAL_PATH=/usr/bin:/bin:/usr/sbin:/sbin

fail() {
  printf '7Z extraction smoke failed: %s\n' "$1" >&2
  exit 1
}

[ -d "$APP_PATH" ] || fail "app bundle not found at $APP_PATH"
APP_PATH=$(cd "$(dirname "$APP_PATH")" && pwd -P)/$(basename "$APP_PATH")
APP_EXECUTABLE="$APP_PATH/Contents/MacOS/convertkit"
[ -x "$APP_EXECUTABLE" ] || fail "app executable is missing"

SMOKE_ROOT=${TMPDIR:-/tmp}
SMOKE_ROOT=${SMOKE_ROOT%/}
SMOKE_DIRECTORY=$(mktemp -d "$SMOKE_ROOT/convertkit-seven-smoke.XXXXXX")
case "$SMOKE_DIRECTORY" in
  "$SMOKE_ROOT"/convertkit-seven-smoke.*) ;;
  *) fail "temporary directory was created outside the expected location" ;;
esac

cleanup() {
  case "$SMOKE_DIRECTORY" in
    "$SMOKE_ROOT"/convertkit-seven-smoke.*)
      [ ! -d "$SMOKE_DIRECTORY" ] || find "$SMOKE_DIRECTORY" -depth -delete
      ;;
    *) printf 'Refusing to clean unexpected directory: %s\n' "$SMOKE_DIRECTORY" >&2 ;;
  esac
}
trap cleanup EXIT HUP INT TERM

# These deterministic fixtures were generated once with sevenz-rust2 0.22.2's
# compress feature. Keeping them inline proves the packaged extractor does not
# depend on a separately installed 7z command.
SAFE_FIXTURE_BASE64='N3q8ryccAARsSmkKYwAAAAAAAABxAAAAAAAAAB3PGJUBABpDb252ZXJ0S2l0IDd6IG5lc3RlZCBzbW9rZQoAAQA/AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJygpKissLS4vMDEyMzQ1Njc4OTo7PD0+PwABBAYAAgkfRAoBb8ngOg1EzZQABwsCAAEhIQEWASEhARYMG0AACAoBS513aYzODhAAAAUCETkAbgBlAHMAdABlAGQALwBmAGkAcgBzAHQALgB0AHgAdAAAAHMAZQBjAG8AbgBkAC4AYgBpAG4AAAAAAA=='
TRAVERSAL_FIXTURE_BASE64='N3q8ryccAASoa+OSFAAAAAAAAABoAAAAAAAAAF3e3J0BAA9tdXN0IG5vdCBlc2NhcGUKAAEEBgABCRQKAdolXMAABwsBAAEhIQEWDBAACAoB7jbwzgAABQERPwAuAC4ALwBjAG8AbgB2AGUAcgB0AGsAaQB0AC0AcwBlAHYAZQBuAC0AZQBzAGMAYQBwAGUALgB0AHgAdAAAAAAA'
ENCRYPTED_FIXTURE_BASE64='N3q8ryccAASznK5XkAAAAAAAAABTAAAAAAAAAPJnDptTE7BRsrBPs8hTtA4Llb1hxt7LUBLDcQ2j01ruoFVSAqiIYBGdETirRrxBJquJ9wRpR5j86iMLHrlxYqKTTs8NS1yUsJaciPNOXWTpWmfcpMKmBKRS+Lo2hy8o9L4or5UXYrPqD4joc8hdlqA1CjIaaoM6dtgVDLXmB3EwUVnAc/7kv56p1uk8sdfePZ71DnwXBiABCXAKAU9XoboABwsBAAIkBvEHASLI/9mOxpIGuxLWdYLzCNiZndl2OKD91YANeXZqitJC2Xz3IwMBAQVdAACAAAEADGprAAgKAU/9U+AAAA=='
CANCEL_FIXTURE_BASE64='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'

SOURCE_DIRECTORY="$SMOKE_DIRECTORY/sources"
OUTPUT_DIRECTORY="$SMOKE_DIRECTORY/output"
TRAVERSAL_OUTPUT_DIRECTORY="$SMOKE_DIRECTORY/traversal-output"
ENCRYPTED_OUTPUT_DIRECTORY="$SMOKE_DIRECTORY/encrypted-output"
DAMAGED_OUTPUT_DIRECTORY="$SMOKE_DIRECTORY/damaged-output"
CANCEL_OUTPUT_DIRECTORY="$SMOKE_DIRECTORY/cancel-output"
RUNTIME_DIRECTORY="$SMOKE_DIRECTORY/runtime"
mkdir \
  "$SOURCE_DIRECTORY" \
  "$OUTPUT_DIRECTORY" \
  "$TRAVERSAL_OUTPUT_DIRECTORY" \
  "$ENCRYPTED_OUTPUT_DIRECTORY" \
  "$DAMAGED_OUTPUT_DIRECTORY" \
  "$CANCEL_OUTPUT_DIRECTORY" \
  "$RUNTIME_DIRECTORY"

SAFE_ARCHIVE="$SOURCE_DIRECTORY/nested-safe.7z"
TRAVERSAL_ARCHIVE="$SOURCE_DIRECTORY/traversal.7z"
ENCRYPTED_ARCHIVE="$SOURCE_DIRECTORY/encrypted.7z"
DAMAGED_ARCHIVE="$SOURCE_DIRECTORY/damaged.7z"
CANCEL_ARCHIVE="$SOURCE_DIRECTORY/cancel.7z"
EXPECTED_FIRST="$SOURCE_DIRECTORY/expected-first.txt"
EXPECTED_SECOND="$SOURCE_DIRECTORY/expected-second.bin"

decode_fixture() {
  encoded=$1
  destination=$2
  printf '%s' "$encoded" | /usr/bin/base64 -D > "$destination"
  [ -s "$destination" ] || fail "could not decode $(basename "$destination")"
}

decode_fixture "$SAFE_FIXTURE_BASE64" "$SAFE_ARCHIVE"
decode_fixture "$TRAVERSAL_FIXTURE_BASE64" "$TRAVERSAL_ARCHIVE"
decode_fixture "$ENCRYPTED_FIXTURE_BASE64" "$ENCRYPTED_ARCHIVE"
decode_fixture "$CANCEL_FIXTURE_BASE64" "$CANCEL_ARCHIVE"
printf 'ConvertKit 7z nested smoke\n' > "$EXPECTED_FIRST"
printf '%s' 'AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJygpKissLS4vMDEyMzQ1Njc4OTo7PD0+Pw==' |
  /usr/bin/base64 -D > "$EXPECTED_SECOND"
dd if="$SAFE_ARCHIVE" of="$DAMAGED_ARCHIVE" bs=1 count=40 2>/dev/null

assert_fixture_hash() {
  fixture=$1
  expected=$2
  label=$3
  actual=$(/usr/bin/shasum -a 256 "$fixture" | /usr/bin/awk '{print $1}')
  [ "$actual" = "$expected" ] || fail "$label fixture hash changed"
}

SAFE_HASH=7ef0a9ca4e966bf44033160d8c34fc1a5512bcf5a0059f177e1ebb60096a8a1a
TRAVERSAL_HASH=3af64638c85bb24e2f80d21365bf918d148598657f46cfeaf3b06bb6bb83bf5b
ENCRYPTED_HASH=8ac5a37fbd7490925c7d3fcf37673c958f55231220a7a39ed221223a6d75e161
CANCEL_HASH=b565dab29385573c01f2b725d7e932b798a541757f5b5c1fc3ab944bce10a9d5
DAMAGED_HASH=c35f473dac1762c182ea43dce3939b09b7aaa9316634a603489de06e5ac17507
assert_fixture_hash "$SAFE_ARCHIVE" "$SAFE_HASH" safe
assert_fixture_hash "$TRAVERSAL_ARCHIVE" "$TRAVERSAL_HASH" traversal
assert_fixture_hash "$ENCRYPTED_ARCHIVE" "$ENCRYPTED_HASH" encrypted
assert_fixture_hash "$CANCEL_ARCHIVE" "$CANCEL_HASH" cancellation
assert_fixture_hash "$DAMAGED_ARCHIVE" "$DAMAGED_HASH" damaged

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

assert_safe_output() {
  extracted_path=$1
  [ -d "$extracted_path" ] || fail "reported extraction folder does not exist"
  /usr/bin/cmp -s "$EXPECTED_FIRST" "$extracted_path/nested/first.txt" ||
    fail "nested text content changed"
  /usr/bin/cmp -s "$EXPECTED_SECOND" "$extracted_path/second.bin" ||
    fail "binary content changed"
  file_count=$(find "$extracted_path" -type f | /usr/bin/wc -l | /usr/bin/tr -d ' ')
  [ "$file_count" -eq 2 ] || fail "safe archive extracted an unexpected file count"
  link_path=$(find "$extracted_path" -type l -print -quit)
  [ -z "$link_path" ] || fail "safe archive produced a symbolic link"
}

FIRST_RESULT="$SMOKE_DIRECTORY/safe-first-result.json"
write_request "$SAFE_ARCHIVE" "$OUTPUT_DIRECTORY" -seven seven-safe-1 "$FIRST_RESULT"
FIRST_OUTPUT=$(success_output "$FIRST_RESULT")
assert_safe_output "$FIRST_OUTPUT"
assert_fixture_hash "$SAFE_ARCHIVE" "$SAFE_HASH" 'safe source after extraction'

# Repeating the exact collision-safe request must retain both extracted folders.
SECOND_RESULT="$SMOKE_DIRECTORY/safe-second-result.json"
write_request "$SAFE_ARCHIVE" "$OUTPUT_DIRECTORY" -seven seven-safe-2 "$SECOND_RESULT"
SECOND_OUTPUT=$(success_output "$SECOND_RESULT")
[ "$SECOND_OUTPUT" != "$FIRST_OUTPUT" ] || fail "keep-both reused the first extraction folder"
assert_safe_output "$SECOND_OUTPUT"
[ -d "$FIRST_OUTPUT" ] || fail "keep-both removed the first extraction folder"
assert_fixture_hash "$SAFE_ARCHIVE" "$SAFE_HASH" 'safe source after keep-both'

# A valid 7Z whose entry escapes its destination must be rejected before any
# path is created outside the app-managed working directory.
TRAVERSAL_RESULT="$SMOKE_DIRECTORY/traversal-result.json"
write_request "$TRAVERSAL_ARCHIVE" "$TRAVERSAL_OUTPUT_DIRECTORY" -unsafe \
  seven-traversal "$TRAVERSAL_RESULT"
assert_error "$TRAVERSAL_RESULT" UnsupportedConversion
[ ! -e "$SMOKE_DIRECTORY/convertkit-seven-escape.txt" ] || fail "traversal escaped its output"
[ -z "$(find "$TRAVERSAL_OUTPUT_DIRECTORY" -mindepth 1 -print -quit)" ] ||
  fail "traversal rejection left an output or partial"
assert_fixture_hash "$TRAVERSAL_ARCHIVE" "$TRAVERSAL_HASH" 'traversal source'

ENCRYPTED_RESULT="$SMOKE_DIRECTORY/encrypted-result.json"
write_request "$ENCRYPTED_ARCHIVE" "$ENCRYPTED_OUTPUT_DIRECTORY" -encrypted \
  seven-encrypted "$ENCRYPTED_RESULT"
assert_error "$ENCRYPTED_RESULT" ArchivePasswordRequired
[ -z "$(find "$ENCRYPTED_OUTPUT_DIRECTORY" -mindepth 1 -print -quit)" ] ||
  fail "encrypted rejection left an output or partial"
assert_fixture_hash "$ENCRYPTED_ARCHIVE" "$ENCRYPTED_HASH" 'encrypted source'

DAMAGED_RESULT="$SMOKE_DIRECTORY/damaged-result.json"
write_request "$DAMAGED_ARCHIVE" "$DAMAGED_OUTPUT_DIRECTORY" -damaged \
  seven-damaged "$DAMAGED_RESULT"
assert_error "$DAMAGED_RESULT" ProcessFailed
[ -z "$(find "$DAMAGED_OUTPUT_DIRECTORY" -mindepth 1 -print -quit)" ] ||
  fail "damaged rejection left an output or partial"
assert_fixture_hash "$DAMAGED_ARCHIVE" "$DAMAGED_HASH" 'damaged source'

# The 16 MiB expanded fixture gives the debug cancellation task time to stop
# extraction between bounded reads without committing the working directory.
CANCEL_RESULT="$SMOKE_DIRECTORY/cancel-result.json"
write_request "$CANCEL_ARCHIVE" "$CANCEL_OUTPUT_DIRECTORY" -cancelled \
  seven-cancel "$CANCEL_RESULT" 10
assert_error "$CANCEL_RESULT" Cancelled
[ -z "$(find "$CANCEL_OUTPUT_DIRECTORY" -mindepth 1 -print -quit)" ] ||
  fail "cancellation left an output or partial"
assert_fixture_hash "$CANCEL_ARCHIVE" "$CANCEL_HASH" 'cancellation source'

PARTIAL_PATH=$(find "$SMOKE_DIRECTORY" -name '.convertkit-*' -print -quit)
[ -z "$PARTIAL_PATH" ] || fail "a hidden extraction partial was left behind"

printf 'Packaged 7Z extraction smoke passed\n'
printf 'Nested safe output: %s\n' "$FIRST_OUTPUT"
printf 'Keep-both output: %s\n' "$SECOND_OUTPUT"
printf 'Traversal, encrypted, damaged, cancellation, source, and partial cleanup checks passed\n'
