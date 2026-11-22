#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
mod common;

use common::*;
use jarvis_observe::compact;
use jarvis_proto::fixtures::{compose_screen, login_screen, node, rect, settings_screen, text};
use jarvis_proto::{Role, UiNode};

fn roles_and_labels(c: &jarvis_proto::CompactScreen) -> Vec<(String, String)> {
    c.elements
        .iter()
        .map(|e| (e.role.to_string(), e.label.clone()))
        .collect()
}

#[test]
fn settings_screen_becomes_a_short_ordered_list() {
    let c = compact(&settings_screen());
    assert_eq!(c.app_bundle_id, "com.apple.Preferences");
    assert!(!c.keyboard_visible);
    assert_eq!(
        roles_and_labels(&c),
        vec![
            ("static_text".into(), "Settings".into()),
            ("search_field".into(), "Search".into()),
            ("switch".into(), "Airplane Mode".into()),
            ("cell".into(), "Wi-Fi, HomeNet".into()),
            ("cell".into(), "Bluetooth, On".into()),
            ("cell".into(), "Notifications".into()),
            ("cell".into(), "General".into()),
        ]
    );
    let indices: Vec<u32> = c.elements.iter().map(|e| e.index).collect();
    assert_eq!(indices, (1..=7).collect::<Vec<u32>>());
}

#[test]
fn cell_label_is_merged_from_child_texts_and_children_are_not_listed_again() {
    let c = compact(&settings_screen());
    let wifi = by_label(&c, "Wi-Fi, HomeNet");
    assert_eq!(wifi.role, Role::Cell);
    assert_eq!(count_label(&c, "Wi-Fi"), 0);
    assert_eq!(count_label(&c, "HomeNet"), 0);
    // The switch row lists the control once, not the row plus the control plus its text.
    assert_eq!(count_label(&c, "Airplane Mode"), 1);
    assert_eq!(by_label(&c, "Airplane Mode").value.as_deref(), Some("0"));
}

#[test]
fn cell_own_label_wins_and_texts_in_reading_order_otherwise() {
    let own = UiNode::new(Role::Cell, r(0.0, 100.0, 393.0, 60.0))
        .with_label("Mail from Sam")
        .with_children(vec![
            label("Sam", 16.0, 110.0),
            label("Lunch?", 16.0, 130.0),
        ]);
    // Children given out of order: the merged label must follow visual order.
    let merged = UiNode::new(Role::Cell, r(0.0, 200.0, 393.0, 60.0)).with_children(vec![
        label("9:41", 300.0, 212.0),
        label("Lunch?", 16.0, 235.0),
        label("Sam", 16.0, 210.0),
    ]);
    let c = compacted(vec![own, merged]);
    assert_eq!(labels(&c), vec!["Mail from Sam", "Sam, 9:41, Lunch?"]);
}

#[test]
fn cell_with_one_unlabeled_button_lends_its_text_to_the_button() {
    let row = UiNode::new(Role::Cell, r(0.0, 100.0, 393.0, 50.0)).with_children(vec![
        label("Alex Kim", 16.0, 112.0),
        UiNode::new(Role::Button, r(330.0, 105.0, 40.0, 40.0)).hittable(true),
    ]);
    let c = compacted(vec![row]);
    assert_eq!(
        roles_and_labels(&c),
        vec![("button".into(), "Alex Kim".into())]
    );
}

#[test]
fn button_takes_label_from_its_text_child() {
    let b = UiNode::new(Role::Button, r(20.0, 300.0, 200.0, 44.0))
        .hittable(true)
        .with_children(vec![label("Continue", 60.0, 312.0)]);
    let c = compacted(vec![b]);
    assert_eq!(
        roles_and_labels(&c),
        vec![("button".into(), "Continue".into())]
    );
}

#[test]
fn nested_other_containers_are_flattened() {
    let deep = other(vec![other(vec![other(vec![button(
        "Deep", 20.0, 200.0, 100.0, 44.0,
    )])])]);
    let c = compacted(vec![deep, other(vec![other(vec![])])]);
    assert_eq!(labels(&c), vec!["Deep"]);
}

