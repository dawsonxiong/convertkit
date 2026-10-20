#!/bin/sh
# Build the whisper-cli that ships inside ConvertKit from a pinned, checksummed
# whisper.cpp release: one static arm64 executable with GGML's CPU, Accelerate
# BLAS and Metal backends built in and the Metal shaders embedded, so there is
# no backend folder, OpenMP runtime or dylib to ship next to it.
#
# Outputs (all gitignored):
#   src-tauri/binaries/whisper-cli-<target>   sidecar executable
#   src-tauri/runtime/whisper/                licences, NOTICE, manifest
#   src-tauri/whisper-build/                  source tarball and cached build
set -eu

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
target=${CARGO_BUILD_TARGET:-$(rustc --print host-tuple)}
if [ "$target" != aarch64-apple-darwin ]; then
  echo "The bundled Whisper runtime is built for Apple Silicon only, not $target" >&2
  exit 1
fi

jobs=$(sysctl -n hw.ncpu)

# build-stamp-begin
# Accelerate's current BLAS interface (cblas_sgemm$NEWLAPACK) starts at 13.3.
deployment_target=13.3
whisper_version=1.9.2
whisper_url="https://github.com/ggml-org/whisper.cpp/archive/refs/tags/v${whisper_version}.tar.gz"
whisper_sha256=a6abd064fcca8b85e794d205abf328c522e9451db43a3eadc178b883b7d0e9cd
# The M1 baseline every Apple Silicon Mac supports. GGML_NATIVE would tune the
# build to whichever Mac or runner compiled it.
cpu_arch=armv8.5-a+dotprod+fp16
# Build tool only; never shipped.
cmake_version=4.4.4
cmake_url="https://github.com/Kitware/CMake/releases/download/v${cmake_version}/cmake-${cmake_version}-macos-universal.tar.gz"
cmake_sha256=4b7b73704b1db9b374e5c9ab8e17ac6148b817b6396ee75cd94a852cbac9d305
# build-stamp-end

build_root="$repo_root/src-tauri/whisper-build"
source_archive="$build_root/whisper.cpp-${whisper_version}.tar.gz"
cmake_archive="$build_root/cmake-${cmake_version}.tar.gz"
work_directory="$build_root/work"
source_directory="$work_directory/whisper.cpp"
built_binary="$build_root/whisper-cli"
binary_output="$repo_root/src-tauri/binaries/whisper-cli-$target"
runtime_directory="$repo_root/src-tauri/runtime/whisper"
license_directory="$runtime_directory/licenses"

# Rebuild only when a pin or build step between the build-stamp markers or in
# the build section changes.
section() {
  /usr/bin/sed -n "/^# $1-stamp-begin\$/,/^# $1-stamp-end\$/p" "$0"
}
stamp=$( (section build; section whisper) | /usr/bin/shasum -a 256 | /usr/bin/awk '{ print $1 }')

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
  /bin/rm -rf "$2"
  mkdir -p "$2"
  /usr/bin/tar -xf "$1" -C "$2" --strip-components 1
}

mkdir -p "$build_root"
fetch "$whisper_url" "$source_archive" "$whisper_sha256"

