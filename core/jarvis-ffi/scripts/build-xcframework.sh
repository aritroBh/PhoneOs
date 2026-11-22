#!/usr/bin/env bash
# Builds JarvisCore.xcframework (iOS device + simulator static libraries, C header and
# module map) plus the generated JarvisCore.swift. macOS with Xcode only.
#
#   scripts/build-xcframework.sh [OUT_DIR]
#
# OUT_DIR defaults to ../../build relative to core/jarvis-ffi (the repository's build/).
# Result:
#   OUT_DIR/JarvisCore.xcframework/   static libs + Headers/JarvisCoreFFI/{JarvisCoreFFI.h,module.modulemap}
#   OUT_DIR/JarvisCore.swift          add to the app target (it does `import JarvisCoreFFI`)
#
# Environment:
#   IPHONEOS_DEPLOYMENT_TARGET  minimum iOS version for the Rust objects (default 16.0)
#
# UNTESTED: written from the UniFFI 0.32 and `xcodebuild -create-xcframework`
# documentation on a Linux machine; only `cargo check --target aarch64-apple-ios` has run.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
crate="$(dirname "$here")"
core="$(dirname "$crate")"
out_arg="${1:-$crate/../../build}"

die() { echo "build-xcframework: $*" >&2; exit 1; }

[ "$(uname -s)" = "Darwin" ] || die "macOS only (needs xcodebuild and the Apple SDKs); on Linux use scripts/gen-swift.sh / scripts/swift-smoke.sh"
command -v xcodebuild >/dev/null 2>&1 || die "xcodebuild not found; install Xcode and run: sudo xcode-select -s /Applications/Xcode.app"
command -v lipo >/dev/null 2>&1 || die "lipo not found (Xcode command line tools)"
command -v rustup >/dev/null 2>&1 || die "rustup not found"

installed="$(cd "$core" && rustup target list --installed)"
need_target() {
  grep -qx "$1" <<<"$installed" || die "Rust target $1 missing; run: rustup target add $1"
}
need_target aarch64-apple-ios
need_target aarch64-apple-ios-sim
sim_targets=(aarch64-apple-ios-sim)
if grep -qx x86_64-apple-ios <<<"$installed"; then
  sim_targets+=(x86_64-apple-ios)
else
  echo "build-xcframework: x86_64-apple-ios not installed; simulator slice is arm64 only" \
       "(rustup target add x86_64-apple-ios for Intel Macs)" >&2
fi

mkdir -p "$out_arg"
out="$(cd "$out_arg" && pwd)"
work="$core/target/xcframework"
rm -rf "$work"
mkdir -p "$work"
export IPHONEOS_DEPLOYMENT_TARGET="${IPHONEOS_DEPLOYMENT_TARGET:-16.0}"

cd "$core"

# 1. Static libraries. `cargo rustc --crate-type staticlib` builds ONLY the staticlib: with
#    the manifest's `lib` (rlib) output alongside, rustc skips LTO for the staticlib
#    (measured on Linux: 34 MB without LTO vs 9.9 MB with). `--no-default-features` keeps
#    the bindings generator (feature `cli`) out of the library.
build_lib() {
  cargo rustc -p jarvis-ffi --lib --release --no-default-features \
    --target "$1" --crate-type staticlib
  echo "$core/target/$1/release/libjarvis_core_ffi.a"
}
device_lib="$(build_lib aarch64-apple-ios | tail -n 1)"
sim_libs=()
for t in "${sim_targets[@]}"; do
  sim_libs+=("$(build_lib "$t" | tail -n 1)")
done
mkdir -p "$work/ios-arm64" "$work/ios-sim"
cp "$device_lib" "$work/ios-arm64/libjarvis_core_ffi.a"
if [ "${#sim_libs[@]}" -gt 1 ]; then
  lipo -create "${sim_libs[@]}" -output "$work/ios-sim/libjarvis_core_ffi.a"
else
  cp "${sim_libs[0]}" "$work/ios-sim/libjarvis_core_ffi.a"
fi

# 2. Bindings, generated from the device library with the host-built generator. The
#    metadata UniFFI needs survives LTO in the static library (checked on Linux); do NOT
#    strip symbols from the libraries before this step.
cargo build -p jarvis-ffi --bin uniffi-bindgen-swift
bindgen="$core/target/debug/uniffi-bindgen-swift"
# Headers in a module-named subdirectory: xcframeworks of static libraries otherwise put
# `module.modulemap` at the include root, which collides with any other such xcframework
# in the same app ("Multiple commands produce .../include/module.modulemap").
headers="$work/include/JarvisCoreFFI"
mkdir -p "$headers"
"$bindgen" "$work/ios-arm64/libjarvis_core_ffi.a" "$work/swift" --swift-sources
"$bindgen" "$work/ios-arm64/libjarvis_core_ffi.a" "$headers" --headers
# A plain (non-framework) module map, because the xcframework holds libraries, not
# .framework bundles. (`--xcframework` would emit `framework module`.)
"$bindgen" "$work/ios-arm64/libjarvis_core_ffi.a" "$headers" --modulemap \
  --module-name JarvisCoreFFI --modulemap-filename module.modulemap
for f in "$work/swift/JarvisCore.swift" "$headers/JarvisCoreFFI.h" "$headers/module.modulemap"; do
  [ -s "$f" ] || die "bindings step did not produce $f"
done

# 3. The xcframework.
rm -rf "$out/JarvisCore.xcframework"
xcodebuild -create-xcframework \
  -library "$work/ios-arm64/libjarvis_core_ffi.a" -headers "$work/include" \
  -library "$work/ios-sim/libjarvis_core_ffi.a" -headers "$work/include" \
  -output "$out/JarvisCore.xcframework"
cp "$work/swift/JarvisCore.swift" "$out/JarvisCore.swift"

echo "build-xcframework: $out/JarvisCore.xcframework"
echo "build-xcframework: $out/JarvisCore.swift"
