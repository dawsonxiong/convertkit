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
manifest="$runtime_directory/runtime-manifest.json"

for required_path in \
  "$main_binary" \
  "$vision_binary" \
  "$whisper_binary" \
  "$manifest" \
  "$runtime_directory/NOTICE.txt" \
  "$runtime_directory/licenses/whisper.cpp-LICENSE.txt"
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
grep -Fq '"backends": "static"' "$manifest" || {
  echo "The runtime manifest does not describe the static Whisper build" >&2
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

for packaged_binary in "$main_binary" "$vision_binary" "$whisper_binary"; do
  require_architecture "$packaged_binary"
done

# The backends are built into whisper-cli, so it links only macOS itself and
# its runtime folder holds no libraries or plug-ins for GGML to load.
otool -L "$whisper_binary" | tail -n +2 | awk '{ print $1 }' | while IFS= read -r dependency; do
  case "$dependency" in
    /System/*|/usr/lib/*) ;;
    *)
      echo "$whisper_binary links a library outside macOS: $dependency" >&2
      exit 1
      ;;
  esac
done
for unexpected in "$runtime_directory"/*.dylib "$runtime_directory"/*.so "$runtime_directory"/backends; do
  [ ! -e "$unexpected" ] || {
    echo "The packaged Whisper runtime still ships a loadable library: $unexpected" >&2
    exit 1
  }
done

if [ "$verify_only" -eq 1 ]; then
  echo "Verified packaged $target Whisper runtime."
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
  cd "$runtime_directory"
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
