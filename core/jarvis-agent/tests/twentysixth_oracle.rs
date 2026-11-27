#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::too_many_lines,
    clippy::cognitive_complexity
)]
//! Twenty-sixth review: an independent oracle of Rule T, written from `docs/ARCHITECTURE.md`
//! alone (jarvis-safety threat model (1)-(3), reviews 16-24, Known gaps), and a fuzzer that
//! drives the real session against it. The oracle never calls the gate's decision logic; it
//! uses only the public observation layer (`compact`, the gesture geometry) and the effects.
//!
//! The oracle's reading of the specification (numbered; `A#` = an ambiguity and the reading
//! chosen):
//!  1. Places: `App(bundle)` = everything outside every web view; a listed browser's chrome
//!     is the browser's App place. A top-level web view of a listed browser whose single
//!     address node (label Address/URL/Address bar/Website address, role text field or
//!     button, not focused, value an ASCII host, the only address-like node) is readable is
//!     `Site(host)`; otherwise `pages in <Browser> with no readable address` (covering). A
//!     top-level web view of any other app is `web content in <App>` (covering); a web view
//!     inside a web view is `nested web content in <App>` (covering).
//!  2. Covering classes are sources for themselves from their first observation, at every
//!     kind (observe included).
//!  3. SpringBoard: a read only when it shows more than icons (a banner, alert, sheet,
//!     Spotlight). As a destination it is "the Home Screen"; it is never an observer (F4).
//!  4. Jarvis's own app (not driven): not a read, never a destination nor an observer; no
//!     touch, typing or look of its screen is sent; its text reaches no prompt.
//!  5. The screen picture: every prompt carrying a screenshot reads one place `screen
//!     pictures (...)`, a source for every place. No screenshot of the Home Screen is given.
//!  6. A tool is `tool:<name>`; its own earlier results are a source for its later calls
//!     unless it is self-contained. Its result is read at the next prompt.
//!  7. Read set: the places shown by every observation ingested since the previous prompt
//!     are added when the next prompt is built (A3: "exposed when ingested, read when the
//!     next prompt is built"); locked observations are never read.
//!  8. A place reads its App iff a node outside every web view carries a label, value or a
//!     control role, other than the readable address node itself (A2: the `Application` and
//!     `Window` nodes' own labels are not content).
//!  9. Places on a screen: its App (always, A1: even when it shows nothing, its accessibility
//!     code serves every snapshot) and every web-view place; on SpringBoard only the Home
//!     Screen; Jarvis's own app has none.
//! 10. Kinds: observe (`wait`, `look`, `press_home`) < scroll < full (everything else). A
//!     scroll is of the scroll kind only when it names no element or a scroll container and
//!     its drag (the runner's path) crosses no reacting control: vertically a picker or
//!     slider; sideways also an enabled button or cell (A8: list rows; the fuzz trees have no
//!     carousels outside the Home Screen).
//! 11. Own destinations: a touch's = the place under the point it lands on (A6: for a sent
//!     command the command's own point); `tap_point` = the App plus the place under the
//!     point; a scroll/swipe = its element's place plus the place under its start point;
//!     typing = its field's place; `open_app` = the App opened; `open_url` = the link host's
//!     site (A13: in Safari); a tool call = the tool; a Home Screen icon whose whole label is a
//!     known app (or `<name>, App` / `<name>, <n> unread|notification(s)`) = that App, any
//!     other icon or row = Unknown; `wait`, `look`, `press_home` none.
//! 12. Observers: every place on the screen that is not an own destination, minus SpringBoard
//!     and Jarvis's own app, each at the observe kind.
//! 13. Sources of a destination D: every read place other than D (D itself too when D is
//!     covering, or a tool that is not self-contained). A known-app Home Screen launch does
//!     not count the Home Screen as a source for the App it opens. Unknown: every read place.
//! 14. Rule T: before a model step is carried out, every (D, S, kind) above must be granted
//!     at that kind or stronger, or approved for this action; otherwise the step asks on the
//!     phone (whatever `confirm` holds), and nothing is sent, looked at or called before the
//!     answer. `done`, `fail`, `ask_user` are never gated.
//! 15. What an approval keeps, per named (D, S): an observer pair keeps observe; an own
//!     pair keeps the action's kind for observe and scroll; for full only with exactly one
//!     decidable own destination and not a swipe; otherwise this action only. A grant never
//!     upgrades; a full grant covers scroll and observe, scroll covers observe.
//! 16. A question names its sources and its destinations: one own destination and no other
//!     named → "here"; several own → after "into"; observers after "to". Its approval allows
//!     exactly named sources × named destinations; a partial question ("Jarvis asks about
//!     them/it next") allows only what it named and the action is decided again.
//! 17. Decline: named sources are blocked into every own destination and observer of the
//!     action for the task; a later step needing a blocked pair is refused or ends the task;
//!     nothing is sent, looked at or called after a decline for that step.
//! 18. Restricted state: some observer-eligible place on the current screen has a read source
//!     not granted into it at the observe kind. Then every observation must directly follow a
//!     sent command, a tool result, an approval, an owner's answer, an unlock or the start.
//! 19. Replayed steps (A9: not generated here), Rule P and the share sheet are not modelled.
//!
//! Ambiguities, besides A1-A3, A6, A8, A9, A13 above: A4 grants given by an approval are kept
//! even when the approved step then goes stale (the looser reading: the implementation may
//! only ask more); A5 a flow approval is kept when a later question of the same step is
//! declined; A7 the name of an Unknown destination is unspecified (any unmapped destination
//! name is accepted when the step has one); A10 the "current screen" of the restricted test
//! is the last observation the session received; A11 when a typing's field is unknown its
//! destinations are every place on the screen (several: keeps this action only).
//!
//! Triage (phase D, after reading the implementation): A12 "the places under a point" are
//! the screen's own place and every web view holding the point (the oracle first took the
//! innermost only: oracle bug); D3 a scroll "starts on a control" when its touch-down is on a
//! button, link or adjustable control (the oracle first counted cells and fields: oracle
//! bug; the document does not say which); the classified mismatches are listed at
//! `CLASSIFIED` below.
//!
//! The model, the phone, the generator and the driver live in `oracle_model/`, shared with
//! `oracle_extended.rs` (pickers, cut trees, system layers, alerts, focus failures, place
//! changes); every extension there is behind `Features`, all zero here, so this file's
//! tasks, random draws and checks are what they were.