#[test]
fn login_screen_keeps_disabled_button_and_hides_keyboard_keys() {
    let c = compact(&login_screen());
    assert!(c.keyboard_visible);
    assert_eq!(
        labels(&c),
        vec![
            "Welcome back",
            "Email",
            "Password",
            "Sign in",
            "Forgot password?"
        ]
    );
    let sign_in = by_label(&c, "Sign in");
    assert!(!sign_in.enabled);
    assert!(by_label(&c, "Email").enabled);
    // No keyboard keys, return key or shift button.
    for l in ["q", "w", "e", "space", "delete", "shift", "go"] {
        assert_eq!(count_label(&c, l), 0, "{l} leaked from the keyboard");
    }
}

#[test]
fn secure_fields_never_carry_a_value() {
    let pw = UiNode::new(Role::SecureTextField, r(20.0, 200.0, 300.0, 44.0))
        .with_label("Password")
        .with_value("hunter2")
        .hittable(true);
    // A plain text field the runner flagged secure (e.g. a revealed-toggle field).
    let flagged = UiNode::new(Role::TextField, r(20.0, 260.0, 300.0, 44.0))
        .with_label("PIN")
        .with_value("1234")
        .secure(true);
    let c = compacted(vec![pw, flagged]);
    for e in &c.elements {
        assert!(e.secure, "{} should be secure", e.label);
        assert_eq!(e.value, None, "{} leaked a value", e.label);
    }
}

#[test]
fn placeholder_echoed_as_value_is_dropped_but_real_text_stays() {
    let empty = UiNode::new(Role::TextField, r(20.0, 200.0, 300.0, 44.0))
        .with_label("Email")
        .with_placeholder("you@example.com")
        .with_value("you@example.com");
    let filled = UiNode::new(Role::TextField, r(20.0, 260.0, 300.0, 44.0))
        .with_label("Name")
        .with_placeholder("Full name")
        .with_value("Alex");
    let c = compacted(vec![empty, filled]);
    assert_eq!(by_label(&c, "Email").value, None);
    assert_eq!(
        by_label(&c, "Email").placeholder.as_deref(),
        Some("you@example.com")
    );
    assert_eq!(by_label(&c, "Name").value.as_deref(), Some("Alex"));
}

#[test]
fn alert_on_top_isolates_its_subtree() {
    let content = vec![
        button("Delete account", 20.0, 300.0, 300.0, 44.0),
        label("Profile", 20.0, 200.0),
    ];
    let alert = UiNode::new(Role::Alert, r(60.0, 330.0, 273.0, 190.0)).with_children(vec![
        label("Delete account?", 80.0, 350.0),
        label("This cannot be undone.", 80.0, 380.0),
        button("Cancel", 70.0, 450.0, 120.0, 44.0),
        button("Delete", 200.0, 450.0, 120.0, 44.0),
    ]);
    let mut children = content.clone();
    children.push(alert);
    let c = compacted(children);
    assert_eq!(
        labels(&c),
        vec![
            "Delete account?",
            "This cannot be undone.",
            "Cancel",
            "Delete"
        ]
    );
    assert_eq!(c.elements[0].index, 1);

    // Without the alert the content is back.
    let c = compacted(content);
    assert_eq!(labels(&c), vec!["Profile", "Delete account"]);
}

#[test]
fn action_sheet_and_hidden_alert() {
    let sheet = UiNode::new(Role::Sheet, r(8.0, 600.0, 377.0, 240.0)).with_children(vec![button(
        "Take Photo",
        16.0,
        610.0,
        360.0,
        56.0,
    )]);
    let hidden_alert = UiNode::new(Role::Alert, r(60.0, 300.0, 273.0, 190.0))
        .visible(false)
        .with_children(vec![button("Ghost", 70.0, 450.0, 120.0, 44.0)]);
    let c = compacted(vec![
        button("Behind", 20.0, 100.0, 100.0, 44.0),
        hidden_alert,
        sheet,
    ]);
    assert_eq!(labels(&c), vec!["Take Photo"]);
}

#[test]
fn keyboard_flag_comes_from_the_node_even_if_the_screen_flag_is_false() {
    let mut s = app_screen_with(vec![button("Send", 300.0, 100.0, 60.0, 44.0), kb()], false);
    s.keyboard_visible = false;
    let c = compact(&s);
    assert!(c.keyboard_visible);
    assert_eq!(labels(&c), vec!["Send"]);
}

