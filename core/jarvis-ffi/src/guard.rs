//! Panic containment at the FFI boundary.
//!
//! What UniFFI does on its own (0.32): every scaffolding function runs inside
//! `catch_unwind` (`uniffi_core::ffi::rustcalls`) and reports a panic to Swift as
//! `CALL_UNEXPECTED_ERROR`. The generated Swift turns that into
//! `UniffiInternalError.rustPanic`, which for a non-throwing function is a `try!`, i.e. a
//! crash of the app. So UniFFI's catch only keeps the unwind from crossing the C ABI.
//!
//! What this crate adds: every exported function and method runs its body through
//! [`guarded`] (or the session's own catch in `step`), so a panic becomes a typed
//! `JarvisError.internal` (or, for `step`, a poisoned session that returns `Finished`)
//! before UniFFI ever sees it. This relies on `panic = "unwind"` (the release profile in
//! `core/Cargo.toml` sets it explicitly); with `abort` nothing here could help.

use std::any::Any;
use std::panic::{catch_unwind, AssertUnwindSafe};

use crate::error::JarvisError;

/// The message of a caught panic payload.
pub(crate) fn panic_message(payload: &(dyn Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_owned()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "unknown panic".to_owned()
    }
}

/// Runs `f`, turning a panic into `JarvisError::Internal` naming `what`.
///
/// `AssertUnwindSafe` is sound here: after a panic nothing `f` touched is used again except
/// through the session's own poisoning (see `session.rs`).
pub(crate) fn guarded<T>(
    what: &str,
    f: impl FnOnce() -> Result<T, JarvisError>,
) -> Result<T, JarvisError> {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(r) => r,
        Err(payload) => Err(JarvisError::Internal {
            message: format!("{what}: {}", panic_message(payload.as_ref())),
        }),
    }
}
