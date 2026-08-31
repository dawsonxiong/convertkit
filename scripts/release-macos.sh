#!/bin/sh

set -eu

mode=${1:-release}
case "$mode" in
  release | --check | --doctor) ;;
  *)
    echo "Usage: scripts/release-macos.sh [release|--check|--doctor]" >&2
    exit 2
    ;;
esac

project_dir=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
temporary_dir=""

cleanup() {
  if [ -n "$temporary_dir" ] && [ -d "$temporary_dir" ]; then
    rm -f "$temporary_dir"/AuthKey_*.p8 "$temporary_dir"/build-start
    rmdir "$temporary_dir" 2>/dev/null || true
  fi
}
trap cleanup EXIT HUP INT TERM

require_command() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "Missing required command: $1" >&2
    exit 1
  fi
}

for command_name in op pnpm security codesign xcrun spctl; do
  require_command "$command_name"
done

if ! op account list --format=json >/dev/null 2>&1; then
  echo "1Password CLI account configuration could not be read" >&2
  exit 1
fi

reference_is_valid() {
  case "$1" in
    op://*/*/*) return 0 ;;
    *) return 1 ;;
  esac
}

require_reference() {
  variable_name=$1
  reference=$2
  if [ -z "$reference" ]; then
    echo "Missing required secret reference: $variable_name" >&2
    exit 1
  fi
  if ! reference_is_valid "$reference"; then
    echo "$variable_name must be an op:// secret reference" >&2
    exit 1
  fi
}

signing_identity_ref=${CONVERTKIT_SIGNING_IDENTITY_REF:-}
api_issuer_ref=${CONVERTKIT_APPLE_API_ISSUER_REF:-}
api_key_id_ref=${CONVERTKIT_APPLE_API_KEY_ID_REF:-}
api_private_key_ref=${CONVERTKIT_APPLE_API_PRIVATE_KEY_REF:-}

if [ "$mode" = "--doctor" ]; then
  configured=0
  for reference in \
    "$signing_identity_ref" \
    "$api_issuer_ref" \
    "$api_key_id_ref" \
    "$api_private_key_ref"
  do
    if [ -n "$reference" ]; then
      configured=$((configured + 1))
      if ! reference_is_valid "$reference"; then
        echo "Release reference $configured is not a valid op:// reference" >&2
        exit 1
      fi
    fi
  done
  echo "Release tooling is available. 1Password references configured: $configured/4."
  exit 0
fi

require_reference CONVERTKIT_SIGNING_IDENTITY_REF "$signing_identity_ref"
require_reference CONVERTKIT_APPLE_API_ISSUER_REF "$api_issuer_ref"
require_reference CONVERTKIT_APPLE_API_KEY_ID_REF "$api_key_id_ref"
require_reference CONVERTKIT_APPLE_API_PRIVATE_KEY_REF "$api_private_key_ref"

temporary_dir=$(mktemp -d "${TMPDIR:-/tmp}/convertkit-release.XXXXXX")

signing_identity=$(op read --no-newline "$signing_identity_ref")
api_issuer=$(op read --no-newline "$api_issuer_ref")
api_key_id=$(op read --no-newline "$api_key_id_ref")

if [ -z "$signing_identity" ] || [ -z "$api_issuer" ] || [ -z "$api_key_id" ]; then
  echo "A required 1Password field resolved to an empty value" >&2
  exit 1
fi
case "$signing_identity" in
  "Developer ID Application: "*) ;;
  *)
    echo "The signing identity must be a Developer ID Application identity" >&2
    exit 1
    ;;
esac
case "$api_key_id" in
  *[!A-Za-z0-9]* | "")
    echo "The App Store Connect key ID must contain only letters and numbers" >&2
    exit 1
    ;;
esac
case "$api_issuer" in
  ????????-????-????-????-????????????) ;;
  *)
    echo "The App Store Connect issuer must use UUID format" >&2
    exit 1
    ;;
esac

private_key_path="$temporary_dir/AuthKey_${api_key_id}.p8"
op read --force --file-mode 0600 --out-file "$private_key_path" "$api_private_key_ref" >/dev/null
if ! grep -q '^-----BEGIN PRIVATE KEY-----' "$private_key_path"; then
  echo "The private-key reference did not resolve to an App Store Connect .p8 key" >&2
  exit 1
fi
if ! security find-identity -v -p codesigning | grep -F -- "\"$signing_identity\"" >/dev/null; then
  echo "The referenced Developer ID Application identity is not installed in the keychain" >&2
  exit 1
fi

if [ "$mode" = "--check" ]; then
  echo "Release references resolve and the signing identity is available."
  exit 0
fi

touch "$temporary_dir/build-start"
cd "$project_dir"

APPLE_SIGNING_IDENTITY=$signing_identity \
APPLE_API_ISSUER=$api_issuer \
APPLE_API_KEY=$api_key_id \
APPLE_API_KEY_PATH=$private_key_path \
  pnpm tauri build --bundles app,dmg

app_path="$project_dir/src-tauri/target/release/bundle/macos/ConvertKit.app"
dmg_path=$(find "$project_dir/src-tauri/target/release/bundle/dmg" \
  -type f -name 'ConvertKit_*.dmg' -newer "$temporary_dir/build-start" -print | head -n 1)

if [ ! -d "$app_path" ] || [ -z "$dmg_path" ] || [ ! -f "$dmg_path" ]; then
  echo "The signed app or DMG was not produced" >&2
  exit 1
fi

codesign --verify --deep --strict --verbose=2 "$app_path"
xcrun stapler validate "$app_path"
xcrun stapler validate "$dmg_path"
spctl --assess --type execute --verbose=2 "$app_path"
spctl --assess --type open --context context:primary-signature --verbose=2 "$dmg_path"

echo "Verified notarized release: $dmg_path"
