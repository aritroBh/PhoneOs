//! The Live view over remote control through the FFI objects only: the phone's
//! `JarvisViewer.feed_and_stream` (one call per driver update) with its `JarvisRemoteHub`,
//! and a controller's `JarvisRemoteClient.receive_all` + its own `JarvisViewer.apply_delta`.
//! Permission off → denied and nothing sent; on → the controller's timeline, screens and
//! scenes equal the phone's for a whole session; revoking mid-stream ends it; dropped
//! frames → one resync, converges; a reconnect starts from a snapshot; stores from before
//! the permission load with it off.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use std::sync::Arc;

use common::*;
use jarvis_core_ffi::*;

/// Test entropy (splitmix64), as in `tests/remote.rs`.
struct Rng(u64);

impl Rng {
    fn bytes(&mut self) -> Vec<u8> {
        let mut out = Vec::with_capacity(32);
        while out.len() < 32 {
            self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = self.0;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            out.extend_from_slice(&(z ^ (z >> 31)).to_le_bytes());
        }
        out
    }
}

fn viewer_config() -> CoreViewerConfig {
    CoreViewerConfig {
        max_steps: 200,
        frames_enabled: false,
        known_apps: vec![AppInfo {
            bundle_id: SETTINGS.to_owned(),
            name: "Settings".to_owned(),
        }],
    }
}

/// The phone: its hub, its Live view and its "Keychain".
struct Phone {
    hub: Arc<JarvisRemoteHub>,
    viewer: Arc<JarvisViewer>,
    keychain: String,
    audits: Vec<AuditEvent>,
    now: u64,
}

/// A paired controller on connection `conn`.
struct Controller {
    conn: u64,
    client: Arc<JarvisRemoteClient>,
    view: Arc<JarvisViewer>,
    credentials: String,
    device_id: String,
    events: Vec<RemoteViewerEvent>,
    /// Bytes the phone sent on this connection (every frame, sealed).
    received_bytes: usize,
    /// Deltas still to drop before applying (a lossy path in the test, not the network).
    drop_deltas: usize,
    resync_requests: usize,
    /// Report writes to the hub (a transport whose writes complete).
    ack: bool,
}

impl Phone {
    fn new(store: &str) -> Self {
        Phone {
            hub: JarvisRemoteHub::new(store.to_owned(), "Owner's phone".into()).unwrap(),
            viewer: JarvisViewer::new(viewer_config()).unwrap(),
            keychain: store.to_owned(),
            audits: Vec::new(),
            now: 1_000,
        }
    }

    /// Carries out effects; returns what goes to each connection, in order.
    fn apply(&mut self, fx: Vec<HubEffect>) -> Vec<(u64, Vec<u8>)> {
        let mut out = Vec::new();
        for e in fx {
            match e {
                HubEffect::Send { conn_id, bytes } => out.push((conn_id, bytes)),
                HubEffect::StoreChanged => self.keychain = self.hub.to_json().unwrap(),
                HubEffect::Audit { entry } => self.audits.push(entry.event),
                HubEffect::Close { conn_id, reason } => {
                    panic!("connection {conn_id} closed: {reason:?}")
                }
                _ => {}
            }
        }
        out
    }

    /// One driver update: the shell's single FFI call.
    fn update(&mut self, u: CoreViewerUpdate) -> Vec<(u64, Vec<u8>)> {
        self.now += 300;
        let fed = self
            .viewer
            .feed_and_stream(u, Arc::clone(&self.hub), self.now)
            .unwrap();
        assert_eq!(fed.hub_error, None);
        self.apply(fed.effects)
    }

    fn pair(&mut self, rng: &mut Rng, conn: u64) -> Controller {
        let offer = self.hub.open_pairing(self.now, rng.bytes()).unwrap();
        let client = JarvisRemoteClient::pair(offer.code, "Work Mac".into(), rng.bytes()).unwrap();
        self.connect(rng, conn, client)
    }

    fn reconnect(&mut self, rng: &mut Rng, conn: u64, credentials: &str) -> Controller {
        let client = JarvisRemoteClient::connect(credentials.to_owned(), rng.bytes()).unwrap();
        let mut c = self.connect(rng, conn, client);
        c.credentials = credentials.to_owned();
        c
    }