mod oracle_model;

use oracle_model::*;

/// Mismatch classes triaged in this review (see the report); empty means every check must
/// hold.
const CLASSIFIED: &[&str] = &[
    // Document gap: a decline blocks the named sources only where they were not already
    // allowed (an earlier approval of the same pair stands). Landed (ruling D1): specified
    // behaviour, `docs/ARCHITECTURE.md`, Confinement, "A decline answers the question
    // shown"; the oracle's rule 17 is the looser reading.
    "G2-decline-over-earlier-grant",
    // Document gap: a place that comes on screen during a step's own looks (the focus look of
    // a typing or a clear) is not asked about for the rest of that step. Landed (ruling D2):
    // a documented residual, `docs/ARCHITECTURE.md`, Known gaps, "A place renamed during a
    // step".
    "gap-mid-step-new-place",
    // Twenty-sixth review, landing: L1 (a long press on an icon resolved as a launch), L2
    // (Jarvis's own app a destination of a wait / press_home) and the debug assertion in
    // the question's composition (`6-panic-summary-slot`) are fixed; their entries are
    // removed so that the fuzzer fails if they come back.
];

#[test]
fn fuzz_rule_t_against_the_independent_oracle() {
    let n = cases();
    let s0 = seed0();
    let one = std::env::var("ORACLE26_ONE").is_ok();
    let mut rep = Report::default();
    let started = std::time::Instant::now();
    for i in 0..n {
        let t = Task::random(s0 + i);
        let mut local = rep.clone();
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            run_task(&t, &mut local, one);
            local
        }));
        match r {
            Ok(l) => rep = l,
            Err(e) => {
                let msg = e
                    .downcast_ref::<String>()
                    .cloned()
                    .or_else(|| e.downcast_ref::<&str>().map(|s| (*s).to_owned()))
                    .unwrap_or_default();
                rep.tasks += 1;
                // LOW, probe `a_typing_question_with_a_flow_note_composes_without_a_panic`:
                // the Return-alone fallback of the summary's sends slot is not bounded by
                // its allotment (a debug assertion; release builds hard-cut the slot).
                let check = if msg.contains("a summary slot exceeds its allotment") {
                    "6-panic-summary-slot"
                } else {
                    "6-panic"
                };
                rep.flag(
                    check,
                    format!(
                        "seed {}: {}",
                        t.seed,
                        msg.chars().take(300).collect::<String>()
                    ),
                );
            }
        }
        if one {
            break;
        }
    }
    let mut load: Vec<usize> = rep.approve_all_load.iter().map(|x| x.0).collect();
    load.sort_unstable();
    let pct = |q: f64| -> usize {
        if load.is_empty() {
            0
        } else {
            load[((load.len() - 1) as f64 * q).round() as usize]
        }
    };
    eprintln!(
        "oracle26: {} tasks, {} model steps, {} sent, {} flow questions ({} declined), {} over-asks, statuses {:?}, {:.1}s",
        rep.tasks,
        rep.steps,
        rep.sent,
        rep.flow_questions,
        rep.declined,
        rep.over_asks,
        rep.statuses,
        started.elapsed().as_secs_f64()
    );
    eprintln!(
        "checked: {} sends, {} flow questions, {} must-ask steps, {} looks in the restricted state; by owner (tasks, steps, tasks >= 30 steps): {:?}",
        rep.sends_checked, rep.questions_checked, rep.must_ask, rep.restricted_looks, rep.by_owner
    );
    eprintln!(
        "approve-all load: {} tasks, median {}, p90 {}, max {}",
        load.len(),
        pct(0.5),
        pct(0.9),
        load.last().copied().unwrap_or(0)
    );
    eprintln!(
        "approve-all flow questions by step kind (steps, questions): {:?}",
        rep.by_kind
    );
    let mut heavy = rep.approve_all_load.clone();
    heavy.sort_by_key(|x| std::cmp::Reverse(x.0));
    for h in heavy.iter().take(3) {
        eprintln!("  heavy: {} questions, {}", h.0, h.1);
    }
    let mut failing = Vec::new();
    for (k, v) in &rep.mismatches {
        eprintln!(
            "MISMATCH {k}: {} events (examples from {} tasks)",
            rep.totals.get(k).copied().unwrap_or(0),
            v.len().min(12)
        );
        for m in v.iter().take(
            std::env::var("ORACLE26_SHOW")
                .ok()
                .and_then(|x| x.parse().ok())
                .unwrap_or(4),
        ) {
            eprintln!("    {m}");
        }
        if !CLASSIFIED.contains(&k.as_str()) {
            failing.push(k.clone());
        }
    }
    assert!(failing.is_empty(), "unclassified mismatches: {failing:?}");
}