#[test]
fn elements_under_the_keyboard_are_dropped_and_straddlers_clipped() {
    // Keyboard fixture occupies y 516..852.
    let children = vec![
        button("Above", 20.0, 400.0, 100.0, 44.0),
        button("Straddling", 20.0, 490.0, 200.0, 60.0),
        button("Covered", 20.0, 600.0, 100.0, 44.0),
        kb(),
    ];
    let c = compact(&app_screen_with(children, true));
    assert_eq!(labels(&c), vec!["Above", "Straddling"]);
    let s = by_label(&c, "Straddling");
    assert_eq!(s.frame.y, 490.0);
    assert_eq!(s.frame.height, 26.0);
    assert!(s.frame.max_y() <= 516.0);
}

#[test]
fn off_screen_and_hidden_and_zero_area_nodes_are_dropped() {
    let hidden = other(vec![button("Hidden child", 20.0, 100.0, 100.0, 44.0)]).visible(false);
    let children = vec![
        button("Fully below", 20.0, 900.0, 100.0, 44.0),
        button("Fully right", 500.0, 100.0, 100.0, 44.0),
        button("Left of screen", -200.0, 100.0, 100.0, 44.0),
        button("Peeking 6pt", 20.0, 846.0, 100.0, 44.0),
        button("Half visible", 20.0, 830.0, 100.0, 44.0),
        button("Zero height", 20.0, 100.0, 100.0, 0.0),
        button("On screen", 20.0, 100.0, 100.0, 44.0),
        hidden,
    ];
    let c = compacted(children);
    assert_eq!(labels(&c), vec!["On screen", "Half visible"]);
    let half = by_label(&c, "Half visible");
    assert_eq!(half.frame.height, 22.0, "frame is clipped to the screen");
}

#[test]
fn tall_text_mostly_off_screen_is_still_listed() {
    let article = UiNode::new(Role::StaticText, r(16.0, 100.0, 361.0, 3000.0))
        .with_label("A very long article");
    let c = compacted(vec![article]);
    assert_eq!(labels(&c), vec!["A very long article"]);
    assert_eq!(c.elements[0].frame.height, 752.0);
}

#[test]
fn same_row_elements_are_ordered_left_to_right_despite_jitter() {
    // Navigation row with 1-4pt jitter in y and different heights, given scrambled.
    let children = vec![
        button("Third", 300.0, 98.0, 60.0, 44.0),
        button("Row two", 20.0, 160.0, 100.0, 44.0),
        button("First", 10.0, 102.0, 60.0, 36.0),
        button("Second", 150.0, 100.0, 80.0, 40.0),
        button("Row three", 20.0, 230.0, 100.0, 44.0),
        button("Row two right", 200.0, 157.0, 100.0, 44.0),
    ];
    let c = compacted(children);
    assert_eq!(
        labels(&c),
        vec![
            "First",
            "Second",
            "Third",
            "Row two",
            "Row two right",
            "Row three"
        ]
    );
}

#[test]
fn compose_screen_navigation_row_reads_left_to_right() {
    let c = compact(&compose_screen());
    assert_eq!(
        labels(&c),
        vec![
            "Messages",
            "Alex",
            "FaceTime",
            "Dinner tonight?",
            "Sure, what time?",
            "Apps",
            "Message",
            "Send"
        ]
    );
    assert_eq!(
        by_label(&c, "Message").value.as_deref(),
        Some("See you at 7")
    );
    assert!(by_label(&c, "Message").focused);
}

#[test]
fn tab_bar_and_navigation_bar_items_are_listed_with_selection() {
    let nav = UiNode::new(Role::NavigationBar, r(0.0, 59.0, 393.0, 44.0)).with_children(vec![
        button("Back", 8.0, 60.0, 60.0, 44.0),
        text("Inbox", r(150.0, 66.0, 93.0, 30.0)),
        button("Edit", 330.0, 60.0, 50.0, 44.0),
    ]);
    let tabs = UiNode::new(Role::TabBar, r(0.0, 769.0, 393.0, 83.0)).with_children(vec![
        UiNode::new(Role::Tab, r(0.0, 770.0, 98.0, 49.0))
            .with_label("Mail")
            .selected(true),
        UiNode::new(Role::Tab, r(98.0, 770.0, 98.0, 49.0)).with_label("Search"),
    ]);
    let c = compacted(vec![nav, tabs]);
    assert_eq!(labels(&c), vec!["Back", "Inbox", "Edit", "Mail", "Search"]);
    assert!(by_label(&c, "Mail").selected);
    assert!(!by_label(&c, "Search").selected);
}

