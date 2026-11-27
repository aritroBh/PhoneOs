#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Run 147 (Mac, iOS 26.5 simulator, `SystemAppAgentTests.testSafariFillsAndSubmitsAForm`):
//! the approved step "Type "Ada Lovelace" into "Full name"" tapped the field 5.6 s after the
//! page loaded, while it was still settling; the look 2.5 s after the tap showed the field
//! NOT focused and no keyboard (tree 07), and the core sent the text anyway — the runner
//! refused it ("no software keyboard and no focused element"). Now the text goes only to a
//! field a look shows focused: one settle wait and one more look, then the focusing tap once
//! more (part of the approved step: no new question), then the step fails unsent. The real
//! trees of the run (05 the page, 07 the field unfocused after the tap, 09 the e-mail field
//! focused with the keyboard up) are replayed.

mod common;

#[path = "../../jarvis-observe/tests/common/ios.rs"]
mod ios;

use common::*;
use jarvis_agent::VERIFY_SETTLE_MS;
use jarvis_memory::MemoryStore;
use jarvis_proto::{Effect, Role, RunnerCommand, Screen, UiNode};

macro_rules! fixture {
    ($name:literal) => {
        include_str!(concat!("fixtures/ios26_probes3/", $name))
    };
}

const PAGE: &str = fixture!("run147-safari-form-05-tree.txt");
const UNFOCUSED: &str = fixture!("run147-safari-form-07-tree.txt");
const EMAIL_FOCUSED: &str = fixture!("run147-safari-form-09-tree.txt");

fn compact_text(c: &jarvis_proto::CompactScreen) -> String {
    format!("{c:?}")
}

fn parse(text: &str) -> Screen {
    let typed = text
        .split("\n== raw XCUITest snapshot")
        .next()
        .unwrap_or(text);
    ios::parse_screen(typed).unwrap()
}

fn edit(n: &mut UiNode, f: &mut impl FnMut(&mut UiNode)) {
    f(n);
    for c in &mut n.children {
        edit(c, f);
    }
}

/// Tree 09 with the focus on "Full name" instead of the e-mail field (what a tap that took
/// would have shown), and with `value` typed into it.
fn name_focused(value: Option<&str>) -> Screen {
    let mut s = parse(EMAIL_FOCUSED);
    edit(&mut s.root, &mut |n| {
        if n.role == Role::TextField {
            n.focused = n.label.as_deref() == Some("Full name");
            if n.focused {
                if let Some(v) = value {
                    n.value = Some(v.into());
                }
            }
        }
    });
    s
}

/// Runs the step on the real page; `after_tap(k)` is the screen after the k-th tap (0-based).
fn typing_run(after_tap: fn(usize) -> Screen) -> Run {
    let page = parse(PAGE);
    let field = name_field(&page);
    let mut taps = 0usize;
    let mut phone = Phone::new(page, move |cmd, s: &Screen| match cmd {
        RunnerCommand::Tap { .. } => {
            taps += 1;
            Transition::Go(after_tap(taps - 1))
        }
        RunnerCommand::TypeText { text, .. } => {
            let mut t = s.clone();
            edit(&mut t.root, &mut |n| {
                if n.focused && n.role == Role::TextField {
                    n.value = Some(text.clone());
                }
            });
            Transition::Go(t)
        }
        _ => Transition::Stay,
    });
    phone.auto_focus = false;
    run(
        config(),
        "Fill in the form: Full name Ada Lovelace",
        MemoryStore::new(),
        phone,
        Script::model(&[
            &format!(
                r#"{{"action":"type","element":{field},"text":"Ada Lovelace","submit":false}}"#
            ),
            r#"{"action":"fail","reason":"stop"}"#,
        ])
        .decisions(&[true; 8]),
    )
}

/// The "Full name" text field's number (a static text with the same label comes first).
fn name_field(s: &Screen) -> u32 {
    jarvis_observe::compact(s)
        .elements
        .iter()
        .find(|e| e.role == Role::TextField && e.label == "Full name")
        .unwrap()
        .index
}

fn count(r: &Run, f: fn(&RunnerCommand) -> bool) -> usize {
    r.acts.iter().filter(|a| f(a)).count()
}

fn types(c: &RunnerCommand) -> bool {
    matches!(c, RunnerCommand::TypeText { .. })
}

fn tap(c: &RunnerCommand) -> bool {
    matches!(c, RunnerCommand::Tap { .. })
}

/// The tap misses once (tree 07, also after the settle wait), the repeated tap focuses the
/// field: the text is typed once, into it, with no question beyond the step's own.
#[test]
fn a_missed_focus_tap_is_repeated_once_and_the_text_typed_once() {
    let r = typing_run(|k| {
        if k == 0 {
            parse(UNFOCUSED)
        } else {
            name_focused(None)
        }
    });
    assert_eq!(count(&r, tap), 2, "{:?}", r.acts);
    assert_eq!(count(&r, types), 1, "{:?}", r.acts);
    let settle = r
        .effects
        .iter()
        .any(|e| matches!(e, Effect::Act(RunnerCommand::Wait { ms }) if *ms == VERIFY_SETTLE_MS));
    assert!(settle, "{:?}", r.effects);
    let u = r.last_user();
    assert!(u.contains("\"Ada Lovelace\" -> ok"), "{u}");
    // The same questions as a tap that took at once.
    let once = typing_run(|_| name_focused(None));
    assert_eq!(count(&once, tap), 1);
    assert_eq!(count(&once, types), 1);
    assert_eq!(r.confirms.len(), once.confirms.len(), "{:#?}", r.confirms);
}

/// The field never takes focus: two taps, no text sent, and the model reads why.
#[test]
fn a_field_that_never_takes_focus_gets_no_text() {
    let r = typing_run(|_| parse(UNFOCUSED));
    assert_eq!(count(&r, tap), 2, "{:?}", r.acts);
    assert_eq!(count(&r, types), 0, "{:?}", r.acts);
    let u = r.last_user();
    assert!(u.contains("the field did not take focus"), "{u}");
}

/// Another field focused after the tap (tree 09: the e-mail field, keyboard up): no text is
/// sent (it would go into the wrong field), and no second tap.
#[test]
fn another_focused_field_gets_no_text() {
    let r = typing_run(|_| parse(EMAIL_FOCUSED));
    assert_eq!(count(&r, types), 0, "{:?}", r.acts);
    assert_eq!(
        count(&r, tap),
        1,
        "{:?} {} {:#?}",
        r.acts,
        r.last_user(),
        r.confirms
    );
    let u = r.last_user();
    assert!(u.contains("the focus was not in that field"), "{u}");
}

/// A field already focused: no tap, no extra look, the text typed at once.
#[test]
fn an_already_focused_field_is_typed_into_at_once() {
    let page = name_focused(None);
    let field = name_field(&page);
    let r = run(
        config(),
        "Fill in the form: Full name Ada Lovelace",
        MemoryStore::new(),
        Phone::new(page, |cmd, s: &Screen| match cmd {
            RunnerCommand::TypeText { .. } => Transition::Go(s.clone()),
            _ => Transition::Stay,
        }),
        Script::model(&[
            &format!(
                r#"{{"action":"type","element":{field},"text":"Ada Lovelace","submit":false}}"#
            ),
            r#"{"action":"fail","reason":"stop"}"#,
        ])
        .decisions(&[true; 8]),
    );
    assert_eq!(count(&r, tap), 0, "{:?}", r.acts);
    assert_eq!(count(&r, types), 1, "{:?}", r.acts);
}
