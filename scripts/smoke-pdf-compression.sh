#!/bin/sh

set -eu

APP_PATH=${1:-"src-tauri/target/debug/bundle/macos/ConvertKit.app"}
MIB=1048576
MINIMAL_PATH=/usr/bin:/bin:/usr/sbin:/sbin

fail() {
  printf 'PDF compression smoke failed: %s\n' "$1" >&2
  exit 1
}

if [ ! -d "$APP_PATH" ]; then
  fail "app bundle not found at $APP_PATH"
fi

APP_PATH=$(cd "$(dirname "$APP_PATH")" && pwd -P)/$(basename "$APP_PATH")
APP_EXECUTABLE="$APP_PATH/Contents/MacOS/convertkit"
[ -x "$APP_EXECUTABLE" ] || fail "app executable is missing"

GHOSTSCRIPT=$(command -v gs || true)
PDFINFO=$(command -v pdfinfo || true)
[ -n "$GHOSTSCRIPT" ] || fail "Ghostscript is not installed"
[ -n "$PDFINFO" ] || fail "pdfinfo is not installed"

SMOKE_DIRECTORY=$(mktemp -d "${TMPDIR:-/tmp}/convertkit-pdf-smoke.XXXXXX")
case "$SMOKE_DIRECTORY" in
  "${TMPDIR:-/tmp}"/convertkit-pdf-smoke.*) ;;
  *) fail "temporary directory was created outside the expected location" ;;
esac

cleanup() {
  if [ -d "$SMOKE_DIRECTORY" ]; then
    find "$SMOKE_DIRECTORY" -depth -delete
  fi
}
trap cleanup EXIT HUP INT TERM

SOURCE_POSTSCRIPT="$SMOKE_DIRECTORY/representative.ps"
SOURCE_PDF="$SMOKE_DIRECTORY/representative.pdf"
VECTOR_POSTSCRIPT="$SMOKE_DIRECTORY/vector.ps"
VECTOR_PDF="$SMOKE_DIRECTORY/vector.pdf"
OUTPUT_DIRECTORY="$SMOKE_DIRECTORY/output"
mkdir "$OUTPUT_DIRECTORY"

{
  printf '%%!PS-Adobe-3.0\n'
  printf '/Helvetica findfont 18 scalefont setfont\n'
  printf '72 735 moveto (ConvertKit PDF compression smoke - high-resolution raster) show\n'
  printf 'gsave 72 180 translate 468 468 scale\n'
  printf '/scanline 2400 string def 12345 srand\n'
  printf '2400 2400 8 [2400 0 0 -2400 0 2400]\n'
  printf '{ 0 1 2399 { scanline exch rand 256 mod put } for scanline } image\n'
  printf 'grestore showpage\n'
  page=1
  while [ "$page" -le 12 ]; do
    printf '72 735 moveto (ConvertKit PDF compression smoke - page %s) show\n' "$page"
    row=0
    while [ "$row" -lt 18 ]; do
      shade=$((20 + (row * 11 + page * 7) % 75))
      printf '0.%s 0.%s 0.%s setrgbcolor 72 %s 468 20 rectfill\n' \
        "$shade" "$((99 - shade))" "$((30 + shade / 2))" "$((690 - row * 30))"
      printf '0 setgray 82 %s moveto (Representative vector content row %s) show\n' \
        "$((696 - row * 30))" "$((row + 1))"
      row=$((row + 1))
    done
    printf 'showpage\n'
    page=$((page + 1))
  done
  printf '%%%%EOF\n'
} > "$SOURCE_POSTSCRIPT"

# Random vector paths do not respond to raster downsampling. This fixture proves
# that an impossible target returns a clean error instead of committing an
# over-ceiling result; it is also slow enough to exercise cancellation reliably.
printf '%s\n' \
  '%!PS-Adobe-3.0' \
  '0.25 setlinewidth' \
  '24680 srand' \
  '0 1 220000 {' \
  'rand 500 mod 50 add rand 700 mod 50 add moveto' \
  'rand 500 mod 50 add rand 700 mod 50 add lineto' \
  'rand 256 mod 255 div rand 256 mod 255 div rand 256 mod 255 div setrgbcolor stroke' \
  '} for' \
  'showpage' \
  '%%EOF' > "$VECTOR_POSTSCRIPT"

"$GHOSTSCRIPT" -q -dSAFER -dBATCH -dNOPAUSE \
  -sDEVICE=pdfwrite -dCompatibilityLevel=1.7 -dPDFSETTINGS=/prepress \
  "-sOutputFile=$SOURCE_PDF" "$SOURCE_POSTSCRIPT"
"$GHOSTSCRIPT" -q -dSAFER -dBATCH -dNOPAUSE \
  -sDEVICE=pdfwrite -dCompatibilityLevel=1.7 -dPDFSETTINGS=/prepress \
  "-sOutputFile=$VECTOR_PDF" "$VECTOR_POSTSCRIPT"

