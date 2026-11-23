//! The Live view over remote control, viewer side (`jarvis_viewer::delta`, streamed by
//! `jarvis_remote`'s hub).
//!
//! - **Phone**: `JarvisViewer.feedAndStream(update:hub:nowMs:)` is the one call per driver
//!   update when remote control is on. It feeds the viewer exactly like `feed` and, under
//!   the same viewer lock, hands the hub the viewer's post-privacy timeline by reference
//!   (`RemoteHub::viewer_update`): nothing is copied across the boundary, and the hub diffs
//!   it into bounded deltas for every connection that watches with the permission. The
//!   returned hub effects are carried out like every other hub effect, in order. Locks are
//!   taken viewer → hub (no hub call takes a viewer).
//! - **Controller**: `JarvisViewer.applyDelta(bytes:)` once per received delta or snapshot
//!   part (`RemoteViewerEvent` from `JarvisRemoteClient.receiveAll`), `needsResync()`; the
//!   timeline, screens and scenes are then read as on the phone.

use std::sync::Arc;

use jarvis_viewer as v;

use crate::error::JarvisError;
use crate::guard::guarded;
use crate::remote::JarvisRemoteHub;
use crate::types::HubEffect;
use crate::viewer::{CoreViewerChange, CoreViewerUpdate, JarvisViewer};

/// What one `feedAndStream` did.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct CoreViewerFed {
    /// What the update changed in this viewer (as `feed` returns it).
    pub change: CoreViewerChange,
    /// The hub's effects: frames for watching devices (and anything a timeout produced).
    /// Carry them out like every hub effect, in order, before effects of later hub calls.
    pub effects: Vec<HubEffect>,
    /// The hub call failed (the viewer was still fed): the hub is unusable now (it
    /// panicked or was already poisoned); rebuild it from the Keychain JSON.
    pub hub_error: Option<String>,
}

/// Why a controller's view needs a snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum CoreResyncReason {
    /// A sequence number was skipped, or a delta came before any reset.
    Gap,
    /// A different generation without a reset.
    Generation,
    /// Bytes that do not decode (or are too large).
    Undecodable,
    /// A delta that does not fit the timeline.
    Invalid,
}

/// What `applyDelta` did with one delta.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum CoreDeltaApplied {
    /// Applied; refetch what changed (as after `feed`).
    Changed { change: CoreViewerChange },
    /// Nothing to do (an unknown kind, a screen already held, anything while a snapshot is
    /// awaited).
    Unchanged,
    /// The stream broke: call `JarvisRemoteClient.requestResync()` and write its bytes.
    /// Reported once per break.
    NeedsResync { reason: CoreResyncReason },
    /// Written by a newer, incompatible phone (its format version): stop watching.
    Incompatible { version: u32 },
}

/// The run number the hub's encoder gets: this viewer's number in the high half, its runs
/// in the low half. A new viewer (the shell makes one per run) is then always a new run
/// for the stream, even though its own count starts again at 1.
fn stream_run(instance: u64, runs: u64) -> u64 {
    (instance << 32) | (runs & 0xFFFF_FFFF)
}

fn resync_reason(r: v::ResyncReason) -> CoreResyncReason {
    match r {
        v::ResyncReason::Gap => CoreResyncReason::Gap,
        v::ResyncReason::Generation => CoreResyncReason::Generation,
        v::ResyncReason::Undecodable => CoreResyncReason::Undecodable,
        v::ResyncReason::Invalid => CoreResyncReason::Invalid,
    }
}

fn change_record(c: v::Change, generation: u64) -> CoreViewerChange {
    CoreViewerChange {
        steps: c.steps,
        screens: c.screens,
        activity: c.activity,
        generation,
        approval: c.approval,
    }
}

#[uniffi::export]
impl JarvisViewer {
    /// `feed(update:)` and the remote stream in ONE call (the phone, while remote control
    /// is on): feeds the update, then gives `hub` this viewer's timeline (by reference,
    /// under the viewer's lock) so it can send deltas to every device that watches with
    /// "Watch the screen layout" on. Devices without it get nothing. Never blocks on the
    /// network: the hub only queues, at most 256 KiB per connection. A hub failure does not
    /// fail the call (the viewer was fed): see `CoreViewerFed.hubError`.
    pub fn feed_and_stream(
        &self,
        update: CoreViewerUpdate,
        hub: Arc<JarvisRemoteHub>,
        now_ms: u64,
    ) -> Result<CoreViewerFed, JarvisError> {
        guarded("JarvisViewer.feed_and_stream", || {
            let mut inner = self.lock();
            let live = &mut inner.live;
            let change = live.apply(update.into());
            let change = change_record(change, live.generation());
            let run = stream_run(self.instance, live.runs());
            let timeline = live.timeline();
            let (effects, hub_error) = match hub.with("viewer_update", |h| {
                Ok(h.viewer_update(now_ms, run, timeline))
            }) {
                Ok(fx) => (fx.into_iter().map(Into::into).collect(), None),
                Err(e) => (Vec::new(), Some(e.to_string())),
            };
            Ok(CoreViewerFed {
                change,
                effects,
                hub_error,
            })
        })
    }

    /// A controller's view: applies one delta or snapshot part received from the phone
    /// (`RemoteViewerEvent.delta` / `.snapshot`), in order. The bytes are untrusted:
    /// bounded, never panics. On `needsResync` ask for a snapshot
    /// (`JarvisRemoteClient.requestResync()`); a snapshot's reset starts a fresh scrubber.
    pub fn apply_delta(&self, bytes: Vec<u8>) -> Result<CoreDeltaApplied, JarvisError> {
        self.with("JarvisViewer.apply_delta", |live| {
            Ok(match live.apply_delta(&bytes) {
                v::Applied::Changed(c) => CoreDeltaApplied::Changed {
                    change: change_record(c, live.generation()),
                },
                v::Applied::Unchanged => CoreDeltaApplied::Unchanged,
                v::Applied::NeedsResync(r) => CoreDeltaApplied::NeedsResync {
                    reason: resync_reason(r),
                },
                v::Applied::Incompatible(version) => CoreDeltaApplied::Incompatible { version },
            })
        })
    }

    /// Whether this view (as a remote controller) waits for a snapshot.
    pub fn needs_resync(&self) -> Result<bool, JarvisError> {
        self.with("JarvisViewer.needs_resync", |live| Ok(live.needs_resync()))
    }
}

#[cfg(test)]
mod tests {
    use super::stream_run;

    #[test]
    fn a_new_viewer_is_a_new_run() {
        assert_ne!(stream_run(1, 1), stream_run(2, 1));
        assert_ne!(stream_run(1, 1), stream_run(1, 2));
        assert_eq!(stream_run(3, 0), 3 << 32);
    }
}
