#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::too_many_lines,
    clippy::cognitive_complexity
)]
//! The independent flow-rule checker, extended (after review pass 31): system pickers in a
//! host's tree, incomplete (cut) trees, SpringBoard modes and system surfaces, system alerts,
//! focus failures and place changes inside a step. The model is the twenty-sixth review's
//! oracle (`oracle_model/`, shared with `twentysixth_oracle.rs`, whose tasks are unchanged:
//! every new behaviour is behind `Features`, all zero there).
//!
//! EXPECTATIONS, written from `docs/ARCHITECTURE.md` (jarvis-safety; *What real trees
//! showed: probe round 3* and *runs 146 and 147*; *Tree bounds*; review passes 30 and 31;
//! *Swift shell*, top layer and system surfaces; Known gaps (12)) and
//! `docs/JARVIS-RUNNER.md` (section 3, cut trees, stale check, system surfaces) BEFORE the
//! session's decision code was read. `X#` = a document gap and the fail-closed reading taken.
//!
//! E1 Pickers. A screen of a third-party app holding a shown node, on screen, outside every
//!    web view, whose identifier is a picker's (`ContactsListView`,
//!    `photosView_content_scroll_view`, `PickerOnboardingHeaderViewIcon`,
//!    `FullDocumentManagerViewControllerNavigationBar`, `DOC.browsingModeTabBar`, prefix
//!    `DOC.browsingRoot`) adds the place `<App> (showing the contact|photo|file picker)`.
//!    a. Read: what a picker screen shows is read from the picker place AND the app.
//!    b. Every input on the screen (a look, a wait, a scroll, Cancel, any tap, typing) goes
//!       into the picker place AND the app; the app stays on the screen's places. With two
//!       own destinations a full input keeps this action only (rule 15 of the base model).
//!    c. The picker is its own source: a grant given on the app's own screen never covers
//!       it, and a grant out of a picker screen names both.
//!    d. Not modelled: stickiness (the generator closes a picker with its identifier and
//!       its backdrop together, so no sticky look arises), pickers in browsers.
//!    X1 Which picker kind each identifier names is not listed; read: Contacts → contact,
//!       the two `photos…`/`PickerOnboarding…` → photo, the document ones → file.
//! E2 Incomplete screens (`Screen::tree_cut`, or a shown node deeper than
//!    `jarvis_proto::MAX_TREE_DEPTH` = 160 levels).
//!    a. Every action that touches the screen (tap, `tap_point`, long press, swipe, scroll in
//!       any direction, typing, clear, Return) and `open_app` / `open_url` asks first, phone
//!       only, whatever the policy relaxes (also under `confirm: []`); the question says
//!       "this screen could not be read completely".
//!    b. `press_home`, `wait`, `look`, `done`, `fail`, `ask_user` are as on any screen:
//!       press_home is never raised by the incomplete-screen rules (no such question for
//!       it) and it is sent once Rule T allows it ("Home is the way out").
//!    c. Places are those of the same tree (the cut changes no place).
//!    X2 The document does not say whether a scroll on an incomplete screen is of the
//!       scroll kind for Rule T. Read (fail closed): of the full kind (its drag may cross
//!       an unread picker or slider, the reason the scroll asks).
//! E3 SpringBoard modes (edit mode `DeleteButton`, App Library `dewey-pod-view`, open
//!    folder `…, Folder name`, on SpringBoard's own nodes) and system surfaces over an app
//!    (Control Centre, Notification Centre, App Switcher: the runner reports
//!    `unknown.front.app` with the application node only).
//!    a. Both are the unidentified layer: a screen Jarvis cannot read, named "an app Jarvis
//!       cannot identify" (as a thin tree, `a screen in <App> that Jarvis cannot read (it
//!       may show another party's page)` with that app name).
//!    b. Every input on it goes to that one place; waits, looks and leaving (Home) are seen
//!       by it; like an undecidable link every place read is a source (itself included);
//!       nothing is kept beyond the action, at any kind.
//!    c. Typing (and every key but Home) is refused: no text or key is sent there.
//!    d. Home works (gated by Rule T only).
//!    e. Read: by the base rule 8 (a node with a label, value or control role other than
//!       the application and window nodes) — the runner's application-node-only reading is
//!       not a read; a SpringBoard mode's tree (icons, names) is.
//!    X3 Whether a SpringBoard mode's (or a surface's) content is a read is not stated.
//!       Read (fail closed): the base rule 8, as above.
//! E4 System alerts (SpringBoard's tree holding a raw `Alert`, the top layer over an app).
//!    a. The place "a system alert": read when shown (a source like any place read), never
//!       an observer; a full input into it (pressing a button) keeps Rule T's rules
//!       (one decidable destination: a full grant).
//!    b. R4: "Allow" asks (phone only) unless the policy relaxes everything.
//!    c. A command planned on the app while the alert is up is refused by the runner
//!       (stale; the phone here enforces `expect` as the runner does) and the step is
//!       decided again.
//!    X4 Whether the app under the alert observes inputs into the alert is not stated, and
//!       the core is not told which app it is (an `Observation` carries no notes). Read:
//!       not modelled (the alert screen's only place is the alert).
//! E5 Focus failures (run 147, `Session::after_focus_tap`).
//!    a. On a typed tree the text goes only to a field the look shows focused (the step's
//!       target field, by its label).
//!    b. With no input focused: a settle wait, one more look, then the focusing tap once
//!       more (no new question: at most two focusing taps per step), only while the field
//!       is the same control in the same app, no system alert is up; still unfocused → the
//!       step fails unsent.
//!    X5 Whether the focus retry's settle look goes through the R24 gate is not stated (it
//!       is stated for the verify-settle look). Read (fail closed): yes, as E6b.
//!       (Triage: not a gap. The R24 paragraph of threat model (3) lists "a typing's focus
//!       settle look" among the looks refused in the restricted state; it had not been read
//!       when E5 was written.)
//! E6 Place changes inside a step.
//!    a. `same_places` (review pass 30, M3): text, Deletes and Return of a step go only
//!       after a look on which the target field's places (app in front, places under the
//!       field, picker, top layer) equal those of the screen the step was decided on;
//!       otherwise nothing more of the step is sent.
//!    b. Verify settle (`VERIFY_SETTLE_MS` = 800): after a step whose look shows no change,
//!       one wait and one more look, chosen by the session, through the R24 gate: no such
//!       look while some place on the current screen may not see a look. At most one per
//!       step; never for a scroll, a swipe or typing. What that look shows is read at the
//!       next prompt like any look (a place that arrives there is a source from then on).
//!
//! TRIAGE (after the first runs; the session's decision code was read only for this, and
//! nothing of it is copied: each fix below cites the document).
//!  T1 (a) E3 misread: the unidentified layer is two places, not one — as on any screen
//!     Jarvis cannot read ("what is read there is unchanged (the app's place and the
//!     screen picture)"): the app's own place, here named "an app Jarvis cannot identify"
//!     (on the screen, read on every such look, an observer), and "a screen in an app
//!     Jarvis cannot identify that Jarvis cannot read (it may show another party's page)",
//!     into which every input goes (a source for itself; nothing kept). The first model (one
//!     place, read by rule 8) gave 457 `3-source-not-read` in 300 tasks.
//!  T2 (a) Not modelled by the base oracle (its trees hold no rows in scrollers): probe
//!     round 3 (a), its review's M3 and pass 28's M2 — a sideways scroll whose drag touches
//!     a raw `Cell` (or a node a cell holds) or a drawn node of row height spanning 0.8 of
//!     its nearest scroller is a full input. The real alert tree has such rows. Added
//!     (`crosses_row`); `twentysixth_oracle.rs`'s output is unchanged by it.
//!  T3 (a) An approved model `look` takes the look after the approval and then the look
//!     itself ("a model look the owner approved is taken"); the base check allowed one.
//!  T4 (a) The incomplete-screen warning is also a note on flow questions ("Warning: this
//!     screen could not be read completely …" before a Home or wait question): only a
//!     question asked for it alone is a raise by the incomplete-screen rules.
//!  T5 (a) The Return of a `type … submit` is gated again on the fresh screen as its own
//!     action (review pass 30, M3); E6a's comparison applies to the text and the Deletes.
//!  X2 (b) Measured: with the fail-closed reading (`ORACLE_EXT_X2=full`) the session keeps
//!     a scroll's own kind on an incomplete screen (an approved scroll keeps scroll grants),
//!     which surfaces as `1-unasked-flow`, `3-pair-not-named`, `2b-…`, `S-…` and others, all
//!     on incomplete screens. Every input there still asks the incomplete-screen question,
//!     so no scroll goes unasked. The default oracle takes the session's reading so that the
//!     other checks stay sharp.
//!  X6 (b) "Of row height" is not given a number. Read: at most `ROW_MAX_HEIGHT` = 200 pt.
//!  X7 (b) Whether a look the session chooses inside an approved action — the vision
//!     fallback's screenshot after the action's own look, a verify or focus settle look —
//!     is refused when the restriction holds only through pairs the action's own approval
//!     allowed "for this action only" (an undecidable or multi-destination input, the
//!     unidentified layer). The session counts that approval; the fail-closed reading does
//!     not. Checks `X7-look-inside-an-approved-action`,
//!     `X7-settle-look-inside-an-approved-action` (the base `2a-…` and `S-…` checks stay for
//!     a look restricted beyond the action's approval).
//!
//! Self-test: `ORACLE26_MUTATE=4` (no picker place) and `5` (the unidentified layer keeps
//! approvals like any place) must make this fuzzer report mismatches.
//!
//! Every mismatch is classified in `CLASSIFIED` below: (a) oracle wrong — fixed in the
//! oracle and said what was misread; (b) document gap; (c) the session stricter than the
//! document; (d) the session weaker than the document — a finding, with a failing test in
//! `oracle_ext_findings.rs`.

