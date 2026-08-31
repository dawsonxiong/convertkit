#!/bin/sh

set -eu

APP_PATH=${1:-"src-tauri/target/debug/bundle/macos/ConvertKit.app"}

fail() {
  printf 'Combine to PDF smoke failed: %s\n' "$1" >&2
  exit 1
}

[ -d "$APP_PATH" ] || fail "app bundle not found at $APP_PATH"
APP_PATH=$(cd "$(dirname "$APP_PATH")" && pwd -P)/$(basename "$APP_PATH")
APP_EXECUTABLE="$APP_PATH/Contents/MacOS/convertkit"
[ -x "$APP_EXECUTABLE" ] || fail "app executable is missing"
[ -x "$APP_PATH/Contents/MacOS/convertkit-image-pdf" ] || fail "image PDF helper is missing"

for executable in gs magick pdfinfo pdftoppm shasum; do
  command -v "$executable" >/dev/null 2>&1 || fail "$executable is not installed"
done

SMOKE_DIRECTORY=$(mktemp -d "${TMPDIR:-/tmp}/convertkit-combine-pdf-smoke.XXXXXX")
case "$SMOKE_DIRECTORY" in
  "${TMPDIR:-/tmp}"/convertkit-combine-pdf-smoke.*) ;;
  *) fail "temporary directory was created outside the expected location" ;;
esac

cleanup() {
  if [ -d "$SMOKE_DIRECTORY" ]; then
    find "$SMOKE_DIRECTORY" -depth -delete
  fi
}
trap cleanup EXIT HUP INT TERM

SOURCE_PNG="$SMOKE_DIRECTORY/first.png"
SOURCE_POSTSCRIPT="$SMOKE_DIRECTORY/middle.ps"
SOURCE_PDF="$SMOKE_DIRECTORY/middle.pdf"
SOURCE_JPEG="$SMOKE_DIRECTORY/last.jpg"
OUTPUT_DIRECTORY="$SMOKE_DIRECTORY/output"
REQUEST_PATH="$SMOKE_DIRECTORY/request.json"
RESULT_PATH="$SMOKE_DIRECTORY/result.json"
RENDER_PREFIX="$SMOKE_DIRECTORY/page"
mkdir "$OUTPUT_DIRECTORY"

magick -size 240x160 'xc:#f02020' -set comment 'combine-private-png' "$SOURCE_PNG"
magick -size 240x160 'xc:#1456f0' -set comment 'combine-private-jpeg' -quality 95 "$SOURCE_JPEG"

printf '%s\n' \
  '%!PS-Adobe-3.0' \
  '<< /PageSize [240 160] >> setpagedevice' \
  '0 0.8 0.4 setrgbcolor 0 0 240 160 rectfill showpage' \
  '1 0.8 0 setrgbcolor 0 0 240 160 rectfill showpage' \
  '%%EOF' > "$SOURCE_POSTSCRIPT"
gs -q -dSAFER -dBATCH -dNOPAUSE -sDEVICE=pdfwrite \
  "-sOutputFile=$SOURCE_PDF" "$SOURCE_POSTSCRIPT"

PNG_HASH=$(shasum -a 256 "$SOURCE_PNG" | awk '{print $1}')
PDF_HASH=$(shasum -a 256 "$SOURCE_PDF" | awk '{print $1}')
JPEG_HASH=$(shasum -a 256 "$SOURCE_JPEG" | awk '{print $1}')

write_request() {
  printf '%s\n' \
    '{' \
    '  "request": {' \
    '    "operation": "mergePdf",' \
    "    \"inputPaths\": [\"$SOURCE_PNG\", \"$SOURCE_PDF\", \"$SOURCE_JPEG\"]," \
    '    "jobId": "combine-pdf-smoke",' \
    '    "outputOptions": {' \
    "      \"directory\": \"$OUTPUT_DIRECTORY\"," \
    '      "suffix": "-combined"' \
    '    }' \
    '  },' \
    "  \"resultPath\": \"$RESULT_PATH\"," \
    '  "cancelAfterMs": null,' \
    '  "cancelJobId": null' \
    '}' > "$REQUEST_PATH"
}

