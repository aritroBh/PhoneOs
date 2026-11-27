//! The structural policy model: denied categories, StepByStep, per-app trust levels,
//! first-use targets, and the `Unverified` / `Call` categories that replace the old
//! stopgaps.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use common::{app_screen, button, el, label, screen};
use jarvis_proto::{
    Action, AppRule, CompactScreen, ConfirmRequest, Direction, Policy, PolicyMode, RiskCategory,
    Role, Tier, ToolSpec, TrustLevel,
};
use jarvis_safety::{
    app_level, changes_state, check_app, classify, evaluate, first_use_app, first_use_request,
    gate, gate_in, label_is_foreign, Decision, Gate, TRUSTED_ALLOWED,
};

fn tap(i: u32) -> Action {
    Action::Tap { element: i }
}

fn decision_category(d: &Decision) -> Option<RiskCategory> {
    match d {
        Decision::Confirm(ConfirmRequest { category, .. }) => Some(*category),
        _ => None,
    }
}

fn shop(elements: Vec<jarvis_proto::Element>) -> CompactScreen {
    screen("com.example.shop", "Shop", elements)
}

fn settings_switch() -> CompactScreen {
    screen(
        "com.apple.Preferences",
        "Settings",
        vec![label(1, "Wi-Fi"), el(2, Role::Switch, "Wi-Fi")],
    )
}

fn tool(name: &str, app: Option<&str>, risk: RiskCategory) -> ToolSpec {
    ToolSpec {
        name: name.to_owned(),
        description: String::new(),
        params: vec![],
        tier: Tier::Native,
        risk,
        app: app.map(str::to_owned),
        self_contained: false,
    }
}

#[test]
fn deny_categories_deny_and_none_is_never_denied() {
    let p = Policy {
        deny_categories: vec![RiskCategory::Payment, RiskCategory::None],
        ..Policy::default()
    };
    assert_eq!(gate(&p, RiskCategory::Payment), Gate::Deny);
    assert_eq!(gate(&p, RiskCategory::Delete), Gate::Confirm);
    assert_eq!(gate(&p, RiskCategory::None), Gate::Allow);
    let s = app_screen(vec![label(1, "Total $42.10"), button(2, "Pay $42.10")]);
    assert!(matches!(evaluate(&p, &tap(2), &s, None), Decision::Deny(_)));
    // A harmless tap on the same policy still runs.
    let s = app_screen(vec![button(1, "Next")]);
    assert_eq!(evaluate(&p, &tap(1), &s, None), Decision::Allow);
    // Denied categories win over Trusted and StepByStep.
    for level in [
        TrustLevel::Trusted,
        TrustLevel::StepByStep,
        TrustLevel::Guarded,
    ] {
        let s = app_screen(vec![button(1, "Pay")]);
        assert_eq!(
            gate_in(&p, RiskCategory::Payment, level, &tap(1), &s),
            Gate::Deny
        );
    }
}

