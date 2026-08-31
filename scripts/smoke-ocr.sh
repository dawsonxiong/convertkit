#!/bin/sh

set -eu

APP_PATH=${1:-"src-tauri/target/debug/bundle/macos/ConvertKit.app"}

fail() {
  printf 'OCR smoke failed: %s\n' "$1" >&2
  exit 1
}

[ -d "$APP_PATH" ] || fail "app bundle not found at $APP_PATH"
APP_PATH=$(cd "$(dirname "$APP_PATH")" && pwd -P)/$(basename "$APP_PATH")
APP_EXECUTABLE="$APP_PATH/Contents/MacOS/convertkit"
VISION_HELPER="$APP_PATH/Contents/MacOS/convertkit-vision-ocr"
IMAGE_PDF_HELPER="$APP_PATH/Contents/MacOS/convertkit-image-pdf"
[ -x "$APP_EXECUTABLE" ] || fail "app executable is missing"
[ -x "$VISION_HELPER" ] || fail "bundled Vision OCR helper is missing"
[ -x "$IMAGE_PDF_HELPER" ] || fail "bundled image PDF helper is missing"

MAGICK=$(command -v magick || true)
PDFINFO=$(command -v pdfinfo || true)
PDFTOPPM=$(command -v pdftoppm || true)
PDFTOTEXT=$(command -v pdftotext || true)
PDFUNITE=$(command -v pdfunite || true)
for executable in "$MAGICK" "$PDFINFO" "$PDFTOPPM" "$PDFTOTEXT" "$PDFUNITE"; do
  [ -x "$executable" ] || fail "a required ImageMagick or Poppler smoke dependency is missing"
done

FONT_PATH='/System/Library/Fonts/Supplemental/Arial Bold.ttf'
[ -f "$FONT_PATH" ] || fail "the Arial Bold fixture font is unavailable"

# The packaged OCR helper must be self-contained apart from Apple system libraries.
/usr/bin/otool -L "$VISION_HELPER" | /usr/bin/tail -n +2 | /usr/bin/awk '
  $1 !~ /^\/System\// && $1 !~ /^\/usr\/lib\// && $1 !~ /^@/ { exit 1 }
' || fail "the bundled Vision OCR helper retains a non-system dependency"

SMOKE_ROOT=${TMPDIR:-/tmp}
SMOKE_ROOT=${SMOKE_ROOT%/}
SMOKE_DIRECTORY=$(mktemp -d "$SMOKE_ROOT/convertkit-ocr-smoke.XXXXXX")
case "$SMOKE_DIRECTORY" in
  "$SMOKE_ROOT"/convertkit-ocr-smoke.*) ;;
  *) fail "temporary directory was created outside the expected location" ;;
esac

cleanup() {
  case "$SMOKE_DIRECTORY" in
    "$SMOKE_ROOT"/convertkit-ocr-smoke.*)
      [ ! -d "$SMOKE_DIRECTORY" ] || find "$SMOKE_DIRECTORY" -depth -delete
      ;;
    *) printf 'Refusing to clean unexpected directory: %s\n' "$SMOKE_DIRECTORY" >&2 ;;
  esac
}
trap cleanup EXIT HUP INT TERM

SOURCE_DIRECTORY="$SMOKE_DIRECTORY/sources"
OUTPUT_DIRECTORY="$SMOKE_DIRECTORY/output"
CANCEL_OUTPUT_DIRECTORY="$SMOKE_DIRECTORY/cancel-output"
RUNTIME_DIRECTORY="$SMOKE_DIRECTORY/runtime"
REQUEST_PATH="$SMOKE_DIRECTORY/request.json"
RESULT_PATH="$SMOKE_DIRECTORY/result.json"
APP_LOG="$SMOKE_DIRECTORY/app.log"
mkdir "$SOURCE_DIRECTORY" "$OUTPUT_DIRECTORY" "$CANCEL_OUTPUT_DIRECTORY" "$RUNTIME_DIRECTORY"