run_request() {
  rm -f "$RESULT_PATH"
  write_request
  /usr/bin/open -F -n --env "CONVERTKIT_DEBUG_SMOKE_REQUEST=$REQUEST_PATH" "$APP_PATH" >&2
  attempt=0
  while [ ! -f "$RESULT_PATH" ] && [ "$attempt" -lt 120 ]; do
    sleep 0.5
    attempt=$((attempt + 1))
  done
  [ -f "$RESULT_PATH" ] || fail "the app did not write a result within 60 seconds"
  attempt=0
  while pgrep -x convertkit >/dev/null 2>&1 && [ "$attempt" -lt 20 ]; do
    sleep 0.1
    attempt=$((attempt + 1))
  done
  if ! /usr/bin/plutil -extract Ok.output_path raw -o - "$RESULT_PATH" 2>/dev/null; then
    /bin/cat "$RESULT_PATH" >&2
    fail "the packaged app returned an error"
  fi
}

assert_color() {
  FILE_PATH=$1
  EXPECTED=$2
  CHANNELS=$(magick "$FILE_PATH" -crop 40x40+100+60 -scale 1x1\! \
    -format '%[fx:mean.r] %[fx:mean.g] %[fx:mean.b]' info:)
  printf '%s\n' "$CHANNELS" | awk -v expected="$EXPECTED" '
    {
      r = $1; g = $2; b = $3;
      ok = (expected == "red" && r > 0.8 && g < 0.25 && b < 0.25) ||
           (expected == "green" && r < 0.25 && g > 0.6 && b < 0.55) ||
           (expected == "yellow" && r > 0.8 && g > 0.6 && b < 0.25) ||
           (expected == "blue" && r < 0.25 && g < 0.55 && b > 0.7);
      exit ok ? 0 : 1;
    }
  ' || fail "page color/order verification failed for $EXPECTED"
}

OUTPUT_PATH=$(run_request)
[ -f "$OUTPUT_PATH" ] || fail "the reported output does not exist"
[ "$(head -c 5 "$OUTPUT_PATH")" = '%PDF-' ] || fail "the output is not a PDF"
pdfinfo "$OUTPUT_PATH" | grep -Eq '^Pages:[[:space:]]+4$' || fail "the output does not have four pages"

pdftoppm -f 1 -l 4 -r 72 -png "$OUTPUT_PATH" "$RENDER_PREFIX" >/dev/null 2>&1
assert_color "$RENDER_PREFIX-1.png" red
assert_color "$RENDER_PREFIX-2.png" green
assert_color "$RENDER_PREFIX-3.png" yellow
assert_color "$RENDER_PREFIX-4.png" blue

[ "$(shasum -a 256 "$SOURCE_PNG" | awk '{print $1}')" = "$PNG_HASH" ] || fail "the PNG source changed"
[ "$(shasum -a 256 "$SOURCE_PDF" | awk '{print $1}')" = "$PDF_HASH" ] || fail "the PDF source changed"
[ "$(shasum -a 256 "$SOURCE_JPEG" | awk '{print $1}')" = "$JPEG_HASH" ] || fail "the JPEG source changed"
if strings "$OUTPUT_PATH" | grep -Eq 'combine-private-(png|jpeg)'; then
  fail "private image metadata leaked into the combined PDF"
fi

SECOND_OUTPUT_PATH=$(run_request)
[ "$SECOND_OUTPUT_PATH" != "$OUTPUT_PATH" ] || fail "keep-both reused the first output path"
[ -f "$SECOND_OUTPUT_PATH" ] || fail "the keep-both output does not exist"
pdfinfo "$SECOND_OUTPUT_PATH" | grep -Eq '^Pages:[[:space:]]+4$' || fail "the keep-both output is invalid"

PARTIAL_PATH=$(find "$OUTPUT_DIRECTORY" -maxdepth 1 -name '.convertkit-*' -print -quit)
[ -z "$PARTIAL_PATH" ] || fail "a partial output was left behind"

printf 'Combine to PDF smoke passed: PNG, two-page PDF, and JPEG preserved their order.\n'
printf 'First output: %s\n' "$OUTPUT_PATH"
printf 'Keep-both output: %s\n' "$SECOND_OUTPUT_PATH"
