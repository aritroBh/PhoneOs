//! The Live view over remote control, end to end in memory: the `watch_screen`
//! permission, start/stop/resync, revocation mid-stream, two devices, backpressure
//! coalescing, the screen rate limit, audit entries, stored records from before the
//! permission, a locked phone, and a controller that rebuilds the phone's timeline.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::cast_precision_loss
)]

mod common;

use common::*;
use jarvis_proto as p;
use jarvis_remote::{
    AuditEvent, ClientMessage, ConnId, DenyReason, DeviceStore, HubEffect, LocalEvent, Received,
    RemoteHub, RemotePermissions, ViewerEndReason, ViewerEvent, ViewerRequest,
    MIN_SCREEN_INTERVAL_MS, VIEWER_QUEUE_BYTES, VIEWER_WINDOW_BYTES,
};
use jarvis_viewer::delta::{wire_timeline, DeltaBody};
use jarvis_viewer::{
    Applied, LiveView, ReducerConfig, Update, ViewerDelta, ViewerTimeline, MAX_SNAPSHOT_BYTES,
};

// ---- the phone's Live view -----------------------------------------------------------

fn button(label: &str, x: f64, y: f64) -> p::UiNode {
    let mut n = p::UiNode::new(p::Role::Button, p::Rect::new(x, y, 38.0, 18.0));
    n.label = Some(label.to_owned());
    n
}

fn screen_tree(labels: &[String]) -> p::UiNode {
    let mut root = p::UiNode::new(p::Role::Application, p::Rect::new(0.0, 0.0, 393.0, 852.0));
    root.label = Some("App".into());
    root.children = labels
        .iter()
        .enumerate()
        .map(|(i, l)| button(l, (i % 10) as f64 * 39.0, 60.0 + (i / 10) as f64 * 19.0))
        .collect();
    root
}

fn observed(root: p::UiNode, locked: bool) -> p::Event {
    p::Event::Observed(p::Observation {
        screen: p::Screen {
            tree_cut: false,
            app_bundle_id: "com.example.app".into(),
            app_name: Some("Example".into()),
            size: p::Size {
                width: 393.0,
                height: 852.0,
            },
            root,
            keyboard_visible: false,
            orientation: p::Orientation::Portrait,
        },
        screenshot: None,
        device_locked: locked,
    })
}

/// The phone's view and the driver's effect counter.
struct Phone {
    live: LiveView,
    number: u64,
}

impl Phone {
    fn new(task: &str) -> Self {
        let mut live = LiveView::new(ReducerConfig::default());
        live.apply(Update::Started {
            task_id: task.into(),
        });
        Phone { live, number: 0 }
    }

    fn observe(&mut self, root: p::UiNode, locked: bool) {
        self.number += 1;
        let effect = p::Effect::Observe { screenshot: false };
        self.live.apply(Update::Effect {
            effect: effect.clone(),
            number: self.number,
        });
        self.live.apply(Update::EffectCompleted {
            effect,
            event: observed(root, locked),
        });
    }

    fn tap(&mut self, x: f64, y: f64) {
        self.number += 1;
        let effect = p::Effect::Act(p::RunnerCommand::Tap { x, y, expect: None });
        self.live.apply(Update::Effect {
            effect: effect.clone(),
            number: self.number,
        });
        self.live.apply(Update::EffectCompleted {
            effect,
            event: p::Event::ActResult(p::RunnerResult::ok()),
        });
    }

    /// Hands the current view to the hub (the shell's one call per update).
    fn publish(&self, rig: &mut Rig) -> Vec<HubEffect> {
        rig.hub
            .viewer_update(rig.now, self.live.runs(), self.live.timeline())
    }
}

// ---- the controller -------------------------------------------------------------------

/// A remote that watches: its link and its own Live view.
struct Watcher {
    link: Link,
    view: LiveView,
    events: Vec<ViewerEvent>,
}

impl Watcher {
    fn new(link: Link) -> Self {
        Watcher {
            link,
            view: LiveView::default(),
            events: Vec::new(),
        }
    }