LEGACY_IMAGE="$SOURCE_DIRECTORY/legacy orchid.png"
SEARCHABLE_IMAGE="$SOURCE_DIRECTORY/searchable cobalt.png"
SCAN_PAGE_ONE="$SOURCE_DIRECTORY/scan alpha.png"
SCAN_PAGE_TWO="$SOURCE_DIRECTORY/scan bravo.png"
SEARCHABLE_IMAGE_BASELINE="$SOURCE_DIRECTORY/searchable cobalt baseline.pdf"
SCAN_PAGE_ONE_PDF="$SOURCE_DIRECTORY/scan alpha.pdf"
SCAN_PAGE_TWO_PDF="$SOURCE_DIRECTORY/scan bravo.pdf"
SCANNED_PDF="$SOURCE_DIRECTORY/two page scan.pdf"
CANCEL_SOURCE_PDF="$SOURCE_DIRECTORY/cancellation scan.pdf"
CANCEL_PAGE_DIRECTORY="$SOURCE_DIRECTORY/cancel-pages"

make_scan() {
  destination=$1
  background=$2
  accent=$3
  headline=$4
  detail=$5
  "$MAGICK" -size 900x600 "xc:$background" \
    -fill "$accent" -draw 'rectangle 0,0 900,86' \
    -fill '#111318' -font "$FONT_PATH" -pointsize 72 -gravity center \
    -annotate +0-24 "$headline" \
    -fill '#30343c' -pointsize 34 -annotate +0+66 "$detail" \
    "$destination"
  [ -s "$destination" ] || fail "could not create OCR fixture $destination"
}

make_scan "$LEGACY_IMAGE" '#f7f5ef' '#d4e2ff' 'LEGACY ORCHID' 'DEFAULT TEXT OUTPUT'
make_scan "$SEARCHABLE_IMAGE" '#f5f7fb' '#a9c2ff' 'COBALT HARBOR' 'SEARCHABLE IMAGE PDF'
make_scan "$SCAN_PAGE_ONE" '#fff3ed' '#eea38c' 'ALPHA MAPLE' 'SCANNED PAGE ONE'
make_scan "$SCAN_PAGE_TWO" '#eef5ff' '#89afea' 'BRAVO RIVER' 'SCANNED PAGE TWO'

"$IMAGE_PDF_HELPER" "$SEARCHABLE_IMAGE" "$SEARCHABLE_IMAGE_BASELINE"
"$IMAGE_PDF_HELPER" "$SCAN_PAGE_ONE" "$SCAN_PAGE_ONE_PDF"
"$IMAGE_PDF_HELPER" "$SCAN_PAGE_TWO" "$SCAN_PAGE_TWO_PDF"
"$PDFUNITE" "$SCAN_PAGE_ONE_PDF" "$SCAN_PAGE_TWO_PDF" "$SCANNED_PDF"

# The PDF fixture must be image-only, otherwise its original text could mask a
# missing OCR layer in the searchable result.
SOURCE_TEXT=$("$PDFTOTEXT" "$SCANNED_PDF" - 2>/dev/null | /usr/bin/tr -d '[:space:]')
[ -z "$SOURCE_TEXT" ] || fail "the scanned PDF fixture unexpectedly contains a text layer"

# Repeating a scanned page creates a long enough real OCR job to exercise
# cancellation while keeping fixture generation deterministic.
mkdir "$CANCEL_PAGE_DIRECTORY"
page=1
while [ "$page" -le 120 ]; do
  page_name=$(printf '%03d.pdf' "$page")
  /bin/cp "$SCAN_PAGE_ONE_PDF" "$CANCEL_PAGE_DIRECTORY/$page_name"
  page=$((page + 1))