mod oracle_model;

use oracle_model::*;

/// Every mismatch class seen in the long runs, with its class: (a) oracle wrong (fixed in
/// the oracle; listed only while a note is useful), (b) document gap, (c) the session
/// stricter than the document, (d) the session weaker than the document: a finding, with a
/// failing test in `oracle_ext_findings.rs`. Any other mismatch fails this test.
const CLASSIFIED: &[(&str, &str, &str)] = &[
    (
        "b",
        "G2-decline-over-earlier-grant",
        "as in twentysixth_oracle.rs: a decline blocks only pairs not already allowed (ruling D1)",
    ),
    (
        "b",
        "gap-mid-step-new-place",
        "as in twentysixth_oracle.rs: a place arriving during a step's own looks (Known gaps (12), ruling D2)",
    ),
    (
        "b",
        "X7-look-inside-an-approved-action",
        "X7: the vision fallback's screenshot after an approved action's own look, restricted only through that action's approval",
    ),
    (
        "b",
        "X7-settle-look-inside-an-approved-action",
        "X7: a verify or focus settle look inside an approved action, restricted only through that action's approval",
    ),
];

fn env<T: std::str::FromStr>(k: &str, d: T) -> T {
    std::env::var(k)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(d)
}

fn pct(v: &[usize], q: f64) -> usize {
    let mut v = v.to_vec();
    v.sort_unstable();
    if v.is_empty() {
        0
    } else {
        v[((v.len() - 1) as f64 * q).round() as usize]
    }
}