    /// Delivers the effects for this connection; applies viewer events to the view and
    /// sends any request the client made (resync) back to the hub. Reports writes when
    /// `ack` is set. Returns the hub effects that followed.
    fn take(&mut self, rig: &mut Rig, fx: &[HubEffect], ack: bool) -> Vec<HubEffect> {
        let bytes = sends(fx, self.link.conn);
        if bytes.is_empty() {
            return Vec::new();
        }
        let Received { events, viewer } = self.link.client.receive_all(&bytes).unwrap();
        let mut more = Vec::new();
        for e in &viewer {
            if let ViewerEvent::Delta { bytes } | ViewerEvent::Snapshot { bytes, .. } = e {
                if let Applied::NeedsResync(_) = self.view.apply_delta(bytes) {
                    if let Some(req) = self.link.client.request_resync().unwrap() {
                        more.extend(rig.hub.receive(self.link.conn, &req, rig.now));
                    }
                }
            }
        }
        self.events.extend(viewer);
        for e in events {
            if let jarvis_remote::ClientEvent::Send { bytes } = e {
                more.extend(rig.hub.receive(self.link.conn, &bytes, rig.now));
            }
        }
        if ack {
            more.extend(rig.hub.written(self.link.conn, bytes.len(), rig.now));
        }
        more
    }

    /// Delivers until nothing more comes.
    fn drain(&mut self, rig: &mut Rig, fx: &[HubEffect]) {
        let mut fx = fx.to_vec();
        for _ in 0..1_000 {
            fx = self.take(rig, &fx, true);
            if sends(&fx, self.link.conn).is_empty() {
                return;
            }
        }
        panic!("the stream never settled");
    }

    fn request(&mut self, rig: &mut Rig, req: ViewerRequest) -> Vec<HubEffect> {
        let bytes = self.link.client.send_viewer(&req).unwrap();
        rig.hub.receive(self.link.conn, &bytes, rig.now)
    }
}

fn allow(rig: &mut Rig, creds: &jarvis_remote::ClientCredentials, allowed: bool) -> Vec<HubEffect> {
    rig.hub.set_watch_screen(creds.device_id, allowed, rig.now)
}

fn same_as_phone(phone: &Phone, view: &LiveView) {
    let want = ViewerTimeline::decode(&wire_timeline(phone.live.timeline()).encode()).unwrap();
    assert_eq!(
        String::from_utf8(view.timeline().encode()).unwrap(),
        String::from_utf8(want.encode()).unwrap()
    );
}

fn labels(n: usize, salt: &str) -> Vec<String> {
    (0..n).map(|i| format!("{salt} control {i}")).collect()
}

fn decoded(events: &[ViewerEvent]) -> Vec<ViewerDelta> {
    events
        .iter()
        .filter_map(|e| match e {
            ViewerEvent::Delta { bytes } | ViewerEvent::Snapshot { bytes, .. } => {
                Some(ViewerDelta::decode(bytes).unwrap())
            }
            _ => None,
        })
        .collect()
}

fn viewer_bytes(conn: ConnId, fx: &[HubEffect]) -> usize {
    sends(fx, conn).len()
}

// ---- tests ------------------------------------------------------------------------------

#[test]
fn without_the_permission_start_is_denied_and_nothing_is_sent() {
    let mut rig = Rig::new();
    let (mut link, creds) = rig.pair("Mac");
    // Everything else on: watch_screen is independent and off by default.
    rig.set_permissions(
        &mut link,
        &creds,
        RemotePermissions {
            start_tasks: true,
            see_progress: true,
            see_screen_text: true,
            approve: true,
        },
    );
    assert!(!rig.hub.watch_screen(creds.device_id));
    let mut w = Watcher::new(link);
    let fx = w.request(&mut rig, ViewerRequest::StartWatching);
    assert!(audits(&fx).iter().any(|a| matches!(
        a,
        AuditEvent::Denied { request, reason: DenyReason::NotPermitted } if request == "start_watching"
    )));
    w.take(&mut rig, &fx, true);
    assert_eq!(
        w.events,
        vec![ViewerEvent::Denied {
            reason: DenyReason::NotPermitted
        }]
    );
    assert!(!rig.hub.is_watching(w.link.conn));
    let mut phone = Phone::new("t");
    phone.observe(screen_tree(&labels(5, "a")), false);
    let fx = phone.publish(&mut rig);
    assert_eq!(viewer_bytes(w.link.conn, &fx), 0);
    // A resync without the permission is refused the same way.
    let fx = w.request(&mut rig, ViewerRequest::ViewerResync);
    assert!(audits(&fx)
        .iter()
        .any(|a| matches!(a, AuditEvent::Denied { request, .. } if request == "viewer_resync")));
}

