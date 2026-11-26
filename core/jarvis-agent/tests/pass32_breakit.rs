#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Review pass 32: break-it tests against b12bb31 (review pass 31).
//!
//! FINDINGS (these tests FAIL on b12bb31, for the reason named):
//! - `f1_padded_list_row_swipe_is_not_bounded_by_the_row`: a real iOS 26.5 list row (P13,
//!   `Cell {{20, 278}, {400, 52}}` in `CollectionView {{0, 0}, {440, 956}}`) inset to 0.79 of
//!   its list (an inset/readable-width list) is read as a "grid cell": the approved row swipe
//!   is sent at the screen cap (0.60 of the row for an element swipe, 0.69 element-less),
//!   at or above the measured delete shares (0.60 leading, 0.66 trailing), not at 0.45.
//! - `f1b_a_one_row_list_is_read_as_a_pager_page`: the same through `PAGE_FILL_SHARE`: a
//!   list sized to its one row reads that full-width row as a pager page (no bound).
//! - `f2_node_count_cut_does_not_bound_the_frame`: the wire's node-count cut does not bound
//!   the encoded size: 20,000 nodes with 1,000-byte labels (well under `MAX_TREE_NODES`)
//!   encode past `MAX_SERVER_FRAME`, so the server answers "the reply is too large" and the
//!   session never sees a (cut, flagged) screen it could leave.
//!
//! SOUND (these pass; properties checked):
//! - `s1_padded_row_swipe_is_still_refused_under_deny_delete_and_asked_as_delete`
//! - `s2_cut_screen_shares_the_digest_but_the_approval_recheck_voids`
//! - `s3_many_stacked_controls_stay_linear_and_name_the_denied_one`
//! - `s4_runner_text_and_typed_cut_screens_both_get_the_incomplete_rules_in_a_session`

mod common;

#[path = "../../jarvis-observe/tests/common/ios.rs"]
mod ios;

use common::*;
use jarvis_memory::MemoryStore;
use jarvis_observe::{compact_view, screen_digest, CompactView};
use jarvis_proto::fixtures::{node, rect, screen, IPHONE_SIZE};
use jarvis_proto::{
    Action, ConfirmRequest, Direction, Effect, Event, ImageRef, ModelOutput, Observation, Policy,
    Rect, RiskCategory, Role, RunnerCommand, RunnerResult, Screen, TaskSpec, TimedEvent, UiNode,
    MAX_TREE_DEPTH,
};
use jarvis_safety::gestures::{GestureKind, ROW_SWIPE_SHARE};
use jarvis_safety::{
    evaluate_owned, planned_drag, Decision, ScreenSource, ScreenTree, INCOMPLETE_SCREEN_WARNING,
    ROW_SWIPE_NOTE,
};

// ---------------------------------------------------------------------------------------
// Helpers.
// ---------------------------------------------------------------------------------------

fn compact_text(c: &jarvis_proto::CompactScreen) -> String {
    format!("{c:?}")
}

fn tree(v: &CompactView) -> ScreenTree<'_> {
    ScreenTree {
        sources: &v.sources,
        parents: &v.parents,
        ends: &v.ends,
        bars: &v.bars,
        nodes: &v.nodes,
        inputs: &v.inputs,
    }
}

fn shop(kids: Vec<UiNode>) -> Screen {
    screen(
        "com.example.shop",
        "Shop",
        UiNode::new(Role::Application, IPHONE_SIZE.to_rect()).with_children(vec![UiNode::new(
            Role::Window,
            IPHONE_SIZE.to_rect(),
        )
        .with_children(kids)]),
        false,
    )
}

fn decide_on(p: &Policy, a: &Action, v: &CompactView) -> Decision {
    evaluate_owned(
        p,
        a,
        &v.screen,
        None,
        ScreenSource::Typed,
        Some(&tree(v)),
        &[],
        &[],
    )
}

fn edit(n: &mut UiNode, f: &mut impl FnMut(&mut UiNode)) {
    f(n);
    for c in &mut n.children {
        edit(c, f);
    }
}

fn parse(text: &str) -> Screen {
    let typed = text
        .split("\n== raw XCUITest snapshot")
        .next()
        .unwrap_or(text);
    ios::parse_screen(typed).unwrap()
}

fn big_stack<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::Builder::new()
        .stack_size(64 << 20)
        .spawn(f)
        .unwrap()
        .join()
        .unwrap()
}

const P13_ROWS: &str = include_str!("fixtures/ios26_probes3/p13-swipe-lengths-01-rows-tree.txt");

