#!/usr/bin/env bash
# Generates the Swift bindings, compiles them as the `JarvisCore` Swift module against the
# host dynamic library, compiles swift-smoke/main.swift against that module and runs it.
# Exits non-zero on any build or check failure. Works on Linux and macOS.
#
# With JARVIS_SMOKE_STATIC=1 it then repeats the run the way the app links: a release
# (LTO, panic=unwind) static library built like scripts/build-xcframework.sh builds it,
# bindings generated from that .a with uniffi-bindgen-swift, the xcframework header layout
# (include/JarvisCoreFFI/{JarvisCoreFFI.h,module.modulemap}), and static linking.
#
# Swift toolchain: $SWIFTC if set, else `swiftc` on PATH, else /opt/swift/usr/bin/swiftc.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
crate="$(dirname "$here")"
gen="$crate/generated"
build="$gen/smoke-build"

if [ -n "${SWIFTC:-}" ]; then
  swiftc="$SWIFTC"
elif command -v swiftc >/dev/null 2>&1; then
  swiftc="$(command -v swiftc)"
elif [ -x /opt/swift/usr/bin/swiftc ]; then
  swiftc=/opt/swift/usr/bin/swiftc
else
  echo "swift-smoke: no Swift compiler (set SWIFTC, put swiftc on PATH, or install /opt/swift)" >&2
  exit 1
fi
echo "swift-smoke: using $("$swiftc" --version 2>&1 | head -n 1)"

# The smoke test needs the deliberate-panic hooks; they exist only with this feature.
lib="$(JARVIS_FFI_FEATURES=panic-hooks "$here/gen-swift.sh" "$gen" | tail -n 1)"
libdir="$(dirname "$lib")"

rm -rf "$build"
mkdir -p "$build"

case "$(uname -s)" in
  Darwin) so=dylib; libpath_var=DYLD_LIBRARY_PATH ;;
  *) so=so; libpath_var=LD_LIBRARY_PATH ;;
esac

# The C header's Clang module. The generated modulemap works on Linux as is (its `use
# "Darwin"` lines only declare dependencies), so it is passed explicitly rather than copied.
ffi_flags=(-I "$gen" -Xcc "-fmodule-map-file=$gen/JarvisCoreFFI.modulemap")

# 1. The bindings as their own module, in Swift 6 language mode (what an app target uses
#    today), so missing `public`s or strict-concurrency problems in the generated code
#    show up here.
"$swiftc" -swift-version 6 -module-name JarvisCore -parse-as-library \
  -emit-library -emit-module -emit-module-path "$build/JarvisCore.swiftmodule" \
  "${ffi_flags[@]}" -L "$libdir" -ljarvis_core_ffi \
  "$gen/JarvisCore.swift" -o "$build/libJarvisCore.$so"

# 2. The smoke program, importing JarvisCore like the app would (Swift 5 mode: the test
#    driver itself uses a plain captured var with concurrentPerform).
"$swiftc" -swift-version 5 -module-name JarvisSmoke \
  -I "$build" "${ffi_flags[@]}" -L "$build" -lJarvisCore -L "$libdir" -ljarvis_core_ffi \
  "$crate/swift-smoke/main.swift" -o "$build/jarvis-smoke"

# 3. Run. Rust's default panic hook prints the two deliberate panics to stderr; that is
#    expected output, not a failure.
echo "swift-smoke: running $build/jarvis-smoke (dynamic, debug)"
RUST_BACKTRACE=0 env "$libpath_var=$build:$libdir${!libpath_var:+:${!libpath_var}}" "$build/jarvis-smoke"

[ "${JARVIS_SMOKE_STATIC:-0}" = "1" ] || exit 0

core="$(dirname "$crate")"
static="$gen/smoke-static"
rm -rf "$static"
mkdir -p "$static/include/JarvisCoreFFI"
cd "$core"
cargo rustc -p jarvis-ffi --lib --release --no-default-features --features panic-hooks \
  --crate-type staticlib >&2
cargo build -p jarvis-ffi --bin uniffi-bindgen-swift >&2
cp "$core/target/release/libjarvis_core_ffi.a" "$static/"
bindgen_swift="$core/target/debug/uniffi-bindgen-swift"
"$bindgen_swift" "$static/libjarvis_core_ffi.a" "$static/swift" --swift-sources >&2
"$bindgen_swift" "$static/libjarvis_core_ffi.a" "$static/include/JarvisCoreFFI" --headers >&2
"$bindgen_swift" "$static/libjarvis_core_ffi.a" "$static/include/JarvisCoreFFI" --modulemap \
  --module-name JarvisCoreFFI --modulemap-filename module.modulemap >&2
"$swiftc" -swift-version 6 -O -module-name JarvisCore -parse-as-library \
  -emit-library -static -emit-module -emit-module-path "$static/JarvisCore.swiftmodule" \
  -I "$static/include" "$static/swift/JarvisCore.swift" -o "$static/libJarvisCore.a"
"$swiftc" -swift-version 5 -O -module-name JarvisSmoke -I "$static" -I "$static/include" \
  "$crate/swift-smoke/main.swift" -L "$static" -lJarvisCore -ljarvis_core_ffi \
  -o "$static/jarvis-smoke"
echo "swift-smoke: running $static/jarvis-smoke (static, release)"
RUST_BACKTRACE=0 "$static/jarvis-smoke"
