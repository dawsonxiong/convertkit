#!/bin/sh
set -eu

usage() {
  echo "Usage: scripts/smoke-whisper-runtime.sh [--verify-only] /path/to/ConvertKit.app" >&2
  exit 2
}

verify_only=0
if [ "${1:-}" = "--verify-only" ]; then
  verify_only=1
  shift
fi
[ "$#" -eq 1 ] || usage

app_path=$(CDPATH= cd -- "$1" && pwd) || {
  echo "The packaged app does not exist: $1" >&2
  exit 1
}
target=${CARGO_BUILD_TARGET:-$(rustc --print host-tuple)}

case "$target" in
  aarch64-apple-darwin) architecture=arm64 ;;
  x86_64-apple-darwin) architecture=x86_64 ;;
  *)
    echo "Whisper runtime smoke checks support macOS targets only: $target" >&2
    exit 1
    ;;
esac

contents="$app_path/Contents"
main_binary="$contents/MacOS/convertkit"
vision_binary="$contents/MacOS/convertkit-vision-ocr"
whisper_binary="$contents/MacOS/whisper-cli"
runtime_directory="$contents/Resources/whisper-runtime"
backend_directory="$runtime_directory/backends"
manifest="$runtime_directory/runtime-manifest.json"

for required_path in \
  "$main_binary" \
  "$vision_binary" \
  "$whisper_binary" \
  "$manifest" \
  "$runtime_directory/libwhisper.1.dylib" \
  "$runtime_directory/libggml.0.dylib" \
  "$runtime_directory/libggml-base.0.dylib" \
  "$runtime_directory/libomp.dylib"
do
  [ -f "$required_path" ] || {
    echo "The packaged app is missing: $required_path" >&2
    exit 1
  }
done

grep -Fq "\"target\": \"$target\"" "$manifest" || {
  echo "The runtime manifest does not match $target" >&2
  exit 1
}

require_architecture() {
  runtime_file=$1
  runtime_architectures=$(lipo -archs "$runtime_file")
  case " $runtime_architectures " in
    *" $architecture "*) ;;
    *)
      echo "$runtime_file does not contain the required $architecture architecture" >&2
      exit 1
      ;;
  esac
}

verify_dependencies() {
  runtime_file=$1
  otool -L "$runtime_file" | tail -n +2 | awk '{ print $1 }' | while IFS= read -r dependency; do
    case "$dependency" in
      /System/*|/usr/lib/*|@loader_path/*|@executable_path/*) ;;
      *)
        echo "$runtime_file retains a non-system dependency: $dependency" >&2
        exit 1
        ;;
    esac
  done
}

for packaged_binary in "$main_binary" "$vision_binary" "$whisper_binary"; do
  require_architecture "$packaged_binary"
done

backend_count=0
for runtime_file in "$whisper_binary" "$runtime_directory"/*.dylib "$backend_directory"/*.so; do
  [ -f "$runtime_file" ] || continue
  require_architecture "$runtime_file"
  verify_dependencies "$runtime_file"
  case "$runtime_file" in "$backend_directory"/*.so) backend_count=$((backend_count + 1)) ;; esac
done
[ "$backend_count" -gt 0 ] || {
  echo "The packaged Whisper runtime has no loadable backends" >&2
  exit 1
}
grep -Fq "\"backendCount\": $backend_count" "$manifest" || {
  echo "The runtime manifest backend count does not match the app bundle" >&2
  exit 1
}

if [ "$verify_only" -eq 1 ]; then
  echo "Verified packaged $target Whisper runtime with $backend_count backends."
  exit 0
fi

for command_name in curl ffmpeg say shasum stat; do
  command -v "$command_name" >/dev/null 2>&1 || {
    echo "Missing smoke-test command: $command_name" >&2
    exit 1
  }
done

temporary_root=${TMPDIR:-/tmp}
smoke_directory=$(mktemp -d "$temporary_root/convertkit-whisper-smoke.XXXXXX")
cleanup() {
  case "$smoke_directory" in
    "$temporary_root"/convertkit-whisper-smoke.*) /bin/rm -rf -- "$smoke_directory" ;;
    *) echo "Refusing to clean an unexpected smoke directory: $smoke_directory" >&2 ;;
  esac
}
trap cleanup EXIT HUP INT TERM

model_path="$smoke_directory/ggml-tiny.bin"
model_url=https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.bin
expected_size=77691713
expected_sha1=bd577a113a864445d4c299885e0cb97d4ba92b5f

if [ -n "${CONVERTKIT_WHISPER_MODEL_PATH:-}" ]; then
  [ -f "$CONVERTKIT_WHISPER_MODEL_PATH" ] || {
    echo "The supplied Tiny model does not exist: $CONVERTKIT_WHISPER_MODEL_PATH" >&2
    exit 1
  }
  /bin/cp "$CONVERTKIT_WHISPER_MODEL_PATH" "$model_path"
else
  curl --fail --location --retry 3 --output "$model_path" "$model_url"
fi
actual_size=$(stat -f %z "$model_path")
[ "$actual_size" = "$expected_size" ] || {
  echo "The Tiny model size did not match its pinned manifest" >&2
  exit 1
}
actual_sha1=$(shasum -a 1 "$model_path" | awk '{ print $1 }')
[ "$actual_sha1" = "$expected_sha1" ] || {
  echo "The Tiny model checksum did not match its pinned manifest" >&2
  exit 1
}

speech_path="$smoke_directory/speech.aiff"
wave_path="$smoke_directory/speech.wav"
output_base="$smoke_directory/transcript"
output_path="$output_base.txt"

say -o "$speech_path" "The local transcription runtime works in this packaged app."
ffmpeg \
  -hide_banner \
  -loglevel error \
  -i "$speech_path" \
  -vn \
  -sn \
  -dn \
  -map 0:a:0 \
  -acodec pcm_s16le \
  -ar 16000 \
  -ac 1 \
  -y \
  "$wave_path"

(
  cd "$backend_directory"
  "$whisper_binary" \
    -m "$model_path" \
    -f "$wave_path" \
    -l en \
    -of "$output_base" \
    -otxt \
    -np
)

[ -s "$output_path" ] || {
  echo "The packaged Whisper runtime did not produce a transcript" >&2
  exit 1
}
transcript=$(tr '[:upper:]' '[:lower:]' < "$output_path")
case "$transcript" in
  *runtime*|*transcription*) ;;
  *)
    echo "The packaged Whisper transcript did not contain the spoken smoke phrase" >&2
    exit 1
    ;;
esac

echo "Completed packaged $target Whisper transcription smoke."
