//! Index and fingerprint lookups, and set-of-marks.

use jarvis_proto::{CompactScreen, Element, Mark};
use thiserror::Error;

/// Why an element index could not be resolved.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ResolveError {
    /// Indices start at 1.
    #[error("element indices start at 1, got 0")]
    Zero,
    /// The screen has no element with this index.
    #[error("no element [{index}] on this screen (it has {count} elements)")]
    OutOfRange {
        /// The requested index.
        index: u32,
        /// Number of elements on the screen.
        count: usize,
    },
}

/// The element with `index`, or the reason there is none.
pub fn resolve(screen: &CompactScreen, index: u32) -> Result<&Element, ResolveError> {
    if index == 0 {
        return Err(ResolveError::Zero);
    }
    screen.get(index).ok_or(ResolveError::OutOfRange {
        index,
        count: screen.elements.len(),
    })
}

/// The first element with this fingerprint, if present.
pub fn find_by_fingerprint(screen: &CompactScreen, fingerprint: u64) -> Option<&Element> {
    screen.find_by_fingerprint(fingerprint)
}

/// One set-of-marks entry per element, in index order.
pub fn marks(screen: &CompactScreen) -> Vec<Mark> {
    screen
        .elements
        .iter()
        .map(|e| Mark {
            index: e.index,
            frame: e.frame,
        })
        .collect()
}
