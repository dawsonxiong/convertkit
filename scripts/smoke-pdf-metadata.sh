#!/bin/sh

set -eu

APP_PATH=${1:-"src-tauri/target/debug/bundle/macos/ConvertKit.app"}
MINIMAL_PATH=/usr/bin:/bin:/usr/sbin:/sbin

fail() {
  printf 'PDF metadata smoke failed: %s\n' "$1" >&2
  exit 1
}

[ -d "$APP_PATH" ] || fail "app bundle not found at $APP_PATH"
APP_PATH=$(cd "$(dirname "$APP_PATH")" && pwd -P)/$(basename "$APP_PATH")
APP_EXECUTABLE="$APP_PATH/Contents/MacOS/convertkit"
[ -x "$APP_EXECUTABLE" ] || fail "app executable is missing"

GHOSTSCRIPT=$(command -v gs || true)
PDFINFO=$(command -v pdfinfo || true)
PDFTOPPM=$(command -v pdftoppm || true)
PDFTOTEXT=$(command -v pdftotext || true)
[ -n "$GHOSTSCRIPT" ] || fail "Ghostscript is not installed"
[ -n "$PDFINFO" ] || fail "pdfinfo is not installed"

SMOKE_ROOT=${TMPDIR:-/tmp}
SMOKE_ROOT=${SMOKE_ROOT%/}
SMOKE_DIRECTORY=$(mktemp -d "$SMOKE_ROOT/convertkit-pdf-metadata-smoke.XXXXXX")
case "$SMOKE_DIRECTORY" in
  "$SMOKE_ROOT"/convertkit-pdf-metadata-smoke.*) ;;
  *) fail "temporary directory was created outside the expected location" ;;
esac

cleanup() {
  case "$SMOKE_DIRECTORY" in
    "$SMOKE_ROOT"/convertkit-pdf-metadata-smoke.*)
      [ ! -d "$SMOKE_DIRECTORY" ] || find "$SMOKE_DIRECTORY" -depth -delete
      ;;
    *) printf 'Refusing to clean unexpected directory: %s\n' "$SMOKE_DIRECTORY" >&2 ;;
  esac
}
trap cleanup EXIT HUP INT TERM

SOURCE_POSTSCRIPT="$SMOKE_DIRECTORY/source.ps"
SOURCE_PDF="$SMOKE_DIRECTORY/tagged-source.pdf"
ENCRYPTED_PDF="$SMOKE_DIRECTORY/encrypted-source.pdf"
CANCEL_POSTSCRIPT="$SMOKE_DIRECTORY/cancel.ps"
CANCEL_PDF="$SMOKE_DIRECTORY/cancel-source.pdf"
OUTPUT_DIRECTORY="$SMOKE_DIRECTORY/output"
ENCRYPTED_OUTPUT_DIRECTORY="$SMOKE_DIRECTORY/encrypted-output"
CANCEL_OUTPUT_DIRECTORY="$SMOKE_DIRECTORY/cancel-output"
RUNTIME_DIRECTORY="$SMOKE_DIRECTORY/runtime"
mkdir "$OUTPUT_DIRECTORY" "$ENCRYPTED_OUTPUT_DIRECTORY" "$CANCEL_OUTPUT_DIRECTORY" "$RUNTIME_DIRECTORY"

