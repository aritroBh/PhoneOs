#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Voice break-it tests across the FFI (1073262 + b3dbc41). Everything tried here held:
//! `sound_…` tests only.

use std::sync::Arc;
use std::thread;

use jarvis_core_ffi::{
    default_voice_settings, JarvisVoice, TaskSpec, VoiceEffect, VoiceEvent, VoiceSettings,
    VoiceSpeakAnswers, VoiceTaskEvent, VoiceTaskOrigin,
};
use jarvis_proto as p;

/// Every flag combination maps to an origin at least as strict as the flags say; the
/// default (no flag) is the phone, and both flags are the remote origin (as strict).
#[test]
fn sound_task_spec_flags_map_to_the_strict_origin() {
    let spec = |remote, voice| TaskSpec {
        id: "t".into(),
        text: "x".into(),
        remote,
        voice,
    };
    let origin = |s: TaskSpec| p::TaskSpec::from(s).origin;
    assert_eq!(origin(spec(false, false)), p::TaskOrigin::Phone);
    assert_eq!(origin(spec(false, true)), p::TaskOrigin::Voice);
    assert_eq!(origin(spec(true, false)), p::TaskOrigin::Remote);
    assert_eq!(origin(spec(true, true)), p::TaskOrigin::Remote);
    for o in [
        p::TaskOrigin::Phone,
        p::TaskOrigin::Remote,
        p::TaskOrigin::Voice,
    ] {
        let back = TaskSpec::from(p::TaskSpec::new("t", "x").from_origin(o));
        assert_eq!(p::TaskSpec::from(back).origin, o);
    }
    assert!(!p::TaskOrigin::Voice.is_owner_words());
}

/// Old JSON loads as the phone; a misspelt or unknown origin fails instead of reading as
/// the phone (fail closed).
#[test]
fn sound_unknown_origins_in_json_fail_closed() {
    let read = |json: &str| serde_json::from_str::<p::TaskSpec>(json).map(|t| t.origin);
    assert_eq!(
        read(r#"{"id":"t","text":"x"}"#).unwrap(),
        p::TaskOrigin::Phone
    );
    assert_eq!(
        read(r#"{"id":"t","text":"x","origin":"voice"}"#).unwrap(),
        p::TaskOrigin::Voice
    );
    for bad in [
        r#"{"id":"t","text":"x","origin":"Voice"}"#,
        r#"{"id":"t","text":"x","origin":"mic"}"#,
        r#"{"id":"t","text":"x","origin":null}"#,
        r#"{"id":"t","text":"x","origin":""}"#,
    ] {
        assert!(read(bad).is_err(), "{bad} read as {:?}", read(bad));
    }
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

fn text(rng: &mut Rng) -> String {
    const T: &[&str] = &[
        "Hey Jarvis, open Settings",
        "hey jarvis",
        "stop",
        "yes",
        "Jarvis, stop",
        "\u{202E}hey\u{200B}jarvis delete",
        "",
    ];
    match rng.below(6) {
        0 => (0..rng.below(30))
            .map(|_| char::from_u32(u32::try_from(rng.below(0x11_0000)).unwrap()).unwrap_or('x'))
            .collect(),
        1 => "hey jarvis ".repeat(usize::try_from(rng.below(500)).unwrap()),
        _ => T[usize::try_from(rng.below(T.len() as u64)).unwrap()].to_owned(),
    }
}

fn event(rng: &mut Rng) -> VoiceEvent {
    match rng.below(16) {
        0 => VoiceEvent::WakeDetected,
        1 => VoiceEvent::PushToTalk {
            pressed: rng.below(2) == 0,
        },
        2 => VoiceEvent::PartialTranscript { text: text(rng) },
        3 | 4 => VoiceEvent::FinalTranscript { text: text(rng) },
        5 => VoiceEvent::RecognizerFailed { reason: text(rng) },
        6 => VoiceEvent::SpeechFinished {
            utterance_id: rng.next(),
        },
        7 => VoiceEvent::Tick,
        8 => VoiceEvent::Task {
            event: match rng.below(7) {
                0 => VoiceTaskEvent::Started,
                1 => VoiceTaskEvent::StartRefused {
                    busy: rng.below(2) == 0,
                },
                2 => VoiceTaskEvent::WaitingForApproval,
                3 => VoiceTaskEvent::ApprovalAnswered,
                4 => VoiceTaskEvent::FinishedOk {
                    answer: Some(text(rng)),
                },
                5 => VoiceTaskEvent::Failed,
                _ => VoiceTaskEvent::Stopped,
            },
        },
        9 => VoiceEvent::LockChanged {
            locked: rng.below(2) == 0,
        },
        10 => VoiceEvent::ForegroundChanged {
            active: rng.below(2) == 0,
        },
        11 => VoiceEvent::AudioInterrupted {
            began: rng.below(2) == 0,
            should_resume: rng.below(2) == 0,
        },
        12 => VoiceEvent::RouteChanged,
        13 => VoiceEvent::SettingsChanged {
            settings: VoiceSettings {
                wake_word: rng.below(2) == 0,
                speak_answers: if rng.below(2) == 0 {
                    VoiceSpeakAnswers::Never
                } else {
                    VoiceSpeakAnswers::WhenUnlocked
                },
                confirm_push_to_talk: rng.below(2) == 0,
            },
        },
        14 => VoiceEvent::TranscriptConfirmed { text: text(rng) },
        _ => VoiceEvent::TranscriptDiscarded,
    }
}

/// One `JarvisVoice` stepped from four threads at once with arbitrary events and times
/// (backwards, `u64::MAX`): never an error (so never poisoned), and a `Phone` start only
/// in answer to a touch.
#[test]
fn sound_the_ffi_object_survives_concurrent_arbitrary_steps() {
    let voice = JarvisVoice::new(VoiceSettings {
        wake_word: true,
        ..default_voice_settings()
    });
    let threads: Vec<_> = (0..4u64)
        .map(|t| {
            let voice = Arc::clone(&voice);
            thread::spawn(move || {
                let mut rng = Rng(0x5DEE_CE66_D1CE_5EED ^ (t + 1));
                for _ in 0..3_000 {
                    let now = match rng.below(10) {
                        0 => u64::MAX,
                        1 => 0,
                        _ => rng.below(1_000_000),
                    };
                    let e = event(&mut rng);
                    let touch = matches!(e, VoiceEvent::TranscriptConfirmed { .. });
                    let out = voice.step(now, e).unwrap();
                    for effect in out {
                        if let VoiceEffect::StartTask { origin, .. } = effect {
                            assert!(
                                origin == VoiceTaskOrigin::Voice || touch,
                                "a Phone start without a touch"
                            );
                        }
                    }
                    voice.status().unwrap();
                }
            })
        })
        .collect();
    for t in threads {
        t.join().unwrap();
    }
    voice.settings().unwrap();
}