SOURCE_HASH=$(shasum -a 256 "$SOURCE_PDF" | awk '{print $1}')
VECTOR_HASH=$(shasum -a 256 "$VECTOR_PDF" | awk '{print $1}')
SOURCE_SIZE=$(stat -f '%z' "$SOURCE_PDF")
VECTOR_SIZE=$(stat -f '%z' "$VECTOR_PDF")
[ "$SOURCE_SIZE" -gt "$MIB" ] || fail "representative PDF is not larger than the target"
[ "$VECTOR_SIZE" -gt "$MIB" ] || fail "unreachable PDF is not larger than the target"

write_request() {
  input_path=$1
  output_directory=$2
  suffix=$3
  job_id=$4
  result_path=$5
  goal=${6:-}
  target_size=${7:-}
  cancel_after=${8:-null}
  request_path="$SMOKE_DIRECTORY/$job_id-request.json"

  {
    printf '%s\n' '{' '  "request": {' '    "operation": "compressPdf",'
    printf '    "inputPath": "%s",\n' "$input_path"
    printf '%s\n' '    "preset": "balanced",'
    if [ -n "$goal" ]; then
      printf '    "compressionGoal": "%s",\n' "$goal"
      printf '    "targetSizeBytes": %s,\n' "$target_size"
    fi
    printf '    "jobId": "%s",\n' "$job_id"
    printf '%s\n' '    "outputOptions": {'
    printf '      "directory": "%s",\n' "$output_directory"
    printf '      "suffix": "%s"\n' "$suffix"
    printf '%s\n' '    }' '  },'
    printf '  "resultPath": "%s",\n' "$result_path"
    printf '  "cancelAfterMs": %s,\n' "$cancel_after"
    if [ "$cancel_after" = null ]; then
      printf '%s\n' '  "cancelJobId": null'
    else
      printf '  "cancelJobId": "%s"\n' "$job_id"
    fi
    printf '%s\n' '}'
  } > "$request_path"

  PATH=$MINIMAL_PATH \
    TMPDIR="$SMOKE_DIRECTORY" \
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
  expected_kind=$2
  actual_kind=$(/usr/bin/plutil -extract Err.kind raw -o - "$result_path" 2>/dev/null || true)
  if [ "$actual_kind" != "$expected_kind" ]; then
    /bin/cat "$result_path" >&2
    fail "expected $expected_kind, got ${actual_kind:-no error}"
  fi
}

assert_pdf() {
  output_path=$1
  [ -f "$output_path" ] || fail "the reported output does not exist"
  output_signature=$(dd if="$output_path" bs=5 count=1 2>/dev/null)
  [ "$output_signature" = '%PDF-' ] || fail "the output is not a PDF"
  "$GHOSTSCRIPT" -q -dSAFER -dBATCH -dNOPAUSE -sDEVICE=nullpage "$output_path"
}

pdf_topology() {
  input_path=$1
  page_count=$("$PDFINFO" "$input_path" | awk '/^Pages:/ { print $2; exit }')
  [ -n "$page_count" ] || fail "could not read the PDF page count"
  printf 'Pages %s\n' "$page_count"
  "$PDFINFO" -f 1 -l "$page_count" -box "$input_path" | awk '
    $1 == "Page" && $3 == "rot:" { print $1, $2, $3, $4 }
    $1 == "Page" && ($3 == "MediaBox:" || $3 == "CropBox:" || $3 == "BleedBox:" || $3 == "TrimBox:" || $3 == "ArtBox:") {
      printf "%s %s %s %.2f %.2f %.2f %.2f\n", $1, $2, $3, $4, $5, $6, $7
    }
  '
}

# Quality and target-size modes must preserve the same effective page topology.
SOURCE_TOPOLOGY="$SMOKE_DIRECTORY/source-topology.txt"
pdf_topology "$SOURCE_PDF" > "$SOURCE_TOPOLOGY"

# A legacy request omits both new fields and must continue to mean Quality.
LEGACY_RESULT="$SMOKE_DIRECTORY/legacy-result.json"
write_request "$SOURCE_PDF" "$OUTPUT_DIRECTORY" -balanced pdf-quality-legacy "$LEGACY_RESULT"
LEGACY_OUTPUT=$(success_output "$LEGACY_RESULT")
assert_pdf "$LEGACY_OUTPUT"
[ "$(stat -f '%z' "$LEGACY_OUTPUT")" -le "$SOURCE_SIZE" ] || fail "quality output grew"
QUALITY_TOPOLOGY="$SMOKE_DIRECTORY/quality-topology.txt"
pdf_topology "$LEGACY_OUTPUT" > "$QUALITY_TOPOLOGY"
cmp -s "$SOURCE_TOPOLOGY" "$QUALITY_TOPOLOGY" || fail "quality output changed page topology"

