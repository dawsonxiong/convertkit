#!/bin/sh
set -eu

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
target=${CARGO_BUILD_TARGET:-$(rustc --print host-tuple)}

case "$target" in
  aarch64-apple-darwin) architecture=arm64 ;;
  x86_64-apple-darwin) architecture=x86_64 ;;
  *)
    echo "The bundled Whisper runtime supports macOS targets only: $target" >&2
    exit 1
    ;;
esac

expected_whisper_version=1.9.2
expected_ggml_version=0.22.0
expected_libomp_version=22.1.8

if ! command -v brew >/dev/null 2>&1; then
  echo "Homebrew is required to stage the pinned Whisper runtime" >&2
  exit 1
fi

formula_version() {
  brew list --versions "$1" 2>/dev/null | awk 'NR == 1 { print $2 }'
}

require_version() {
  formula=$1
  expected=$2
  installed=$(formula_version "$formula")
  if [ "$installed" != "$expected" ]; then
    echo "Expected $formula $expected for the bundled runtime, found ${installed:-nothing}" >&2
    exit 1
  fi
}

require_version whisper-cpp "$expected_whisper_version"
require_version ggml "$expected_ggml_version"
require_version libomp "$expected_libomp_version"

whisper_prefix=$(brew --prefix whisper-cpp)
if [ ! -d "$whisper_prefix" ]; then
  whisper_prefix="$(brew --prefix)/opt/whisper-cpp"
fi
ggml_prefix=$(brew --prefix ggml)
libomp_prefix=$(brew --prefix libomp)
binary_source="$whisper_prefix/bin/whisper-cli"
whisper_library_source="$whisper_prefix/lib/libwhisper.1.dylib"
ggml_library_source="$ggml_prefix/lib/libggml.0.dylib"
ggml_base_library_source="$ggml_prefix/lib/libggml-base.0.dylib"
libomp_library_source="$libomp_prefix/lib/libomp.dylib"
backend_source_directory="$ggml_prefix/libexec"

output_directory="$repo_root/src-tauri/binaries"
binary_output="$output_directory/whisper-cli-$target"
runtime_directory="$repo_root/src-tauri/runtime/whisper"
backend_directory="$runtime_directory/backends"
license_directory="$runtime_directory/licenses"

case "$runtime_directory" in
  "$repo_root/src-tauri/runtime/whisper") ;;
  *)
    echo "Refusing to replace an unexpected runtime directory: $runtime_directory" >&2
    exit 1
    ;;
esac

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

for source_file in \
  "$binary_source" \
  "$whisper_library_source" \
  "$ggml_library_source" \
  "$ggml_base_library_source" \
  "$libomp_library_source"
do
  [ -f "$source_file" ] || {
    echo "Missing Whisper runtime file: $source_file" >&2
    exit 1
  }
  require_architecture "$source_file"
done

backend_count=0
for backend_source in "$backend_source_directory"/*.so; do
  [ -f "$backend_source" ] || continue
  require_architecture "$backend_source"
  backend_count=$((backend_count + 1))
done
if [ "$backend_count" -eq 0 ]; then
  echo "The pinned GGML installation did not provide any runtime backends" >&2
  exit 1
fi

/bin/rm -rf -- "$runtime_directory"
mkdir -p "$output_directory" "$backend_directory" "$license_directory"

/bin/cp -L "$binary_source" "$binary_output"
/bin/cp -L "$whisper_library_source" "$runtime_directory/libwhisper.1.dylib"
/bin/cp -L "$ggml_library_source" "$runtime_directory/libggml.0.dylib"
/bin/cp -L "$ggml_base_library_source" "$runtime_directory/libggml-base.0.dylib"
/bin/cp -L "$libomp_library_source" "$runtime_directory/libomp.dylib"
for backend_source in "$backend_source_directory"/*.so; do
  [ -f "$backend_source" ] || continue
  /bin/cp -L "$backend_source" "$backend_directory/${backend_source##*/}"
done

/bin/cp -L "$whisper_prefix/LICENSE" "$license_directory/LICENSE-whisper-cpp.txt"
/bin/cp -L "$ggml_prefix/LICENSE" "$license_directory/LICENSE-ggml.txt"
/bin/cp -L "$libomp_prefix/LICENSE.TXT" "$license_directory/LICENSE-libomp.txt"

