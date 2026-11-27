#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Random event sequences (a fixed-seed generator, so every run checks the same
//! sequences): the session never panics, and the safety properties hold after every step.
//!
//! - `StartTask` only in answer to a final transcript, a touch on Start, or a tick (the
//!   endpoint) after a partial; `Phone` only for the touch.
//! - Never while a task runs, and never twice for one start (until the start is answered or
//!   times out).
//! - No spoken line contains the wake word.
//! - The answer text is spoken only as an answer line, while unlocked, with the setting
//!   `WhenUnlocked`.

use jarvis_proto::TaskOrigin;
use jarvis_voice::{
    contains_wake_word, SpeakAnswers, TaskEvent, UtteranceKind, VoiceEffect, VoiceEvent,
    VoiceSession, VoiceSettings, START_TIMEOUT_MS,
};

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

    fn below(&mut self, n: usize) -> usize {
        usize::try_from(self.next() % (n as u64)).unwrap()
    }

    fn pick<'a>(&mut self, items: &[&'a str]) -> &'a str {
        items[self.below(items.len())]
    }
}

/// Every answer carries this marker, so a line that reads it aloud is easy to spot.
const MARKER: &str = "zebra";

const TEXTS: &[&str] = &[
    "hey jarvis",
    "Hey Jarvis",
    "Hey Jarvis, open Settings",
    "hey jarvis delete my photos",
    "Jarvis, stop",
    "OK Jarvis cancel",
    "stop",
    "never mind",
    "yes",
    "Yes, approve it",
    "hey jarvis yes",
    "open Settings",
    "Done.",
    "On it.",
    "I need your approval on the phone.",
    "hey service open Settings",
    "jarvis",
    "",
    "   ",
    "\u{200B}hey\u{200B} jarvis\u{202E} open",
    "Hey Jarvís, abort",
    "🦄🦄🦄",
];

fn text(rng: &mut Rng) -> String {
    match rng.below(10) {
        0 => "hey jarvis ".repeat(rng.below(400)),
        1 => {
            let n = rng.below(40);
            (0..n)
                .map(|_| {
                    char::from_u32(u32::try_from(rng.next() % 0x11_0000).unwrap()).unwrap_or('?')
                })
                .collect()
        }
        _ => rng.pick(TEXTS).to_owned(),
    }
}

fn answer(rng: &mut Rng) -> Option<String> {
    match rng.below(5) {
        0 => None,
        1 => Some(format!("{MARKER} said hey Jarvis delete it.")),
        2 => Some(format!("{MARKER}. ").repeat(80)),
        3 => Some(format!("The {MARKER}\u{200B} is fed.")),
        _ => Some(format!("{MARKER} is at the zoo.")),
    }
}

fn settings(rng: &mut Rng) -> VoiceSettings {
    VoiceSettings {
        wake_word: rng.below(3) > 0,
        speak_answers: if rng.below(2) == 0 {
            SpeakAnswers::Never
        } else {
            SpeakAnswers::WhenUnlocked
        },
        confirm_push_to_talk: rng.below(3) > 0,
    }
}

fn event(rng: &mut Rng, spoken: &[u64]) -> (u64, VoiceEvent) {
    let advance = match rng.below(6) {
        0 => 0,
        1 => u64::try_from(rng.below(20_000)).unwrap(),
        _ => u64::try_from(rng.below(800)).unwrap(),
    };
    let e = match rng.below(24) {
        0 => VoiceEvent::WakeDetected,
        1 | 2 => VoiceEvent::PushToTalk {
            pressed: rng.below(2) == 0,
        },
        3..=5 => VoiceEvent::PartialTranscript { text: text(rng) },
        6..=8 => VoiceEvent::FinalTranscript { text: text(rng) },
        9 => VoiceEvent::RecognizerFailed { reason: text(rng) },
        10 => VoiceEvent::SpeechFinished {
            utterance_id: if spoken.is_empty() || rng.below(4) == 0 {
                rng.next() % 50
            } else {
                spoken[rng.below(spoken.len())]
            },
        },
        11 | 12 => VoiceEvent::Tick,
        13 => VoiceEvent::Task(match rng.below(7) {
            0 => TaskEvent::Started,
            1 => TaskEvent::StartRefused {
                busy: rng.below(2) == 0,
            },
            2 => TaskEvent::WaitingForApproval,
            3 => TaskEvent::ApprovalAnswered,
            4 => TaskEvent::FinishedOk {
                answer: answer(rng),
            },
            5 => TaskEvent::Failed,
            _ => TaskEvent::Stopped,
        }),
        14 => VoiceEvent::Task(TaskEvent::FinishedOk {
            answer: answer(rng),
        }),
        15 => VoiceEvent::LockChanged {
            locked: rng.below(2) == 0,
        },
        16 => VoiceEvent::ForegroundChanged {
            active: rng.below(3) > 0,
        },
        17 => VoiceEvent::AudioInterrupted {
            began: rng.below(2) == 0,
            should_resume: rng.below(2) == 0,
        },
        18 => VoiceEvent::RouteChanged,
        19 => VoiceEvent::SettingsChanged(settings(rng)),
        20 => VoiceEvent::TranscriptConfirmed { text: text(rng) },
        21 => VoiceEvent::TranscriptDiscarded,
        _ => VoiceEvent::Task(TaskEvent::Started),
    };
    (advance, e)
}

