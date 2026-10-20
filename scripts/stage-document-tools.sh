#!/bin/sh
# Stage the official Pandoc and Tectonic release binaries as ConvertKit
# sidecars, with their licences and a NOTICE. Every download is pinned to a
# SHA-256 checksum; the binaries are shipped exactly as released.
#
# Outputs (all gitignored):
#   src-tauri/binaries/pandoc-<target>, tectonic-<target>   sidecar executables
#   src-tauri/runtime/documents/                            licences and NOTICE
#   src-tauri/third-party/sources/                          the pinned downloads
#
# Pandoc is GPL-2.0-or-later, so its source is part of each release's
# third-party source archive (scripts/package-third-party-sources.sh).
set -eu

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
target=${CARGO_BUILD_TARGET:-$(rustc --print host-tuple)}
if [ "$target" != aarch64-apple-darwin ]; then
  echo "The bundled document tools are staged for Apple Silicon only, not $target" >&2
  exit 1
fi

# name|version|url|sha256 — versions match Homebrew's formulae at pin time.
pandoc_version=3.12.1
tectonic_version=0.17.0
downloads="
pandoc-binary|$pandoc_version|https://github.com/jgm/pandoc/releases/download/$pandoc_version/pandoc-$pandoc_version-arm64-macOS.zip|c2146cf33a583f3ac93eccddf1bb662371d5cfcce1aad1c87ad860cca1435d46
pandoc-source|$pandoc_version|https://github.com/jgm/pandoc/archive/refs/tags/$pandoc_version.tar.gz|cf9b3725b90e471aa70c99ac3b351adcb6e40cd904d36fcbb0f70d712e135686
tectonic-binary|$tectonic_version|https://github.com/tectonic-typesetting/tectonic/releases/download/tectonic%40$tectonic_version/tectonic-$tectonic_version-aarch64-apple-darwin.tar.gz|a3f1cac7c5678f01661a92212f58480ae3b0634115d880dbc59e2953ded45667
tectonic-source|$tectonic_version|https://github.com/tectonic-typesetting/tectonic/archive/refs/tags/tectonic%40$tectonic_version.tar.gz|30adda98f67dd5389844f6023adeeb54b5475c17a54b777900644468fbc9765d
"

source_directory="$repo_root/src-tauri/third-party/sources"
work_directory="$repo_root/src-tauri/third-party/work"
binary_directory="$repo_root/src-tauri/binaries"
runtime_directory="$repo_root/src-tauri/runtime/documents"
license_directory="$runtime_directory/licenses"

archive_path() {
  case "$3" in
    *.zip) suffix=zip ;;
    *) suffix=tar.gz ;;
  esac
  printf '%s/%s-%s.%s' "$source_directory" "$1" "$2" "$suffix"
}

# Corresponding-source mode: print the GPL source download and stop.
if [ "${1:-}" = --list-sources ]; then
  echo "$downloads" | while IFS='|' read -r name version url sha256; do
    if [ "$name" = pandoc-source ]; then
      archive_path "$name" "$version" "$url"
      echo
    fi
  done
  exit 0
fi

fetch() {
  url=$1
  output=$2
  sha256=$3
  if [ -f "$output" ] && [ "$(/usr/bin/shasum -a 256 "$output" | /usr/bin/awk '{ print $1 }')" = "$sha256" ]; then
    return
  fi
  /bin/rm -f "$output.partial"
  /usr/bin/curl --fail --location --retry 3 --silent --show-error --output "$output.partial" "$url"
  actual=$(/usr/bin/shasum -a 256 "$output.partial" | /usr/bin/awk '{ print $1 }')
  if [ "$actual" != "$sha256" ]; then
    /bin/rm -f "$output.partial"
    echo "Checksum mismatch for $url: expected $sha256, got $actual" >&2
    exit 1
  fi
  /bin/mv "$output.partial" "$output"
}

mkdir -p "$source_directory"
echo "$downloads" | while IFS='|' read -r name version url sha256; do
  [ -n "$name" ] || continue
  fetch "$url" "$(archive_path "$name" "$version" "$url")" "$sha256"
done

