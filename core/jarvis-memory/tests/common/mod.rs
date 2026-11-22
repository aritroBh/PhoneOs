#![allow(dead_code)]

use jarvis_memory::*;
use jarvis_proto::fixtures::IPHONE_SIZE;
use jarvis_proto::{Action, CompactScreen, Element, Rect, Role};

pub fn el(index: u32, role: Role, label: &str, fingerprint: u64) -> Element {
    Element {
        index,
        role,
        label: label.to_owned(),
        value: None,
        placeholder: None,
        frame: Rect::new(10.0, 40.0 * f64::from(index), 300.0, 30.0),
        enabled: true,
        focused: false,
        selected: false,
        secure: false,
        fingerprint,
    }
}

pub fn screen(app: &str, elements: Vec<Element>) -> CompactScreen {
    CompactScreen {
        app_bundle_id: app.to_owned(),
        app_name: None,
        size: IPHONE_SIZE,
        keyboard_visible: false,
        elements,
    }
}

pub const MSG: &str = "com.apple.MobileSMS";

/// Messages: list of conversations.
pub fn inbox() -> CompactScreen {
    screen(
        MSG,
        vec![
            el(1, Role::SearchField, "Search", 100),
            el(2, Role::Cell, "Mom", 101),
            el(3, Role::Cell, "Dad", 102),
            el(4, Role::Button, "Compose", 103),
        ],
    )
}

/// Messages: a conversation with Mom.
pub fn thread() -> CompactScreen {
    let mut field = el(2, Role::TextField, "iMessage", 201);
    field.focused = true;
    let mut s = screen(
        MSG,
        vec![
            el(1, Role::Button, "Back", 200),
            field,
            el(3, Role::Button, "Send", 202),
        ],
    );
    s.keyboard_visible = true;
    s
}

pub fn home() -> CompactScreen {
    screen(
        "com.apple.springboard",
        vec![el(1, Role::Button, "Messages", 1)],
    )
}

pub const BUNDLE_HOME: &str = "com.apple.springboard";

/// Records the canonical 4-step "text mom" flow for `task` typing `typed`.
pub fn record_text_flow(task: &str, typed: &str) -> Option<Flow> {
    let (h, i, t) = (home(), inbox(), thread());
    let mut rec = Recorder::new(task);
    rec.record(RecordedStep {
        action: Action::OpenApp {
            app: "Messages".to_owned(),
        },
        element: None,
        screen: &h,
    });
    rec.record(RecordedStep {
        action: Action::Tap { element: 2 },
        element: i.get(2),
        screen: &i,
    });
    rec.record(RecordedStep {
        action: Action::Type {
            element: Some(2),
            text: typed.to_owned(),
            submit: false,
        },
        element: t.get(2),
        screen: &t,
    });
    rec.record(RecordedStep {
        action: Action::Tap { element: 3 },
        element: t.get(3),
        screen: &t,
    });
    rec.finish(true, 1_000)
}

/// Replays `flow` against the canonical screens; returns the emitted actions.
pub fn replay_all(flow: Flow, bindings: Bindings) -> Vec<Action> {
    let screens = [home(), inbox(), thread(), thread()];
    let mut rp = Replayer::new(flow, bindings);
    let mut out = Vec::new();
    for s in &screens {
        match rp.next(s) {
            Next::Action(a) => out.push(a),
            other => panic!("unexpected {other:?}"),
        }
        rp.advance();
    }
    assert_eq!(rp.next(&thread()), Next::Complete);
    out
}