#[test]
fn images_need_a_label_and_unlabeled_buttons_fall_back_to_identifier() {
    let children = vec![
        UiNode::new(Role::Image, r(20.0, 100.0, 50.0, 50.0)),
        UiNode::new(Role::Image, r(20.0, 160.0, 50.0, 50.0)).with_label("Profile photo"),
        UiNode::new(Role::Button, r(20.0, 220.0, 44.0, 44.0))
            .with_identifier("share-icon")
            .hittable(true),
        UiNode::new(Role::Other("other".into()), r(20.0, 280.0, 100.0, 30.0)).with_label("Badge 3"),
    ];
    let c = compacted(children);
    assert_eq!(labels(&c), vec!["Profile photo", "share-icon", "Badge 3"]);
}

#[test]
fn exact_duplicates_collapse_but_same_label_elsewhere_stays() {
    let children = vec![
        button("Buy", 20.0, 100.0, 100.0, 44.0),
        button("Buy", 20.0, 100.0, 100.0, 44.0),
        button("Buy", 20.0, 300.0, 100.0, 44.0),
        label("Price", 200.0, 100.0),
        UiNode::new(Role::StaticText, r(200.0, 100.0, 120.0, 20.0)).with_label("Price"),
    ];
    let c = compacted(children);
    assert_eq!(labels(&c), vec!["Buy", "Price", "Buy"]);
}

#[test]
fn text_repeating_a_neighbouring_controls_label_is_dropped() {
    let row = other(vec![
        label("Wi-Fi", 16.0, 100.0),
        UiNode::new(Role::Switch, r(310.0, 96.0, 51.0, 31.0))
            .with_label("Wi-Fi")
            .with_value("1")
            .hittable(true),
    ]);
    let c = compacted(vec![row, label("Wi-Fi", 16.0, 400.0)]);
    // The text next to the switch is redundant; the unrelated text far away stays.
    assert_eq!(
        c.elements
            .iter()
            .filter(|e| e.role == Role::StaticText)
            .count(),
        1
    );
    assert_eq!(
        c.elements.iter().filter(|e| e.role == Role::Switch).count(),
        1
    );
}

#[test]
fn labels_are_whitespace_normalised() {
    let c = compacted(vec![node(
        Role::Button,
        "  Sign\n in\t now  ",
        rect(20.0, 100.0, 100.0, 44.0),
    )]);
    assert_eq!(labels(&c), vec!["Sign in now"]);
}

#[test]
fn large_tree_compacts_quickly_with_contiguous_indices() {
    let rows: Vec<UiNode> = (0..500)
        .map(|i| {
            let y = f64::from(i) * 4.0;
            UiNode::new(Role::Cell, r(0.0, y, 393.0, 44.0)).with_children(vec![
                label(&format!("Row {i}"), 16.0, y + 4.0),
                label(&format!("Detail {i}"), 200.0, y + 4.0),
                UiNode::new(Role::Image, r(300.0, y, 20.0, 20.0)),
            ])
        })
        .collect();
    let started = std::time::Instant::now();
    let c = compacted(vec![
        UiNode::new(Role::Table, r(0.0, 0.0, 393.0, 852.0)).with_children(rows)
    ]);
    assert!(started.elapsed().as_secs() < 2);
    assert!(!c.elements.is_empty());
    for (i, e) in c.elements.iter().enumerate() {
        assert_eq!(e.index as usize, i + 1);
    }
}

#[test]
fn pathologically_deep_tree_does_not_overflow() {
    let mut n = button("Bottom", 20.0, 100.0, 100.0, 44.0);
    for _ in 0..5000 {
        n = other(vec![n]);
    }
    // Cut off beyond the depth limit instead of recursing forever.
    let c = compacted(vec![n]);
    assert!(c.elements.is_empty());
}