# The metadata sentinels deliberately contain no spaces so both pdfinfo and the
# raw strings scan can prove that document-info and XMP copies were removed.
{
  printf '%s\n' \
    '%!PS-Adobe-3.0' \
    '[/Title (CKPRIVATE-TITLE-8624) /Author (CKPRIVATE-AUTHOR-8624) /Subject (CKPRIVATE-SUBJECT-8624) /Keywords (CKPRIVATE-KEYWORDS-8624) /Creator (CKPRIVATE-CREATOR-8624) /CreationDate (D:20260831101500-04\04700\047) /ModDate (D:20260831102500-04\04700\047) /DOCINFO pdfmark' \
    '<< /PageSize [612 792] >> setpagedevice' \
    '[/CropBox [24 30 588 762] /PAGE pdfmark' \
    '/Helvetica-Bold findfont 22 scalefont setfont' \
    '72 720 moveto (VISIBLE PAGE ONE ALPHA) show' \
    '0.15 0.35 0.85 setrgbcolor 72 500 300 120 rectfill' \
    '0 setgray 2 setlinewidth newpath 72 450 moveto 500 620 lineto stroke' \
    'showpage' \
    '<< /PageSize [500 700] >> setpagedevice' \
    '[/CropBox [30 40 470 660] /Rotate 90 /PAGE pdfmark' \
    '/Helvetica-Bold findfont 22 scalefont setfont' \
    '72 620 moveto (VISIBLE PAGE TWO BRAVO) show' \
    '0.85 0.25 0.2 setrgbcolor 90 300 260 180 rectstroke' \
    '0.2 0.7 0.45 setrgbcolor newpath 80 180 moveto 230 430 lineto 410 170 lineto closepath fill' \
    'showpage' \
    '<< /PageSize [612 792] >> setpagedevice' \
    '/Helvetica-Bold findfont 22 scalefont setfont' \
    '72 720 moveto (VISIBLE PAGE THREE CHARLIE) show' \
    '0.55 0.3 0.75 setrgbcolor 260 390 110 0 360 arc fill' \
    '0 setgray /Helvetica findfont 14 scalefont setfont' \
    '72 120 moveto (ConvertKit visual and text preservation sentinel.) show' \
    'showpage' \
    '%%EOF'
} > "$SOURCE_POSTSCRIPT"

"$GHOSTSCRIPT" -q -dSAFER -dBATCH -dNOPAUSE \
  -sDEVICE=pdfwrite -dCompatibilityLevel=1.7 -dCompressPages=false \
  "-sOutputFile=$SOURCE_PDF" "$SOURCE_POSTSCRIPT"

"$GHOSTSCRIPT" -q -dSAFER -dBATCH -dNOPAUSE \
  -sDEVICE=pdfwrite -dCompatibilityLevel=1.7 \
  -sOwnerPassword=owner-secret -sUserPassword=reader-secret \
  "-sOutputFile=$ENCRYPTED_PDF" "$SOURCE_PDF"

# A dense vector fixture makes the real packaged PDF rewrite long enough for
# the debug harness to issue cancellation before a commit can occur.
{
  printf '%s\n' \
    '%!PS-Adobe-3.0' \
    '[/Title (CKPRIVATE-CANCEL-8624) /Author (CKPRIVATE-CANCEL-AUTHOR-8624) /DOCINFO pdfmark' \
    '<< /PageSize [612 792] >> setpagedevice' \
    '0.2 setlinewidth' \
    '24680 srand' \
    '0 1 650000 {' \
    'pop' \
    'rand 500 mod 50 add rand 700 mod 50 add moveto' \
    'rand 500 mod 50 add rand 700 mod 50 add lineto' \
    'rand 256 mod 255 div rand 256 mod 255 div rand 256 mod 255 div setrgbcolor stroke' \
    '} for' \
    'showpage' \
    '%%EOF'
} > "$CANCEL_POSTSCRIPT"

"$GHOSTSCRIPT" -q -dSAFER -dBATCH -dNOPAUSE \
  -sDEVICE=pdfwrite -dCompatibilityLevel=1.7 -dPDFSETTINGS=/prepress \
  "-sOutputFile=$CANCEL_PDF" "$CANCEL_POSTSCRIPT"

SOURCE_HASH=$(/usr/bin/shasum -a 256 "$SOURCE_PDF" | /usr/bin/awk '{print $1}')
ENCRYPTED_HASH=$(/usr/bin/shasum -a 256 "$ENCRYPTED_PDF" | /usr/bin/awk '{print $1}')
CANCEL_HASH=$(/usr/bin/shasum -a 256 "$CANCEL_PDF" | /usr/bin/awk '{print $1}')