# Target mode must respect the exact byte ceiling and preserve effective page
# count, rotation, and every standard page box.
TARGET_TOPOLOGY="$SMOKE_DIRECTORY/target-topology.txt"
TARGET_RESULT="$SMOKE_DIRECTORY/target-result.json"
write_request "$SOURCE_PDF" "$OUTPUT_DIRECTORY" -target pdf-target "$TARGET_RESULT" fileSize "$MIB"
TARGET_OUTPUT=$(success_output "$TARGET_RESULT")
assert_pdf "$TARGET_OUTPUT"
TARGET_SIZE=$(stat -f '%z' "$TARGET_OUTPUT")
[ "$TARGET_SIZE" -le "$MIB" ] || fail "target output exceeded the exact byte ceiling"
pdf_topology "$TARGET_OUTPUT" > "$TARGET_TOPOLOGY"
cmp -s "$SOURCE_TOPOLOGY" "$TARGET_TOPOLOGY" || fail "target output changed page topology"

# Repeating the same request must keep both files with distinct names.
KEEP_BOTH_RESULT="$SMOKE_DIRECTORY/keep-both-result.json"
write_request "$SOURCE_PDF" "$OUTPUT_DIRECTORY" -target pdf-target-keep-both "$KEEP_BOTH_RESULT" fileSize "$MIB"
KEEP_BOTH_OUTPUT=$(success_output "$KEEP_BOTH_RESULT")
assert_pdf "$KEEP_BOTH_OUTPUT"
[ "$KEEP_BOTH_OUTPUT" != "$TARGET_OUTPUT" ] || fail "keep-both reused the existing output path"
[ "$(stat -f '%z' "$KEEP_BOTH_OUTPUT")" -le "$MIB" ] || fail "keep-both output exceeded the ceiling"

# Cancellation must kill Ghostscript and leave neither committed nor hidden
# partial output behind.
CANCEL_DIRECTORY="$SMOKE_DIRECTORY/cancel-output"
mkdir "$CANCEL_DIRECTORY"
CANCEL_RESULT="$SMOKE_DIRECTORY/cancel-result.json"
write_request "$VECTOR_PDF" "$CANCEL_DIRECTORY" -cancelled pdf-target-cancel "$CANCEL_RESULT" fileSize "$MIB" 50
assert_error "$CANCEL_RESULT" Cancelled
[ -z "$(find "$CANCEL_DIRECTORY" -type f -print -quit)" ] || fail "cancellation left an output behind"
sleep 1
if pgrep -f "[g]s.*$(basename "$VECTOR_PDF")" >/dev/null 2>&1; then
  fail "cancellation left a Ghostscript child running"
fi

# A deterministic vector-only input cannot reach 1 MiB through raster
# downsampling. It must fail clearly and clean every attempt.
UNREACHABLE_DIRECTORY="$SMOKE_DIRECTORY/unreachable-output"
mkdir "$UNREACHABLE_DIRECTORY"
UNREACHABLE_RESULT="$SMOKE_DIRECTORY/unreachable-result.json"
write_request "$VECTOR_PDF" "$UNREACHABLE_DIRECTORY" -target pdf-target-unreachable "$UNREACHABLE_RESULT" fileSize "$MIB"
assert_error "$UNREACHABLE_RESULT" ProcessFailed
UNREACHABLE_MESSAGE=$(/usr/bin/plutil -extract Err.detail.message raw -o - "$UNREACHABLE_RESULT" 2>/dev/null || true)
case "$UNREACHABLE_MESSAGE" in
  *"could not reach the 1 MB target"*) ;;
  *) fail "unreachable target did not return a clear error" ;;
esac
[ -z "$(find "$UNREACHABLE_DIRECTORY" -type f -print -quit)" ] || fail "unreachable target left an output behind"

CURRENT_SOURCE_HASH=$(shasum -a 256 "$SOURCE_PDF" | awk '{print $1}')
CURRENT_VECTOR_HASH=$(shasum -a 256 "$VECTOR_PDF" | awk '{print $1}')
[ "$SOURCE_HASH" = "$CURRENT_SOURCE_HASH" ] || fail "the representative source PDF changed"
[ "$VECTOR_HASH" = "$CURRENT_VECTOR_HASH" ] || fail "the vector source PDF changed"

PARTIAL_PATH=$(find "$OUTPUT_DIRECTORY" "$CANCEL_DIRECTORY" "$UNREACHABLE_DIRECTORY" -name '.convertkit-*' -print -quit)
[ -z "$PARTIAL_PATH" ] || fail "a hidden partial output was left behind"

printf 'PDF compression smoke passed\n'
printf 'Source: %s bytes\n' "$SOURCE_SIZE"
printf 'Target output: %s bytes (ceiling %s)\n' "$TARGET_SIZE" "$MIB"
printf 'Keep-both output: %s\n' "$KEEP_BOTH_OUTPUT"
printf 'Legacy quality, topology, cancellation, and unreachable-target checks passed\n'