chmod u+w "$binary_output" "$runtime_directory"/*.dylib "$backend_directory"/*.so

install_name_tool \
  -change @rpath/libwhisper.1.dylib @executable_path/../Resources/whisper-runtime/libwhisper.1.dylib \
  -change /opt/homebrew/opt/ggml/lib/libggml.0.dylib @executable_path/../Resources/whisper-runtime/libggml.0.dylib \
  -change /usr/local/opt/ggml/lib/libggml.0.dylib @executable_path/../Resources/whisper-runtime/libggml.0.dylib \
  -change /opt/homebrew/opt/ggml/lib/libggml-base.0.dylib @executable_path/../Resources/whisper-runtime/libggml-base.0.dylib \
  -change /usr/local/opt/ggml/lib/libggml-base.0.dylib @executable_path/../Resources/whisper-runtime/libggml-base.0.dylib \
  "$binary_output"

install_name_tool \
  -id @loader_path/libwhisper.1.dylib \
  -change /opt/homebrew/opt/ggml/lib/libggml.0.dylib @loader_path/libggml.0.dylib \
  -change /usr/local/opt/ggml/lib/libggml.0.dylib @loader_path/libggml.0.dylib \
  -change /opt/homebrew/opt/ggml/lib/libggml-base.0.dylib @loader_path/libggml-base.0.dylib \
  -change /usr/local/opt/ggml/lib/libggml-base.0.dylib @loader_path/libggml-base.0.dylib \
  "$runtime_directory/libwhisper.1.dylib"

install_name_tool \
  -id @loader_path/libggml.0.dylib \
  -change @rpath/libggml-base.0.dylib @loader_path/libggml-base.0.dylib \
  "$runtime_directory/libggml.0.dylib"

install_name_tool \
  -id @loader_path/libggml-base.0.dylib \
  -change /opt/homebrew/opt/libomp/lib/libomp.dylib @loader_path/libomp.dylib \
  -change /usr/local/opt/libomp/lib/libomp.dylib @loader_path/libomp.dylib \
  "$runtime_directory/libggml-base.0.dylib"

install_name_tool -id @loader_path/libomp.dylib "$runtime_directory/libomp.dylib"

embedded_backend_directory=$(
  strings "$runtime_directory/libggml.0.dylib" |
    awk 'index($0, "/Cellar/ggml/") && $0 ~ /\/libexec$/ { print; exit }'
)
if [ -z "$embedded_backend_directory" ]; then
  echo "Could not locate GGML's compiled backend directory" >&2
  exit 1
fi
# Homebrew bakes its Cellar backend directory into libggml. Replace that string with
# the current directory without changing the Mach-O size; ConvertKit launches the
# bundled sidecar from its signed backend resource directory.
EMBEDDED_BACKEND_DIRECTORY="$embedded_backend_directory" /usr/bin/perl -0pi -e '
  BEGIN {
    $old = $ENV{"EMBEDDED_BACKEND_DIRECTORY"};
    $new = "." . ("\0" x (length($old) - 1));
    $count = 0;
  }
  $count += s/\Q$old\E/$new/g;
  END { die "Expected one compiled GGML backend directory, found $count\n" unless $count == 1; }
' "$runtime_directory/libggml.0.dylib"

for backend_file in "$backend_directory"/*.so; do
  install_name_tool \
    -change @rpath/libggml-base.0.dylib @loader_path/../libggml-base.0.dylib \
    -change /opt/homebrew/opt/libomp/lib/libomp.dylib @loader_path/../libomp.dylib \
    -change /usr/local/opt/libomp/lib/libomp.dylib @loader_path/../libomp.dylib \
    "$backend_file"
done

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

for runtime_file in "$binary_output" "$runtime_directory"/*.dylib "$backend_directory"/*.so; do
  verify_dependencies "$runtime_file"
  codesign --force --sign - "$runtime_file" >/dev/null
done

printf '%s\n' \
  '{' \
  "  \"target\": \"$target\"," \
  "  \"whisperCpp\": \"$expected_whisper_version\"," \
  "  \"ggml\": \"$expected_ggml_version\"," \
  "  \"libomp\": \"$expected_libomp_version\"," \
  "  \"backendCount\": $backend_count" \
  '}' > "$runtime_directory/runtime-manifest.json"

chmod 755 "$binary_output"
