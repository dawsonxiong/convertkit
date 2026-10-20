#!/bin/sh
# Archive the Corresponding Source for the GPL programs ConvertKit bundles:
# FFmpeg (the exact pinned tarballs and the script that built them) and
# Pandoc (its source at the shipped tag), with the shipped notices.
# Publish the result next to each release's DMG.
#
# Usage: scripts/package-third-party-sources.sh [output-directory]
set -eu

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
output_directory=${1:-"$repo_root/src-tauri/target/release/bundle/dmg"}
version=$(node -p "require('$repo_root/src-tauri/tauri.conf.json').version")
name="ConvertKit_${version}_third-party-source"
runtime_directory="$repo_root/src-tauri/runtime"

# Building and staging first guarantees every download is present and
# matches its pin, and that the notices describe exactly what ships.
sh "$repo_root/scripts/build-ffmpeg.sh" >/dev/null
sh "$repo_root/scripts/stage-document-tools.sh" >/dev/null

staging=$(mktemp -d "${TMPDIR:-/tmp}/convertkit-third-party-source.XXXXXX")
trap 'rm -rf "$staging"' EXIT HUP INT TERM
mkdir -p "$staging/$name/ffmpeg/sources" "$staging/$name/pandoc" "$output_directory"

copy_sources() {
  while IFS= read -r archive; do
    [ -n "$archive" ] || continue
    [ -f "$archive" ] || {
      echo "Missing pinned source: $archive" >&2
      exit 1
    }
    cp "$archive" "$1/"
  done
}
sh "$repo_root/scripts/build-ffmpeg.sh" --list-sources | copy_sources "$staging/$name/ffmpeg/sources"
sh "$repo_root/scripts/stage-document-tools.sh" --list-sources | copy_sources "$staging/$name/pandoc"

cp "$repo_root/scripts/build-ffmpeg.sh" "$staging/$name/ffmpeg/"
cp "$runtime_directory/ffmpeg/NOTICE.txt" "$staging/$name/ffmpeg/"
cp -R "$runtime_directory/ffmpeg/licenses" "$staging/$name/ffmpeg/"
cp "$repo_root/scripts/stage-document-tools.sh" "$staging/$name/pandoc/"
cp "$runtime_directory/documents/NOTICE.txt" "$staging/$name/pandoc/"
cp -R "$runtime_directory/documents/licenses" "$staging/$name/pandoc/"
cat > "$staging/$name/README.txt" <<README
Corresponding source for the GPL programs bundled in ConvertKit $version.

ffmpeg/ holds the unmodified upstream tarballs FFmpeg and its codecs were
built from (sources/), the script that built them (build-ffmpeg.sh) and the
shipped notices. To rebuild, place build-ffmpeg.sh at scripts/build-ffmpeg.sh
in a ConvertKit checkout, copy sources/ to src-tauri/ffmpeg-build/sources/,
and run: sh scripts/build-ffmpeg.sh

pandoc/ holds Pandoc's source at the released tag and the shipped notices.
ConvertKit ships Pandoc's official macOS binary unmodified;
stage-document-tools.sh records where it came from and its checksum.
README

archive="$output_directory/$name.tar.gz"
tar -czf "$archive" -C "$staging" "$name"
echo "$archive"
