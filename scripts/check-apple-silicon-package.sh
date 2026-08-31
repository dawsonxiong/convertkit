#!/bin/sh

set -eu

fail() {
  printf 'Apple Silicon package gate failed: %s\n' "$1" >&2
  exit 1
}

require_command() {
  command -v "$1" >/dev/null 2>&1 || fail "missing required command: $1"
}

for command_name in node pnpm rustc codesign lipo; do
  require_command "$command_name"
done

[ "$(uname -s)" = Darwin ] || fail "this gate must run on macOS"
[ "$(uname -m)" = arm64 ] || fail "this gate requires a native Apple Silicon shell"

TARGET=aarch64-apple-darwin
HOST_TARGET=$(rustc --print host-tuple)
[ "$HOST_TARGET" = "$TARGET" ] ||
  fail "Rust host is $HOST_TARGET; install and run native aarch64 Rust"

PROJECT_DIRECTORY=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
APP_PATH="$PROJECT_DIRECTORY/src-tauri/target/$TARGET/debug/bundle/macos/ConvertKit.app"
APP_EXECUTABLE="$APP_PATH/Contents/MacOS/convertkit"

cd "$PROJECT_DIRECTORY"

printf 'Building the packaged Apple Silicon debug app...\n'
CARGO_BUILD_TARGET=$TARGET \
  pnpm tauri build --ci --debug --target "$TARGET" --bundles app

[ -d "$APP_PATH" ] || fail "the build did not produce $APP_PATH"

printf 'Verifying packaged Finder file associations...\n'
node scripts/verify-file-associations.mjs \
  --plist "$APP_PATH/Contents/Info.plist"

for packaged_binary in \
  "$APP_EXECUTABLE" \
  "$APP_PATH/Contents/MacOS/convertkit-vision-ocr" \
  "$APP_PATH/Contents/MacOS/convertkit-image-pdf" \
  "$APP_PATH/Contents/MacOS/whisper-cli"
do
  [ -x "$packaged_binary" ] || fail "missing packaged executable: $packaged_binary"
  architectures=$(lipo -archs "$packaged_binary")
  [ "$architectures" = arm64 ] ||
    fail "$packaged_binary has architectures '$architectures', expected arm64"
done

# This is a package-integrity gate, not the credentialed release signature.
# Re-signing ad hoc makes the local contract deterministic even if a developer
# has a signing identity configured in their shell or keychain.
codesign --force --deep --sign - "$APP_PATH" >/dev/null
codesign --verify --deep --strict --verbose=2 "$APP_PATH"
codesign --display --verbose=4 "$APP_PATH" 2>&1 |
  /usr/bin/grep -q '^Signature=adhoc$' || fail "the debug app is not ad-hoc signed"

printf 'Auditing the packaged Apple Silicon runtime...\n'
CARGO_BUILD_TARGET=$TARGET \
  sh scripts/smoke-whisper-runtime.sh --verify-only "$APP_PATH"

printf 'Running the packaged quality-video gate with the resolved FFmpeg installation...\n'
CARGO_BUILD_TARGET=$TARGET \
  sh scripts/smoke-video-quality.sh "$APP_PATH"

printf 'Running the packaged target-size video gate with the resolved FFmpeg installation...\n'
CARGO_BUILD_TARGET=$TARGET \
  sh scripts/smoke-video-target-size.sh "$APP_PATH"

printf 'Running the packaged audio-compression gate with the resolved FFmpeg installation...\n'
CARGO_BUILD_TARGET=$TARGET \
  sh scripts/smoke-audio-compression.sh "$APP_PATH"

printf 'Running the packaged target-size image gate with the resolved ImageMagick installation...\n'
CARGO_BUILD_TARGET=$TARGET \
  sh scripts/smoke-image-target-size.sh "$APP_PATH"

printf 'Running packaged image and PDF utility gates...\n'
for smoke_script in \
  scripts/smoke-image-pdf.sh \
  scripts/smoke-combine-pdf.sh \
  scripts/smoke-ocr.sh \
  scripts/smoke-pdf-compression.sh \
  scripts/smoke-pdf-metadata.sh
do
  CARGO_BUILD_TARGET=$TARGET sh "$smoke_script" "$APP_PATH"
done

printf 'Running dependency-contained packaged archive gates...\n'
for smoke_script in \
  scripts/smoke-archive.sh \
  scripts/smoke-7z-creation.sh \
  scripts/smoke-gzip-creation.sh \
  scripts/smoke-7z-extraction.sh \
  scripts/smoke-gzip-extraction.sh
do
  CARGO_BUILD_TARGET=$TARGET sh "$smoke_script" "$APP_PATH"
done

printf 'Running dependency-contained packaged general utility gates...\n'
for smoke_script in \
  scripts/smoke-inspect.sh \
  scripts/smoke-checksum-manifest.sh \
  scripts/smoke-batch-rename.sh
do
  CARGO_BUILD_TARGET=$TARGET sh "$smoke_script" "$APP_PATH"
done

printf 'Apple Silicon package gate passed: arm64 bundle, ad-hoc signature, runtime, quality and target-size video, audio compression, exact target-size image optimization, image-to-PDF, Combine to PDF, OCR, PDF compression and metadata removal, archive creation/extraction, Inspect with checksum manifests, and Batch Rename.\n'