#[test]
fn non_english_and_unlabeled_controls_are_unverified() {
    for lbl in [
        "Bezahlen",
        "Jetzt kaufen",
        "Löschen",
        "Payer",
        "Enviar",
        "Café",
        "Ünïcödé",
    ] {
        assert!(label_is_foreign(lbl), "{lbl}");
        let s = app_screen(vec![button(1, lbl)]);
        assert_eq!(
            classify(&tap(1), &s, None),
            RiskCategory::Unverified,
            "{lbl}"
        );
        assert_eq!(
            decision_category(&evaluate(&Policy::default(), &tap(1), &s, None)),
            Some(RiskCategory::Unverified),
            "{lbl}"
        );
    }
    for lbl in ["Send", "Next", "Settings", "42"] {
        assert!(!label_is_foreign(lbl), "{lbl}");
    }
    // A definite English signal on the same label still wins.
    let s = app_screen(vec![button(1, "Pay · Bezahlen")]);
    assert_eq!(classify(&tap(1), &s, None), RiskCategory::Payment);
    // Rows carry names with accents: a "Café Central" row is content, not a control.
    let s = app_screen(vec![el(1, Role::Cell, "Café Central")]);
    assert_eq!(classify(&tap(1), &s, None), RiskCategory::None);
    // ...but a foreign action word on a row is still unverified.
    let s = app_screen(vec![el(1, Role::Cell, "Bezahlen")]);
    assert_eq!(classify(&tap(1), &s, None), RiskCategory::Unverified);
    // Unlabeled buttons and unknown symbol names.
    for lbl in ["", "chevron.right", "ellipsis.circle"] {
        let s = app_screen(vec![button(1, lbl)]);
        assert_eq!(
            classify(&tap(1), &s, None),
            RiskCategory::Unverified,
            "{lbl:?}"
        );
    }
    // Context still wins over "unknown": a trash prompt makes an unlabeled button Delete.
    let s = app_screen(vec![label(1, "Delete this photo?"), button(2, "")]);
    assert_eq!(classify(&tap(2), &s, None), RiskCategory::Delete);
    // Tabs and keys are never pressed into confirmation.
    let s = app_screen(vec![el(1, Role::Tab, "Kasse")]);
    assert_eq!(classify(&tap(1), &s, None), RiskCategory::None);
    // Tool calls without a spec are unverified (and evaluate denies them).
    let call = Action::CallTool {
        name: "mystery".to_owned(),
        args: serde_json::json!({}),
    };
    assert_eq!(classify(&call, &s, None), RiskCategory::Unverified);
    assert!(matches!(
        evaluate(&Policy::default(), &call, &s, None),
        Decision::Deny(_)
    ));
}

#[test]
fn call_placing_rows_and_urls_are_call() {
    let s = screen(
        "com.apple.mobilephone",
        "Phone",
        vec![label(1, "Favorites"), el(2, Role::Cell, "Mom")],
    );
    assert_eq!(classify(&tap(2), &s, None), RiskCategory::Call);
    let req = match evaluate(&Policy::default(), &tap(2), &s, None) {
        Decision::Confirm(r) => r,
        other => panic!("{other:?}"),
    };
    assert_eq!(req.category, RiskCategory::Call);
    // Fourth review: the text stacked above the target names its group.
    assert_eq!(req.summary, "Tap \"Mom\" under \"Favorites\" in Phone");
    let s = screen(
        "com.apple.facetime",
        "FaceTime",
        vec![el(1, Role::Cell, "Alex")],
    );
    assert_eq!(classify(&tap(1), &s, None), RiskCategory::Call);
    for url in [
        "tel:+14155550123",
        "telprompt:5550123",
        "facetime-audio:alex@example.com",
    ] {
        let a = Action::OpenUrl {
            url: url.to_owned(),
        };
        assert_eq!(classify(&a, &s, None), RiskCategory::Call, "{url}");
    }
    // A Call-only confirm list still asks for calls but not for messages.
    let p = Policy {
        confirm: vec![RiskCategory::Call],
        ..Policy::default()
    };
    let s = screen(
        "com.apple.mobilephone",
        "Phone",
        vec![el(1, Role::Cell, "Mom")],
    );
    assert_eq!(
        decision_category(&evaluate(&p, &tap(1), &s, None)),
        Some(RiskCategory::Call)
    );
    let s = app_screen(vec![button(1, "Send")]);
    assert_eq!(evaluate(&p, &tap(1), &s, None), Decision::Allow);
}