metadata_value() {
  pdf_path=$1
  field=$2
  "$PDFINFO" "$pdf_path" 2>/dev/null | /usr/bin/awk -v label="$field:" '
    index($0, label) == 1 {
      sub("^[^:]*:[[:space:]]*", "")
      print
      exit
    }
  '
}

for field in Title Author Subject Keywords Creator Producer CreationDate ModDate; do
  [ -n "$(metadata_value "$SOURCE_PDF" "$field")" ] || fail "source fixture is missing $field metadata"
done
for sentinel in \
  CKPRIVATE-TITLE-8624 \
  CKPRIVATE-AUTHOR-8624 \
  CKPRIVATE-SUBJECT-8624 \
  CKPRIVATE-KEYWORDS-8624 \
  CKPRIVATE-CREATOR-8624; do
  /usr/bin/strings "$SOURCE_PDF" | /usr/bin/grep -Fq "$sentinel" ||
    fail "source fixture does not expose $sentinel for the raw metadata check"
done
"$PDFINFO" -upw reader-secret "$ENCRYPTED_PDF" | /usr/bin/grep -Eq '^Encrypted:[[:space:]]+yes' ||
  fail "encrypted fixture is not encrypted"

pdf_topology() {
  pdf_path=$1
  page_count=$("$PDFINFO" "$pdf_path" | /usr/bin/awk '/^Pages:/ { print $2; exit }')
  [ -n "$page_count" ] || fail "could not read page count for $pdf_path"
  printf 'Pages %s\n' "$page_count"
  "$PDFINFO" -f 1 -l "$page_count" -box "$pdf_path" | /usr/bin/awk '
    $1 == "Page" && $3 == "rot:" { print $1, $2, $3, $4 }
    $1 == "Page" && ($3 == "MediaBox:" || $3 == "CropBox:" || $3 == "BleedBox:" || $3 == "TrimBox:" || $3 == "ArtBox:") {
      printf "%s %s %s %.2f %.2f %.2f %.2f\n", $1, $2, $3, $4, $5, $6, $7
    }
  '
}

SOURCE_TOPOLOGY="$SMOKE_DIRECTORY/source-topology.txt"
pdf_topology "$SOURCE_PDF" > "$SOURCE_TOPOLOGY"
/usr/bin/grep -Eq '^Pages 3$' "$SOURCE_TOPOLOGY" || fail "source fixture is not multipage"
/usr/bin/grep -Eq '^Page 2 rot: (90|180|270)$' "$SOURCE_TOPOLOGY" ||
  fail "source fixture does not contain a rotated page"
/usr/bin/awk '
  $3 == "MediaBox:" { media[$2] = $4 " " $5 " " $6 " " $7 }
  $3 == "CropBox:" { crop[$2] = $4 " " $5 " " $6 " " $7 }
  END {
    for (page in crop) if (crop[page] != media[page]) found = 1
    exit found ? 0 : 1
  }
' "$SOURCE_TOPOLOGY" || fail "source fixture does not contain a cropped page"