    fn connect(&mut self, rng: &mut Rng, conn: u64, client: Arc<JarvisRemoteClient>) -> Controller {
        let mut c = Controller {
            conn,
            client,
            view: JarvisViewer::new(viewer_config()).unwrap(),
            credentials: String::new(),
            device_id: String::new(),
            events: Vec::new(),
            received_bytes: 0,
            drop_deltas: 0,
            resync_requests: 0,
            ack: true,
        };
        let fx = self
            .hub
            .connection_opened(conn, self.now, rng.bytes())
            .unwrap();
        let out = self.apply(fx);
        self.deliver(&mut c, out);
        c.device_id = c.client.device_id().unwrap().unwrap();
        assert!(c.client.is_connected().unwrap());
        c
    }

    /// Moves bytes both ways until quiet; applies the Live view to the controller's view.
    fn deliver(&mut self, c: &mut Controller, mut to_client: Vec<(u64, Vec<u8>)>) {
        for _ in 0..10_000 {
            let mut to_phone = Vec::new();
            let mut written = 0usize;
            for (conn, bytes) in to_client.drain(..) {
                if conn != c.conn {
                    continue;
                }
                c.received_bytes += bytes.len();
                written += bytes.len();
                let got = c.client.receive_all(bytes).unwrap();
                for e in got.events {
                    match e {
                        ClientEvent::Send { bytes } => to_phone.push(bytes),
                        ClientEvent::Paired {
                            credentials_json, ..
                        } => c.credentials = credentials_json,
                        _ => {}
                    }
                }
                for e in got.viewer {
                    match &e {
                        RemoteViewerEvent::Delta { bytes }
                        | RemoteViewerEvent::Snapshot { bytes, .. } => {
                            if c.drop_deltas > 0 && matches!(e, RemoteViewerEvent::Delta { .. }) {
                                c.drop_deltas -= 1;
                            } else if let CoreDeltaApplied::NeedsResync { .. } =
                                c.view.apply_delta(bytes.clone()).unwrap()
                            {
                                if let Some(req) = c.client.request_resync().unwrap() {
                                    c.resync_requests += 1;
                                    to_phone.push(req);
                                }
                            }
                        }
                        RemoteViewerEvent::ResyncRequested => c.resync_requests += 1,
                        _ => {}
                    }
                    c.events.push(e);
                }
            }
            if c.ack && written > 0 {
                self.now += 1;
                let fx = self.hub.written(c.conn, written as u64, self.now).unwrap();
                to_client.extend(self.apply(fx));
            }
            for bytes in to_phone {
                self.now += 1;
                let fx = self.hub.receive(c.conn, bytes, self.now).unwrap();
                to_client.extend(self.apply(fx));
            }
            if to_client.is_empty() {
                return;
            }
        }
        panic!("no quiescence");
    }

    fn start_watching(&mut self, c: &mut Controller) {
        let bytes = c.client.start_watching().unwrap();
        self.now += 1;
        let fx = self.hub.receive(c.conn, bytes, self.now).unwrap();
        let out = self.apply(fx);
        self.deliver(c, out);
    }

    fn set_watch(&mut self, c: &mut Controller, allowed: bool) -> Vec<(u64, Vec<u8>)> {
        self.now += 1;
        let fx = self
            .hub
            .set_watch_screen(self.now, c.device_id.clone(), allowed)
            .unwrap();
        self.apply(fx)
    }
}

/// Every update of the Settings → Wi-Fi session, in the driver's order.
fn session_updates() -> Vec<CoreViewerUpdate> {
    let mut ups = Vec::new();
    drive_updates(|u| ups.push(u), || {});
    ups
}

/// The timeline as the controller can know it (no generation, no frame keys).
fn comparable(t: CoreViewerTimeline) -> CoreViewerTimeline {
    CoreViewerTimeline {
        generation: 0,
        screens: t
            .screens
            .into_iter()
            .map(|s| CoreViewerScreenInfo {
                frame_key: None,
                ..s
            })
            .collect(),
        ..t
    }
}

fn scene_at(v: &JarvisViewer, number: u32) -> CoreLiveScene {
    v.replay(CoreReplayAction::ScrubTo { number }, 50_000.0, true)
        .unwrap();
    CoreLiveScene {
        generation: 0,
        ..v.scene(CoreSceneQuery {
            now_ms: 50_000.0,
            reduce_motion: true,
            view_width: 300.0,
            view_height: 600.0,
            image_width: 0.0,
            image_height: 0.0,
        })
        .unwrap()
    }
}