case "$runtime_directory" in
  "$repo_root/src-tauri/runtime/documents") ;;
  *)
    echo "Refusing to replace an unexpected runtime directory: $runtime_directory" >&2
    exit 1
    ;;
esac
/bin/rm -rf -- "$work_directory" "$runtime_directory"
mkdir -p "$work_directory" "$binary_directory" "$license_directory"

pinned() {
  echo "$downloads" | /usr/bin/awk -F'|' -v name="$1" '$1 == name { print $2 "|" $3 }' | {
    IFS='|' read -r version url
    archive_path "$1" "$version" "$url"
  }
}

/usr/bin/ditto -x -k "$(pinned pandoc-binary)" "$work_directory/pandoc"
/usr/bin/tar -xzf "$(pinned tectonic-binary)" -C "$work_directory"
/usr/bin/tar -xzf "$(pinned pandoc-source)" -C "$work_directory"
/usr/bin/tar -xzf "$(pinned tectonic-source)" -C "$work_directory"

/bin/cp "$work_directory/pandoc/pandoc-$pandoc_version-arm64/bin/pandoc" "$binary_directory/pandoc-$target"
/bin/cp "$work_directory/tectonic" "$binary_directory/tectonic-$target"
chmod 755 "$binary_directory/pandoc-$target" "$binary_directory/tectonic-$target"

/bin/cp "$work_directory/pandoc-$pandoc_version/COPYING.md" "$license_directory/pandoc-COPYING.md"
/bin/cp "$work_directory/pandoc-$pandoc_version/COPYRIGHT" "$license_directory/pandoc-COPYRIGHT.txt"
/bin/cp "$work_directory/tectonic-tectonic-$tectonic_version/LICENSE" "$license_directory/tectonic-LICENSE.txt"

{
  echo "ConvertKit bundles two document tools, shipped unmodified as released"
  echo "by their projects:"
  echo
  echo "  Pandoc $pandoc_version, GNU GPL version 2 or later (pandoc-COPYING.md,"
  echo "  pandoc-COPYRIGHT.txt). https://pandoc.org"
  echo "  Tectonic $tectonic_version, MIT licence (tectonic-LICENSE.txt)."
  echo "  https://tectonic-typesetting.github.io"
  echo
  echo "ConvertKit itself is MIT-licensed and runs these as separate programs."
  echo
  echo "Downloads and SHA-256 checksums:"
  echo "$downloads" | while IFS='|' read -r name version url sha256; do
    [ -n "$name" ] || continue
    echo "  $name $version"
    echo "    $url"
    echo "    sha256 $sha256"
  done
  echo
  echo "Corresponding source: every release of ConvertKit is published together"
  echo "with ConvertKit_<version>_third-party-source.tar.gz, which holds Pandoc's"
  echo "source at the tag above. Pandoc's own release builds pin its Haskell"
  echo "dependencies; see cabal.project in that source."
  echo
  echo "Tectonic downloads TeX support files the first time it builds a PDF and"
  echo "caches them; later PDF builds work offline."
} > "$runtime_directory/NOTICE.txt"

for file in "$binary_directory/pandoc-$target" "$binary_directory/tectonic-$target"; do
  otool -L "$file" | tail -n +2 | awk '{ print $1 }' | while IFS= read -r dependency; do
    case "$dependency" in
      /System/*|/usr/lib/*) ;;
      *)
        echo "$file depends on a library outside macOS: $dependency" >&2
        exit 1
        ;;
    esac
  done
  case " $(lipo -archs "$file") " in
    *" arm64 "*) ;;
    *) echo "$file has no arm64 code" >&2; exit 1 ;;
  esac
done
[ "$("$binary_directory/pandoc-$target" --version | head -n 1)" = "pandoc $pandoc_version" ] || {
  echo "The staged Pandoc is not version $pandoc_version" >&2
  exit 1
}
[ "$("$binary_directory/tectonic-$target" --version)" = "Tectonic $tectonic_version" ] || {
  echo "The staged Tectonic is not version $tectonic_version" >&2
  exit 1
}
/bin/rm -rf -- "$work_directory"

echo "Staged Pandoc $pandoc_version and Tectonic $tectonic_version"
