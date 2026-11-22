#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
mod common;

use std::collections::BTreeSet;

use common::*;
use jarvis_observe::{compact, find_by_fingerprint, fingerprint, fnv1a64, identity_key};
use jarvis_proto::fixtures::{login_screen, settings_screen};
use jarvis_proto::{Role, UiNode};

#[test]
fn fnv1a_matches_published_test_vectors() {
    assert_eq!(fnv1a64(b""), 0xcbf2_9ce4_8422_2325);
    assert_eq!(fnv1a64(b"a"), 0xaf63_dc4c_8601_ec8c);
    assert_eq!(fnv1a64(b"foobar"), 0x8594_4171_f739_67e8);
}

#[test]
fn fingerprint_is_pinned_so_it_cannot_drift_between_releases() {
    let fp = fingerprint(
        &Role::Button,
        &identity_key(None, "Sign in"),
        &[&Role::Application, &Role::Window],
        0,
    );
    assert_eq!(fp, PINNED_SIGN_IN);
    // The same control found by compact() on a real tree gets the same value.
    let c = compact(&login_screen());
    let form_path = [
        &Role::Application,
        &Role::Window,
        &Role::Other("other".into()),
    ];
    let by_ident = fingerprint(
        &Role::Button,
        &identity_key(Some("login.submit"), "Sign in"),
        &form_path,
        0,
    );
    assert_eq!(by_label_fp(&c, "Sign in"), by_ident);
}

const PINNED_SIGN_IN: u64 = 5_117_440_691_500_259_783;

fn by_label_fp(c: &jarvis_proto::CompactScreen, l: &str) -> u64 {
    by_label(c, l).fingerprint
}

#[test]
fn fingerprint_ignores_frame_and_index_when_the_layout_shifts() {
    let before = compact(&login_screen());

    // Same controls, but everything moved down 40pt, resized, and a new banner inserted
    // above (shifting every index).
    let mut moved = login_screen();
    for n in moved.root.children.iter_mut() {
        for w in n.children.iter_mut() {
            w.frame.y += 40.0;
            w.frame.width -= 10.0;
            for c in w.children.iter_mut() {
                c.frame.y += 40.0;
            }
        }
    }
    let banner = jarvis_proto::fixtures::text("Maintenance tonight", r(24.0, 60.0, 345.0, 20.0));
    moved.root.children[0].children.insert(0, banner);
    let after = compact(&moved);

    assert_eq!(after.elements.len(), before.elements.len() + 1);
    for e in &before.elements {
        let same = find_by_fingerprint(&after, e.fingerprint)
            .unwrap_or_else(|| panic!("{} lost its fingerprint", e.label));
        assert_eq!(same.label, e.label);
        assert_ne!(same.index, e.index, "{} should have shifted index", e.label);
    }
}

#[test]
fn identifier_beats_label_so_relabelling_keeps_identity() {
    let make = |label: &str| {
        compacted(vec![
            button(label, 20.0, 100.0, 100.0, 44.0).with_identifier("cart.checkout")
        ])
    };
    let a = make("Checkout (2)");
    let b = make("Checkout (3)");
    assert_eq!(a.elements[0].fingerprint, b.elements[0].fingerprint);

    // Without an identifier the label is the identity.
    let c = compacted(vec![button("Checkout (2)", 20.0, 100.0, 100.0, 44.0)]);
    let d = compacted(vec![button("Checkout (3)", 20.0, 100.0, 100.0, 44.0)]);
    assert_ne!(c.elements[0].fingerprint, d.elements[0].fingerprint);
}

#[test]
fn role_and_ancestor_path_are_part_of_the_identity() {
    let plain = compacted(vec![button("Done", 20.0, 100.0, 100.0, 44.0)]);
    let in_nav = compacted(vec![UiNode::new(
        Role::NavigationBar,
        r(0.0, 59.0, 393.0, 44.0),
    )
    .with_children(vec![button("Done", 20.0, 60.0, 100.0, 44.0)])]);
    let as_link = compacted(vec![jarvis_proto::fixtures::node(
        Role::Link,
        "Done",
        r(20.0, 100.0, 100.0, 44.0),
    )]);
    let fps = [
        plain.elements[0].fingerprint,
        in_nav.elements[0].fingerprint,
        as_link.elements[0].fingerprint,
    ];
    assert_eq!(fps.iter().collect::<BTreeSet<_>>().len(), 3);
}

#[test]
fn identical_siblings_get_distinct_stable_fingerprints() {
    let rows = |extra_above: bool| {
        let mut v = Vec::new();
        if extra_above {
            v.push(button("Header", 20.0, 60.0, 100.0, 30.0));
        }
        for i in 0..4 {
            v.push(button(
                "Delete",
                20.0,
                100.0 + f64::from(i) * 60.0,
                100.0,
                44.0,
            ));
        }
        compacted(v)
    };
    let a = rows(false);
    let b = rows(false);
    let fps: Vec<u64> = a.elements.iter().map(|e| e.fingerprint).collect();
    assert_eq!(
        fps.iter().collect::<BTreeSet<_>>().len(),
        4,
        "all four distinct"
    );
    // Stable across runs.
    assert_eq!(
        fps,
        b.elements.iter().map(|e| e.fingerprint).collect::<Vec<_>>()
    );
    // Stable when an unrelated element is inserted before them.
    let c = rows(true);
    let shifted: Vec<u64> = c.elements.iter().skip(1).map(|e| e.fingerprint).collect();
    assert_eq!(fps, shifted);
}

#[test]
fn identical_controls_in_different_wrappers_do_not_collide() {
    let c = compacted(vec![
        other(vec![button("Add", 20.0, 100.0, 80.0, 44.0)]),
        other(vec![button("Add", 20.0, 200.0, 80.0, 44.0)]),
    ]);
    assert_eq!(c.elements.len(), 2);
    assert_ne!(c.elements[0].fingerprint, c.elements[1].fingerprint);
}

#[test]
fn all_fingerprints_on_a_real_screen_are_unique() {
    for s in [
        settings_screen(),
        login_screen(),
        jarvis_proto::fixtures::compose_screen(),
    ] {
        let c = compact(&s);
        let set: BTreeSet<u64> = c.elements.iter().map(|e| e.fingerprint).collect();
        assert_eq!(set.len(), c.elements.len());
    }
}