#[test]
fn hittable_other_wrapper_never_merges_its_children() {
    // The runner-text parser marks every node hittable; that must not turn a plain
    // wrapper into a cell.
    let mut texts: Vec<UiNode> = (0..10)
        .map(|i| label(&format!("P{i}"), 16.0, 100.0 + f64::from(i) * 30.0))
        .collect();
    texts.insert(0, label("Headline", 16.0, 60.0));
    texts.push(UiNode::new(Role::Button, r(300.0, 60.0, 44.0, 44.0)).hittable(true));
    let wrapper = other(texts).hittable(true);
    let c = compacted(vec![wrapper]);
    assert_eq!(c.elements.len(), 12, "{:?}", labels(&c));
    for i in 0..10 {
        assert_eq!(count_label(&c, &format!("P{i}")), 1);
    }
    assert_eq!(count_label(&c, "Headline"), 1);
    let icon = c.elements.iter().find(|e| e.role == Role::Button).unwrap();
    assert_eq!(icon.label, "", "icon button must not borrow sibling texts");
}

#[test]
fn cell_with_many_texts_marks_truncation_instead_of_dropping_silently() {
    let kids: Vec<UiNode> = (0..20)
        .map(|i| label(&format!("T{i}"), 4.0 + f64::from(i) * 18.0, 110.0))
        .collect();
    let c = compacted(vec![
        UiNode::new(Role::Cell, r(0.0, 100.0, 393.0, 44.0)).with_children(kids)
    ]);
    assert_eq!(c.elements.len(), 1);
    assert!(
        c.elements[0].label.ends_with(", …"),
        "{}",
        c.elements[0].label
    );
    assert!(c.elements[0].label.starts_with("T0, T1"));
}

#[test]
fn fifty_thousand_nodes_nested_deeply_compact_in_linear_time() {
    // 150 nested wrappers (hittable, like parsed runner text) around 50k texts.
    let leaves: Vec<UiNode> = (0..50_000)
        .map(|i| label(&format!("T{i}"), 4.0, f64::from(i % 800)))
        .collect();
    let mut n = other(leaves).hittable(true);
    for _ in 0..150 {
        n = other(vec![n]).hittable(true);
    }
    let started = std::time::Instant::now();
    let c = compacted(vec![n]);
    let took = started.elapsed();
    assert!(took.as_secs() < 10, "took {took:?}");
    assert!(c.elements.len() > 10_000, "got {}", c.elements.len());
    assert_eq!(c.elements.first().map(|e| e.index), Some(1));
}

// ---- hidden flag: container versus leaf -----------------------------------------------------

fn zero_hidden(role: Role, children: Vec<UiNode>) -> UiNode {
    UiNode::new(role, r(0.0, 0.0, 0.0, 0.0))
        .visible(false)
        .with_children(children)
}

#[test]
fn hidden_zero_size_container_does_not_hide_its_visible_children() {
    let page = zero_hidden(
        Role::Button,
        vec![
            button("Settings", 310.0, 418.0, 72.0, 95.0),
            // A child that is itself hidden stays out, whatever its frame.
            button("Flagged", 20.0, 418.0, 72.0, 95.0).visible(false),
            // So does one that is off screen by its own frame.
            button("Next page", 447.0, 92.0, 72.0, 95.0).visible(false),
            button("Zero", 0.0, 0.0, 0.0, 0.0).visible(false),
        ],
    );
    let c = compacted(vec![page]);
    assert_eq!(labels(&c), vec!["Settings"]);
    assert_eq!(c.elements[0].frame, r(310.0, 418.0, 72.0, 95.0));
}

#[test]
fn zero_size_wrappers_of_any_role_are_transparent_even_if_not_flagged() {
    let cell = UiNode::new(Role::Cell, r(0.0, 0.0, 0.0, 0.0)).with_children(vec![button(
        "Inside cell",
        20.0,
        100.0,
        100.0,
        44.0,
    )]);
    let nested = zero_hidden(
        Role::Other("Other".into()),
        vec![zero_hidden(
            Role::Button,
            vec![button("Deep", 20.0, 200.0, 100.0, 44.0)],
        )],
    );
    let c = compacted(vec![cell, nested]);
    assert_eq!(labels(&c), vec!["Inside cell", "Deep"]);
}

#[test]
fn hidden_container_with_a_real_frame_hides_its_whole_subtree() {
    // An off-screen page or a hidden alert: what it holds cannot be reached, even where a
    // child's own flag says visible and its frame pokes onto the screen.
    let page = other(vec![button("Peeking", 430.0, 100.0, 60.0, 44.0)]).visible(false);
    let off = UiNode::new(Role::Other("Other".into()), r(447.0, 0.0, 440.0, 956.0))
        .visible(false)
        .with_children(vec![button("Fitness", 447.0, 92.0, 72.0, 95.0)]);
    let scroll = UiNode::new(Role::ScrollView, r(-465.0, 0.0, 440.0, 956.0))
        .visible(false)
        .with_children(vec![button("Left of home", 10.0, 100.0, 100.0, 44.0)]);
    let c = compacted(vec![
        page,
        off,
        scroll,
        button("Here", 20.0, 300.0, 100.0, 44.0),
    ]);
    assert_eq!(labels(&c), vec!["Here"]);
}

