#!/usr/bin/env bash
# Builds the Rust core library and puts the generated UniFFI files where Package.swift
# expects them. Run it once after cloning and again whenever core/ changes.
#
#   scripts/bootstrap.sh [CORE_DIR]
#
# CORE_DIR (or $JARVIS_CORE_DIR) is the Rust workspace; default: <repo>/core.
# CARGO_TARGET_DIR is honoured (cargo reads it), so the core tree can stay read-only.
#
# Host mode (Linux, or macOS with JARVIS_LINK=host):
#   Generated/JarvisCoreFFI/{JarvisCoreFFI.h,module.modulemap}   the C module
#   Generated/lib/libjarvis_core_ffi.a                           host static library
#   Sources/JarvisCore/JarvisCore.swift                          generated bindings
# xcframework mode (macOS default): runs core/jarvis-ffi/scripts/build-xcframework.sh and
#   installs Frameworks/JarvisCoreFFI.xcframework (iOS device, iOS simulator and macOS
#   slices; the binaryTarget Package.swift uses on Apple platforms, so `swift test` works on
#   the Mac host too) plus Sources/JarvisCore/JarvisCore.swift from that build. The host
#   library is not built in this mode: it would recompile the whole Rust tree for nothing.
#
# All outputs are .gitignored. Environment:
#   JARVIS_CORE_DIR   Rust workspace (overridden by the first argument)
#   JARVIS_LINK       host | xcframework   (macOS only; default xcframework)
#   JARVIS_FFI_PROFILE release (default) | debug   profile of the host static library
#                      (host mode only; the xcframework is always release)
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
shell="$(dirname "$here")"
repo="$(dirname "$shell")"
core_arg="${1:-${JARVIS_CORE_DIR:-$repo/core}}"
profile="${JARVIS_FFI_PROFILE:-release}"

die() { echo "bootstrap: $*" >&2; exit 1; }

[ -f "$core_arg/Cargo.toml" ] || die "no Rust workspace at $core_arg (pass CORE_DIR or set JARVIS_CORE_DIR)"
[ -f "$core_arg/jarvis-ffi/Cargo.toml" ] || die "$core_arg has no jarvis-ffi crate"
core="$(cd "$core_arg" && pwd)"
command -v cargo >/dev/null 2>&1 || die "cargo not found (install Rust with rustup)"

case "$profile" in
  release) profile_flag=(--release) ;;
  debug) profile_flag=() ;;
  *) die "JARVIS_FFI_PROFILE must be release or debug, got '$profile'" ;;
esac

gen="$shell/Generated"
swift_out="$shell/Sources/JarvisCore"
mkdir -p "$swift_out"

if [ "$(uname -s)" = "Darwin" ] && [ "${JARVIS_LINK:-xcframework}" = "xcframework" ]; then
  mode=xcframework
else
  mode=host
fi

cd "$core"
if [ "$mode" = "xcframework" ]; then
  xc_script="$core/jarvis-ffi/scripts/build-xcframework.sh"
  [ -x "$xc_script" ] || die "$xc_script not found or not executable"
  xc_out="$gen/xcframework-build"
  rm -rf "$xc_out"
  mkdir -p "$xc_out"
  echo "bootstrap: building the xcframework with $xc_script" >&2
  "$xc_script" "$xc_out" >&2
  [ -d "$xc_out/JarvisCore.xcframework" ] || die "build-xcframework.sh produced no xcframework"
  mkdir -p "$shell/Frameworks"
  rm -rf "$shell/Frameworks/JarvisCoreFFI.xcframework"
  # Renamed so the binaryTarget is named for the C module it provides (JarvisCoreFFI)
  # and does not collide with the Swift target JarvisCore. A library xcframework's
  # Info.plist does not contain its own name, so the rename is safe.
  cp -R "$xc_out/JarvisCore.xcframework" "$shell/Frameworks/JarvisCoreFFI.xcframework"
  # The Swift bindings must match the xcframework's library, so use its copy.
  cp "$xc_out/JarvisCore.swift" "$swift_out/JarvisCore.swift"
  [ -s "$swift_out/JarvisCore.swift" ] || die "no JarvisCore.swift installed"
else
  # Build into a staging directory and swap at the end: a failed build (for example a core
  # that does not compile yet) must leave the previous bindings in place and Shell buildable.
  final_gen="$gen"
  gen="$shell/Generated.new"
  rm -rf "$gen"
  mkdir -p "$gen/JarvisCoreFFI" "$gen/lib"
  target_dir="${CARGO_TARGET_DIR:-}"
  if [ -z "$target_dir" ]; then
    target_dir="$(cargo metadata --format-version 1 --no-deps \
      | sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p')"
  fi
  target_dir="${target_dir:-$core/target}"

  # 1. The bindings generator (debug, feature `cli`). Built first: building a bin also
  #    uplifts the library's crate types into target/debug, which must not overwrite the
  #    static library built next when the profile is debug.
  echo "bootstrap: building uniffi-bindgen-swift" >&2
  cargo build -p jarvis-ffi --bin uniffi-bindgen-swift >&2
  bindgen="$target_dir/debug/uniffi-bindgen-swift"
  [ -x "$bindgen" ] || die "$bindgen was not built"

  # 2. The host static library, built like build-xcframework.sh builds the device library:
  #    staticlib only (so release gets LTO) and without the generator (`cli`) inside.
  echo "bootstrap: building the host static library ($profile)" >&2
  cargo rustc -p jarvis-ffi --lib ${profile_flag[@]+"${profile_flag[@]}"} --no-default-features \
    --crate-type staticlib >&2
  lib="$target_dir/$profile/libjarvis_core_ffi.a"
  [ -f "$lib" ] || die "$lib was not built"
  cp "$lib" "$gen/lib/libjarvis_core_ffi.a"

  # 3. Bindings from exactly the library we link: Swift sources, header, and a plain
  #    (non-framework) module map named module.modulemap so the directory is a Clang module.
  echo "bootstrap: generating bindings" >&2
  "$bindgen" "$gen/lib/libjarvis_core_ffi.a" "$gen/swift" --swift-sources >&2
  "$bindgen" "$gen/lib/libjarvis_core_ffi.a" "$gen/JarvisCoreFFI" --headers >&2
  "$bindgen" "$gen/lib/libjarvis_core_ffi.a" "$gen/JarvisCoreFFI" --modulemap \
    --module-name JarvisCoreFFI --modulemap-filename module.modulemap >&2
  for f in "$gen/swift/JarvisCore.swift" "$gen/JarvisCoreFFI/JarvisCoreFFI.h" \
           "$gen/JarvisCoreFFI/module.modulemap"; do
    [ -s "$f" ] || die "bindings step did not produce $f"
  done
  rm -rf "$final_gen"
  mv "$gen" "$final_gen"
  cp "$final_gen/swift/JarvisCore.swift" "$swift_out/JarvisCore.swift"
fi

echo "bootstrap: core $(sed -n 's/^version *= *"\(.*\)"/\1/p' "$core/Cargo.toml" | head -n 1) from $core ($mode mode)" >&2
echo "bootstrap: done; now run: swift build && swift test" >&2
