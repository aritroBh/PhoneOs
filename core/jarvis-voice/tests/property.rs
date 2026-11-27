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
//! - No `Voice` start while an answer is read or within `ECHO_WINDOW_MS` after it ended.
//! - A stop acted on while a request was starting ends in `StopTask` when that request's
//!   `Started` arrives (whenever it arrives), never in "On it."; otherwise no task.
//! - A `Started` without the core's id (none, or one it never issued) never makes the core
//!   speak, start the recogniser for it, or take it for the voice task.
//! - No `StartTask` and no answer read while Jarvis is not active (covered or in the
//!   background).
//! - In a stop context (the voice task starts or runs, or another task runs), a transcript
//!   whose words hold a wake phrase right before a stop phrase ends in `StopTask` at once (a
//!   final; a partial with other words around it), and a transcript with neither that nor
//!   only stop and wake words (a bare stop inside other speech) never stops. Checked on
//!   streams whose every transcript was heard in the stop context.
//! - One stop acts once: after a stop, the same words again on that stream stop nothing.
//!
//! Task events travel as the shell sends them: most with the latest request's id (the task
//! the shell started for it), others with none (a task started elsewhere) or a wrong id.

use jarvis_proto::TaskOrigin;
use std::collections::HashSet;

use jarvis_voice::phrases::ON_IT;
use jarvis_voice::{
    contains_wake_word, only_stop_words, parse_utterance, wake_then_stop, AppScene, SpeakAnswers,
    TaskEvent, UtteranceKind, VoiceEffect, VoiceEvent, VoicePhase, VoiceSession, VoiceSettings,
    ECHO_WINDOW_MS, MAX_SPEAKING_MS, START_TIMEOUT_MS,
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
    "and the weather tonight is mild. Jarvis, stop",
    "Jarvis, stop the weather tonight is mild",
    "Hey Jarvis stop the music",
    "and then I told him to stop",
    "stop the music",
    "Stop, stop!",
    "Stop, Jarvis!",
    "Jarvis, stop. Jarvis, stop.",
    "Cancel, cancel",
    "the news at nine, hey jarvis never mind and more news",
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

fn task_event(rng: &mut Rng) -> TaskEvent {
    match rng.below(7) {
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
    }
}

/// A task event as the shell may report it: for the latest request (most), for none, or for
/// a wrong id (an earlier request, or one never issued).
fn reported(rng: &mut Rng, last_request: Option<u64>, event: TaskEvent) -> VoiceEvent {
    match (rng.below(10), last_request) {
        (0..=5, Some(request)) => VoiceEvent::RequestTask { request, event },
        (6, Some(request)) => VoiceEvent::RequestTask {
            request: request.saturating_sub(1),
            event,
        },
        (7, _) => VoiceEvent::RequestTask {
            request: 1_000_000 + rng.next() % 5,
            event,
        },
        _ => VoiceEvent::Task(event),
    }
}

fn event(rng: &mut Rng, spoken: &[u64], last_request: Option<u64>) -> (u64, VoiceEvent) {
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
        13 => {
            let e = task_event(rng);
            reported(rng, last_request, e)
        }
        14 => {
            let e = TaskEvent::FinishedOk {
                answer: answer(rng),
            };
            reported(rng, last_request, e)
        }
        15 => VoiceEvent::LockChanged {
            locked: rng.below(2) == 0,
        },
        16 => match rng.below(4) {
            0 => VoiceEvent::ForegroundChanged {
                active: rng.below(3) > 0,
            },
            1 => VoiceEvent::SceneChanged {
                scene: AppScene::Inactive,
            },
            2 => VoiceEvent::SceneChanged {
                scene: AppScene::Background,
            },
            _ => VoiceEvent::SceneChanged {
                scene: AppScene::Active,
            },
        },
        17 => VoiceEvent::AudioInterrupted {
            began: rng.below(2) == 0,
            should_resume: rng.below(2) == 0,
        },
        18 => VoiceEvent::RouteChanged,
        19 => VoiceEvent::SettingsChanged(settings(rng)),
        20 => VoiceEvent::TranscriptConfirmed { text: text(rng) },
        21 => VoiceEvent::TranscriptDiscarded,
        _ => reported(rng, last_request, TaskEvent::Started),
    };
    (advance, e)
}