#[test]
fn hidden_leaves_are_dropped_even_inside_a_transparent_parent() {
    let page = zero_hidden(
        Role::Other("Other".into()),
        vec![
            label("Gone", 20.0, 100.0).visible(false),
            label("Kept", 20.0, 140.0),
        ],
    );
    let c = compacted(vec![page]);
    assert_eq!(labels(&c), vec!["Kept"]);
}

// ---- nested same-role controls --------------------------------------------------------------

fn switch_row(inner: Vec<UiNode>) -> UiNode {
    UiNode::new(Role::Switch, r(16.0, 168.0, 361.0, 58.0))
        .with_label("Show tips")
        .with_value("0")
        .with_identifier("toggles.tips")
        .with_children(inner)
}

fn inner_switch() -> UiNode {
    UiNode::new(Role::Switch, r(300.0, 183.0, 63.0, 28.0)).with_value("0")
}

#[test]
fn switch_row_takes_the_inner_switch_frame_and_keeps_label_and_value() {
    let c = compacted(vec![switch_row(vec![inner_switch()])]);
    assert_eq!(c.elements.len(), 1);
    let e = &c.elements[0];
    assert_eq!(
        (e.label.as_str(), e.value.as_deref()),
        ("Show tips", Some("0"))
    );
    assert_eq!(e.frame, r(300.0, 183.0, 63.0, 28.0));
    assert_eq!(e.frame.center(), jarvis_proto::Point::new(331.5, 197.0));
}

#[test]
fn nesting_is_found_through_wrappers_and_followed_down_a_chain() {
    let innermost = UiNode::new(Role::Switch, r(310.0, 185.0, 40.0, 24.0));
    let middle = UiNode::new(Role::Switch, r(300.0, 183.0, 63.0, 28.0))
        .with_children(vec![other(vec![innermost])]);
    let c = compacted(vec![switch_row(vec![other(vec![middle])])]);
    assert_eq!(c.elements.len(), 1);
    assert_eq!(c.elements[0].frame, r(310.0, 185.0, 40.0, 24.0));
    assert_eq!(c.elements[0].label, "Show tips");
}

#[test]
fn inner_state_fills_in_what_the_wrapper_lacks() {
    let mut inner = inner_switch();
    inner.value = Some("1".into());
    let mut outer = switch_row(vec![inner]);
    outer.value = None;
    let c = compacted(vec![outer]);
    assert_eq!(c.elements[0].value.as_deref(), Some("1"));

    let wrapper = UiNode::new(Role::Switch, r(16.0, 168.0, 361.0, 58.0))
        .with_children(vec![inner_switch().with_label("Airplane Mode")]);
    let c = compacted(vec![wrapper]);
    assert_eq!(c.elements[0].label, "Airplane Mode");

    let mut disabled = inner_switch();
    disabled.enabled = false;
    let c = compacted(vec![switch_row(vec![disabled])]);
    assert!(!c.elements[0].enabled);
}

#[test]
fn ambiguous_or_foreign_nested_controls_keep_the_wrapper_frame() {
    let row = r(16.0, 168.0, 361.0, 58.0);
    // Two inner switches: no way to pick one.
    let two = switch_row(vec![
        inner_switch(),
        UiNode::new(Role::Switch, r(200.0, 183.0, 63.0, 28.0)).with_value("0"),
    ]);
    // Sticking out of the wrapper.
    let outside = switch_row(vec![UiNode::new(Role::Switch, r(340.0, 183.0, 63.0, 28.0))]);
    // Hidden or without area.
    let hidden = switch_row(vec![inner_switch().visible(false)]);
    let flat = switch_row(vec![UiNode::new(Role::Switch, r(300.0, 183.0, 0.0, 0.0))]);
    // Different identity: a card with its own button, a different value.
    let card = UiNode::new(Role::Button, r(16.0, 168.0, 361.0, 58.0))
        .with_label("Product")
        .with_children(vec![button("Add", 300.0, 183.0, 63.0, 28.0)]);
    let other_value = switch_row(vec![inner_switch().with_value("1")]);
    let other_id = switch_row(vec![inner_switch().with_identifier("toggles.other")]);
    for (name, node) in [
        ("two", two),
        ("outside", outside),
        ("hidden", hidden),
        ("flat", flat),
        ("card", card),
        ("other value", other_value),
        ("other id", other_id),
    ] {
        let c = compacted(vec![node]);
        assert_eq!(c.elements.len(), 1, "{name}: {:?}", labels(&c));
        assert_eq!(c.elements[0].frame, row, "{name}");
    }
}