#[test]
fn a_watcher_gets_a_snapshot_then_deltas_and_rebuilds_the_phone_timeline() {
    let mut rig = Rig::new();
    let (link, creds) = rig.pair("Mac");
    let fx = allow(&mut rig, &creds, true);
    assert!(fx.contains(&HubEffect::StoreChanged));
    assert!(audits(&fx).iter().any(|a| matches!(
        a,
        AuditEvent::Accepted { request, detail: Some(d) } if request == "watch_screen_permission" && d == "on"
    )));
    // watch_screen leaves the other permissions (and so approvals) as they were.
    assert_eq!(
        rig.hub.devices()[0].permissions,
        RemotePermissions::NEW_DEVICE
    );
    let mut phone = Phone::new("t");
    phone.observe(screen_tree(&labels(5, "first")), false);
    phone.tap(10.0, 70.0);
    phone.publish(&mut rig);

    let mut w = Watcher::new(link);
    let fx = w.request(&mut rig, ViewerRequest::StartWatching);
    assert!(audits(&fx)
        .iter()
        .any(|a| matches!(a, AuditEvent::Accepted { request, .. } if request == "start_watching")));
    w.drain(&mut rig, &fx);
    assert!(matches!(w.events[0], ViewerEvent::Snapshot { part: 0, .. }));
    same_as_phone(&phone, &w.view);
    for i in 0..30 {
        rig.now += 1_000;
        if i % 3 == 0 {
            phone.observe(screen_tree(&labels(8, &format!("s{i}"))), false);
            let fx = phone.publish(&mut rig);
            w.drain(&mut rig, &fx);
        }
        phone.tap(20.0, 80.0 + f64::from(i));
        let fx = phone.publish(&mut rig);
        w.drain(&mut rig, &fx);
    }
    phone
        .live
        .apply(Update::Finished(p::OutcomeStatus::Success));
    let fx = phone.publish(&mut rig);
    w.drain(&mut rig, &fx);
    same_as_phone(&phone, &w.view);
    assert!(!w.events.contains(&ViewerEvent::ResyncRequested));
    // Stop: acknowledged and audited; nothing more is sent.
    let fx = w.request(&mut rig, ViewerRequest::StopWatching);
    assert!(audits(&fx)
        .iter()
        .any(|a| matches!(a, AuditEvent::Accepted { request, .. } if request == "stop_watching")));
    w.take(&mut rig, &fx, true);
    assert_eq!(
        w.events.last(),
        Some(&ViewerEvent::Ended {
            reason: ViewerEndReason::Stopped
        })
    );
    phone.tap(1.0, 1.0);
    assert_eq!(viewer_bytes(w.link.conn, &phone.publish(&mut rig)), 0);
}

#[test]
fn revoking_the_permission_stops_the_stream_in_the_same_call() {
    let mut rig = Rig::new();
    let (link, creds) = rig.pair("Mac");
    allow(&mut rig, &creds, true);
    let mut phone = Phone::new("t");
    phone.observe(screen_tree(&labels(5, "x")), false);
    let mut w = Watcher::new(link);
    let fx = w.request(&mut rig, ViewerRequest::StartWatching);
    w.drain(&mut rig, &fx);
    let fx = allow(&mut rig, &creds, false);
    assert!(!rig.hub.is_watching(w.link.conn));
    assert!(fx.contains(&HubEffect::StoreChanged));
    let a = audits(&fx);
    assert!(a.iter().any(|a| matches!(
        a,
        AuditEvent::Accepted { request, detail: Some(d) } if request == "watch_screen_permission" && d == "off"
    )));
    assert!(a.iter().any(|a| matches!(
        a,
        AuditEvent::Accepted { request, detail: Some(d) } if request == "stop_watching" && d == "permission revoked"
    )));
    w.take(&mut rig, &fx, true);
    assert_eq!(
        w.events.last(),
        Some(&ViewerEvent::Ended {
            reason: ViewerEndReason::PermissionRevoked
        })
    );
    phone.tap(10.0, 70.0);
    assert_eq!(viewer_bytes(w.link.conn, &phone.publish(&mut rig)), 0);
    rig.now += 5_000;
    assert_eq!(viewer_bytes(w.link.conn, &rig.hub.tick(rig.now)), 0);
    // Starting again is refused.
    let fx = w.request(&mut rig, ViewerRequest::StartWatching);
    w.take(&mut rig, &fx, true);
    assert_eq!(
        w.events.last(),
        Some(&ViewerEvent::Denied {
            reason: DenyReason::NotPermitted
        })
    );
}

