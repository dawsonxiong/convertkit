#!/bin/sh
set -eu

script_directory=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)

sh "$script_directory/build-vision-ocr.sh"
sh "$script_directory/build-image-pdf.sh"
sh "$script_directory/build-imageio.sh"
sh "$script_directory/build-pdf.sh"
sh "$script_directory/build-ffmpeg.sh"
sh "$script_directory/stage-document-tools.sh"
sh "$script_directory/stage-whisper-runtime.sh"
