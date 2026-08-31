#!/bin/sh
set -eu

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
temporary_directory=$(mktemp -d "${TMPDIR:-/tmp}/convertkit-image-pdf-smoke.XXXXXX")
trap 'rm -rf -- "$temporary_directory"' EXIT HUP INT TERM

if [ "$#" -gt 1 ]; then
  echo "Usage: smoke-image-pdf.sh [ConvertKit.app]" >&2
  exit 2
fi

if [ "$#" -eq 1 ]; then
  helper="$1/Contents/MacOS/convertkit-image-pdf"
else
  target=${CARGO_BUILD_TARGET:-$(rustc --print host-tuple)}
  helper="$repo_root/src-tauri/binaries/convertkit-image-pdf-$target"
fi

for executable in "$helper" magick pdfinfo pdftoppm shasum; do
  if ! command -v "$executable" >/dev/null 2>&1 && [ ! -x "$executable" ]; then
    echo "Required smoke dependency is unavailable: $executable" >&2
    exit 3
  fi
done

source_image="$temporary_directory/source.png"
output_pdf="$temporary_directory/source.pdf"
rendered_image="$temporary_directory/rendered.png"

magick -size 300x150 "xc:#16233f" \
  -fill "#9eb7ff" -draw "rectangle 0,0 149,149" \
  -set comment "convertkit-private-smoke-marker" \
  "$source_image"
source_hash=$(shasum -a 256 "$source_image" | awk '{print $1}')

"$helper" "$source_image" "$output_pdf"
test "$(head -c 5 "$output_pdf")" = "%PDF-"
pdfinfo "$output_pdf" | grep -Eq '^Pages:[[:space:]]+1$'
pdfinfo "$output_pdf" | grep -Eq '^Page size:[[:space:]]+300 x 150 pts'

pdftoppm -f 1 -l 1 -singlefile -r 72 -png "$output_pdf" \
  "$temporary_directory/rendered" >/dev/null 2>&1
test "$(magick identify -format '%wx%h' "$rendered_image")" = "300x150"
test "$(shasum -a 256 "$source_image" | awk '{print $1}')" = "$source_hash"
if strings "$output_pdf" | grep -q "convertkit-private-smoke-marker"; then
  echo "Private source metadata leaked into the PDF." >&2
  exit 4
fi

echo "Image to PDF smoke passed: one 300 x 150 point page, visible render, source preserved."