#[test]
fn two_devices_with_different_permissions() {
    let mut rig = Rig::new();
    let (a, creds_a) = rig.pair("Mac");
    let (mut b, _creds_b) = rig.pair("iPad");
    allow(&mut rig, &creds_a, true);
    // B sees progress (default) but may not watch.
    rig.request(&mut b, ClientMessage::Subscribe);
    let mut wa = Watcher::new(a);
    let mut wb = Watcher::new(b);
    let fx = wa.request(&mut rig, ViewerRequest::StartWatching);
    wa.drain(&mut rig, &fx);
    let fx = wb.request(&mut rig, ViewerRequest::StartWatching);
    wb.take(&mut rig, &fx, true);
    let mut phone = Phone::new("t");
    for i in 0..5 {
        rig.now += 1_000;
        phone.observe(screen_tree(&labels(4, &format!("p{i}"))), false);
        phone.tap(10.0, 70.0);
        let fx = phone.publish(&mut rig);
        assert!(viewer_bytes(wa.link.conn, &fx) > 0);
        assert_eq!(viewer_bytes(wb.link.conn, &fx), 0);
        wa.drain(&mut rig, &fx);
    }
    same_as_phone(&phone, &wa.view);
    assert_eq!(
        wb.events,
        vec![ViewerEvent::Denied {
            reason: DenyReason::NotPermitted
        }]
    );
}

#[test]
fn a_slow_link_is_coalesced_into_one_snapshot() {
    let mut rig = Rig::new();
    let (link, creds) = rig.pair("Mac");
    allow(&mut rig, &creds, true);
    let mut w = Watcher::new(link);
    let mut phone = Phone::new("t");
    let fx = w.request(&mut rig, ViewerRequest::StartWatching);
    // The snapshot goes out; the shell never reports a write, so the window fills up.
    let mut out = viewer_bytes(w.link.conn, &fx);
    let mut held = vec![fx];
    for i in 0..200 {
        rig.now += MIN_SCREEN_INTERVAL_MS;
        phone.observe(screen_tree(&labels(300, &format!("big{i}"))), false);
        phone.tap(10.0, 70.0);
        let fx = phone.publish(&mut rig);
        out += viewer_bytes(w.link.conn, &fx);
        held.push(fx);
    }
    // Bounded: the window, plus at most one more message, plus the start snapshot; the
    // rest was queued (bounded too) and then dropped.
    assert!(
        out <= VIEWER_WINDOW_BYTES + 70_000 + 4 * MAX_SNAPSHOT_BYTES / 3 + 10_000,
        "{out}"
    );
    let _ = VIEWER_QUEUE_BYTES;
    // The link catches up: everything held is delivered and confirmed, then the hub
    // sends one snapshot instead of the dropped deltas.
    for fx in held {
        w.take(&mut rig, &fx, false);
    }
    let fx = rig.hub.written(w.link.conn, usize::MAX, rig.now);
    let snapshots_before = w
        .events
        .iter()
        .filter(|e| matches!(e, ViewerEvent::Snapshot { part: 0, .. }))
        .count();
    w.drain(&mut rig, &fx);
    let snapshots = w
        .events
        .iter()
        .filter(|e| matches!(e, ViewerEvent::Snapshot { part: 0, .. }))
        .count();
    assert_eq!(snapshots, snapshots_before + 1);
    // The controller converged on the newest steps and the current screen.
    let t = w.view.timeline();
    let phone_t = phone.live.timeline();
    assert_eq!(
        t.steps.last().map(|s| s.number),
        phone_t.steps.last().map(|s| s.number)
    );
    assert_eq!(t.current_screen_id, phone_t.current_screen_id);
    assert!(t.current_screen().is_some());
    assert!(!w.view.needs_resync());
}

#[test]
fn at_most_one_screen_per_interval_and_newer_screens_replace_waiting_ones() {
    let mut rig = Rig::new();
    let (link, creds) = rig.pair("Mac");
    allow(&mut rig, &creds, true);
    let mut w = Watcher::new(link);
    let mut phone = Phone::new("t");
    let fx = w.request(&mut rig, ViewerRequest::StartWatching);
    w.drain(&mut rig, &fx);
    rig.now += 10_000;
    let mut screens = Vec::new();
    for salt in ["one", "two", "three"] {
        phone.observe(screen_tree(&labels(3, salt)), false);
        phone.tap(10.0, 70.0);
        let fx = phone.publish(&mut rig);
        let before = w.events.len();
        w.drain(&mut rig, &fx);
        screens.push(
            decoded(&w.events[before..])
                .into_iter()
                .filter_map(|d| match d.body {
                    DeltaBody::Screen(s) => Some(s.id),
                    _ => None,
                })
                .collect::<Vec<_>>(),
        );
        rig.now += 10;
    }
    // The first screen at once; the second waits and is replaced by the third.
    assert_eq!(screens, vec![vec![1], vec![], vec![]]);
    rig.now += MIN_SCREEN_INTERVAL_MS;
    let fx = rig.hub.tick(rig.now);
    let before = w.events.len();
    w.drain(&mut rig, &fx);
    let late: Vec<u32> = decoded(&w.events[before..])
        .into_iter()
        .filter_map(|d| match d.body {
            DeltaBody::Screen(s) => Some(s.id),
            _ => None,
        })
        .collect();
    assert_eq!(late, vec![3]);
    // The replaced screen is "not available"; no gap, no resync.
    let t = w.view.timeline();
    assert!(t.screen(Some(2)).is_none());
    assert!(t.steps.iter().any(|s| s.screen_id == Some(2)));
    assert!(t.current_screen().is_some());
    assert!(!w.events.contains(&ViewerEvent::ResyncRequested));
    assert!(!w.view.needs_resync());
}