#[test]
fn step_by_step_confirms_every_state_change() {
    let p = Policy {
        mode: PolicyMode::StepByStep,
        ..Policy::default()
    };
    let s = shop(vec![
        button(1, "Next"),
        el(2, Role::TextField, "Search"),
        el(3, Role::Picker, "Size"),
        el(4, Role::Table, "Results"),
        button(5, "Delete"),
    ]);
    let confirms = [
        tap(1),
        tap(2),
        Action::Type {
            element: Some(2),
            text: "shoes".to_owned(),
            submit: false,
        },
        Action::Clear { element: 2 },
        Action::Scroll {
            direction: Direction::Down,
            element: Some(3),
        },
        Action::Scroll {
            direction: Direction::Left,
            element: None,
        },
        Action::Swipe {
            direction: Direction::Up,
            element: None,
        },
        Action::PressHome,
        Action::OpenApp {
            app: "com.apple.Preferences".to_owned(),
        },
        Action::OpenUrl {
            url: "https://example.com".to_owned(),
        },
    ];
    for a in &confirms {
        assert!(changes_state(a, &s), "{a:?}");
        let d = evaluate(&p, a, &s, None);
        assert!(matches!(d, Decision::Confirm(_)), "{a:?}: {d:?}");
    }
    // Harmless-looking steps ask with category None and a readable summary.
    let plain = shop(vec![button(1, "Next")]);
    match evaluate(&p, &tap(1), &plain, None) {
        Decision::Confirm(r) => {
            assert_eq!(r.category, RiskCategory::None);
            assert_eq!(r.summary, "Tap \"Next\" in Shop");
            assert_eq!(r.app, "com.example.shop");
        }
        other => panic!("{other:?}"),
    }
    // Risky steps keep their category.
    assert_eq!(
        decision_category(&evaluate(&p, &tap(5), &s, None)),
        Some(RiskCategory::Delete)
    );
    let allowed = [
        Action::Wait { ms: 500 },
        Action::Look,
        Action::AskUser {
            question: "Which size?".to_owned(),
        },
        Action::Scroll {
            direction: Direction::Down,
            element: None,
        },
        Action::Scroll {
            direction: Direction::Up,
            element: Some(4),
        },
    ];
    for a in &allowed {
        assert!(!changes_state(a, &s), "{a:?}");
        assert_eq!(evaluate(&p, a, &s, None), Decision::Allow, "{a:?}");
    }
    assert!(!changes_state(
        &Action::Done {
            summary: String::new()
        },
        &s
    ));
    assert!(!changes_state(
        &Action::Fail {
            reason: String::new()
        },
        &s
    ));
    // Tool calls are steps too.
    let t = tool("create_note", None, RiskCategory::None);
    let call = Action::CallTool {
        name: "create_note".to_owned(),
        args: serde_json::json!({"text": "milk"}),
    };
    assert!(matches!(
        evaluate(&p, &call, &s, Some(&t)),
        Decision::Confirm(_)
    ));
}

#[test]
fn app_rules_set_the_level_with_longest_prefix() {
    let p = Policy {
        app_rules: vec![
            AppRule::new("com.example", TrustLevel::Denied),
            AppRule::new("com.example.shop", TrustLevel::StepByStep),
            AppRule::new("com.example.shop.lite", TrustLevel::Trusted),
            AppRule::new("com.apple.Preferences", TrustLevel::Trusted),
        ],
        ..Policy::default()
    };
    assert_eq!(app_level(&p, "com.example.mail"), TrustLevel::Denied);
    assert_eq!(app_level(&p, "COM.EXAMPLE.SHOP"), TrustLevel::StepByStep);
    assert_eq!(app_level(&p, "com.example.shop.lite"), TrustLevel::Trusted);
    assert_eq!(app_level(&p, "com.apple.Preferences"), TrustLevel::Trusted);
    // Prefixes match whole components only.
    assert_eq!(app_level(&p, "com.examples.app"), TrustLevel::Guarded);
    assert_eq!(app_level(&p, "com.other"), TrustLevel::Guarded);
    // The deny list wins over any rule.
    let mut q = p.clone();
    q.app_rules
        .push(AppRule::new("com.apple.Passwords", TrustLevel::Trusted));
    assert_eq!(app_level(&q, "com.apple.Passwords"), TrustLevel::Denied);
    // Unlisted apps follow the mode.
    let step = Policy {
        mode: PolicyMode::StepByStep,
        ..p.clone()
    };
    assert_eq!(app_level(&step, "com.other"), TrustLevel::StepByStep);
    assert_eq!(
        app_level(&step, "com.example.shop.lite"),
        TrustLevel::Trusted
    );

    // The StepByStep app asks for a plain tap; a Guarded app does not.
    let s = shop(vec![button(1, "Next")]);
    assert!(matches!(
        evaluate(&p, &tap(1), &s, None),
        Decision::Confirm(_)
    ));
    let s = screen("com.other", "Other", vec![button(1, "Next")]);
    assert_eq!(evaluate(&p, &tap(1), &s, None), Decision::Allow);
}