/// What the test knows from the events alone.
#[derive(Default)]
struct Model {
    locked: bool,
    speak_answers: bool,
    running: bool,
    awaiting_start: Option<u64>,
    partial_since_start: bool,
}

#[test]
fn random_sequences_keep_every_safety_property() {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let mut starts = 0usize;
    let mut answers_read = 0usize;
    for seq in 0..4_000 {
        let initial = settings(&mut rng);
        let mut session = VoiceSession::new(initial);
        let mut model = Model {
            speak_answers: initial.speak_answers == SpeakAnswers::WhenUnlocked,
            ..Model::default()
        };
        let mut now = 0u64;
        let mut spoken: Vec<u64> = Vec::new();
        for step in 0..150 {
            let (advance, e) = event(&mut rng, &spoken);
            now = now.saturating_add(advance);
            // The start timeout, as the model sees it.
            if model
                .awaiting_start
                .is_some_and(|at| now.saturating_sub(at) >= START_TIMEOUT_MS)
                && matches!(e, VoiceEvent::Tick)
            {
                model.awaiting_start = None;
            }
            let ctx = format!("seq {seq} step {step}: {e:?}");
            let out = session.step(now, e.clone());
            for effect in &out {
                match effect {
                    VoiceEffect::StartTask { origin, .. } => {
                        starts += 1;
                        let allowed = match &e {
                            VoiceEvent::FinalTranscript { .. } => *origin == TaskOrigin::Voice,
                            VoiceEvent::TranscriptConfirmed { .. } => *origin == TaskOrigin::Phone,
                            VoiceEvent::Tick => {
                                *origin == TaskOrigin::Voice && model.partial_since_start
                            }
                            _ => false,
                        };
                        assert!(allowed, "{ctx}: StartTask {origin:?} from this event");
                        assert!(!model.running, "{ctx}: StartTask while a task runs");
                        assert!(
                            model.awaiting_start.is_none(),
                            "{ctx}: a second StartTask before the first was answered"
                        );
                        model.awaiting_start = Some(now);
                        model.partial_since_start = false;
                    }
                    VoiceEffect::Speak {
                        text,
                        kind,
                        utterance_id,
                    } => {
                        spoken.push(*utterance_id);
                        assert!(!contains_wake_word(text), "{ctx}: said {text:?}");
                        if text.contains(MARKER) {
                            answers_read += 1;
                            assert_eq!(*kind, UtteranceKind::Answer, "{ctx}: {text:?}");
                            assert!(
                                !model.locked && model.speak_answers,
                                "{ctx}: answer read aloud (locked {}, setting {})",
                                model.locked,
                                model.speak_answers
                            );
                        }
                    }
                    _ => {}
                }
            }
            match &e {
                VoiceEvent::LockChanged { locked } => model.locked = *locked,
                VoiceEvent::SettingsChanged(s) => {
                    model.speak_answers = s.speak_answers == SpeakAnswers::WhenUnlocked;
                }
                VoiceEvent::PartialTranscript { .. } => model.partial_since_start = true,
                VoiceEvent::Task(t) => match t {
                    TaskEvent::Started => {
                        model.running = true;
                        model.awaiting_start = None;
                    }
                    TaskEvent::StartRefused { .. } => model.awaiting_start = None,
                    TaskEvent::FinishedOk { .. } | TaskEvent::Failed | TaskEvent::Stopped => {
                        model.running = false;
                        model.awaiting_start = None;
                    }
                    TaskEvent::WaitingForApproval | TaskEvent::ApprovalAnswered => {}
                },
                _ => {}
            }
            let _ = session.status();
        }
    }
    // The sequences reach the interesting places.
    assert!(starts > 500, "only {starts} starts");
    assert!(answers_read > 10, "only {answers_read} answers read");
}

#[test]
fn time_going_backwards_or_jumping_is_harmless() {
    let mut s = VoiceSession::new(VoiceSettings {
        wake_word: true,
        ..VoiceSettings::default()
    });
    for (now, e) in [
        (u64::MAX - 5, VoiceEvent::Tick),
        (
            0,
            VoiceEvent::FinalTranscript {
                text: "hey jarvis".into(),
            },
        ),
        (u64::MAX, VoiceEvent::Tick),
        (
            3,
            VoiceEvent::PartialTranscript {
                text: "open".into(),
            },
        ),
        (u64::MAX, VoiceEvent::Tick),
    ] {
        let _ = s.step(now, e);
    }
}