/// The real P13 list (iOS 26.5, `CollectionView {{0, 0}, {440, 956}}`, 400-pt rows at x=20)
/// with every row's `Cell` — and everything it holds that spans it — inset to `share` of
/// the list's width, centred; the list itself keeps its width. This is an inset list (a
/// list with wider section insets, a readable-width list on a wide screen): every row
/// still spans most of its list and keeps its native swipe actions.
fn padded(share: f64) -> (Screen, f64) {
    let mut s = parse(P13_ROWS);
    let width = 440.0 * share;
    let x = (440.0 - width) / 2.0;
    edit(&mut s.root, &mut |n| {
        if n.role == Role::Cell {
            edit(n, &mut |k| {
                if k.frame.width >= 399.0 && k.frame.width <= 401.0 {
                    k.frame.x = x;
                    k.frame.width = width;
                }
            });
        }
    });
    (s, width)
}

fn travel(s: &Screen, label: Option<&str>) -> f64 {
    let v = compact_view(s);
    let frame = label.map(|l| {
        v.screen
            .elements
            .iter()
            .find(|e| e.label == l)
            .unwrap_or_else(|| panic!("{l}"))
            .frame
    });
    let g = planned_drag(
        GestureKind::Swipe,
        Direction::Left,
        frame,
        &v.screen,
        Some(&tree(&v)),
    )
    .unwrap();
    (g.to.x - g.from.x).hypot(g.to.y - g.from.y)
}

// ---------------------------------------------------------------------------------------
// F1 (finding): a list row inset below 0.8 of its list is read as a grid cell, and the
// approved row swipe is sent at the screen cap.
// ---------------------------------------------------------------------------------------

