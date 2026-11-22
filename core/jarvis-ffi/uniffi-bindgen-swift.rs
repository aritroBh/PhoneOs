//! UniFFI's Swift-specific generator (headers, XCFramework module maps, sources).
//!
//! `cargo run -p jarvis-ffi --bin uniffi-bindgen-swift -- <lib> <out> --headers --modulemap --xcframework`

fn main() {
    uniffi::uniffi_bindgen_swift()
}
