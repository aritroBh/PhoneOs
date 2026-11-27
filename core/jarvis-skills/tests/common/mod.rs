//! Shared fixtures: a few realistic skills.

#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

use jarvis_skills::{Skill, SkillKind, SkillParam, SkillStore};

pub const MESSAGES: &str = "com.apple.MobileSMS";
pub const SETTINGS: &str = "com.apple.Preferences";

pub fn param(name: &str, required: bool) -> SkillParam {
    SkillParam {
        name: name.to_owned(),
        description: format!("The {name}"),
        required,
    }
}

/// "Text someone": slotted triggers, confined to Messages.
pub fn text_skill() -> Skill {
    Skill {
        id: "text-someone".into(),
        name: "Text someone".into(),
        summary: "Send a short iMessage".into(),
        kind: SkillKind::Written,
        triggers: vec![
            "text {contact} {message}".into(),
            "send {contact} a message saying {message}".into(),
        ],
        instructions: "Open Messages and pick the existing thread.\nKeep the message as I said it."
            .into(),
        apps: vec![MESSAGES.into()],
        confine_to_apps: true,
        params: vec![param("contact", true), param("message", true)],
        created_ms: 10,
        updated_ms: 10,
        ..Skill::default()
    }
}

/// "Wi-Fi off": a literal trigger in Settings.
pub fn wifi_skill() -> Skill {
    Skill {
        id: "wifi-off".into(),
        name: "Wi-Fi off".into(),
        triggers: vec!["turn off wi-fi".into(), "wifi off".into()],
        instructions: "Use the switch on the Wi-Fi page, not Control Center.".into(),
        apps: vec![SETTINGS.into()],
        ..Skill::default()
    }
}

pub fn store_with(skills: Vec<Skill>) -> SkillStore {
    let mut s = SkillStore::new();
    for k in skills {
        s.add(k).unwrap();
    }
    s
}
