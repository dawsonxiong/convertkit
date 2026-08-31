#!/bin/sh

set -eu

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
project_dir=$(dirname -- "$script_dir")
source_icon="$project_dir/design/icon-concepts/convertkit-reference-final.png"
icon_dir="$project_dir/src-tauri/icons"
temporary_dir=$(mktemp -d "${TMPDIR:-/tmp}/convertkit-icons.XXXXXX")

cleanup() {
  rm -rf -- "$temporary_dir"
}
trap cleanup EXIT HUP INT TERM

command -v magick >/dev/null 2>&1 || {
  echo "ImageMagick is required to generate the macOS icon." >&2
  exit 1
}
command -v iconutil >/dev/null 2>&1 || {
  echo "iconutil is required to generate the macOS icon." >&2
  exit 1
}

cd "$project_dir"

magick "$source_icon" -filter Lanczos -resize 128x128 "$icon_dir/128x128.png"
magick "$source_icon" -filter Lanczos -resize 512x512 "$icon_dir/icon.png"

# Legacy macOS ICNS artwork uses Apple's 80.5% optical footprint: 824px of a
# 1024px canvas, which becomes 206px inside a 256px Dock representation.
magick "$source_icon" \
  -filter Lanczos \
  -resize 824x824 \
  -gravity center \
  -background none \
  -extent 1024x1024 \
  "$temporary_dir/convertkit-macos.png"

iconset="$temporary_dir/ConvertKit.iconset"
mkdir -p "$iconset"

set -- \
  16 icon_16x16.png \
  32 icon_16x16@2x.png \
  32 icon_32x32.png \
  64 icon_32x32@2x.png \
  128 icon_128x128.png \
  256 icon_128x128@2x.png \
  256 icon_256x256.png \
  512 icon_256x256@2x.png \
  512 icon_512x512.png \
  1024 icon_512x512@2x.png
while [ "$#" -gt 0 ]; do
  size=$1
  filename=$2
  shift 2
  magick "$temporary_dir/convertkit-macos.png" \
    -filter Lanczos \
    -resize "${size}x${size}" \
    "$iconset/$filename"
done

iconutil --convert icns --output "$icon_dir/icon.icns" "$iconset"

echo "Generated the app-shell, Tauri runtime, and macOS-normalized Dock icons."