#[test]
fn denied_app_rule_behaves_like_the_deny_list() {
    let by_rule = Policy {
        app_rules: vec![AppRule::new("com.examplebank", TrustLevel::Denied)],
        ..Policy::default()
    };
    let mut by_list = Policy::default();
    by_list.denied_apps.push("com.examplebank".to_owned());
    for p in [&by_rule, &by_list] {
        assert!(check_app(p, "com.examplebank.mobile").is_denied());
        assert!(check_app(p, "ExampleBank").is_denied());
        let s = screen(
            "com.examplebank.mobile",
            "Bank",
            vec![button(1, "Accounts")],
        );
        assert!(matches!(evaluate(p, &tap(1), &s, None), Decision::Deny(_)));
        assert!(matches!(
            evaluate(p, &Action::Look, &s, None),
            Decision::Deny(_)
        ));
        // Leaving stays possible.
        assert_eq!(evaluate(p, &Action::PressHome, &s, None), Decision::Allow);
        let home = screen("com.apple.springboard", "Home", vec![]);
        let open = Action::OpenApp {
            app: "com.examplebank.mobile".to_owned(),
        };
        assert!(matches!(evaluate(p, &open, &home, None), Decision::Deny(_)));
        let t = tool(
            "transfer",
            Some("com.examplebank.mobile"),
            RiskCategory::Payment,
        );
        let call = Action::CallTool {
            name: "transfer".to_owned(),
            args: serde_json::json!({}),
        };
        assert!(matches!(
            evaluate(p, &call, &home, Some(&t)),
            Decision::Deny(_)
        ));
    }
    // A more specific rule carves an app out of a denied prefix (deny list entries cannot).
    let carve = Policy {
        app_rules: vec![
            AppRule::new("com.example", TrustLevel::Denied),
            AppRule::new("com.example.notes", TrustLevel::Guarded),
        ],
        ..Policy::default()
    };
    assert!(!check_app(&carve, "com.example.notes").is_denied());
    assert!(check_app(&carve, "com.example.mail").is_denied());
}

#[test]
fn trusted_apps_skip_lower_stakes_categories_only() {
    assert_eq!(
        TRUSTED_ALLOWED,
        &[RiskCategory::SystemSetting, RiskCategory::Unverified]
    );
    let p = Policy {
        app_rules: vec![
            AppRule::new("com.apple.Preferences", TrustLevel::Trusted),
            AppRule::new("com.example.shop", TrustLevel::Trusted),
        ],
        ..Policy::default()
    };
    // A Settings switch: SystemSetting, allowed without asking.
    let s = settings_switch();
    assert_eq!(classify(&tap(2), &s, None), RiskCategory::SystemSetting);
    assert_eq!(evaluate(&p, &tap(2), &s, None), Decision::Allow);
    assert!(matches!(
        evaluate(&Policy::default(), &tap(2), &s, None),
        Decision::Confirm(_)
    ));
    // Unreadable controls in a trusted app run.
    let s = shop(vec![button(1, "Bezahlen")]);
    assert_eq!(evaluate(&p, &tap(1), &s, None), Decision::Allow);
    // The keyword net still applies for the high-stakes categories.
    for (lbl, cat) in [
        ("Pay now", RiskCategory::Payment),
        ("Buy Now", RiskCategory::Purchase),
        ("Delete", RiskCategory::Delete),
        ("Change Password", RiskCategory::AccountChange),
        ("Send", RiskCategory::SendMessage),
        ("Post", RiskCategory::PublicPost),
    ] {
        let s = shop(vec![button(1, lbl)]);
        assert_eq!(
            decision_category(&evaluate(&p, &tap(1), &s, None)),
            Some(cat),
            "{lbl}"
        );
    }
    let s = shop(vec![el(1, Role::TextField, "Password")]);
    let typed = Action::Type {
        element: Some(1),
        text: "hunter2".to_owned(),
        submit: false,
    };
    assert_eq!(
        decision_category(&evaluate(&p, &typed, &s, None)),
        Some(RiskCategory::Credential)
    );
}