/// Run 146/149 measured on iOS 26.5: a 400-pt row was deleted from 0.66 of its width, a
/// leading-action row from 0.60, narrow rows from 0.75; 0.45 is the bound
/// (`ROW_SWIPE_SHARE`). Pass 31 applies that bound only to rows spanning >= 0.8 of their
/// scroller. The real P13 rows inset to 0.79 (347.6 pt) are no longer bounded: the element
/// swipe travels 0.6 of the row (208.6 pt; 0.60 deleted a real row) and the element-less
/// swipe 240 pt (0.69 of the row; 0.66 deleted a real row). Both are sent once the owner
/// approves the row-swipe question (default policy) — and unasked when the owner cleared
/// the confirm list. Expected: at most `ROW_SWIPE_SHARE` of the row, as for a 0.8 row.
#[test]
fn f1_padded_list_row_swipe_is_not_bounded_by_the_row() {
    // Control: at 0.80 the bound holds.
    let (s80, w80) = padded(0.80);
    assert!(travel(&s80, Some("Full swipe row 1")) <= ROW_SWIPE_SHARE * w80 + 1e-6);
    let mut failures = Vec::new();
    for share in [0.79f64, 0.70, 0.60] {
        let (s, w) = padded(share);
        for label in [Some("Full swipe row 1"), None] {
            let t = travel(&s, label);
            if t > ROW_SWIPE_SHARE * w + 1e-6 {
                failures.push(format!(
                    "share {share}: {label:?} travels {t:.1} pt = {:.2} of the {w:.1}-pt row (bound {:.2})",
                    t / w,
                    ROW_SWIPE_SHARE
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// The same class through `PAGE_FILL_SHARE`: a list sized to its content (a one-row table
/// in a form, the list's frame one row tall — a common UIKit/SwiftUI layout) makes its one
/// full-width deletable row "a page of a pager" (it fills >= 0.9 of the scroller's height),
/// so the approved swipe on it gets the screen cap (240 pt, 0.60 of the 400-pt row) instead
/// of the 0.45 bound. (On this row kind 240 pt was measured to reveal only and 264 pt to
/// delete; 0.60 of the row is the lowest share measured to delete on any row kind — the
/// leading-action row. Not shown to delete here: the bound is simply not applied.)
#[test]
fn f1b_a_one_row_list_is_read_as_a_pager_page() {
    let mut s = parse(P13_ROWS);
    // The real P13 row "Full swipe row 1": `Cell {{20, 278}, {400, 52}}`.
    edit(&mut s.root, &mut |n| {
        if n.role == Role::CollectionView {
            n.frame = Rect::new(0.0, 278.0, 440.0, 52.0);
        }
    });
    let t = travel(&s, Some("Full swipe row 1"));
    assert!(
        t <= ROW_SWIPE_SHARE * 400.0 + 1e-6,
        "travels {t:.1} pt = {:.2} of the 400-pt row (bound {ROW_SWIPE_SHARE})",
        t / 400.0
    );
}

// ---------------------------------------------------------------------------------------
// F2 (finding): the wire's node-count cut does not bound the frame size.
// ---------------------------------------------------------------------------------------

/// The commit's claim: a tree past the wire's bounds is "a deliberate cut, not a refusal:
/// the session can then leave the screen". A page with 20,000 text nodes of 1,000 bytes
/// each (a long article; no node or depth bound is reached) encodes to ~20 MB, past
/// `MAX_SERVER_FRAME` (16 MB): `RunnerServerChannel::reply` then sends "the reply is too
/// large for the runner protocol" instead of a snapshot, every look fails
/// (`ObserveFailed`), and the task ends "Could not read the screen" — the session never gets
/// an incomplete screen it could press Home on. Expected: the encoder cuts (and flags) by
/// bytes too, so every snapshot fits the frame.
#[test]
fn f2_node_count_cut_does_not_bound_the_frame() {
    use jarvis_runner::message::encode_reply;
    use jarvis_runner::{LockState, Reply, Snapshot};
    let long = "x".repeat(1_000);
    let kids: Vec<UiNode> = (0..20_000)
        .map(|i| {
            UiNode::new(Role::StaticText, rect(0.0, f64::from(i), 393.0, 1.0))
                .with_label(format!("{i} {long}"))
        })
        .collect();
    let s = shop(kids);
    let snap = Snapshot {
        root: s.root.clone(),
        app_bundle_id: s.app_bundle_id.clone(),
        app_name: s.app_name.clone(),
        screen_size: s.size,
        keyboard_visible: false,
        orientation: s.orientation,
        device_locked: false,
        lock_state: LockState::Unlocked,
        notes: vec![],
        screenshot: None,
        tree_cut: false,
    };
    let bytes = encode_reply(7, &Reply::Snapshot(Box::new(snap))).expect("encoded");
    assert!(
        bytes.len() + jarvis_runner::channel::DATA_OVERHEAD <= jarvis_runner::MAX_SERVER_FRAME,
        "a {}-node tree encodes to {} bytes, past the {}-byte frame: the server replies \
         'too large' and the session cannot read (or leave) the screen",
        20_002,
        bytes.len(),
        jarvis_runner::MAX_SERVER_FRAME
    );
}

// ---------------------------------------------------------------------------------------
// SOUND: properties checked and found to hold.
// ---------------------------------------------------------------------------------------

/// The other half of F1: the deny policy still holds for the "grid cell" (the row-swipe
/// question reads ANY row, `CrossedRows::any`, and carries `Delete`), and the default asks
/// as `Delete`, phone only, with the row-swipe note.
#[test]
fn s1_padded_row_swipe_is_still_refused_under_deny_delete_and_asked_as_delete() {
    for share in [0.79f64, 0.60] {
        let (s, _) = padded(share);
        let v = compact_view(&s);
        let row = v
            .screen
            .elements
            .iter()
            .find(|e| e.label == "Full swipe row 1")
            .unwrap();
        for a in [
            Action::Swipe {
                direction: Direction::Left,
                element: Some(row.index),
            },
            Action::Swipe {
                direction: Direction::Left,
                element: None,
            },
            Action::Scroll {
                direction: Direction::Right,
                element: Some(row.index),
            },
        ] {
            let mut deny = config().policy;
            deny.deny_categories = vec![RiskCategory::Delete];
            assert!(
                matches!(decide_on(&deny, &a, &v), Decision::Deny(_)),
                "{share} {a:?}"
            );
            let Decision::Confirm(q) = decide_on(&config().policy, &a, &v) else {
                panic!("{share} {a:?}: not asked")
            };
            assert_eq!(q.category, RiskCategory::Delete);
            assert!(q.phone_only && q.summary.contains(ROW_SWIPE_NOTE));
        }
    }
}

fn nest(depth: usize, leaf: UiNode) -> UiNode {
    let mut n = leaf;
    for _ in 0..depth {
        n = UiNode::new(Role::Other("Other".into()), IPHONE_SIZE.to_rect()).with_children(vec![n]);
    }
    n
}

fn buy_now() -> UiNode {
    UiNode::new(Role::Button, rect(16.0, 400.0, 361.0, 44.0))
        .with_label("Buy now")
        .hittable(true)
}

/// A screen exactly `MAX_TREE_DEPTH` levels deep, "Buy now" the deepest node; with
/// `overlay`, "Buy now" also holds a full-screen "Confirm transfer of $5,000" button (one
/// level past the bound), cut by the FFI/wire intake.
fn at_bound(overlay: bool) -> Screen {
    let mut leaf = buy_now();
    if overlay {
        leaf = leaf.with_children(vec![node(
            Role::Button,
            "Confirm transfer of $5,000",
            IPHONE_SIZE.to_rect(),
        )]);
    }
    let mut s = shop(vec![
        node(Role::StaticText, "Account", rect(16.0, 100.0, 200.0, 30.0)),
        node(Role::Button, "Help", rect(16.0, 150.0, 100.0, 44.0)),
        nest(MAX_TREE_DEPTH - 3, leaf),
    ]);
    if overlay {
        s.tree_cut = jarvis_proto::cut_tree(&mut s.root);
        assert!(s.tree_cut);
    }
    s
}

/// A driver that shows `first` until the first question, then `after` (the app swaps the
/// screen while the owner answers).
struct Swapped {
    acts: Vec<RunnerCommand>,
    confirms: Vec<ConfirmRequest>,
}

fn run_swapped(first: Screen, after: Screen, steps: Vec<String>, decisions: Vec<bool>) -> Swapped {
    let mut session = jarvis_agent::Session::new(
        vision_config(),
        TaskSpec::new("t1", "Buy the item"),
        MemoryStore::new(),
    );
    let mut steps: std::collections::VecDeque<String> = steps.into();
    let mut decisions: std::collections::VecDeque<bool> = decisions.into();
    let mut phone = first;
    let mut out = Swapped {
        acts: Vec::new(),
        confirms: Vec::new(),
    };
    let mut now = 1_000u64;
    let mut event = Event::Start;
    for _ in 0..MAX_ITERATIONS {
        let effect = session.step(TimedEvent::new(now, event));
        now += TICK_MS;
        event = match effect {
            Effect::Observe { screenshot } => Event::Observed(Observation {
                screen: phone.clone(),
                screenshot: screenshot.then(|| ImageRef {
                    id: "shot".into(),
                    width: 1179,
                    height: 2556,
                }),
                device_locked: false,
            }),
            Effect::Infer(_) => match steps.pop_front() {
                Some(text) => Event::ModelOutput(ModelOutput { text }),
                None => Event::ModelFailed {
                    error: "script exhausted".into(),
                },
            },
            Effect::Act(cmd) => {
                out.acts.push(cmd.without_expect());
                Event::ActResult(RunnerResult::ok())
            }
            Effect::Confirm(req) => {
                out.confirms.push(req);
                phone = after.clone();
                Event::UserDecision {
                    approved: decisions.pop_front().unwrap_or(false),
                }
            }
            Effect::AskUser { .. } => Event::UserAnswer {
                text: String::new(),
            },
            Effect::CallTool(_) => panic!("no tools"),
            Effect::AwaitUnlock { .. } => Event::DeviceUnlocked,
            Effect::Finished(_) => return out,
        };
    }
    panic!("did not finish");
}

/// The digest does not hash `tree_cut`, and a cut drops what lies past the bound: the cut
/// screen with a full-screen overlay past the bound has the SAME digest as the complete
/// screen without it (A1 of the approval re-check cannot tell them apart). The re-check is
/// still sound: the gate runs again on the fresh (incomplete) screen and its question
/// differs (the incomplete-screen warning), so the approval is void and nothing is tapped;
/// the model's retry is asked with the warning, phone only, and declining sends nothing.
/// (Under a denied category the tap is refused outright.)
#[test]
fn s2_cut_screen_shares_the_digest_but_the_approval_recheck_voids() {
    let r = big_stack(|| {
        let a = at_bound(false);
        let b = at_bound(true);
        assert!(!compact_view(&a).screen.cues.truncated);
        assert!(compact_view(&b).screen.cues.truncated);
        assert_eq!(screen_digest(&a), screen_digest(&b), "digest observation");
        let v = compact_view(&a);
        let buy = v
            .screen
            .elements
            .iter()
            .find(|e| e.label == "Buy now")
            .unwrap()
            .index;
        let r = run_swapped(
            a,
            b,
            vec![
                format!(r#"{{"action":"tap","element":{buy}}}"#),
                format!(r#"{{"action":"tap","element":{buy}}}"#),
                r#"{"action":"fail","reason":"stop"}"#.to_owned(),
            ],
            vec![true, false, false],
        );
        (r.acts, r.confirms)
    });
    let (acts, confirms) = r;
    assert!(
        !acts.iter().any(|a| matches!(a, RunnerCommand::Tap { .. })),
        "{acts:?} {confirms:#?}"
    );
    // The retry's question about the swapped screen carries the warning, phone only.
    assert!(confirms.len() >= 2, "{confirms:#?}");
    for q in confirms.iter().skip(1) {
        assert!(
            q.summary.starts_with(INCOMPLETE_SCREEN_WARNING) && q.phone_only,
            "{confirms:#?}"
        );
    }
}

/// M2: thousands of stacked buttons over a "Delete account" button: a denied `Delete` is
/// refused at every count, the default names "Delete account" (never a decoy), and the
/// classification stays linear (5x the controls costs at most ~7x; measured in a debug
/// build: ~0.45 ms per stacked control under a `tap_point`).
#[test]
fn s3_many_stacked_controls_stay_linear_and_name_the_denied_one() {
    let stack = |n: usize| {
        let mut kids = vec![node(
            Role::Button,
            "Delete account",
            rect(16.0, 380.0, 361.0, 84.0),
        )];
        for k in 0..n {
            // Decoys sorted smallest first AND last in screen order: ordering cannot hide it.
            let d = 2.0 + (k as f64) * 0.001;
            kids.push(node(
                Role::Button,
                &format!("Info {k}"),
                rect(196.5 - d, 422.0 - d, 2.0 * d, 2.0 * d),
            ));
        }
        kids.reverse();
        shop(kids)
    };
    let point = Action::TapPoint {
        x: 196.5 / IPHONE_SIZE.width,
        y: 422.0 / IPHONE_SIZE.height,
    };
    let mut times = Vec::new();
    for n in [400usize, 2_000] {
        let v = compact_view(&stack(n));
        let mut deny = config().policy;
        deny.deny_categories = vec![RiskCategory::Delete];
        let t = std::time::Instant::now();
        assert!(matches!(decide_on(&deny, &point, &v), Decision::Deny(_)));
        times.push(t.elapsed().as_secs_f64());
        let Decision::Confirm(q) = decide_on(&config().policy, &point, &v) else {
            panic!("not asked")
        };
        assert_eq!(q.category, RiskCategory::Delete);
        assert!(q.phone_only);
        assert!(q.summary.contains("Delete account"), "{}", q.summary);
        assert!(
            q.summary
                .contains(&format!("{} controls are under this point", n + 1)),
            "{}",
            q.summary
        );
    }
    assert!(times[1] < times[0] * 7.0 + 0.05, "{times:?}");
}

/// Claim 1 checked on the session's paths: a typed tree flagged as cut (any intake) and
/// runner text cut by the parser both reach the gate as incomplete — a tap asks with the
/// warning (phone only) by default and is refused under a denied category, an element-less
/// vertical scroll too; Home is sent. (The session's only `Current` is built in `ingest`,
/// and every gate call reads `self.current.source`.)
#[test]
fn s4_runner_text_and_typed_cut_screens_both_get_the_incomplete_rules_in_a_session() {
    let mut s = shop(vec![buy_now()]);
    s.tree_cut = true;
    let v = compact_view(&s);
    let buy = v
        .screen
        .elements
        .iter()
        .find(|e| e.label == "Buy now")
        .unwrap()
        .index;
    let tap = format!(r#"{{"action":"tap","element":{buy}}}"#);
    let fail = r#"{"action":"fail","reason":"stop"}"#;
    // Typed.
    let r = run(
        config(),
        "Buy",
        MemoryStore::new(),
        Phone::still(s.clone()),
        Script::model(&[&tap, fail]),
    );
    let q = r.confirms.first().expect("asked");
    assert!(q.phone_only && q.summary.starts_with(INCOMPLETE_SCREEN_WARNING));
    let mut c = config();
    c.policy.deny_categories = vec![RiskCategory::Payment];
    for step in [
        tap.as_str(),
        r#"{"action":"scroll","direction":"down"}"#,
        r#"{"action":"open_url","url":"https://example.com"}"#,
    ] {
        let r = run(
            c.clone(),
            "Buy",
            MemoryStore::new(),
            Phone::still(s.clone()),
            Script::model(&[step, fail]).decisions(&[true; 4]),
        );
        assert!(r.confirms.is_empty(), "{step}: {:?}", r.confirms);
        assert!(
            r.acts
                .iter()
                .all(|a| matches!(a, RunnerCommand::Wait { .. })),
            "{step}: {:?}",
            r.acts
        );
    }
    let r = run(
        c,
        "Buy",
        MemoryStore::new(),
        Phone::still(s),
        Script::model(&[r#"{"action":"press_home"}"#, fail]).decisions(&[true; 4]),
    );
    assert!(r.acts.iter().any(|a| matches!(
        a,
        RunnerCommand::PressKey {
            key: jarvis_proto::Key::Home,
            ..
        }
    )));
    let _ = Rect::new(0.0, 0.0, 1.0, 1.0);
}
