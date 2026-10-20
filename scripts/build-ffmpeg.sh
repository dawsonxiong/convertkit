#!/bin/sh
# Build the FFmpeg that ships inside ConvertKit from pinned, checksummed
# sources: a GPLv2-or-later build with x264, x265, libvpx, opus, lame and
# vorbis, linked against Apple's VideoToolbox and AudioToolbox frameworks.
#
# Outputs (all gitignored):
#   src-tauri/binaries/ffmpeg-<target>, ffprobe-<target>   sidecar executables
#   src-tauri/runtime/ffmpeg/                              dylibs, licences, NOTICE
#   src-tauri/ffmpeg-build/sources/                        the exact source tarballs
#
# The source tarballs plus this script are the Corresponding Source for the
# GPL; scripts/package-third-party-sources.sh archives them for each release.
set -eu

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
target=${CARGO_BUILD_TARGET:-$(rustc --print host-tuple)}
if [ "$target" != aarch64-apple-darwin ]; then
  echo "The bundled FFmpeg is built for Apple Silicon only, not $target" >&2
  exit 1
fi

deployment_target=12.0
jobs=$(sysctl -n hw.ncpu)

# build-stamp-begin
# name|version|url|sha256 — versions match Homebrew's formulae at pin time.
x264_commit=b35605ace3ddf7c1a5d67a2eb553f034aef41d55
sources="
ffmpeg|9.0.2|https://ffmpeg.org/releases/ffmpeg-9.0.2.tar.xz|8c3850283eb25fa026482078a04051e0be17347b09ef81a0849bec15a96e002e
x264|r3222-${x264_commit}|https://code.videolan.org/videolan/x264/-/archive/${x264_commit}/x264-${x264_commit}.tar.bz2|6eeb82934e69fd51e043bd8c5b0d152839638d1ce7aa4eea65a3fedcf83ff224
x265|4.3|https://github.com/Multicorewareinc/x265/releases/download/4.3/x265_4.3.tar.gz|83c53e4c8bbb8f1e33ed59e10a7d621d1d7801ca853910c3eb41f038b8ffb121
libvpx|1.17.0|https://github.com/webmproject/libvpx/archive/refs/tags/v1.17.0.tar.gz|1020f184046187baa2985dbde38e0691f49c44088bca7a1842b0236c6081dc0a
opus|1.6.1|https://ftp.osuosl.org/pub/xiph/releases/opus/opus-1.6.1.tar.gz|6ffcb593207be92584df15b32466ed64bbec99109f007c82205f0194572411a1
lame|4.0|https://downloads.sourceforge.net/project/lame/lame/4.0/lame-4.0.tar.gz|3df5124d5ad3a98312ffd7ba6a9b36230e4f8a3e66d3ce0f425e336c32d216eb
libogg|1.3.6|https://ftp.osuosl.org/pub/xiph/releases/ogg/libogg-1.3.6.tar.gz|83e6704730683d004d20e21b8f7f55dcb3383cdf84c0daedf30bde175f774638
libvorbis|1.3.7|https://ftp.osuosl.org/pub/xiph/releases/vorbis/libvorbis-1.3.7.tar.xz|b33cc4934322bcbf6efcbacf49e3ca01aadbea4114ec9589d1b1e9d20f72954b
"
# Build tool only; never shipped.
cmake_version=4.4.4
cmake_url="https://github.com/Kitware/CMake/releases/download/v${cmake_version}/cmake-${cmake_version}-macos-universal.tar.gz"
cmake_sha256=4b7b73704b1db9b374e5c9ab8e17ac6148b817b6396ee75cd94a852cbac9d305

build_root="$repo_root/src-tauri/ffmpeg-build"
source_directory="$build_root/sources"
work_directory="$build_root/work"
prefix="$build_root/prefix"
# Builds install through this fixed alias so no home-directory path ends up
# in the shipped binaries (FFmpeg and libvpx embed their configure lines) and
# the output does not depend on where the checkout lives.
build_prefix=/tmp/convertkit-ffmpeg-prefix
binary_directory="$repo_root/src-tauri/binaries"
runtime_directory="$repo_root/src-tauri/runtime/ffmpeg"
license_directory="$runtime_directory/licenses"

