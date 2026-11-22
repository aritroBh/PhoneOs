#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Tenth review probes (end to end through compaction and the safety gate). Each asserts
//! the SAFE behaviour; a failing probe is a finding.
use jarvis_observe::compact_view;
use jarvis_proto::fixtures::{node, rect, screen, text, IPHONE_SIZE};
use jarvis_proto::{Action, Policy, Role, UiNode};
use jarvis_safety::{evaluate_with, Decision, ScreenSource, TypedText};

fn tree(view: &jarvis_observe::CompactView) -> jarvis_safety::ScreenTree<'_> {
    jarvis_safety::ScreenTree {
        sources: &view.sources,
        parents: &view.parents,
        ends: &view.ends,
        bars: &view.bars,
        nodes: &view.nodes,
        inputs: &view.inputs,
    }
}

fn find<'a>(view: &'a jarvis_observe::CompactView, label: &str) -> &'a jarvis_proto::Element {
    view.screen
        .elements
        .iter()
        .find(|e| e.label == label)
        .unwrap_or_else(|| panic!("no {label:?} in {:#?}", view.screen.elements))
}

/// A feedback form in a grouped list: the comment row is a cell holding an unlabelled text
/// view and the UITextView "placeholder" label apps draw themselves (hidden once text is
/// typed), the send row is a cell with a neutral "Continue".
fn feedback_form(value: Option<&str>) -> jarvis_proto::Screen {
    let size = IPHONE_SIZE.to_rect();
    let mut tv = UiNode::new(Role::TextView, rect(16.0, 140.0, 361.0, 160.0));
    let mut row = Vec::new();
    match value {
        Some(v) => tv = tv.with_value(v),
        None => row.push(text(
            "Tell the seller what happened",
            rect(24.0, 148.0, 300.0, 20.0),
        )),
    }
    row.insert(0, tv);
    let list = UiNode::new(Role::CollectionView, size).with_children(vec![
        UiNode::new(Role::Cell, rect(0.0, 130.0, 393.0, 180.0)).with_children(row),
        UiNode::new(Role::Cell, rect(0.0, 330.0, 393.0, 50.0)).with_children(vec![node(
            Role::Button,
            "Continue",
            rect(16.0, 333.0, 361.0, 44.0),
        )]),
    ]);
    let root = UiNode::new(Role::Application, size)
        .with_label("Shop")
        .with_children(vec![
            UiNode::new(Role::Window, size).with_children(vec![list])
        ]);
    screen("com.example.shop", "Shop", root, false)
}

/// Jarvis types into the comment view (its fingerprint taken from the screen it typed on, as
/// the session does), the self-drawn placeholder disappears, and Jarvis taps "Continue":
/// the tap must be presumed to submit what Jarvis typed and ask.
#[test]
fn probe10_typed_field_fingerprint_changes_when_its_placeholder_label_hides() {
    let before = compact_view(&feedback_form(None));
    let field = before
        .screen
        .elements
        .iter()
        .find(|e| e.role == Role::TextView)
        .expect("text view listed");
    let typed = vec![TypedText {
        field: field.fingerprint,
        text: "Item arrived broken, please refund to my card".to_owned(),
    }];
    let after = compact_view(&feedback_form(Some(
        "Item arrived broken, please refund to my card",
    )));
    let tap = find(&after, "Continue");
    let d = evaluate_with(
        &Policy::default(),
        &Action::Tap { element: tap.index },
        &after.screen,
        None,
        ScreenSource::Typed,
        Some(&tree(&after)),
        &typed,
    );
    assert!(
        matches!(d, Decision::Confirm(_)),
        "a tap after typing is not presumed to send once the field's self-drawn placeholder \
         hid (fingerprint before {:x}, after {:?}): {d:?}",
        field.fingerprint,
        after
            .screen
            .elements
            .iter()
            .filter(|e| e.role == Role::TextView)
            .map(|e| (e.label.clone(), format!("{:x}", e.fingerprint)))
            .collect::<Vec<_>>()
    );
}