done
# shellcheck disable=SC2086
"$PDFUNITE" "$CANCEL_PAGE_DIRECTORY"/*.pdf "$CANCEL_SOURCE_PDF"
"$PDFINFO" "$CANCEL_SOURCE_PDF" | /usr/bin/grep -Eq '^Pages:[[:space:]]+120$' ||
  fail "the cancellation fixture does not contain 120 pages"

LEGACY_HASH=$(/usr/bin/shasum -a 256 "$LEGACY_IMAGE" | /usr/bin/awk '{print $1}')
SEARCHABLE_HASH=$(/usr/bin/shasum -a 256 "$SEARCHABLE_IMAGE" | /usr/bin/awk '{print $1}')
SCANNED_HASH=$(/usr/bin/shasum -a 256 "$SCANNED_PDF" | /usr/bin/awk '{print $1}')
CANCEL_HASH=$(/usr/bin/shasum -a 256 "$CANCEL_SOURCE_PDF" | /usr/bin/awk '{print $1}')

write_request() {
  input_path=$1
  output_format=$2
  job_id=$3
  output_directory=$4
  suffix=$5
  cancel_after_ms=$6

  {
    printf '%s\n' \
      '{' \
      '  "request": {' \
      '    "operation": "recognizeText",' \
      "    \"inputPath\": \"$input_path\"," 
    if [ -n "$output_format" ]; then
      printf '    "outputFormat": "%s",\n' "$output_format"
    fi
    printf '%s\n' \
      "    \"jobId\": \"$job_id\"," \
      '    "outputOptions": {' \
      "      \"directory\": \"$output_directory\"," \
      "      \"suffix\": \"$suffix\"" \
      '    }' \
      '  },' \
      "  \"resultPath\": \"$RESULT_PATH\"," 
    if [ -n "$cancel_after_ms" ]; then
      printf '  "cancelAfterMs": %s,\n' "$cancel_after_ms"
      printf '  "cancelJobId": "%s"\n' "$job_id"
    else
      printf '%s\n' '  "cancelAfterMs": null,' '  "cancelJobId": null'
    fi
    printf '%s\n' '}'
  } > "$REQUEST_PATH"
}

run_request() {
  /bin/rm -f "$RESULT_PATH"
  : > "$APP_LOG"
  /usr/bin/open -F -n \
    --env 'PATH=/usr/bin:/bin:/usr/sbin:/sbin' \
    --env "TMPDIR=$RUNTIME_DIRECTORY" \
    --env "CONVERTKIT_DEBUG_SMOKE_REQUEST=$REQUEST_PATH" \
    "$APP_PATH" > "$APP_LOG" 2>&1

  attempt=0
  while [ ! -f "$RESULT_PATH" ] && [ "$attempt" -lt 360 ]; do
    sleep 0.5
    attempt=$((attempt + 1))
  done
  [ -f "$RESULT_PATH" ] || {
    /bin/cat "$APP_LOG" >&2
    fail "the packaged app did not write a result within 180 seconds"
  }
  sleep 0.2
}

read_success_output() {
  if ! /usr/bin/plutil -extract Ok.output_path raw -o - "$RESULT_PATH" 2>/dev/null; then
    /bin/cat "$RESULT_PATH" >&2
    fail "the packaged app returned an error"
  fi
}

uppercase_text() {
  /usr/bin/tr '[:lower:]' '[:upper:]'
}

assert_text_contains_in_order() {
  text=$1
  first=$2
  second=$3
  label=$4
  case "$text" in
    *"$first"*"$second"*) ;;
    *) fail "$label does not contain $first followed by $second" ;;
  esac
}

assert_pdf() {
  pdf_path=$1
  page_count=$2
  [ -s "$pdf_path" ] || fail "reported searchable PDF is missing"
  [ "$(/usr/bin/head -c 5 "$pdf_path")" = '%PDF-' ] || fail "searchable output is not a PDF"
  "$PDFINFO" "$pdf_path" | /usr/bin/grep -Eq "^Pages:[[:space:]]+$page_count$" ||
    fail "searchable output has the wrong page count"
}

render_pdf_page() {
  pdf_path=$1
  page_number=$2
  prefix=$3
  "$PDFTOPPM" -f "$page_number" -l "$page_number" -singlefile -r 72 -png \
    "$pdf_path" "$prefix" >/dev/null 2>&1
  [ -s "$prefix.png" ] || fail "could not render $pdf_path page $page_number"
}

assert_visual_match() {
  expected=$1
  actual=$2
  label=$3
  expected_dimensions=$("$MAGICK" identify -format '%wx%h' "$expected")
  actual_dimensions=$("$MAGICK" identify -format '%wx%h' "$actual")
  [ "$expected_dimensions" = "$actual_dimensions" ] || fail "$label dimensions changed"

  metric=$("$MAGICK" compare -metric MAE "$expected" "$actual" null: 2>&1 || true)
  printf '%s\n' "$metric" | /usr/bin/awk '
    {
      normalized = $2;
      gsub(/[()]/, "", normalized);
      if (normalized == "") normalized = $1;
      exit (normalized + 0) <= 0.02 ? 0 : 1;
    }
  ' || fail "$label changed visibly beyond the smoke tolerance"
}

assert_source_hash() {
  source_path=$1
  expected_hash=$2
  label=$3
  actual_hash=$(/usr/bin/shasum -a 256 "$source_path" | /usr/bin/awk '{print $1}')
  [ "$actual_hash" = "$expected_hash" ] || fail "$label source changed"
}

# Omitting outputFormat is deliberate: it proves old requests still default to a
# UTF-8 text file after searchable PDF support is introduced.
write_request "$LEGACY_IMAGE" '' 'ocr-legacy-text-smoke' "$OUTPUT_DIRECTORY" '-legacy' ''
run_request
LEGACY_OUTPUT=$(read_success_output)
[ -s "$LEGACY_OUTPUT" ] || fail "legacy text output is missing"
case "$LEGACY_OUTPUT" in *.txt) ;; *) fail "legacy request did not default to TXT" ;; esac
LEGACY_TEXT=$(uppercase_text < "$LEGACY_OUTPUT")
assert_text_contains_in_order "$LEGACY_TEXT" 'LEGACY' 'ORCHID' 'legacy OCR output'
assert_source_hash "$LEGACY_IMAGE" "$LEGACY_HASH" 'legacy image'

write_request "$SEARCHABLE_IMAGE" 'searchablePdf' 'ocr-image-pdf-smoke-1' \
  "$OUTPUT_DIRECTORY" '-searchable' ''
run_request
IMAGE_OUTPUT=$(read_success_output)
assert_pdf "$IMAGE_OUTPUT" 1
IMAGE_TEXT=$("$PDFTOTEXT" "$IMAGE_OUTPUT" - 2>/dev/null | uppercase_text)
assert_text_contains_in_order "$IMAGE_TEXT" 'COBALT' 'HARBOR' 'searchable image PDF'
assert_source_hash "$SEARCHABLE_IMAGE" "$SEARCHABLE_HASH" 'searchable image'

render_pdf_page "$SEARCHABLE_IMAGE_BASELINE" 1 "$SMOKE_DIRECTORY/image-baseline"
render_pdf_page "$IMAGE_OUTPUT" 1 "$SMOKE_DIRECTORY/image-output"
assert_visual_match "$SMOKE_DIRECTORY/image-baseline.png" "$SMOKE_DIRECTORY/image-output.png" \
  'searchable image PDF'

# Running the same request twice must keep both complete PDFs and choose a
# collision-safe numbered name rather than replacing either source or output.
write_request "$SEARCHABLE_IMAGE" 'searchablePdf' 'ocr-image-pdf-smoke-2' \
  "$OUTPUT_DIRECTORY" '-searchable' ''
run_request
SECOND_IMAGE_OUTPUT=$(read_success_output)
[ "$SECOND_IMAGE_OUTPUT" != "$IMAGE_OUTPUT" ] || fail "keep-both reused the first PDF path"
case "$(basename "$SECOND_IMAGE_OUTPUT")" in
  *' (1).pdf') ;;
  *) fail "keep-both did not use the expected numbered PDF name" ;;
esac
assert_pdf "$SECOND_IMAGE_OUTPUT" 1
assert_source_hash "$SEARCHABLE_IMAGE" "$SEARCHABLE_HASH" 'keep-both image'

write_request "$SCANNED_PDF" 'searchablePdf' 'ocr-multipage-pdf-smoke' \
  "$OUTPUT_DIRECTORY" '-searchable' ''
run_request
SCANNED_OUTPUT=$(read_success_output)
assert_pdf "$SCANNED_OUTPUT" 2
SCANNED_TEXT=$("$PDFTOTEXT" "$SCANNED_OUTPUT" - 2>/dev/null | uppercase_text)
assert_text_contains_in_order "$SCANNED_TEXT" 'ALPHA' 'BRAVO' 'multipage searchable PDF'
assert_source_hash "$SCANNED_PDF" "$SCANNED_HASH" 'multipage PDF'

render_pdf_page "$SCANNED_PDF" 1 "$SMOKE_DIRECTORY/source-page-1"
render_pdf_page "$SCANNED_PDF" 2 "$SMOKE_DIRECTORY/source-page-2"
render_pdf_page "$SCANNED_OUTPUT" 1 "$SMOKE_DIRECTORY/output-page-1"
render_pdf_page "$SCANNED_OUTPUT" 2 "$SMOKE_DIRECTORY/output-page-2"
assert_visual_match "$SMOKE_DIRECTORY/source-page-1.png" "$SMOKE_DIRECTORY/output-page-1.png" \
  'multipage searchable PDF page 1'
assert_visual_match "$SMOKE_DIRECTORY/source-page-2.png" "$SMOKE_DIRECTORY/output-page-2.png" \
  'multipage searchable PDF page 2'

write_request "$CANCEL_SOURCE_PDF" 'searchablePdf' 'ocr-cancel-smoke' \
  "$CANCEL_OUTPUT_DIRECTORY" '-cancelled' '250'
run_request
if /usr/bin/plutil -extract Ok.output_path raw -o - "$RESULT_PATH" >/dev/null 2>&1; then
  /bin/cat "$RESULT_PATH" >&2
  fail "the cancelled OCR job committed an output"
fi
if ! CANCEL_KIND=$(/usr/bin/plutil -extract Err.kind raw -o - "$RESULT_PATH" 2>/dev/null); then
  /bin/cat "$RESULT_PATH" >&2
  fail "the cancelled OCR job returned an unreadable result"
fi
[ "$CANCEL_KIND" = 'Cancelled' ] || {
  /bin/cat "$RESULT_PATH" >&2
  fail "OCR cancellation did not return the Cancelled error"
}
assert_source_hash "$CANCEL_SOURCE_PDF" "$CANCEL_HASH" 'cancellation PDF'

attempt=0
while { /usr/bin/pgrep -f "$VISION_HELPER" >/dev/null 2>&1 ||
  /usr/bin/pgrep -f 'pdftoppm.*convertkit-ocr-' >/dev/null 2>&1; } && [ "$attempt" -lt 50 ]; do
  sleep 0.1
  attempt=$((attempt + 1))
done
/usr/bin/pgrep -f "$VISION_HELPER" >/dev/null 2>&1 && fail "Vision OCR helper survived cancellation"
/usr/bin/pgrep -f 'pdftoppm.*convertkit-ocr-' >/dev/null 2>&1 &&
  fail "PDF renderer survived cancellation"

CANCELLED_OUTPUT=$(find "$CANCEL_OUTPUT_DIRECTORY" -mindepth 1 -print -quit)
[ -z "$CANCELLED_OUTPUT" ] || fail "cancelled OCR left an output or hidden partial"
PARTIAL_OUTPUT=$(find "$OUTPUT_DIRECTORY" -maxdepth 1 -name '.convertkit-*' -print -quit)
[ -z "$PARTIAL_OUTPUT" ] || fail "OCR left a hidden partial output"
OCR_WORKSPACE=$(find "$RUNTIME_DIRECTORY" -name 'convertkit-ocr-*' -print -quit)
[ -z "$OCR_WORKSPACE" ] || fail "OCR left a temporary page workspace"

assert_source_hash "$LEGACY_IMAGE" "$LEGACY_HASH" 'legacy image'
assert_source_hash "$SEARCHABLE_IMAGE" "$SEARCHABLE_HASH" 'searchable image'
assert_source_hash "$SCANNED_PDF" "$SCANNED_HASH" 'multipage PDF'

printf 'Packaged OCR smoke passed under a minimal launch PATH\n'
printf 'Legacy default: %s\n' "$LEGACY_OUTPUT"
printf 'Searchable image PDF: %s\n' "$IMAGE_OUTPUT"
printf 'Searchable keep-both PDF: %s\n' "$SECOND_IMAGE_OUTPUT"
printf 'Searchable multipage PDF: %s\n' "$SCANNED_OUTPUT"
printf 'Cancellation left no output, partial, workspace, or helper process\n'