#[test]
fn a_locked_phone_streams_nothing_of_its_screen() {
    let mut rig = Rig::new();
    let (link, creds) = rig.pair("Mac");
    allow(&mut rig, &creds, true);
    let mut w = Watcher::new(link);
    let fx = w.request(&mut rig, ViewerRequest::StartWatching);
    w.drain(&mut rig, &fx);
    let mut phone = Phone::new("t");
    phone.observe(
        screen_tree(&[
            "Balance 12,345.67".to_owned(),
            "Unlock code 4242".to_owned(),
        ]),
        true,
    );
    phone.tap(10.0, 70.0);
    phone.live.apply(Update::AwaitingUnlock);
    let fx = phone.publish(&mut rig);
    w.drain(&mut rig, &fx);
    // And a reconnect's snapshot of the same state.
    let fx = w.request(&mut rig, ViewerRequest::ViewerResync);
    w.drain(&mut rig, &fx);
    let mut text = String::new();
    for e in &w.events {
        if let ViewerEvent::Delta { bytes } | ViewerEvent::Snapshot { bytes, .. } = e {
            text.push_str(std::str::from_utf8(bytes).unwrap());
        }
    }
    assert!(!text.contains("Balance") && !text.contains("4242") && !text.contains("12,345"));
    assert!(text.contains(r#""source":"locked""#));
    let t = w.view.timeline();
    assert!(t.current_screen().unwrap().elements.is_empty());
}

#[test]
fn stored_devices_from_before_the_permission_load_with_it_off() {
    let mut rig = Rig::new();
    let (_link, creds) = rig.pair("Mac");
    // What older versions wrote: no watch_screen field anywhere.
    let old = rig.hub.to_json();
    assert!(!old.contains("watch_screen"));
    let hub = RemoteHub::from_json("Phone", &old).unwrap();
    assert!(!hub.watch_screen(creds.device_id));
    // On: persisted and restored; the plain store reader still accepts the record.
    allow(&mut rig, &creds, true);
    let json = rig.hub.to_json();
    assert!(json.contains(r#""watch_screen":true"#));
    let hub = RemoteHub::from_json("Phone", &json).unwrap();
    assert!(hub.watch_screen(creds.device_id));
    assert_eq!(DeviceStore::from_json(&json).unwrap(), rig.hub.store());
    // Off again: the record is byte for byte the old shape.
    allow(&mut rig, &creds, false);
    assert_eq!(*rig.hub.to_json(), *old);
    // Revoking the device forgets the permission.
    allow(&mut rig, &creds, true);
    rig.local(LocalEvent::DeviceRevoked {
        device_id: creds.device_id,
    });
    assert!(!rig.hub.watch_screen(creds.device_id));
}

#[test]
fn a_new_run_continues_the_stream_with_a_reset() {
    let mut rig = Rig::new();
    let (link, creds) = rig.pair("Mac");
    allow(&mut rig, &creds, true);
    let mut w = Watcher::new(link);
    let fx = w.request(&mut rig, ViewerRequest::StartWatching);
    w.drain(&mut rig, &fx);
    let mut phone = Phone::new("one");
    phone.observe(screen_tree(&labels(3, "a")), false);
    phone.tap(10.0, 70.0);
    let fx = phone.publish(&mut rig);
    w.drain(&mut rig, &fx);
    rig.now += 1_000;
    phone.live.apply(Update::Started {
        task_id: "two".into(),
    });
    phone.number = 0;
    phone.observe(screen_tree(&labels(3, "b")), false);
    phone.tap(10.0, 70.0);
    let fx = phone.publish(&mut rig);
    w.drain(&mut rig, &fx);
    same_as_phone(&phone, &w.view);
    assert!(!w.events.contains(&ViewerEvent::ResyncRequested));
}
