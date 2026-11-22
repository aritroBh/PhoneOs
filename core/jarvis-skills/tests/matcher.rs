//! Matching requests to skills: normalisation, slots, scores, ambiguity, cost.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use std::time::{Duration, Instant};

use common::*;
use jarvis_skills::{Ambiguity, Skill, SkillMatch, MAX_TASK_CHARS};

fn best(matches: &[SkillMatch]) -> &SkillMatch {
    matches.first().expect("a match")
}

#[test]
fn literal_triggers_match_after_normalisation() {
    let s = store_with(vec![wifi_skill(), text_skill()]);
    for t in [
        "turn off wi-fi",
        "Turn off Wi-Fi.",
        "please   TURN OFF the WiFi",
        "Hey Jarvis, can you switch off the wi fi for me?",
        "disable wifi",
        "WiFi off!",
    ] {
        let m = s.match_task(t);
        assert_eq!(m.len(), 1, "{t:?}: {m:?}");
        assert_eq!(m[0].skill_id, "wifi-off");
        assert_eq!(m[0].score, 1000, "{t:?}");
        assert!(m[0].args.is_empty());
    }
    assert_eq!(
        s.match_task("turn off wi-fi")[0].matched_trigger,
        "turn off wi-fi"
    );
    assert_eq!(s.match_task("wifi off")[0].matched_trigger, "wifi off");
}

#[test]
fn high_overlap_matches_below_exact_and_low_overlap_does_not() {
    let s = store_with(vec![Skill {
        id: "morning".into(),
        name: "Morning briefing".into(),
        triggers: vec!["read my morning briefing aloud".into()],
        ..Skill::default()
    }]);
    // Same words, different order.
    let m = s.match_task("read aloud my morning briefing");
    assert_eq!(m.len(), 1);
    assert!(m[0].score < 1000 && m[0].score >= 600, "{}", m[0].score);
    // One extra word out of five still counts.
    assert_eq!(s.match_task("read my morning briefing aloud now").len(), 1);
    // Half the words do not.
    assert!(s.match_task("read my emails aloud").is_empty());
    assert!(s.match_task("briefing").is_empty());
}

#[test]
fn slots_capture_from_the_original_text() {
    let s = store_with(vec![text_skill()]);
    let m = s.match_task("text mom i am on my way");
    assert_eq!(m.len(), 1);
    let m = best(&m);
    assert_eq!(m.args["contact"], "mom");
    assert_eq!(
        m.args["message"], "i am on my way",
        "filler words inside a slot are kept"
    );
    assert_eq!(m.matched_trigger, "text {contact} {message}");
    assert!(m.missing.is_empty());

    let m = s.match_task("Please text Mom: I'm running late!");
    let m = best(&m);
    assert_eq!(m.args["contact"], "Mom");
    assert_eq!(m.args["message"], "I'm running late");

    let m = s.match_task("send Alex a message saying the train is late");
    let m = best(&m);
    assert_eq!(
        m.matched_trigger,
        "send {contact} a message saying {message}"
    );
    assert_eq!(m.args["contact"], "Alex");
    assert_eq!(m.args["message"], "the train is late");
    assert!(m.score > 800);

    // A slot needs at least one word.
    assert!(s.match_task("text mom").is_empty());
    assert!(s.match_task("text").is_empty());
    // Literals must come in order.
    assert!(s.match_task("mom text hi").is_empty());
}

#[test]
fn slot_followed_by_literal_takes_the_shortest_capture() {
    let s = store_with(vec![Skill {
        id: "remind".into(),
        name: "Reminder".into(),
        triggers: vec!["remind me to {task} at {time}".into()],
        params: vec![param("task", true), param("time", true)],
        ..Skill::default()
    }]);
    let m = s.match_task("Remind me to call the bank at 5pm");
    let m = best(&m);
    assert_eq!(m.args["task"], "call the bank");
    assert_eq!(m.args["time"], "5pm");
    // Trailing literals must be present.
    assert!(s.match_task("remind me to call the bank").is_empty());
}

#[test]
fn optional_params_and_missing_required_ones_are_reported() {
    let s = store_with(vec![Skill {
        id: "call".into(),
        name: "Call someone".into(),
        triggers: vec!["call {who}".into(), "make a call".into()],
        params: vec![param("who", true)],
        ..Skill::default()
    }]);
    let m = s.match_task("make a call");
    assert_eq!(m[0].missing, vec!["who".to_owned()]);
    let m = s.match_task("call grandma");
    assert!(m[0].missing.is_empty());
}

#[test]
fn disabled_skills_never_match() {
    let mut w = wifi_skill();
    w.enabled = false;
    let s = store_with(vec![w]);
    assert!(s.match_task("turn off wi-fi").is_empty());
}