#[test]
fn first_use_targets_the_app_acted_in() {
    let p = Policy::default();
    let s = shop(vec![button(1, "Next")]);
    assert_eq!(
        first_use_app(&p, &tap(1), &s, None).as_deref(),
        Some("com.example.shop")
    );
    for a in [
        Action::Look,
        Action::Wait { ms: 10 },
        Action::Scroll {
            direction: Direction::Down,
            element: None,
        },
        Action::PressHome,
        Action::OpenApp {
            app: "com.example.mail".to_owned(),
        },
        Action::OpenUrl {
            url: "https://example.com".to_owned(),
        },
    ] {
        assert_eq!(first_use_app(&p, &a, &s, None), None, "{a:?}");
    }
    // Tools act inside their declared app; app-less tools inside none.
    let call = Action::CallTool {
        name: "order".to_owned(),
        args: serde_json::json!({}),
    };
    let t = tool("order", Some("com.example.food"), RiskCategory::Purchase);
    assert_eq!(
        first_use_app(&p, &call, &s, Some(&t)).as_deref(),
        Some("com.example.food")
    );
    let t = tool("order", None, RiskCategory::Purchase);
    assert_eq!(first_use_app(&p, &call, &s, Some(&t)), None);
    // SpringBoard, Trusted apps and a disabled flag never need it.
    let home = screen("com.apple.springboard", "Home", vec![button(1, "Shop")]);
    assert_eq!(first_use_app(&p, &tap(1), &home, None), None);
    let trusted = Policy {
        app_rules: vec![AppRule::new("com.example.shop", TrustLevel::Trusted)],
        ..Policy::default()
    };
    assert_eq!(first_use_app(&trusted, &tap(1), &s, None), None);
    let off = Policy {
        first_use_approval: false,
        ..Policy::default()
    };
    assert_eq!(first_use_app(&off, &tap(1), &s, None), None);

    let req = first_use_request("com.example.shop", Some("Shop"));
    assert_eq!(req.category, RiskCategory::FirstUse);
    assert_eq!(req.summary, "Let Jarvis act in Shop (com.example.shop)?");
    assert_eq!(req.app, "com.example.shop");
    let req = first_use_request("com.example.shop", Some("  "));
    assert_eq!(req.summary, "Let Jarvis act in com.example.shop?");
    // Names from the screen are sanitised.
    let req = first_use_request("com.x", Some("Shop\u{202E}\"evil\""));
    assert!(!req.summary.contains('\u{202E}'));
    assert!(!req.summary.contains('"'));
}

#[test]
fn an_app_less_tool_follows_the_mode_not_the_foreground_rule() {
    let p = Policy {
        app_rules: vec![AppRule::new("com.apple.Preferences", TrustLevel::Trusted)],
        ..Policy::default()
    };
    let s = settings_switch();
    let call = Action::CallTool {
        name: "set_brightness".to_owned(),
        args: serde_json::json!({"level": 0.2}),
    };
    // Sixteenth review (F7): the number is model text, so the owner's request states it.
    let owner = ["Set the brightness to 0.2"];
    let owned = |t: &ToolSpec| {
        jarvis_safety::evaluate_owned(
            &p,
            &call,
            &s,
            Some(t),
            jarvis_safety::ScreenSource::Typed,
            None,
            &[],
            &owner,
        )
    };
    // A setting changed by a tool that acts in no app is not covered by Settings' trust.
    let t = tool("set_brightness", None, RiskCategory::SystemSetting);
    assert_eq!(
        decision_category(&owned(&t)),
        Some(RiskCategory::SystemSetting)
    );
    // A number the owner did not state asks first, whatever the mode.
    assert_eq!(
        decision_category(&evaluate(&p, &call, &s, Some(&t))),
        Some(RiskCategory::SendMessage)
    );
    // The same tool declared as acting in Settings is.
    let t = tool(
        "set_brightness",
        Some("com.apple.Preferences"),
        RiskCategory::SystemSetting,
    );
    assert_eq!(owned(&t), Decision::Allow);
}
