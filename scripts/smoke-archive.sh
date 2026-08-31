#!/bin/sh

set -eu

APP_PATH=${1:-"src-tauri/target/debug/bundle/macos/ConvertKit.app"}
MINIMAL_PATH=/usr/bin:/bin:/usr/sbin:/sbin

fail() {
  printf 'Archive smoke failed: %s\n' "$1" >&2
  exit 1
}

[ -d "$APP_PATH" ] || fail "app bundle not found at $APP_PATH"
APP_PATH=$(cd "$(dirname "$APP_PATH")" && pwd -P)/$(basename "$APP_PATH")
APP_EXECUTABLE="$APP_PATH/Contents/MacOS/convertkit"
[ -x "$APP_EXECUTABLE" ] || fail "app executable is missing"

SMOKE_DIRECTORY=$(mktemp -d "${TMPDIR:-/tmp}/convertkit-archive-smoke.XXXXXX")
case "$SMOKE_DIRECTORY" in
  "${TMPDIR:-/tmp}"/convertkit-archive-smoke.*) ;;
  *) fail "temporary directory was created outside the expected location" ;;
esac

cleanup() {
  if [ -d "$SMOKE_DIRECTORY" ]; then
    find "$SMOKE_DIRECTORY" -depth -delete
  fi
}
trap cleanup EXIT HUP INT TERM

SOURCE_DIRECTORY="$SMOKE_DIRECTORY/source"
OUTPUT_DIRECTORY="$SMOKE_DIRECTORY/output"
FIRST_SOURCE="$SOURCE_DIRECTORY/first.txt"
SECOND_SOURCE="$SOURCE_DIRECTORY/second.bin"
REQUEST_PATH="$SMOKE_DIRECTORY/request.json"
RESULT_PATH="$SMOKE_DIRECTORY/result.json"
mkdir "$SOURCE_DIRECTORY" "$OUTPUT_DIRECTORY"
printf 'ConvertKit archive smoke\n' > "$FIRST_SOURCE"
printf '%s' 'AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJygpKissLS4vMDEyMzQ1Njc4Pz4=' |
  /usr/bin/base64 -D > "$SECOND_SOURCE"

FIRST_HASH=$(shasum -a 256 "$FIRST_SOURCE" | awk '{print $1}')
SECOND_HASH=$(shasum -a 256 "$SECOND_SOURCE" | awk '{print $1}')

printf '%s\n' \
  '{' \
  '  "request": {' \
  '    "operation": "createArchive",' \
  '    "entries": [' \
  "      { \"inputPath\": \"$FIRST_SOURCE\", \"archivePath\": \"nested/first.txt\" }," \
  "      { \"inputPath\": \"$SECOND_SOURCE\", \"archivePath\": \"second.bin\" }" \
  '    ],' \
  '    "format": "tarGz",' \
  '    "jobId": "archive-create-smoke",' \
  '    "outputOptions": {' \
  "      \"directory\": \"$OUTPUT_DIRECTORY\"," \
  '      "suffix": "-archive-smoke"' \
  '    }' \
  '  },' \
  "  \"resultPath\": \"$RESULT_PATH\"," \
  '  "cancelAfterMs": null,' \
  '  "cancelJobId": null' \
  '}' > "$REQUEST_PATH"

PATH=$MINIMAL_PATH CONVERTKIT_DEBUG_SMOKE_REQUEST="$REQUEST_PATH" "$APP_EXECUTABLE"
[ -f "$RESULT_PATH" ] || fail "the app did not write the creation result"
if ! ARCHIVE_PATH=$(/usr/bin/plutil -extract Ok.output_path raw -o - "$RESULT_PATH" 2>/dev/null); then
  /bin/cat "$RESULT_PATH" >&2
  fail "the packaged app could not create TAR.GZ"
fi

[ -f "$ARCHIVE_PATH" ] || fail "the reported TAR.GZ does not exist"
/usr/bin/tar -tzf "$ARCHIVE_PATH" | grep -qx 'nested/first.txt' || fail "nested file is missing"
/usr/bin/tar -tzf "$ARCHIVE_PATH" | grep -qx 'second.bin' || fail "binary file is missing"

printf '%s\n' \
  '{' \
  '  "request": {' \
  '    "operation": "extractArchive",' \
  "    \"inputPath\": \"$ARCHIVE_PATH\"," \
  '    "jobId": "archive-extract-smoke",' \
  '    "outputOptions": {' \
  "      \"directory\": \"$OUTPUT_DIRECTORY\"," \
  '      "suffix": "-unpacked"' \
  '    }' \
  '  },' \
  "  \"resultPath\": \"$RESULT_PATH\"," \
  '  "cancelAfterMs": null,' \
  '  "cancelJobId": null' \
  '}' > "$REQUEST_PATH"

PATH=$MINIMAL_PATH CONVERTKIT_DEBUG_SMOKE_REQUEST="$REQUEST_PATH" "$APP_EXECUTABLE"
if ! EXTRACTED_PATH=$(/usr/bin/plutil -extract Ok.output_path raw -o - "$RESULT_PATH" 2>/dev/null); then
  /bin/cat "$RESULT_PATH" >&2
  fail "the packaged app could not extract TAR.GZ"
fi

[ -d "$EXTRACTED_PATH" ] || fail "the reported extraction folder does not exist"
cmp "$FIRST_SOURCE" "$EXTRACTED_PATH/nested/first.txt" || fail "text contents changed"
cmp "$SECOND_SOURCE" "$EXTRACTED_PATH/second.bin" || fail "binary contents changed"
[ "$FIRST_HASH" = "$(shasum -a 256 "$FIRST_SOURCE" | awk '{print $1}')" ] || fail "first source changed"
[ "$SECOND_HASH" = "$(shasum -a 256 "$SECOND_SOURCE" | awk '{print $1}')" ] || fail "second source changed"

PARTIAL_PATH=$(find "$OUTPUT_DIRECTORY" -maxdepth 1 -name '.convertkit-*' -print -quit)
[ -z "$PARTIAL_PATH" ] || fail "a partial output was left behind"

printf 'Packaged TAR.GZ create/extract smoke passed\n'