/// The controller shows what the phone shows: timeline, every screen, and the scene at
/// every step.
fn assert_same(phone: &JarvisViewer, view: &JarvisViewer) {
    assert_converged(phone, view, true);
}

/// After a snapshot (a reconnect, a resync, an overflow) the controller holds only the
/// current screen of what came before (documented: a snapshot carries the newest steps
/// without their screens); everything else must be equal, and so must the scene of every
/// step whose screen it holds.
fn assert_converged(phone: &JarvisViewer, view: &JarvisViewer, every_screen: bool) {
    let pt = comparable(phone.timeline().unwrap());
    let vt = comparable(view.timeline().unwrap());
    if every_screen {
        assert_eq!(pt, vt);
    } else {
        assert_eq!(
            CoreViewerTimeline {
                screens: vec![],
                ..pt.clone()
            },
            CoreViewerTimeline {
                screens: vec![],
                ..vt.clone()
            }
        );
        assert!(vt.screens.iter().all(|s| pt.screens.contains(s)));
        assert!(vt
            .screens
            .iter()
            .any(|s| Some(s.id) == vt.current_screen_id));
    }
    for s in &vt.screens {
        let a = phone.screen(s.id).unwrap().unwrap();
        let b = view.screen(s.id).unwrap().unwrap();
        assert_eq!(a.elements, b.elements);
    }
    let held: Vec<u32> = vt.screens.iter().map(|s| s.id).collect();
    let numbers: Vec<u32> = pt
        .steps
        .iter()
        .filter(|s| s.screen_id.is_none_or(|id| held.contains(&id)))
        .map(|s| s.number)
        .collect();
    assert!(!numbers.is_empty() || !every_screen);
    for n in numbers {
        assert_eq!(scene_at(phone, n), scene_at(view, n), "step {n}");
    }
    phone
        .replay(CoreReplayAction::GoLive, 60_000.0, true)
        .unwrap();
    view.replay(CoreReplayAction::GoLive, 60_000.0, true)
        .unwrap();
}

fn viewer_text(events: &[RemoteViewerEvent]) -> String {
    let mut text = String::new();
    for e in events {
        if let RemoteViewerEvent::Delta { bytes } | RemoteViewerEvent::Snapshot { bytes, .. } = e {
            text.push_str(std::str::from_utf8(bytes).unwrap());
            text.push('\n');
        }
    }
    text
}

#[test]
fn without_the_permission_watching_is_denied_and_nothing_is_sent() {
    let mut rng = Rng(11);
    let mut phone = Phone::new("");
    let mut c = phone.pair(&mut rng, 1);
    assert!(!phone.hub.watch_screen(c.device_id.clone()).unwrap());
    assert!(!phone.hub.devices().unwrap()[0].watch_screen);
    phone.start_watching(&mut c);
    assert_eq!(
        c.events,
        vec![RemoteViewerEvent::Denied {
            reason: DenyReason::NotPermitted
        }]
    );
    assert!(phone.audits.iter().any(|a| matches!(
        a,
        AuditEvent::Denied { request, reason: DenyReason::NotPermitted } if request == "start_watching"
    )));
    assert!(!c.client.is_watching().unwrap());
    assert!(!phone.hub.is_watching(1).unwrap());
    let before = c.received_bytes;
    for u in session_updates() {
        let out = phone.update(u);
        assert!(
            out.iter().all(|(conn, _)| *conn != c.conn),
            "sent to a device without watch_screen"
        );
        phone.deliver(&mut c, out);
    }
    assert_eq!(c.received_bytes, before);
    // The phone's own view was fed all along.
    assert_eq!(phone.viewer.timeline().unwrap().steps.len(), 2);
}

