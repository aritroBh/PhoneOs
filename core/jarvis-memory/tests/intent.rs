#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
mod common;
use jarvis_memory::*;

#[test]
fn intent_key_table() {
    let table = [
        (
            "Hey Jarvis, can you please open the Settings app?",
            "open settings app",
        ),
        ("  Turn ON   airplane mode!!! ", "enable airplane mode"),
        ("Text my mom: I'm running late.", "text mom im running late"),
        (
            "could you set a timer for 10 minutes",
            "set timer for 10 minutes",
        ),
        ("OK Jarvis, play my Discover Weekly", "play discover weekly"),
        ("Open the {1} app", "open 1 app"),
        ("please", ""),
        ("", ""),
        ("Café — réservation à 19h", "café réservation à 19h"),
        ("for me, call Dad", "call dad"),
    ];
    for (input, want) in table {
        assert_eq!(intent_key(input), want, "input: {input:?}");
    }
}

#[test]
fn spelling_variants_share_one_key() {
    let groups: &[&[&str]] = &[
        &[
            "Turn off Wi-Fi",
            "turn off wifi",
            "Switch off wi fi",
            "disable WiFi",
            "Disable Wi‑Fi",
        ],
        &["Turn on Wi-Fi", "switch on wifi", "Enable Wi Fi"],
        &[
            "Send an e-mail to Sam",
            "send an email to sam",
            "Send an E Mail to Sam",
        ],
        &["Log in to Shop", "login to shop", "log-in to Shop"],
        &["Sign in to Shop", "signin to shop", "sign-in to shop"],
        &["Sign out of Shop", "signout of shop"],
    ];
    for group in groups {
        let keys: Vec<String> = group.iter().map(|t| intent_key(t)).collect();
        assert!(
            keys.windows(2).all(|w| w[0] == w[1]),
            "{group:?} -> {keys:?}"
        );
    }
    assert_eq!(intent_key("Turn off Wi-Fi"), "disable wifi");
    assert_eq!(intent_key("Log in to Shop"), "login to shop");
    // Words that only look similar stay apart.
    assert_ne!(intent_key("turn off wifi"), intent_key("turn on wifi"));
    assert_ne!(intent_key("log in"), intent_key("log out"));
    assert_eq!(intent_key("switch to dark mode"), "switch to dark mode");
    assert_eq!(intent_key("e"), "e");
    // Every canonical word is itself stable.
    for (_, canon) in EQUIVALENT_PHRASES {
        assert_eq!(intent_key(canon), *canon);
    }
}

#[test]
fn stored_keys_are_renormalized_and_slots_survive() {
    assert_eq!(renormalize_key("turn off wi fi"), "disable wifi");
    assert_eq!(
        renormalize_key("text mom {1} via e mail"),
        "text mom {1} via email"
    );
    assert_eq!(renormalize_key("disable wifi"), "disable wifi");
    // Slot values keep the task's own spelling.
    let b = match_slotted("email {1} to sam", "E-mail Wi-Fi password to Sam").unwrap();
    assert_eq!(b.get(&1).map(String::as_str), Some("Wi-Fi password"));
}

#[test]
fn intent_key_is_idempotent_and_never_contains_braces() {
    for s in ["Text mom {1} hi", "}{", "a{2}b"] {
        let k = intent_key(s);
        assert!(!k.contains('{') && !k.contains('}'));
        assert_eq!(intent_key(&k), k);
    }
}

#[test]
fn slotted_match_extracts_original_text() {
    let b = match_slotted("text mom {1}", "Hey Jarvis, text mom on my way!").unwrap();
    assert_eq!(b.get(&1).map(String::as_str), Some("on my way"));
    let b = match_slotted("text mom {1}", "text my mom \"Running late, sorry\"").unwrap();
    assert_eq!(b.get(&1).map(String::as_str), Some("Running late, sorry"));
}

#[test]
fn slot_in_the_middle_needs_a_unique_anchor() {
    let key = "search {1} on youtube";
    let b = match_slotted(key, "search cats and dogs on YouTube").unwrap();
    assert_eq!(b.get(&1).map(String::as_str), Some("cats and dogs"));
    // anchor occurs twice: ambiguous
    assert!(match_slotted(key, "search youtube on youtube on youtube").is_none());
}

#[test]
fn slotted_match_rejects_mismatches() {
    assert!(match_slotted("text mom {1}", "text dad running late").is_none());
    assert!(match_slotted("text mom {1}", "text mom").is_none());
    assert!(
        match_slotted("text mom", "text mom hi").is_none(),
        "no slot in key"
    );
    assert!(match_slotted("{1} {2}", "a b").is_none(), "adjacent slots");
    assert!(
        match_slotted("open {1} app", "open maps app now").is_none(),
        "trailing words"
    );
    assert!(match_slotted("open {1} app", "please open maps app").is_some());
}

#[test]
fn recorded_slot_roundtrip_shares_one_flow() {
    let rec = |task: &str, typed: &str| {
        let (h, i, t) = (common::home(), common::inbox(), common::thread());
        let mut r = Recorder::new(task);
        r.record(RecordedStep {
            action: jarvis_proto::Action::OpenApp {
                app: "Messages".into(),
            },
            element: None,
            screen: &h,
        });
        r.record(RecordedStep {
            action: jarvis_proto::Action::Tap { element: 2 },
            element: i.get(2),
            screen: &i,
        });
        r.record(RecordedStep {
            action: jarvis_proto::Action::Type {
                element: Some(2),
                text: typed.into(),
                submit: false,
            },
            element: t.get(2),
            screen: &t,
        });
        r.finish(true, 5)
    };
    let flow = rec("Hey Jarvis, text mom running late", "running late").unwrap();
    assert_eq!(flow.intent_key, "text mom {1}");
    assert!(matches!(
        &flow.steps[2].action,
        FlowAction::Type {
            text: FlowText::Slot { slot: 1 },
            ..
        }
    ));
    // The typed text is not stored anywhere in the flow.
    assert!(!serde_json::to_string(&flow)
        .unwrap()
        .contains("running late"));

    let mut store = FlowStore::new();
    store.upsert(flow);
    let (f, b) = store.find("text mom on my way").unwrap();
    assert_eq!(b.get(&1).map(String::as_str), Some("on my way"));
    let actions = {
        let screens = [common::home(), common::inbox(), common::thread()];
        let mut rp = Replayer::new(f, b);
        let mut v = vec![];
        for s in &screens {
            if let Next::Action(a) = rp.next(s) {
                v.push(a);
            } else {
                panic!("diverged");
            }
            rp.advance();
        }
        v
    };
    assert_eq!(
        actions[2],
        jarvis_proto::Action::Type {
            element: Some(2),
            text: "on my way".into(),
            submit: false
        }
    );
    assert!(store.find("text dad on my way").is_none());
}

#[test]
fn typed_text_inside_a_longer_word_or_repeated_is_not_a_slot() {
    // "late" appears inside "plate": raw match count is 2 -> literal.
    let f = common::record_text_flow("text mom plate late", "late").unwrap();
    assert_eq!(f.intent_key, "text mom plate late");
    // Too short.
    let f = common::record_text_flow("text mom a b", "b").unwrap();
    assert_eq!(f.intent_key, "text mom b");
    // Not in the task at all.
    let f = common::record_text_flow("text mom", "hello").unwrap();
    assert_eq!(f.intent_key, "text mom");
}

#[test]
fn a_key_with_no_literal_word_is_not_slotted() {
    let f = common::record_text_flow("running late", "running late").unwrap();
    assert_eq!(f.intent_key, "running late");
}