# whisper-stamp-begin
if [ "$(cat "$build_root/.whisper-stamp" 2>/dev/null || true)" != "$stamp" ] || [ ! -f "$built_binary" ]; then
  echo "Building whisper-cli from pinned sources..."
  /bin/rm -rf "$work_directory" "$built_binary"
  mkdir -p "$work_directory"
  fetch "$cmake_url" "$cmake_archive" "$cmake_sha256"
  unpack "$cmake_archive" "$work_directory/cmake"
  cmake="$work_directory/cmake/CMake.app/Contents/bin/cmake"
  unpack "$source_archive" "$source_directory"

  # Only Apple's toolchain and SDK are visible, so nothing from Homebrew or
  # /usr/local leaks in. GGML_ASSERT embeds __FILE__; the prefix map keeps
  # this checkout's path out of the shipped binary.
  path_flags="-ffile-prefix-map=$source_directory=whisper.cpp"
  PATH=/usr/bin:/bin:/usr/sbin:/sbin \
    "$cmake" -S "$source_directory" -B "$work_directory/build" \
    -DCMAKE_BUILD_TYPE=Release \
    -DCMAKE_C_COMPILER=clang \
    -DCMAKE_CXX_COMPILER=clang++ \
    -DCMAKE_C_FLAGS="$path_flags" \
    -DCMAKE_CXX_FLAGS="$path_flags" \
    -DCMAKE_OSX_ARCHITECTURES=arm64 \
    -DCMAKE_OSX_DEPLOYMENT_TARGET="$deployment_target" \
    -DCMAKE_FIND_FRAMEWORK=LAST \
    -DCMAKE_IGNORE_PREFIX_PATH="/opt/homebrew;/usr/local" \
    -DBUILD_SHARED_LIBS=OFF \
    -DGGML_BACKEND_DL=OFF \
    -DGGML_NATIVE=OFF \
    -DGGML_CPU_ARM_ARCH="$cpu_arch" \
    -DGGML_OPENMP=OFF \
    -DGGML_ACCELERATE=ON \
    -DGGML_BLAS=ON \
    -DGGML_BLAS_VENDOR=Apple \
    -DGGML_METAL=ON \
    -DGGML_METAL_EMBED_LIBRARY=ON \
    -DGGML_CCACHE=OFF \
    -DWHISPER_BUILD_EXAMPLES=ON \
    -DWHISPER_BUILD_TESTS=OFF \
    -DWHISPER_BUILD_SERVER=OFF \
    -DWHISPER_CURL=OFF \
    -DWHISPER_SDL2=OFF
  PATH=/usr/bin:/bin:/usr/sbin:/sbin \
    "$cmake" --build "$work_directory/build" --target whisper-cli --parallel "$jobs"
  /bin/cp "$work_directory/build/bin/whisper-cli" "$built_binary"
  printf '%s\n' "$stamp" > "$build_root/.whisper-stamp"
fi
# whisper-stamp-end

# --- Stage the sidecar, licences and notice ---------------------------------

case "$runtime_directory" in
  "$repo_root/src-tauri/runtime/whisper") ;;
  *)
    echo "Refusing to replace an unexpected runtime directory: $runtime_directory" >&2
    exit 1
    ;;
esac
/bin/rm -rf -- "$runtime_directory"
mkdir -p "$(dirname "$binary_output")" "$license_directory"

/bin/cp "$built_binary" "$binary_output"
chmod 755 "$binary_output"

# whisper.cpp's LICENSE covers whisper.cpp and the GGML copy it vendors.
/usr/bin/tar -xOf "$source_archive" "whisper.cpp-${whisper_version}/LICENSE" \
  > "$license_directory/whisper.cpp-LICENSE.txt"

{
  echo "ConvertKit bundles whisper-cli from whisper.cpp $whisper_version, built from"
  echo "the upstream release below by scripts/build-whisper.sh."
  echo
  echo "  whisper.cpp $whisper_version"
  echo "    $whisper_url"
  echo "    sha256 $whisper_sha256"
  echo
  echo "whisper.cpp and the GGML library it includes are MIT-licensed"
  echo "(licenses/whisper.cpp-LICENSE.txt). whisper-cli also compiles in"
  echo "miniaudio (public domain or MIT-0) and stb_vorbis (public domain or MIT)"
  echo "from the same release; their terms are at the end of each source file."
  echo
  echo "Build: static arm64, macOS $deployment_target or later, CPU ($cpu_arch),"
  echo "Accelerate BLAS and Metal backends built in, Metal shaders embedded,"
  echo "no OpenMP. No changes to the upstream sources."
} > "$runtime_directory/NOTICE.txt"

printf '%s\n' \
  '{' \
  "  \"target\": \"$target\"," \
  "  \"whisperCpp\": \"$whisper_version\"," \
  '  "backends": "static"' \
  '}' > "$runtime_directory/runtime-manifest.json"

# --- Verify -----------------------------------------------------------------

otool -L "$binary_output" | tail -n +2 | awk '{ print $1 }' | while IFS= read -r dependency; do
  case "$dependency" in
    /System/*|/usr/lib/*) ;;
    *)
      echo "whisper-cli depends on a library outside macOS: $dependency" >&2
      exit 1
      ;;
  esac
done
[ "$(lipo -archs "$binary_output")" = arm64 ] || {
  echo "$binary_output is not arm64-only" >&2
  exit 1
}
if /usr/bin/strings "$binary_output" | grep -F "$repo_root" >/dev/null; then
  echo "The bundled whisper-cli embeds a path from this checkout" >&2
  exit 1
fi
codesign --force --sign - "$binary_output" >/dev/null 2>&1
"$binary_output" --help >/dev/null 2>&1 || {
  echo "The bundled whisper-cli does not run" >&2
  exit 1
}

echo "Staged the bundled whisper-cli $whisper_version ($(du -h "$binary_output" | awk '{ print $1 }'))"