# Rebuild only what a changed pin or build step affects: the libraries when
# anything between the build-stamp markers changes, FFmpeg when that or its
# own configure step changes.
section() {
  /usr/bin/sed -n "/^# $1-stamp-begin\$/,/^# $1-stamp-end\$/p" "$0"
}
deps_stamp=$(section build | /usr/bin/shasum -a 256 | /usr/bin/awk '{ print $1 }')
ffmpeg_stamp=$( (section build; section ffmpeg) | /usr/bin/shasum -a 256 | /usr/bin/awk '{ print $1 }')

archive_name() {
  case "$1" in
    *.tar.xz) printf '%s-%s.tar.xz' "$2" "$3" ;;
    *.tar.bz2) printf '%s-%s.tar.bz2' "$2" "$3" ;;
    *) printf '%s-%s.tar.gz' "$2" "$3" ;;
  esac
}

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

unpack() {
  name=$1
  archive=$2
  /bin/rm -rf "$work_directory/$name"
  mkdir -p "$work_directory/$name"
  /usr/bin/tar -xf "$archive" -C "$work_directory/$name" --strip-components 1
}

# Corresponding-source mode: print each pinned tarball's path and stop.
if [ "${1:-}" = --list-sources ]; then
  echo "$sources" | while IFS='|' read -r name version url sha256; do
    [ -n "$name" ] || continue
    printf '%s/%s\n' "$source_directory" "$(archive_name "$url" "$name" "$version")"
  done
  exit 0
fi

mkdir -p "$source_directory"
echo "$sources" | while IFS='|' read -r name version url sha256; do
  [ -n "$name" ] || continue
  fetch "$url" "$source_directory/$(archive_name "$url" "$name" "$version")" "$sha256"
done

# Shared build environment. Only this prefix is visible to pkg-config and
# the compiler, so nothing from Homebrew or /usr/local leaks into the build.
source_archive() {
  echo "$sources" | /usr/bin/awk -F'|' -v name="$1" '$1 == name { print $2 "|" $3 }' | {
    IFS='|' read -r version url
    printf '%s/%s' "$source_directory" "$(archive_name "$url" "$1" "$version")"
  }
}

export MACOSX_DEPLOYMENT_TARGET="$deployment_target"
export CC=clang CXX=clang++
export CFLAGS="-arch arm64 -mmacosx-version-min=$deployment_target -O2"
export CXXFLAGS="$CFLAGS"
export LDFLAGS="-arch arm64 -mmacosx-version-min=$deployment_target"
export PKG_CONFIG_LIBDIR="$build_prefix/lib/pkgconfig"
export PKG_CONFIG_PATH=
pkg_config=$(command -v pkg-config) || {
  echo "pkg-config is required to build the bundled FFmpeg (brew install pkg-config)" >&2
  exit 1
}
export PATH="$work_directory/bin:/usr/bin:/bin:/usr/sbin:/sbin"

autotools() {
  name=$1
  shift
  unpack "$name" "$(source_archive "$name")"
  (
    cd "$work_directory/$name"
    if [ "$name" = libvorbis ]; then
      # Apple's current linker rejects this obsolete flag; Homebrew drops it too.
      /usr/bin/sed -i '' 's/ -force_cpusubtype_ALL//g' configure
    fi
    ./configure --prefix="$build_prefix" --disable-shared --enable-static --with-pic "$@"
    make -j"$jobs"
    make install
  )
}

if [ "$(cat "$prefix/.deps-stamp" 2>/dev/null || true)" != "$deps_stamp" ]; then
  echo "Building FFmpeg's libraries from pinned sources (several minutes)..."
  /bin/rm -rf "$work_directory" "$prefix"
  mkdir -p "$work_directory/bin" "$prefix"
  if [ -e "$build_prefix" ] && [ ! -L "$build_prefix" ]; then
    echo "$build_prefix exists and is not this build's link; remove it first" >&2
    exit 1
  fi
  ln -sfn "$prefix" "$build_prefix"
  ln -sf "$pkg_config" "$work_directory/bin/pkg-config"
  cmake_archive="$build_root/cmake-${cmake_version}.tar.gz"
  fetch "$cmake_url" "$cmake_archive" "$cmake_sha256"
  unpack cmake "$cmake_archive"
  cmake="$work_directory/cmake/CMake.app/Contents/bin/cmake"

  autotools libogg
  autotools libvorbis --with-ogg="$build_prefix" --disable-oggtest --disable-docs --disable-examples
  autotools opus --disable-doc --disable-extra-programs
  autotools lame --disable-frontend --disable-decoder

  unpack libvpx "$(source_archive libvpx)"
  (
    cd "$work_directory/libvpx"
    ./configure --prefix="$build_prefix" --target=arm64-darwin20-gcc --enable-pic \
      --enable-vp9-highbitdepth --disable-examples --disable-tools --disable-docs \
      --disable-unit-tests
    make -j"$jobs"
    make install
  )

  unpack x264 "$(source_archive x264)"
  (
    cd "$work_directory/x264"
    ./configure --prefix="$build_prefix" --enable-static --enable-pic --disable-cli
    make -j"$jobs"
    make install
  )

  unpack x265 "$(source_archive x265)"
  "$cmake" -S "$work_directory/x265/source" -B "$work_directory/x265/build" \
    -DCMAKE_BUILD_TYPE=Release \
    -DCMAKE_INSTALL_PREFIX="$build_prefix" \
    -DCMAKE_OSX_ARCHITECTURES=arm64 \
    -DCMAKE_OSX_DEPLOYMENT_TARGET="$deployment_target" \
    -DCMAKE_POLICY_VERSION_MINIMUM=3.5 \
    -DENABLE_SHARED=OFF \
    -DENABLE_CLI=OFF
  "$cmake" --build "$work_directory/x265/build" --parallel "$jobs"
  "$cmake" --install "$work_directory/x265/build"

  printf '%s\n' "$deps_stamp" > "$prefix/.deps-stamp"
