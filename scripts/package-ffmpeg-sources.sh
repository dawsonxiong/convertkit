#!/bin/sh
# Archive the Corresponding Source for the bundled GPL FFmpeg: the exact
# pinned tarballs, the script that built them, and the shipped notices.
# Publish the result next to each release's DMG.
#
# Usage: scripts/package-ffmpeg-sources.sh [output-directory]
set -eu

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
output_directory=${1:-"$repo_root/src-tauri/target/release/bundle/dmg"}
version=$(node -p "require('$repo_root/src-tauri/tauri.conf.json').version")
name="ConvertKit_${version}_ffmpeg-source"
runtime_directory="$repo_root/src-tauri/runtime/ffmpeg"

# Building first guarantees the tarballs are present and match their pins,
# and that the notices describe this exact build.
sh "$repo_root/scripts/build-ffmpeg.sh" >/dev/null

staging=$(mktemp -d "${TMPDIR:-/tmp}/convertkit-ffmpeg-source.XXXXXX")
trap 'rm -rf "$staging"' EXIT HUP INT TERM
mkdir -p "$staging/$name/sources" "$output_directory"

sh "$repo_root/scripts/build-ffmpeg.sh" --list-sources | while IFS= read -r archive; do
  [ -f "$archive" ] || {
    echo "Missing pinned source tarball: $archive" >&2
    exit 1
  }
  cp "$archive" "$staging/$name/sources/"
done
cp "$repo_root/scripts/build-ffmpeg.sh" "$staging/$name/"
cp "$runtime_directory/NOTICE.txt" "$staging/$name/"
cp -R "$runtime_directory/licenses" "$staging/$name/"
cat > "$staging/$name/README.txt" <<README
Corresponding source for the FFmpeg bundled in ConvertKit $version.

sources/ holds the unmodified upstream tarballs; NOTICE.txt lists their
versions, URLs and SHA-256 checksums. build-ffmpeg.sh is the script that
built the shipped binaries. To rebuild, place it at scripts/build-ffmpeg.sh
in a ConvertKit checkout, copy sources/ to src-tauri/ffmpeg-build/sources/,
and run: sh scripts/build-ffmpeg.sh
README

archive="$output_directory/$name.tar.gz"
tar -czf "$archive" -C "$staging" "$name"
echo "$archive"