type Load = (usize, usize, bool);

#[test]
fn fuzz_the_flow_rule_with_pickers_cut_trees_system_layers_alerts_focus_and_place_changes() {
    let n: u64 = env("ORACLE_EXT_CASES", 120);
    let s0: u64 = env("ORACLE_EXT_SEED", 60_000);
    let one = std::env::var("ORACLE_EXT_ONE").is_ok();
    let show: usize = env("ORACLE_EXT_SHOW", 3);
    let mut rep = Report::default();
    let started = std::time::Instant::now();
    for i in 0..n {
        let t = Task::random_ext(s0 + i);
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
                rep.flag(
                    "6-panic",
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
    eprintln!(
        "oracle-ext: {} tasks, {} model steps, {} sent, {} flow questions ({} checked, {} declined), {} must-ask steps, {} sends checked, {} looks in the restricted state, statuses {:?}, {:.1}s",
        rep.tasks,
        rep.steps,
        rep.sent,
        rep.flow_questions,
        rep.questions_checked,
        rep.declined,
        rep.must_ask,
        rep.sends_checked,
        rep.restricted_looks,
        rep.statuses,
        started.elapsed().as_secs_f64()
    );
    eprintln!("tasks with each feature: {:?}", rep.feature_tasks);
    eprintln!("counts: {:?}", rep.ext_counts);
    let split = |want: Option<bool>| {
        let of = |x: &&Load| want.is_none_or(|w| x.2 == w);
        let flows: Vec<usize> = rep.ext_loads.iter().filter(of).map(|x| x.0).collect();
        let all: Vec<usize> = rep.ext_loads.iter().filter(of).map(|x| x.1).collect();
        (
            flows.len(),
            pct(&flows, 0.5),
            pct(&flows, 0.9),
            pct(&all, 0.5),
            pct(&all, 0.9),
        )
    };
    for (name, want) in [
        ("all", None),
        ("with a new feature", Some(true)),
        ("without", Some(false)),
    ] {
        let (k, m, p, am, ap) = split(want);
        eprintln!(
            "approve-all load ({name}): {k} tasks; flow questions median {m}, p90 {p}; all questions median {am}, p90 {ap}"
        );
    }
    let mut failing = Vec::new();
    for (k, v) in &rep.mismatches {
        let class = CLASSIFIED
            .iter()
            .find(|c| c.1 == k.as_str())
            .map_or("UNCLASSIFIED", |c| c.0);
        eprintln!(
            "MISMATCH [{class}] {k}: {} events (examples from {} tasks)",
            rep.totals.get(k).copied().unwrap_or(0),
            v.len().min(12)
        );
        for m in v.iter().take(show) {
            eprintln!("    {}", m.chars().take(3000).collect::<String>());
        }
        if class == "UNCLASSIFIED" {
            failing.push(k.clone());
        }
    }
    assert!(failing.is_empty(), "unclassified mismatches: {failing:?}");
}