fi
# build-stamp-end

# ffmpeg-stamp-begin
if [ "$(cat "$prefix/.ffmpeg-stamp" 2>/dev/null || true)" != "$ffmpeg_stamp" ]; then
  echo "Building FFmpeg..."
  ln -sfn "$prefix" "$build_prefix"
  unpack ffmpeg "$(source_archive ffmpeg)"
  (
    cd "$work_directory/ffmpeg"
    # Autodetection is off so the build cannot pick up whatever happens to be
    # installed; every external component is named here. No --enable-version3
    # or --enable-nonfree keeps the result GPLv2-or-later.
    ./configure \
      --prefix="$build_prefix" \
      --arch=arm64 \
      --cc=clang \
      --cxx=clang++ \
      --extra-cflags="-mmacosx-version-min=$deployment_target -I$build_prefix/include" \
      --extra-ldflags="-mmacosx-version-min=$deployment_target -L$build_prefix/lib" \
      --extra-libs=-liconv \
      --extra-ldexeflags="-Wl,-rpath,@executable_path/../Resources/ffmpeg-runtime -Wl,-rpath,@executable_path/../runtime/ffmpeg" \
      --pkg-config-flags=--static \
      --install-name-dir=@rpath \
      --enable-shared \
      --disable-static \
      --enable-gpl \
      --disable-autodetect \
      --enable-videotoolbox \
      --enable-audiotoolbox \
      --enable-zlib \
      --enable-bzlib \
      --enable-iconv \
      --enable-libx264 \
      --enable-libx265 \
      --enable-libvpx \
      --enable-libopus \
      --enable-libmp3lame \
      --enable-libvorbis \
      --disable-network \
      --disable-ffplay \
      --disable-doc \
      --disable-debug
    make -j"$jobs"
    make install
  )
  printf '%s\n' "$ffmpeg_stamp" > "$prefix/.ffmpeg-stamp"
fi
# ffmpeg-stamp-end

# --- Stage the sidecars, runtime libraries and licences ---------------------

case "$runtime_directory" in
  "$repo_root/src-tauri/runtime/ffmpeg") ;;
  *)
    echo "Refusing to replace an unexpected runtime directory: $runtime_directory" >&2
    exit 1
    ;;
esac
/bin/rm -rf -- "$runtime_directory"
mkdir -p "$binary_directory" "$license_directory"

for program in ffmpeg ffprobe; do
  /bin/cp "$prefix/bin/$program" "$binary_directory/$program-$target"
  chmod 755 "$binary_directory/$program-$target"
done
# Copy each library under the name its dependants load it by.
for library in avcodec avdevice avfilter avformat avutil swresample swscale; do
  install_name=$(otool -D "$prefix/lib/lib$library.dylib" | tail -n 1)
  /bin/cp -L "$prefix/lib/lib$library.dylib" "$runtime_directory/${install_name#@rpath/}"
done