/// What the test knows from the events alone.
#[derive(Default)]
struct Model {
    locked: bool,
    speak_answers: bool,
    running: bool,
    /// When the latest request was asked for, and its id, until it is answered or times out.
    awaiting_start: Option<(u64, u64)>,
    partial_since_start: bool,
    /// The latest `StartTask`'s request id.
    last_request: Option<u64>,
    /// Answer lines in flight: id and when they started.
    answers: Vec<(u64, u64)>,
    /// No `Voice` start before this (an answer ended less than `ECHO_WINDOW_MS` ago).
    answer_window_until: u64,
    /// Requests a stop was acted on for while they were starting.
    stopped_while_starting: HashSet<u64>,
    /// Jarvis is not active (covered or in the background).
    not_active: bool,
    /// The current listening stream heard a transcript outside a stop context.
    stream_dirty: bool,
    /// The words that made the current stream's latest stop, once one acted.
    stream_stop_words: Option<Vec<String>>,
    /// The current stream's latest partial words.
    stream_partial: Vec<String>,
}

impl Model {
    fn answer_ended(&mut self, now: u64) {
        self.answer_window_until = self.answer_window_until.max(now + ECHO_WINDOW_MS);
    }
}

#[test]
fn random_sequences_keep_every_safety_property() {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let mut starts = 0usize;
    let mut answers_read = 0usize;
    let mut stopped_starts = 0usize;
    let mut foreign_starts = 0usize;
    let mut wake_stops = 0usize;
    let mut bare_inside = 0usize;
    let mut repeats = 0usize;
    let mut not_active_heard = 0usize;
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
            let (advance, e) = event(&mut rng, &spoken, model.last_request);
            now = now.saturating_add(advance);
            // The start timeout, as the model sees it.
            if model
                .awaiting_start
                .is_some_and(|(at, _)| now.saturating_sub(at) >= START_TIMEOUT_MS)
                && matches!(e, VoiceEvent::Tick)
            {
                model.awaiting_start = None;
            }
            // Answers past the speaking cap count as ended at this step.
            let (expired, open): (Vec<_>, Vec<_>) = model
                .answers
                .iter()
                .partition(|(_, at)| now.saturating_sub(*at) >= MAX_SPEAKING_MS);
            model.answers = open;
            if !expired.is_empty() {
                model.answer_ended(now);
            }
            if let VoiceEvent::SpeechFinished { utterance_id } = &e {
                if let Some(i) = model.answers.iter().position(|(id, _)| id == utterance_id) {
                    model.answers.remove(i);
                    model.answer_ended(now);
                }
            }
            let ctx = format!("seq {seq} step {step}: {e:?}");
            let status = session.status();
            let before = status.phase.clone();
            let out = session.step(now, e.clone());
            // The stop grammar in a stop context, on a stream heard only in it.
            let stop_context = matches!(
                before,
                VoicePhase::Starting | VoicePhase::TaskRunning { .. }
            ) || (before == VoicePhase::Idle && status.other_task_running);
            let mic_on = status.wake_armed || status.recognizing;
            // Only a transcript that reaches a listening stream is read (and parsed).
            let heard = match &e {
                VoiceEvent::PartialTranscript { text } if mic_on => {
                    Some((parse_utterance(text), false))
                }
                VoiceEvent::FinalTranscript { text } if mic_on => {
                    Some((parse_utterance(text), true))
                }
                _ => None,
            };
            let stopped = out.contains(&VoiceEffect::StopTask);
            if let Some((u, is_final)) = &heard {
                if model.not_active {
                    not_active_heard += 1;
                }
                if stop_context && !model.stream_dirty {
                    let words = &u.words;
                    let wake_stop = wake_then_stop(words).is_some();
                    let only = only_stop_words(words);
                    let repeated = model
                        .stream_stop_words
                        .as_ref()
                        .is_some_and(|w| w.len() >= words.len() && w.starts_with(words));
                    if repeated {
                        repeats += 1;
                        assert!(!stopped, "{ctx}: one stop acted twice: {out:?}");
                    } else if model.stream_stop_words.is_none() && wake_stop && (*is_final || !only)
                    {
                        wake_stops += 1;
                        assert!(stopped, "{ctx}: wake + stop did not stop: {out:?}");
                    }
                    if model.stream_stop_words.is_none() && !wake_stop && !only {
                        bare_inside += 1;
                        assert!(!stopped, "{ctx}: other speech stopped: {out:?}");
                    }
                }
                if !stop_context {
                    model.stream_dirty = true;
                }
            }
            if stopped {
                let words = match &heard {
                    Some((u, _)) => u.words.clone(),
                    None => model.stream_partial.clone(),
                };
                model.stream_stop_words = Some(words);
            }
            if let Some((u, false)) = &heard {
                model.stream_partial = u.words.clone();
            }
            // A stop context that began or ended over a stream that heard something: what it
            // heard is read for the next context (never read as its stop).
            let after = session.status();
            let stop_context_after = matches!(
                after.phase,
                VoicePhase::Starting | VoicePhase::TaskRunning { .. }
            ) || (after.phase == VoicePhase::Idle
                && after.other_task_running);
            if stop_context_after != stop_context && !model.stream_partial.is_empty() {
                model.stream_dirty = true;
            }
            // A new stream: a final or a failure ends one, a start or an arming begins one.
            if matches!(
                e,
                VoiceEvent::FinalTranscript { .. } | VoiceEvent::RecognizerFailed { .. }
            ) || out
                .iter()
                .any(|x| matches!(x, VoiceEffect::StartRecognizer | VoiceEffect::ArmWakeWord))
            {
                model.stream_dirty = false;
                model.stream_stop_words = None;
                model.stream_partial.clear();
            }
            for effect in &out {
                match effect {
                    VoiceEffect::StartTask {
                        origin, request, ..
                    } => {
                        starts += 1;
                        assert!(
                            !model.not_active,
                            "{ctx}: StartTask while Jarvis is not active"
                        );
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
                        if *origin == TaskOrigin::Voice {
                            assert!(
                                model.answers.is_empty() && now >= model.answer_window_until,
                                "{ctx}: a voice start while an answer echoes"
                            );
                        }
                        assert!(
                            model.last_request.is_none_or(|r| *request > r),
                            "{ctx}: request id {request} not fresh"
                        );
                        model.awaiting_start = Some((now, *request));
                        model.last_request = Some(*request);
                        model.partial_since_start = false;
                    }
                    VoiceEffect::Speak {
                        text,
                        kind,
                        utterance_id,
                    } => {
                        spoken.push(*utterance_id);
                        assert!(!contains_wake_word(text), "{ctx}: said {text:?}");
                        if *kind == UtteranceKind::Answer {
                            model.answers.push((*utterance_id, now));
                        }
                        if text.contains(MARKER) {
                            answers_read += 1;
                            assert_eq!(*kind, UtteranceKind::Answer, "{ctx}: {text:?}");
                            assert!(
                                !model.locked && model.speak_answers && !model.not_active,
                                "{ctx}: answer read aloud (locked {}, setting {})",
                                model.locked,
                                model.speak_answers
                            );
                        }
                    }
                    VoiceEffect::StopSpeaking => {
                        if !model.answers.is_empty() {
                            model.answers.clear();
                            model.answer_ended(now);
                        }
                    }
                    VoiceEffect::StopTask => {
                        // A stop heard (a final, or a partial at the endpoint tick).
                        let heard =
                            matches!(e, VoiceEvent::FinalTranscript { .. } | VoiceEvent::Tick);
                        if heard && before == VoicePhase::Starting {
                            if let Some(r) = model.last_request {
                                model.stopped_while_starting.insert(r);
                            }
                        }
                    }
                    _ => {}
                }
            }
            // The request's own start, after a stop was acted on while it was starting.
            if let VoiceEvent::RequestTask {
                request,
                event: TaskEvent::Started,
            } = &e
            {
                if model.stopped_while_starting.remove(request) {
                    assert!(
                        out.contains(&VoiceEffect::StopTask),
                        "{ctx}: a stopped request started and was not stopped: {out:?}"
                    );
                    stopped_starts += 1;
                }
                if model.last_request.is_some_and(|r| r == *request) {
                    let said: Vec<_> = out
                        .iter()
                        .filter_map(|e| match e {
                            VoiceEffect::Speak { text, .. } => Some(text.as_str()),
                            _ => None,
                        })
                        .collect();
                    if out.contains(&VoiceEffect::StopTask) {
                        assert!(!said.contains(&ON_IT), "{ctx}: {said:?}");
                    }
                }
            }
            // A start that is not the core's request is never spoken or listened for.
            let foreign = match &e {
                VoiceEvent::Task(TaskEvent::Started) => true,
                VoiceEvent::RequestTask {
                    request,
                    event: TaskEvent::Started,
                } => model.last_request.is_none_or(|r| *request > r) || *request == 0,
                _ => false,
            };
            if foreign {
                foreign_starts += 1;
                assert!(
                    !out.iter().any(|e| matches!(
                        e,
                        VoiceEffect::Speak { .. } | VoiceEffect::StartRecognizer
                    )),
                    "{ctx}: a foreign start made the core speak or listen: {out:?}"
                );
                let after = session.status().phase;
                assert!(
                    !matches!(after, VoicePhase::TaskRunning { .. })
                        || matches!(before, VoicePhase::TaskRunning { .. }),
                    "{ctx}: a foreign start was taken for the voice task"
                );
            }
            let answered = |model: &Model, request: Option<u64>| {
                request.is_some() && model.awaiting_start.map(|(_, r)| r) == request
            };
            let (request, t) = match &e {
                VoiceEvent::Task(t) => (None, Some(t)),
                VoiceEvent::RequestTask { request, event } => (Some(*request), Some(event)),
                _ => (None, None),
            };
            match &e {
                VoiceEvent::ForegroundChanged { active } => model.not_active = !*active,
                VoiceEvent::SceneChanged { scene } => model.not_active = *scene != AppScene::Active,
                VoiceEvent::LockChanged { locked } => model.locked = *locked,
                VoiceEvent::SettingsChanged(s) => {
                    model.speak_answers = s.speak_answers == SpeakAnswers::WhenUnlocked;
                }
                VoiceEvent::PartialTranscript { .. } => model.partial_since_start = true,
                _ => {}
            }
            if let Some(t) = t {
                match t {
                    TaskEvent::Started => {
                        model.running = true;
                        if answered(&model, request) {
                            model.awaiting_start = None;
                        }
                    }
                    TaskEvent::StartRefused { .. } => {
                        if answered(&model, request) {
                            model.awaiting_start = None;
                        }
                    }
                    TaskEvent::FinishedOk { .. } | TaskEvent::Failed | TaskEvent::Stopped => {
                        model.running = false;
                        if answered(&model, request) {
                            model.awaiting_start = None;
                        }
                    }
                    TaskEvent::WaitingForApproval | TaskEvent::ApprovalAnswered => {}
                }
            }
            let _ = session.status();
        }
    }
    // The sequences reach the interesting places.
    assert!(starts > 500, "only {starts} starts");
    assert!(answers_read > 10, "only {answers_read} answers read");
    assert!(stopped_starts > 10, "only {stopped_starts} stopped starts");
    assert!(
        foreign_starts > 1_000,
        "only {foreign_starts} foreign starts"
    );
    assert!(
        wake_stops > 100,
        "only {wake_stops} wake + stop transcripts"
    );
    assert!(
        bare_inside > 100,
        "only {bare_inside} other-speech transcripts"
    );
    assert!(repeats > 20, "only {repeats} repeated stops");
    assert!(
        not_active_heard > 100,
        "only {not_active_heard} transcripts while not active"
    );
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
