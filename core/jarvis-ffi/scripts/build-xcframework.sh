#!/usr/bin/env bash
# Builds JarvisCore.xcframework (static libraries for iOS device, iOS simulator and macOS,
# with the C header and module map) plus the generated JarvisCore.swift. macOS with Xcode only.
#
#   scripts/build-xcframework.sh [OUT_DIR]
#
# OUT_DIR defaults to ../../build relative to core/jarvis-ffi (the repository's build/).
# Result:
#   OUT_DIR/JarvisCore.xcframework/   three slices; each has Headers/{JarvisCoreFFI.h,module.modulemap}
#   OUT_DIR/JarvisCore.swift          add to the app target (it does `import JarvisCoreFFI`)
#
# Slices (`xcodebuild -create-xcframework` takes exactly one -library per platform variant,
# so architectures of one variant are lipo'd together first):
#   ios-arm64               aarch64-apple-ios                      required
#   ios-arm64(_x86_64)-simulator  aarch64-apple-ios-sim [+ x86_64-apple-ios]   arm64 required
#   macos-arm64(_x86_64)    aarch64-apple-darwin [+ x86_64-apple-darwin]       arm64 required
# The macOS slice is what lets `swift test` link the Rust core on the Mac host.
#
# Environment:
#   IPHONEOS_DEPLOYMENT_TARGET  minimum iOS version for the Rust objects (default 16.0)
#   MACOSX_DEPLOYMENT_TARGET    minimum macOS version for the Rust objects (default 15.0,
#                               the Shell package's platform)
#   CARGO_TARGET_DIR            honoured, like bootstrap.sh
#
# Verified on Linux: the three staticlibs build without any Apple linker and the bindgen
# steps work on them. NOT verified (needs a Mac): lipo and `xcodebuild -create-xcframework`.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
crate="$(dirname "$here")"
core="$(dirname "$crate")"
out_arg="${1:-$crate/../../build}"

die() { echo "build-xcframework: $*" >&2; exit 1; }
log() { echo "build-xcframework: $*" >&2; }

[ "$(uname -s)" = "Darwin" ] || die "macOS only (needs xcodebuild and the Apple SDKs); on Linux use scripts/gen-swift.sh / scripts/swift-smoke.sh"
command -v xcodebuild >/dev/null 2>&1 || die "xcodebuild not found; install Xcode and run: sudo xcode-select -s /Applications/Xcode.app"
command -v lipo >/dev/null 2>&1 || die "lipo not found (Xcode command line tools)"
command -v rustup >/dev/null 2>&1 || die "rustup not found"
command -v cargo >/dev/null 2>&1 || die "cargo not found"

cd "$core"
installed="$(rustup target list --installed)"
need_target() {
  grep -qx "$1" <<<"$installed" || die "Rust target $1 missing; run: rustup target add $1"
}
need_target aarch64-apple-ios
need_target aarch64-apple-ios-sim
need_target aarch64-apple-darwin
sim_targets=(aarch64-apple-ios-sim)
mac_targets=(aarch64-apple-darwin)
if grep -qx x86_64-apple-ios <<<"$installed"; then
  sim_targets+=(x86_64-apple-ios)
else
  log "x86_64-apple-ios not installed; simulator slice is arm64 only (rustup target add x86_64-apple-ios for Intel Macs)"
fi
if grep -qx x86_64-apple-darwin <<<"$installed"; then
  mac_targets+=(x86_64-apple-darwin)
else
  log "x86_64-apple-darwin not installed; macOS slice is arm64 only"
fi

mkdir -p "$out_arg"
out="$(cd "$out_arg" && pwd)"
target_dir="${CARGO_TARGET_DIR:-}"
if [ -z "$target_dir" ]; then
  target_dir="$(cargo metadata --format-version 1 --no-deps \
    | sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p')"
