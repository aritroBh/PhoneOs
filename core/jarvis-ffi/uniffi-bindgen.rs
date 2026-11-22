//! The UniFFI bindings generator, built from the same `uniffi` version as the scaffolding.
//!
//! `cargo run -p jarvis-ffi --bin uniffi-bindgen generate --library <lib> --language swift --out-dir <dir>`

fn main() {
    uniffi::uniffi_bindgen_main()
}
