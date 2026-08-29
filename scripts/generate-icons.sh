#!/bin/sh

set -eu

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
project_dir=$(dirname -- "$script_dir")
source_icon="$project_dir/output/imagegen/app-icons/convertkit-reference-final.png"
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

cd "$project_dir"

# Generate the full-size assets used by Windows, Linux, mobile, and the app UI.
pnpm tauri icon "$source_icon" --output "$icon_dir"

# Legacy macOS ICNS artwork uses Apple's 80.5% optical footprint: 824px of a
# 1024px canvas, which becomes 206px inside a 256px Dock representation.
magick "$source_icon" \
  -filter Lanczos \
  -resize 824x824 \
  -gravity center \
  -background none \
  -extent 1024x1024 \
  "$temporary_dir/convertkit-macos.png"

pnpm tauri icon "$temporary_dir/convertkit-macos.png" --output "$temporary_dir/generated"
cp "$temporary_dir/generated/icon.icns" "$icon_dir/icon.icns"

echo "Generated platform icons with a macOS-normalized Dock footprint."