write_request() {
  input_path=$1
  output_directory=$2
  suffix=$3
  job_id=$4
  result_path=$5
  cancel_after_ms=${6:-null}
  request_path="$SMOKE_DIRECTORY/$job_id-request.json"

  {
    printf '%s\n' '{' '  "request": {' '    "operation": "removeMetadata",'
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

  PATH=$MINIMAL_PATH \
    TMPDIR="$RUNTIME_DIRECTORY" \
    CONVERTKIT_DEBUG_SMOKE_REQUEST="$request_path" \
    "$APP_EXECUTABLE" >/dev/null 2>&1
  [ -f "$result_path" ] || fail "$job_id did not write a result"
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
  expected_kind=${2:-}
  if /usr/bin/plutil -extract Ok.output_path raw -o - "$result_path" >/dev/null 2>&1; then
    /bin/cat "$result_path" >&2
    fail "a request expected to fail committed an output"
  fi
  actual_kind=$(/usr/bin/plutil -extract Err.kind raw -o - "$result_path" 2>/dev/null || true)
  [ -n "$actual_kind" ] || {
    /bin/cat "$result_path" >&2
    fail "the packaged app returned an unreadable error"
  }
  if [ -n "$expected_kind" ] && [ "$actual_kind" != "$expected_kind" ]; then
    /bin/cat "$result_path" >&2
    fail "expected $expected_kind, got $actual_kind"
  fi
}

assert_pdf() {
  output_path=$1
  [ -s "$output_path" ] || fail "reported output does not exist"
  [ "$(dd if="$output_path" bs=5 count=1 2>/dev/null)" = '%PDF-' ] || fail "output is not a PDF"
  "$GHOSTSCRIPT" -q -dSAFER -dBATCH -dNOPAUSE -sDEVICE=nullpage "$output_path"
}

assert_metadata_absent() {
  output_path=$1
  for field in Title Author Subject Keywords Creator Producer CreationDate ModDate; do
    [ -z "$(metadata_value "$output_path" "$field")" ] || fail "$field metadata remains in $output_path"
  done
  for sentinel in \
    CKPRIVATE-TITLE-8624 \
    CKPRIVATE-AUTHOR-8624 \
    CKPRIVATE-SUBJECT-8624 \
    CKPRIVATE-KEYWORDS-8624 \
    CKPRIVATE-CREATOR-8624; do
    if /usr/bin/strings "$output_path" | /usr/bin/grep -Fq "$sentinel"; then
      fail "$sentinel remains in the cleaned PDF"
    fi
  done
}

assert_source_hash() {
  source_path=$1
  expected_hash=$2
  label=$3
  actual_hash=$(/usr/bin/shasum -a 256 "$source_path" | /usr/bin/awk '{print $1}')
  [ "$actual_hash" = "$expected_hash" ] || fail "$label source changed"
}

FIRST_RESULT="$SMOKE_DIRECTORY/first-result.json"
write_request "$SOURCE_PDF" "$OUTPUT_DIRECTORY" -clean pdf-metadata-clean-1 "$FIRST_RESULT"
FIRST_OUTPUT=$(success_output "$FIRST_RESULT")
assert_pdf "$FIRST_OUTPUT"
assert_metadata_absent "$FIRST_OUTPUT"
FIRST_TOPOLOGY="$SMOKE_DIRECTORY/first-topology.txt"
pdf_topology "$FIRST_OUTPUT" > "$FIRST_TOPOLOGY"
/usr/bin/cmp -s "$SOURCE_TOPOLOGY" "$FIRST_TOPOLOGY" || fail "cleaned PDF changed page boxes or rotation"
assert_source_hash "$SOURCE_PDF" "$SOURCE_HASH" 'tagged PDF'

if [ -n "$PDFTOTEXT" ]; then
  "$PDFTOTEXT" -layout "$SOURCE_PDF" "$SMOKE_DIRECTORY/source.txt"
  "$PDFTOTEXT" -layout "$FIRST_OUTPUT" "$SMOKE_DIRECTORY/first.txt"
  /usr/bin/cmp -s "$SMOKE_DIRECTORY/source.txt" "$SMOKE_DIRECTORY/first.txt" ||
    fail "cleaned PDF changed extractable page text"
fi

if [ -n "$PDFTOPPM" ]; then
  page=1
  while [ "$page" -le 3 ]; do
    "$PDFTOPPM" -f "$page" -l "$page" -singlefile -r 72 -png \
      "$SOURCE_PDF" "$SMOKE_DIRECTORY/source-page-$page" >/dev/null 2>&1
    "$PDFTOPPM" -f "$page" -l "$page" -singlefile -r 72 -png \
      "$FIRST_OUTPUT" "$SMOKE_DIRECTORY/output-page-$page" >/dev/null 2>&1
    /usr/bin/cmp -s "$SMOKE_DIRECTORY/source-page-$page.png" "$SMOKE_DIRECTORY/output-page-$page.png" ||
      fail "cleaned PDF changed the rendered pixels on page $page"
    page=$((page + 1))
  done
fi

# Repeating the same collision-safe request must preserve both cleaned outputs.
SECOND_RESULT="$SMOKE_DIRECTORY/second-result.json"
write_request "$SOURCE_PDF" "$OUTPUT_DIRECTORY" -clean pdf-metadata-clean-2 "$SECOND_RESULT"
SECOND_OUTPUT=$(success_output "$SECOND_RESULT")
[ "$SECOND_OUTPUT" != "$FIRST_OUTPUT" ] || fail "keep-both reused the first output path"
assert_pdf "$SECOND_OUTPUT"
assert_metadata_absent "$SECOND_OUTPUT"
SECOND_TOPOLOGY="$SMOKE_DIRECTORY/second-topology.txt"
pdf_topology "$SECOND_OUTPUT" > "$SECOND_TOPOLOGY"
/usr/bin/cmp -s "$SOURCE_TOPOLOGY" "$SECOND_TOPOLOGY" || fail "keep-both output changed page topology"
assert_source_hash "$SOURCE_PDF" "$SOURCE_HASH" 'tagged PDF after keep-both'

# Password-protected input must be rejected without creating an output or a
# hidden working file. The exact typed error may differ by PDF engine.
ENCRYPTED_RESULT="$SMOKE_DIRECTORY/encrypted-result.json"
write_request "$ENCRYPTED_PDF" "$ENCRYPTED_OUTPUT_DIRECTORY" -clean \
  pdf-metadata-encrypted "$ENCRYPTED_RESULT"
assert_error "$ENCRYPTED_RESULT"
assert_source_hash "$ENCRYPTED_PDF" "$ENCRYPTED_HASH" 'encrypted PDF'
[ -z "$(find "$ENCRYPTED_OUTPUT_DIRECTORY" -mindepth 1 -print -quit)" ] ||
  fail "encrypted-input failure left an output or partial"

# Cancellation must stop the rewrite and leave no committed or hidden partial.
CANCEL_RESULT="$SMOKE_DIRECTORY/cancel-result.json"
write_request "$CANCEL_PDF" "$CANCEL_OUTPUT_DIRECTORY" -cancelled \
  pdf-metadata-cancel "$CANCEL_RESULT" 50
assert_error "$CANCEL_RESULT" Cancelled
assert_source_hash "$CANCEL_PDF" "$CANCEL_HASH" 'cancellation PDF'
[ -z "$(find "$CANCEL_OUTPUT_DIRECTORY" -mindepth 1 -print -quit)" ] ||
  fail "cancellation left an output or partial"

PARTIAL_OUTPUT=$(find \
  "$OUTPUT_DIRECTORY" "$ENCRYPTED_OUTPUT_DIRECTORY" "$CANCEL_OUTPUT_DIRECTORY" \
  -name '.convertkit-*' -print -quit)
[ -z "$PARTIAL_OUTPUT" ] || fail "a hidden partial output was left behind"

printf 'PDF metadata packaged smoke passed\n'
printf 'Metadata fields and raw sentinels removed: %s\n' "$FIRST_OUTPUT"
printf 'Keep-both output: %s\n' "$SECOND_OUTPUT"
printf 'Page count, boxes, rotation, source hashes, encrypted rejection, and cancellation passed\n'
if [ -n "$PDFTOTEXT" ]; then
  printf 'Extracted page text preserved\n'
else
  printf 'pdftotext unavailable; extracted-text comparison skipped\n'
fi
if [ -n "$PDFTOPPM" ]; then
  printf 'Rendered page pixels preserved\n'
else
  printf 'pdftoppm unavailable; rendered-page comparison skipped\n'
fi
