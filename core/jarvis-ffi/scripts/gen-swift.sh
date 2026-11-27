#!/usr/bin/env bash
# Builds the host library and generates the Swift bindings (JarvisCore.swift), the C header
# (JarvisCoreFFI.h) and the Clang module map (JarvisCoreFFI.modulemap).
#
#   scripts/gen-swift.sh [OUT_DIR]          default: core/jarvis-ffi/generated
#
# Environment:
#   JARVIS_FFI_PROFILE   debug (default) or release
#   JARVIS_FFI_FEATURES  cargo features for jarvis-ffi, e.g. "panic-hooks" (smoke tests only)
#
# Prints the path of the built host library on the last line of stdout.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
crate="$(dirname "$here")"
core="$(dirname "$crate")"
out="${1:-$crate/generated}"
profile="${JARVIS_FFI_PROFILE:-debug}"
features="${JARVIS_FFI_FEATURES:-}"

case "$profile" in
  debug) profile_flag=() ;;
  release) profile_flag=(--release) ;;
  *) echo "gen-swift: JARVIS_FFI_PROFILE must be debug or release, got '$profile'" >&2; exit 2 ;;
esac
feature_flag=()
if [ -n "$features" ]; then
  feature_flag=(--features "$features")
fi

case "$(uname -s)" in
  Darwin) dylib_ext=dylib ;;
  *) dylib_ext=so ;;
esac

cd "$core"
# Library and bindgen in ONE cargo invocation with the same features: a separate
# `cargo run --bin uniffi-bindgen` with other features would rebuild (and overwrite) the
# library without them.
cargo build -p jarvis-ffi --lib --bin uniffi-bindgen "${profile_flag[@]}" "${feature_flag[@]}" >&2

target_dir="$(cargo metadata --format-version 1 --no-deps \
  | sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p')"
target_dir="${target_dir:-$core/target}"
lib="$target_dir/$profile/libjarvis_core_ffi.$dylib_ext"
bindgen="$target_dir/$profile/uniffi-bindgen"
[ -f "$lib" ] || { echo "gen-swift: $lib was not built" >&2; exit 1; }

mkdir -p "$out"
rm -f "$out"/*.swift "$out"/*.h "$out"/*.modulemap
# The bindgen reads uniffi.toml (Swift module names) via cargo metadata, so run it from the
# workspace. swift-format is optional; without it the bindgen prints a warning only.
"$bindgen" generate --library "$lib" --language swift --out-dir "$out" >&2

for f in JarvisCore.swift JarvisCoreFFI.h JarvisCoreFFI.modulemap; do
  [ -s "$out/$f" ] || { echo "gen-swift: $out/$f missing" >&2; exit 1; }
done
echo "gen-swift: bindings in $out" >&2
echo "$lib"