#[test]
fn ambiguity_is_reported_not_guessed() {
    let mut other = wifi_skill();
    other.id = "wifi-off-quick".into();
    other.name = "Quick Wi-Fi off".into();
    let s = store_with(vec![wifi_skill(), other]);
    let m = s.match_task("turn off wifi");
    assert_eq!(m.len(), 2);
    assert!(m.is_ambiguous());
    assert!(m.as_slice().is_ambiguous());
    // Ties are ordered by id, deterministically.
    assert_eq!(m[0].skill_id, "wifi-off");
    assert_eq!(m[1].skill_id, "wifi-off-quick");

    // A clearly better match is not ambiguous: exact literal over a slotted one.
    let s = store_with(vec![
        Skill {
            id: "say-hi".into(),
            name: "Say hi to Mom".into(),
            triggers: vec!["text mom hello".into()],
            ..Skill::default()
        },
        text_skill(),
    ]);
    let m = s.match_task("text mom hello");
    assert_eq!(m.len(), 2);
    assert_eq!(m[0].skill_id, "say-hi");
    assert!(!m.is_ambiguous(), "{m:?}");
    assert!(!s.match_task("text mom see you").is_ambiguous());
}

#[test]
fn matching_is_deterministic() {
    let skills: Vec<Skill> = (0..40)
        .map(|i| Skill {
            id: format!("s{i:02}"),
            name: format!("Skill {i}"),
            triggers: vec![
                format!("open {{thing}} number {}", i % 5),
                "open the door".into(),
            ],
            params: vec![param("thing", true)],
            ..Skill::default()
        })
        .collect();
    let s = store_with(skills);
    let a = s.match_task("open the door");
    let b = s.match_task("open the door");
    assert_eq!(a, b);
    assert_eq!(a.len(), 40);
    let ids: Vec<&str> = a.iter().map(|m| m.skill_id.as_str()).collect();
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    assert_eq!(ids, sorted);
}

#[test]
fn over_long_and_empty_requests_match_nothing() {
    let s = store_with(vec![wifi_skill(), text_skill()]);
    assert!(s.match_task("").is_empty());
    assert!(s.match_task("  please  ").is_empty());
    let long = format!("text mom {}", "a ".repeat(MAX_TASK_CHARS));
    assert!(s.match_task(&long).is_empty());
    // Just under the cap still matches.
    let ok = format!("text mom {}", "a".repeat(MAX_TASK_CHARS - 20));
    assert_eq!(s.match_task(&ok).len(), 1);
}

/// A store at the limits: 500 skills, 12 slotted triggers each of ~120 characters.
fn worst_case_store() -> jarvis_skills::SkillStore {
    let skills: Vec<Skill> = (0..500)
        .map(|i| Skill {
            id: format!("skill-{i:03}"),
            name: format!("Skill {i}"),
            triggers: (0..12)
                .map(|t| {
                    format!(
                        "b {{p0}} b {{p1}} b {{p2}} b {{p3}} b {{p4}} b {{p5}} b {{p6}} b {{p7}} b end{t}"
                    )
                })
                .collect(),
            params: (0..8).map(|p| param(&format!("p{p}"), false)).collect(),
            ..Skill::default()
        })
        .collect();
    store_with(skills)
}

#[test]
fn huge_and_adversarial_inputs_finish_fast() {
    let s = worst_case_store();
    // A megabyte of text: refused by the length cap without scanning it all.
    let mb = "a ".repeat(500_000);
    let t = Instant::now();
    assert!(s.match_task(&mb).is_empty());
    // A megabyte without spaces, and a megabyte of multi-byte characters.
    assert!(s.match_task(&"x".repeat(1_000_000)).is_empty());
    assert!(s.match_task(&"é".repeat(1_000_000)).is_empty());
    assert!(
        t.elapsed() < Duration::from_millis(500),
        "{:?}",
        t.elapsed()
    );

    // An input at the cap that passes the literal pre-check of every one of the 6,000
    // triggers and fails each only at the very end, so every slot table is filled:
    // "b b b ... b end0 b".
    let mut adversarial = "b ".repeat(240);
    adversarial.push_str("end0 b");
    assert!(adversarial.chars().count() <= MAX_TASK_CHARS);
    let t = Instant::now();
    for _ in 0..3 {
        assert!(s.match_task(&adversarial).is_empty());
    }
    // And one that matches all 500 skills (trigger 0 each), capturing every slot.
    let matching = format!("{}end0", "b ".repeat(240));
    let m = s.match_task(&matching);
    assert_eq!(m.len(), 500);
    assert!(m.is_ambiguous());
    assert_eq!(m[0].args["p0"], "b");
    assert_eq!(m[0].args["p7"].split(' ').count(), 240 - 16);
    // Generous for debug builds; the point is no quadratic or exponential blow-up.
    assert!(t.elapsed() < Duration::from_secs(3), "{:?}", t.elapsed());
}