work_source() {
  printf '%s/%s/%s' "$work_directory" "$1" "$2"
}
/bin/cp "$(work_source ffmpeg COPYING.GPLv2)" "$license_directory/FFmpeg-COPYING.GPLv2.txt"
/bin/cp "$(work_source ffmpeg LICENSE.md)" "$license_directory/FFmpeg-LICENSE.md"
/bin/cp "$(work_source x264 COPYING)" "$license_directory/x264-COPYING.txt"
/bin/cp "$(work_source x265 COPYING)" "$license_directory/x265-COPYING.txt"
/bin/cp "$(work_source libvpx LICENSE)" "$license_directory/libvpx-LICENSE.txt"
/bin/cp "$(work_source libvpx PATENTS)" "$license_directory/libvpx-PATENTS.txt"
/bin/cp "$(work_source opus COPYING)" "$license_directory/opus-COPYING.txt"
/bin/cp "$(work_source lame COPYING)" "$license_directory/lame-COPYING.txt"
/bin/cp "$(work_source libogg COPYING)" "$license_directory/libogg-COPYING.txt"
/bin/cp "$(work_source libvorbis COPYING)" "$license_directory/libvorbis-COPYING.txt"

configuration=$("$binary_directory/ffmpeg-$target" -hide_banner -buildconf | tail -n +2 | sed 's/^ *//')
{
  echo "ConvertKit bundles FFmpeg, built from the upstream sources below."
  echo
  echo "The ffmpeg and ffprobe executables and the libraries in this folder are"
  echo "licensed under the GNU General Public License, version 2 or later"
  echo "(FFmpeg-COPYING.GPLv2.txt). ConvertKit itself is MIT-licensed and runs"
  echo "these as separate programs."
  echo
  echo "Components:"
  echo "$sources" | while IFS='|' read -r name version url sha256; do
    [ -n "$name" ] || continue
    echo "  $name $version"
    echo "    $url"
    echo "    sha256 $sha256"
  done
  echo
  echo "Build adjustments (made by scripts/build-ffmpeg.sh, not in the tarballs):"
  echo "  libvorbis: configure no longer passes -force_cpusubtype_ALL to the linker."
  echo
  echo "Corresponding source: every release of ConvertKit is published together"
  echo "with ConvertKit_<version>_third-party-source.tar.gz, which holds the"
  echo "exact tarballs above and scripts/build-ffmpeg.sh, the script that built"
  echo "them."
  echo
  echo "FFmpeg configuration:"
  echo "$configuration"
} > "$runtime_directory/NOTICE.txt"

# --- Verify -----------------------------------------------------------------

verify_dependencies() {
  otool -L "$1" | tail -n +2 | awk '{ print $1 }' | while IFS= read -r dependency; do
    case "$dependency" in
      /System/*|/usr/lib/*|@rpath/*|@loader_path/*) ;;
      *)
        echo "$1 depends on a library outside the bundle: $dependency" >&2
        exit 1
        ;;
    esac
  done
}
for file in "$binary_directory/ffmpeg-$target" "$binary_directory/ffprobe-$target" "$runtime_directory"/*.dylib; do
  verify_dependencies "$file"
  [ "$(lipo -archs "$file")" = arm64 ] || {
    echo "$file is not arm64-only" >&2
    exit 1
  }
  codesign --force --sign - "$file" >/dev/null 2>&1
done

case "$configuration" in
  *--enable-gpl*) ;;
  *) echo "The FFmpeg build is not configured with --enable-gpl" >&2; exit 1 ;;
esac
case "$configuration" in
  *--enable-version3*|*--enable-nonfree*)
    echo "The FFmpeg build must stay GPLv2-or-later and redistributable" >&2
    exit 1
    ;;
esac
encoders=$("$binary_directory/ffmpeg-$target" -hide_banner -encoders)
for encoder in libx264 libx265 libvpx-vp9 libopus libmp3lame libvorbis aac ffv1 flac h264_videotoolbox aac_at; do
  printf '%s\n' "$encoders" | grep -q " $encoder " || {
    echo "The bundled FFmpeg is missing the $encoder encoder" >&2
    exit 1
  }
done
"$binary_directory/ffprobe-$target" -hide_banner -version >/dev/null
if /usr/bin/strings "$binary_directory/ffmpeg-$target" "$binary_directory/ffprobe-$target" \
  "$runtime_directory"/*.dylib | grep -F "$repo_root" >/dev/null; then
  echo "The bundled FFmpeg embeds a path from this checkout" >&2
  exit 1
fi

echo "Staged the bundled FFmpeg $(echo "$sources" | awk -F'|' '$1 == "ffmpeg" { print $2 }') ($(du -sh "$runtime_directory" | awk '{ print $1 }') of libraries)"