#[test]
fn button_in_button_with_the_same_label_taps_the_inner_button() {
    let outer = UiNode::new(Role::Button, r(16.0, 100.0, 360.0, 60.0))
        .with_label("Sign In")
        .with_children(vec![UiNode::new(
            Role::Button,
            r(140.0, 110.0, 100.0, 40.0),
        )
        .with_label("Sign In")]);
    let c = compacted(vec![outer]);
    assert_eq!(c.elements.len(), 1);
    assert_eq!(c.elements[0].frame, r(140.0, 110.0, 100.0, 40.0));
}

#[test]
fn text_field_wrapper_takes_the_inner_field_frame_and_value() {
    let inner = UiNode::new(Role::TextField, r(60.0, 110.0, 300.0, 24.0))
        .with_value("ada@example.com")
        .focused(true);
    let outer = UiNode::new(Role::TextField, r(20.0, 100.0, 360.0, 44.0))
        .with_label("Email")
        .with_children(vec![inner]);
    let c = compacted(vec![outer]);
    assert_eq!(c.elements.len(), 1);
    let e = &c.elements[0];
    assert_eq!(
        (e.label.as_str(), e.value.as_deref()),
        ("Email", Some("ada@example.com"))
    );
    assert_eq!(e.frame, r(60.0, 110.0, 300.0, 24.0));
    assert!(e.focused);
}

#[test]
fn cell_in_cell_is_not_refined_and_still_merges_child_text() {
    let inner = UiNode::new(Role::Cell, r(40.0, 110.0, 200.0, 30.0))
        .with_children(vec![text("Inner text", r(44.0, 115.0, 100.0, 20.0))]);
    let outer = UiNode::new(Role::Cell, r(0.0, 100.0, 393.0, 60.0))
        .with_children(vec![text("Outer text", r(8.0, 105.0, 100.0, 20.0)), inner]);
    let c = compacted(vec![outer]);
    assert_eq!(c.elements.len(), 1);
    assert_eq!(c.elements[0].frame, r(0.0, 100.0, 393.0, 60.0));
    assert_eq!(c.elements[0].label, "Outer text, Inner text");
}

#[test]
fn nested_control_text_is_still_merged_into_an_unlabeled_wrapper() {
    let inner = UiNode::new(Role::Button, r(60.0, 110.0, 120.0, 40.0));
    let outer = UiNode::new(Role::Button, r(16.0, 100.0, 360.0, 60.0))
        .with_children(vec![text("Continue", r(70.0, 120.0, 100.0, 20.0)), inner]);
    let c = compacted(vec![outer]);
    assert_eq!(c.elements.len(), 1);
    assert_eq!(c.elements[0].label, "Continue");
    assert_eq!(c.elements[0].frame, r(60.0, 110.0, 120.0, 40.0));
}

#[test]
fn long_chain_of_nested_controls_is_followed_in_linear_time() {
    let mut n = UiNode::new(Role::Switch, r(100.0, 100.0, 50.0, 30.0));
    for i in 0..3000 {
        let grow = f64::from(i + 1) * 0.01;
        n = UiNode::new(
            Role::Switch,
            r(
                100.0 - grow,
                100.0 - grow,
                50.0 + 2.0 * grow,
                30.0 + 2.0 * grow,
            ),
        )
        .with_children(vec![n]);
    }
    let mut outer = n;
    outer.label = Some("Deep switch".into());
    let started = std::time::Instant::now();
    let c = compacted(vec![outer]);
    assert!(started.elapsed().as_secs() < 5);
    assert_eq!(c.elements.len(), 1);
    assert_eq!(c.elements[0].frame, r(100.0, 100.0, 50.0, 30.0));
    assert_eq!(c.elements[0].label, "Deep switch");
}