#[test]
fn with_the_permission_the_controller_shows_what_the_phone_shows() {
    let mut rng = Rng(12);
    let mut phone = Phone::new("");
    let mut c = phone.pair(&mut rng, 1);
    let out = phone.set_watch(&mut c, true);
    assert!(out.is_empty());
    assert!(phone.keychain.contains(r#""watch_screen":true"#));
    assert!(phone.audits.iter().any(|a| matches!(
        a,
        AuditEvent::Accepted { request, detail: Some(d) } if request == "watch_screen_permission" && d == "on"
    )));
    let device = &phone.hub.devices().unwrap()[0];
    assert!(device.watch_screen);
    // The other permissions (and so approvals) are untouched.
    assert_eq!(device.permissions, new_device_remote_permissions().unwrap());
    phone.start_watching(&mut c);
    assert!(c.client.is_watching().unwrap());
    assert!(phone.hub.is_watching(1).unwrap());
    assert!(matches!(
        c.events.first(),
        Some(RemoteViewerEvent::Snapshot { part: 0, .. })
    ));
    let start = c.received_bytes;
    let updates = session_updates();
    let count = updates.len();
    for u in updates {
        let out = phone.update(u);
        phone.deliver(&mut c, out);
    }
    assert_same(&phone.viewer, &c.view);
    assert_eq!(c.resync_requests, 0);
    let deltas = c
        .events
        .iter()
        .filter(|e| matches!(e, RemoteViewerEvent::Delta { .. }))
        .count();
    eprintln!(
        "Settings → Wi-Fi session: {count} driver updates, {deltas} deltas, {} bytes on the wire (sealed frames)",
        c.received_bytes - start
    );
    // Wireframes and captions only: no frame keys, no approval picture, no element values.
    let text = viewer_text(&c.events);
    assert!(!text.contains("frame_key") && !text.contains("observation"));
    // Stop: acknowledged; nothing more is sent.
    let bytes = c.client.stop_watching().unwrap();
    let fx = phone.hub.receive(1, bytes, phone.now).unwrap();
    let out = phone.apply(fx);
    phone.deliver(&mut c, out);
    assert_eq!(
        c.events.last(),
        Some(&RemoteViewerEvent::Ended {
            reason: RemoteViewerEndReason::Stopped
        })
    );
    let out = phone.update(CoreViewerUpdate::Ignored);
    assert!(out.is_empty());
}

#[test]
fn revoking_the_permission_mid_stream_ends_it_at_once() {
    let mut rng = Rng(13);
    let mut phone = Phone::new("");
    let mut c = phone.pair(&mut rng, 1);
    phone.set_watch(&mut c, true);
    phone.start_watching(&mut c);
    let updates = session_updates();
    let half = updates.len() / 2;
    let mut rest = updates.into_iter();
    for u in rest.by_ref().take(half) {
        let out = phone.update(u);
        phone.deliver(&mut c, out);
    }
    let out = phone.set_watch(&mut c, false);
    assert_eq!(out.len(), 1, "exactly the viewer_ended frame");
    phone.deliver(&mut c, out);
    assert_eq!(
        c.events.last(),
        Some(&RemoteViewerEvent::Ended {
            reason: RemoteViewerEndReason::PermissionRevoked
        })
    );
    assert!(phone.audits.iter().any(|a| matches!(
        a,
        AuditEvent::Accepted { request, detail: Some(d) } if request == "stop_watching" && d == "permission revoked"
    )));
    assert!(!phone.hub.is_watching(1).unwrap());
    assert!(!c.client.is_watching().unwrap());
    assert!(!phone.hub.devices().unwrap()[0].watch_screen);
    let before = c.received_bytes;
    for u in rest {
        let out = phone.update(u);
        assert!(out.is_empty());
    }
    assert_eq!(c.received_bytes, before);
    // Asking again is refused.
    phone.start_watching(&mut c);
    assert!(matches!(
        c.events.last(),
        Some(RemoteViewerEvent::Denied { .. })
    ));
}

#[test]
fn dropped_frames_cost_one_resync_and_the_view_converges() {
    let mut rng = Rng(14);
    let mut phone = Phone::new("");
    let mut c = phone.pair(&mut rng, 1);
    phone.set_watch(&mut c, true);
    phone.start_watching(&mut c);
    let updates = session_updates();
    let third = updates.len() / 3;
    for (i, u) in updates.into_iter().enumerate() {
        if i == third {
            c.drop_deltas = 2;
        }
        let out = phone.update(u);
        phone.deliver(&mut c, out);
    }
    assert_eq!(c.resync_requests, 1, "one request per gap");
    assert!(!c.view.needs_resync().unwrap());
    assert_converged(&phone.viewer, &c.view, false);
}

#[test]
fn a_reconnect_starts_from_a_snapshot() {
    let mut rng = Rng(15);
    let mut phone = Phone::new("");
    let mut c = phone.pair(&mut rng, 1);
    phone.set_watch(&mut c, true);
    phone.start_watching(&mut c);
    let updates = session_updates();
    let half = updates.len() / 2;
    let mut rest = updates.into_iter();
    for u in rest.by_ref().take(half) {
        let out = phone.update(u);
        phone.deliver(&mut c, out);
    }
    // The connection drops; updates go on without it.
    let fx = phone.hub.connection_closed(1, phone.now).unwrap();
    phone.apply(fx);
    for u in rest {
        phone.update(u);
    }
    let credentials = c.credentials.clone();
    let view = Arc::clone(&c.view);
    let mut c = phone.reconnect(&mut rng, 2, &credentials);
    // The same controller view continues from the snapshot.
    c.view = view;
    phone.start_watching(&mut c);
    assert!(matches!(
        c.events.first(),
        Some(RemoteViewerEvent::Snapshot { part: 0, .. })
    ));
    assert_converged(&phone.viewer, &c.view, false);
}

#[test]
fn a_connection_that_never_confirms_writes_stays_bounded() {
    let mut rng = Rng(16);
    let mut phone = Phone::new("");
    let mut c = phone.pair(&mut rng, 1);
    phone.set_watch(&mut c, true);
    c.ack = false;
    phone.start_watching(&mut c);
    // Many runs of the session, never confirming a write: the hub hands out at most its
    // window (plus the frame or snapshot that crossed it) and never blocks the feeds.
    for _ in 0..40 {
        // The controller stays alive (the hub closes a connection idle for 2 minutes).
        let ping = c.client.send(ClientMessage::Ping).unwrap();
        let fx = phone.hub.receive(1, ping, phone.now).unwrap();
        let out = phone.apply(fx);
        phone.deliver(&mut c, out);
        phone.viewer = JarvisViewer::new(viewer_config()).unwrap();
        for u in session_updates() {
            let out = phone.update(u);
            phone.deliver(&mut c, out);
        }
    }
    assert!(
        c.received_bytes < 128 * 1024 + 400 * 1024,
        "{} bytes handed out without confirmation",
        c.received_bytes
    );
    // Confirming lets the stream continue with one snapshot (coalesced).
    let mark = c.events.len();
    c.ack = true;
    let fx = phone
        .hub
        .written(1, c.received_bytes as u64, phone.now)
        .unwrap();
    let out = phone.apply(fx);
    phone.deliver(&mut c, out);
    let out = phone.update(CoreViewerUpdate::Ignored);
    phone.deliver(&mut c, out);
    assert!(!c.view.needs_resync().unwrap());
    assert!(matches!(
        c.events.get(mark),
        Some(RemoteViewerEvent::Snapshot { part: 0, .. })
    ));
    assert_converged(&phone.viewer, &c.view, false);
}

#[test]
fn stores_from_before_the_permission_load_with_it_off() {
    let mut rng = Rng(17);
    let mut phone = Phone::new("");
    let mut c = phone.pair(&mut rng, 1);
    // What older versions wrote: no watch_screen field.
    let old = phone.hub.to_json().unwrap();
    assert!(!old.contains("watch_screen"));
    let hub = JarvisRemoteHub::new(old.clone(), "Phone".into()).unwrap();
    assert!(!hub.watch_screen(c.device_id.clone()).unwrap());
    assert!(!hub.devices().unwrap()[0].watch_screen);
    // On: persisted with the device and restored.
    phone.set_watch(&mut c, true);
    let hub = JarvisRemoteHub::new(phone.keychain.clone(), "Phone".into()).unwrap();
    assert!(hub.watch_screen(c.device_id.clone()).unwrap());
    assert!(hub.devices().unwrap()[0].watch_screen);
    // Unknown devices are refused, as for the other permissions.
    assert!(matches!(
        phone
            .hub
            .set_watch_screen(1, "00".repeat(16), true)
            .unwrap_err(),
        JarvisError::RemoteUnknownDevice { .. }
    ));
}