fi
target_dir="${target_dir:-$core/target}"
work="$target_dir/xcframework"
rm -rf "$work"
mkdir -p "$work"
export IPHONEOS_DEPLOYMENT_TARGET="${IPHONEOS_DEPLOYMENT_TARGET:-16.0}"
export MACOSX_DEPLOYMENT_TARGET="${MACOSX_DEPLOYMENT_TARGET:-15.0}"

# 1. Static libraries. `cargo rustc --crate-type staticlib` builds ONLY the staticlib: with
#    the manifest's `lib` (rlib) output alongside, rustc skips LTO for the staticlib
#    (measured on Linux: 34 MB without LTO vs 9.9 MB with). `--no-default-features` keeps
#    the bindings generator (feature `cli`) out of the library.
lib_path() { echo "$target_dir/$1/release/libjarvis_core_ffi.a"; }
build_lib() {
  log "cargo rustc --target $1"
  cargo rustc -p jarvis-ffi --lib --release --no-default-features \
    --target "$1" --crate-type staticlib >&2
  [ -f "$(lib_path "$1")" ] || die "$(lib_path "$1") was not built"
}

# Combines the per-architecture libraries of one platform variant into $1/libjarvis_core_ffi.a.
make_slice() {
  local dir="$1"; shift
  local libs=() t
  for t in "$@"; do
    build_lib "$t"
    libs+=("$(lib_path "$t")")
  done
  mkdir -p "$dir"
  if [ "${#libs[@]}" -gt 1 ]; then
    lipo -create "${libs[@]}" -output "$dir/libjarvis_core_ffi.a"
  else
    cp "${libs[0]}" "$dir/libjarvis_core_ffi.a"
  fi
}
make_slice "$work/ios-arm64" aarch64-apple-ios
make_slice "$work/ios-sim" "${sim_targets[@]}"
make_slice "$work/macos" "${mac_targets[@]}"

# 2. Bindings, generated from the device library with the host-built generator. The
#    metadata UniFFI needs survives LTO in the static library (checked on Linux); do NOT
#    strip symbols from the libraries before this step. The output is identical for all
#    targets (checked on Linux: byte-identical Swift for ios and ios-sim).
log "building uniffi-bindgen-swift"
cargo build -p jarvis-ffi --bin uniffi-bindgen-swift >&2
bindgen="$target_dir/debug/uniffi-bindgen-swift"
[ -x "$bindgen" ] || die "$bindgen was not built"
# The header and a plain (non-framework) module map sit at the root of Headers/, which is
# where SwiftPM looks for a binary target's module map. (`--xcframework` would emit
# `framework module`, wrong for library slices.)
headers="$work/include"
mkdir -p "$headers"
lib0="$work/ios-arm64/libjarvis_core_ffi.a"
"$bindgen" "$lib0" "$work/swift" --swift-sources >&2
"$bindgen" "$lib0" "$headers" --headers >&2
"$bindgen" "$lib0" "$headers" --modulemap \
  --module-name JarvisCoreFFI --modulemap-filename module.modulemap >&2
for f in "$work/swift/JarvisCore.swift" "$headers/JarvisCoreFFI.h" "$headers/module.modulemap"; do
  [ -s "$f" ] || die "bindings step did not produce $f"
done

# 3. The xcframework: one -library/-headers pair per platform variant.
rm -rf "$out/JarvisCore.xcframework"
xcodebuild -create-xcframework \
  -library "$work/ios-arm64/libjarvis_core_ffi.a" -headers "$headers" \
  -library "$work/ios-sim/libjarvis_core_ffi.a" -headers "$headers" \
  -library "$work/macos/libjarvis_core_ffi.a" -headers "$headers" \
  -output "$out/JarvisCore.xcframework" >&2
cp "$work/swift/JarvisCore.swift" "$out/JarvisCore.swift"

log "slices in $out/JarvisCore.xcframework:"
find "$out/JarvisCore.xcframework" -mindepth 1 -maxdepth 2 | sort >&2
log "$out/JarvisCore.xcframework"
log "$out/JarvisCore.swift"
