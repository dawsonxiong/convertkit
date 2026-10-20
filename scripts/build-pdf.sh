#!/bin/sh
set -eu

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
target=${CARGO_BUILD_TARGET:-$(rustc --print host-tuple)}

case "$target" in
  aarch64-apple-darwin) architecture=arm64 ;;
  x86_64-apple-darwin) architecture=x86_64 ;;
  *)
    echo "The PDF helper supports macOS targets only: $target" >&2
    exit 1
    ;;
esac

output_directory="$repo_root/src-tauri/binaries"
output="$output_directory/convertkit-pdf-$target"
mkdir -p "$output_directory"

xcrun clang \
  -arch "$architecture" \
  -mmacosx-version-min=12.0 \
  -fobjc-arc \
  -Os \
  "$repo_root/src-tauri/native/pdf.m" \
  -Wall -Werror \
  -framework CoreGraphics \
  -framework Foundation \
  -framework ImageIO \
  -framework PDFKit \
  -o "$output"

chmod 755 "$output"
