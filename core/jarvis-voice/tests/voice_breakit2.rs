#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Voice, second break-it review (against 384eb5c: the review fixes d79dd37 and 822f431).
//!
//! `f<N>_…` tests FAIL today and pass once the finding is fixed; `sound_…` tests are
//! things tried that held. Each finding's doc comment gives the severity, the exact input,
//! what happens, what should happen and why it matters.
//!
//! Helpers (`common`): `task` reports the event with the latest voice request's id (the
//! voice task, as the shell reports it), `other` with no id (a task started elsewhere),
//! `for_request` with a chosen id. Each test says which it means.

mod common;

use common::*;
use jarvis_proto::TaskOrigin;
use jarvis_voice::phrases::{BUSY, ON_IT, STOPPED};
use jarvis_voice::{
    TaskEvent, UtteranceKind, VoiceEffect, VoiceEvent, VoicePhase, VoiceSettings, ECHO_WINDOW_MS,
    ENDPOINT_SILENCE_MS, SPEAKING_TAIL_MS,
};

/// A hands-free voice task that runs ("On it." said and finished, the barge-in recogniser
/// listening). `task` (the voice request's id) reports its start.
fn running(settings: VoiceSettings) -> Voice {
    let mut v = Voice::new(settings);
    v.final_("Hey Jarvis, turn off Wi-Fi");
    v.task(TaskEvent::Started);
    v.speech_done();
    assert_eq!(
        v.session.status().phase,
        VoicePhase::TaskRunning {
            question_pending: false
        }
    );
    assert!(v.session.status().recognizing);
    v
}

fn count(effects: &[VoiceEffect], wanted: &VoiceEffect) -> usize {
    effects.iter().filter(|e| *e == wanted).count()
}

// ---------------------------------------------------------------------------------------
// Findings.
// ---------------------------------------------------------------------------------------

/// **f1, medium: the owner's natural stop forms do not stop the voice task.**
///
/// Input: a hands-free voice task runs (barge-in recogniser on; `task` = its own events);
/// the recogniser delivers one of these finals: "Stop, stop!", "Stop, Jarvis!", "Jarvis,
/// stop. Jarvis, stop.", "Cancel, cancel".
///
/// What happens: `Utterance::is_stop` needs the request (after a leading wake phrase) to be
/// exactly one stop phrase. A repeated stop, or the wake word after it, is "a request" —
/// and in TaskRunning a request without the wake phrase is ignored, with it "I am busy."
/// is said. The task goes on.
///
/// Should: in a stop context (barge-in while the voice task starts or runs) an utterance
/// made only of stop phrases and wake phrases, in any order and repeated, stops (stopping
/// is the safe direction; nothing else may act there anyway).
///
/// Why: someone whose phone is doing something they did not want says "stop, stop!" or
/// "stop, Jarvis!" — the core's own rule 3 says stop always works.
#[test]
fn f1_repeated_or_trailing_wake_stop_forms_do_not_stop_the_task() {
    let mut not_stopped = Vec::new();
    for t in [
        "Stop, stop!",
        "Stop, Jarvis!",
        "Jarvis, stop. Jarvis, stop.",
        "Cancel, cancel",
    ] {
        let mut v = running(wake_on());
        let out = v.final_(t);
        if !has(&out, &VoiceEffect::StopTask) {
            not_stopped.push((t, said(&out)));
        }
    }
    assert!(not_stopped.is_empty(), "did not stop: {not_stopped:?}");
}

/// **f2, medium: "Jarvis, stop" after other speech in the same recognition never stops.**
///
/// Input A (barge-in): a hands-free voice task runs; the room (a video, a call on
/// speaker) talks, so the barge-in recognition's partial is "and the weather tonight is
/// mild"; the owner says "Jarvis, stop": the partial becomes "and the weather tonight is
/// mild. Jarvis, stop" and stays so for 2 x `ENDPOINT_SILENCE_MS`; then the final with the
/// same text.
///
/// Input B (wake word's stream while a task started elsewhere runs, `other` = no id): the
/// stream's partial "the news at nine. Jarvis, stop", unchanged for 2 x the endpoint, then
/// the final.
///
/// What happens: a recognition's transcript is everything heard since it started (the
/// on-device recogniser does not cut an utterance at a pause on its own; the wake word's
/// stream starts a new recognition only after a final). The wake phrase counts only at the
/// start and the stop words must be the whole request, so neither the stable partial nor
/// the final stops anything. Any speech in the room before "Jarvis, stop" in the same
/// recognition swallows it.
///
/// Should: where only the stop words can act (a stop context), the stop words as the last
/// words of the transcript, after a wake phrase (or bare in barge-in), stop.
///
/// Why: barge-in and "Jarvis, stop" exist for exactly the noisy case (media playing,
/// someone else talking); in a quiet room they are rarely needed.
#[test]
fn f2_stop_after_other_speech_in_the_same_recognition_never_stops() {
    let mut v = running(wake_on());
    v.partial("and the weather tonight is mild");
    v.wait(300);
    v.partial("and the weather tonight is mild. Jarvis, stop");
    v.wait(ENDPOINT_SILENCE_MS);
    v.wait(ENDPOINT_SILENCE_MS);
    v.final_("and the weather tonight is mild. Jarvis, stop");
    assert!(
        count(&v.log, &VoiceEffect::StopTask) >= 1,
        "barge-in: the stop was swallowed: {:?}",
        v.log
    );

    let mut v = Voice::new(wake_on());
    v.other(TaskEvent::Started);
    assert!(v.session.status().other_task_running);
    assert!(v.session.status().wake_armed);
    v.partial("the news at nine. Jarvis, stop");
    v.wait(ENDPOINT_SILENCE_MS);
    v.wait(ENDPOINT_SILENCE_MS);
    v.final_("the news at nine. Jarvis, stop");
    assert!(
        count(&v.log, &VoiceEffect::StopTask) >= 1,
        "another task: the stop was swallowed: {:?}",
        v.log
    );
}

/// **f3, medium: "Jarvis, stop" that the room keeps talking over never stops.**
///
/// Input: a hands-free voice task runs (barge-in; `task` = its own events). The owner says
/// "Jarvis, stop" (partial "Jarvis, stop"); 700 ms later the room's audio, still playing,
/// is added to the same recognition: partials "Jarvis, stop the", "Jarvis, stop the
/// weather tonight is mild" every 700 ms; then the final "Jarvis, stop the weather tonight
/// is mild and dry".
///
/// What happens: the partial stop waits for `ENDPOINT_SILENCE_MS` (1.5 s) of no change;
/// the next partial (more words) clears it; the final is a request ("stop the weather …"),
/// which in TaskRunning only makes Jarvis say "I am busy.". The grammar needs 1.5 s of
/// silence right after the stop words, which a playing video never gives.
///
/// Should: in a stop context, a wake phrase followed by a stop phrase as the first words of
/// the request stops, whatever follows (stopping is the safe direction; the bare-word rule
/// `review_fixes::a_partial_stop_acts_only_after_the_endpoint_and_only_if_unchanged` —
/// "stop" then "stop the music" — can stay as it is).
///
/// Why: the realistic case for "Jarvis, stop" is a room that is not quiet.
#[test]
fn f3_jarvis_stop_talked_over_by_the_room_never_stops() {
    let mut v = running(wake_on());
    v.partial("Jarvis, stop");
    v.wait(700);
    v.partial("Jarvis, stop the");
    v.wait(700);
    v.partial("Jarvis, stop the weather tonight is mild");
    v.wait(700);
    v.final_("Jarvis, stop the weather tonight is mild and dry");
    v.wait(ENDPOINT_SILENCE_MS * 2);
    assert!(
        count(&v.log, &VoiceEffect::StopTask) >= 1,
        "the stop was swallowed; said {:?}",
        v.said()
    );
}

/// **f4, low: a partial stop whose punctuation or case flaps never reaches the endpoint.**
///
/// Input: a voice task runs (barge-in); the recogniser (punctuation on, as the adapter sets
/// `addsPunctuation`) revises its partial between "Stop" and "Stop." every 600 ms for 6 s,
/// and no final arrives (the recognition goes on).
///
/// What happens: the pending stop is keyed on the raw text, so each revision restarts its
/// endpoint; it never fires. The same words were heard the whole time.
///
/// Should: compare the words (`parse_utterance(..).words`), not the raw text: unchanged
/// words for `ENDPOINT_SILENCE_MS` stop.
#[test]
fn f4_a_partial_stop_that_flaps_in_punctuation_never_stops() {
    let mut v = running(wake_on());
    for i in 0..10 {
        v.partial(if i % 2 == 0 { "Stop" } else { "Stop." });
        v.wait(600);
    }
    assert!(
        count(&v.log, &VoiceEffect::StopTask) >= 1,
        "the stop was swallowed: {:?}",
        v.log
    );
}

// ---------------------------------------------------------------------------------------
// Sound.
// ---------------------------------------------------------------------------------------

/// The partial stop's endpoint, both sides of the boundary: one millisecond short does
/// nothing, the boundary stops (once).
#[test]
fn sound_the_partial_stop_endpoint_is_exact_on_both_sides() {
    let mut v = running(wake_on());
    v.partial("stop");
    assert!(!has(
        &v.wait(ENDPOINT_SILENCE_MS - 1),
        &VoiceEffect::StopTask
    ));
    assert!(has(&v.wait(1), &VoiceEffect::StopTask));
    v.wait(ENDPOINT_SILENCE_MS * 2);
    assert_eq!(count(&v.log, &VoiceEffect::StopTask), 1);
}

/// A late end of request 1, arriving while request 2 runs (`for_request(1, …)`): request 2
/// is not finished, request 1's answer is not read as 2's, nothing is said.
#[test]
fn sound_a_late_end_of_an_earlier_request_is_not_the_current_one() {
    let mut v = running(speak_answers());
    v.task(TaskEvent::FinishedOk { answer: None });
    v.speech_done();
    v.final_("Hey Jarvis, turn on Bluetooth");
    let second = requests(&v.log);
    assert_eq!(second, [1, 2]);
    v.task(TaskEvent::Started);
    v.speech_done();
    let said = v.said().len();
    let out = v.for_request(
        1,
        TaskEvent::FinishedOk {
            answer: Some("Your bank code is 1234.".into()),
        },
    );
    assert!(said_kinds(&out).is_empty(), "{out:?}");
    for e in [TaskEvent::Failed, TaskEvent::Stopped, TaskEvent::Started] {
        let out = v.for_request(1, e.clone());
        assert!(said_kinds(&out).is_empty(), "{e:?}: {out:?}");
    }
    assert_eq!(v.said().len(), said);
    assert_eq!(
        v.session.status().phase,
        VoicePhase::TaskRunning {
            question_pending: false
        }
    );
}

/// `Started` reported twice for the voice task (`task` twice): "On it." once, one start.
#[test]
fn sound_started_twice_is_one_start() {
    let mut v = Voice::new(wake_on());
    v.final_("Hey Jarvis, turn off Wi-Fi");
    v.task(TaskEvent::Started);
    v.task(TaskEvent::Started);
    assert_eq!(
        v.said().iter().filter(|s| s.as_str() == ON_IT).count(),
        1,
        "{:?}",
        v.said()
    );
    assert_eq!(count(&v.log, &VoiceEffect::StopTask), 0);
}

/// The voice request is refused, then its `Started` arrives anyway (`for_request` with the
/// refused id): it is stopped at once and never adopted ("On it." never said).
#[test]
fn sound_a_start_after_its_refusal_is_stopped_not_adopted() {
    let mut v = Voice::new(wake_on());
    v.final_("Hey Jarvis, turn off Wi-Fi");
    let id = *requests(&v.log).last().unwrap();
    v.task(TaskEvent::StartRefused { busy: false });
    v.speech_done();
    let out = v.for_request(id, TaskEvent::Started);
    assert!(has(&out, &VoiceEffect::StopTask), "{out:?}");
    assert!(!v.said().contains(&ON_IT.to_owned()), "{:?}", v.said());
    assert_eq!(v.session.status().phase, VoicePhase::Idle);
}

/// A start reported for an id never issued (`for_request(99, …)`) while voice's request 1
/// is starting is not the voice task: no "On it.", still Starting.
#[test]
fn sound_a_start_for_an_id_never_issued_is_not_adopted() {
    let mut v = Voice::new(wake_on());
    v.final_("Hey Jarvis, turn off Wi-Fi");
    let out = v.for_request(99, TaskEvent::Started);
    assert!(said_kinds(&out).is_empty(), "{out:?}");
    assert_eq!(v.session.status().phase, VoicePhase::Starting);
}

/// A stop heard while Starting, then a refusal: the kept stop does not reach the next
/// request (its start says "On it." and is not stopped).
#[test]
fn sound_a_kept_stop_does_not_carry_to_the_next_request() {
    let mut v = Voice::new(wake_on());
    v.final_("Hey Jarvis, turn off Wi-Fi");
    v.final_("stop");
    v.task(TaskEvent::StartRefused { busy: false });
    v.speech_done();
    v.wait(ECHO_WINDOW_MS);
    let stops = count(&v.log, &VoiceEffect::StopTask);
    v.final_("Hey Jarvis, turn on Bluetooth");
    let out = v.task(TaskEvent::Started);
    assert!(!has(&out, &VoiceEffect::StopTask), "{out:?}");
    assert!(said(&out).contains(&ON_IT.to_owned()), "{out:?}");
    assert_eq!(count(&v.log, &VoiceEffect::StopTask), stops);
}

/// A stop heard while Starting survives a lock and an interruption in between: the start
/// that follows (`task`) is stopped at once and "Stopped." is said when it has.
#[test]
fn sound_a_kept_stop_survives_a_lock_and_an_interruption() {
    let mut v = Voice::new(wake_on());
    v.final_("Hey Jarvis, turn off Wi-Fi");
    v.final_("stop");
    v.send(VoiceEvent::LockChanged { locked: true });
    v.send(VoiceEvent::AudioInterrupted {
        began: true,
        should_resume: false,
    });
    v.send(VoiceEvent::AudioInterrupted {
        began: false,
        should_resume: true,
    });
    v.send(VoiceEvent::LockChanged { locked: false });
    let out = v.task(TaskEvent::Started);
    assert!(has(&out, &VoiceEffect::StopTask), "{out:?}");
    let out = v.task(TaskEvent::Stopped);
    assert_eq!(said(&out), [STOPPED.to_owned()]);
}

/// An answer whose reading was cut by the lock (or by leaving the front) still opens the
/// echo window: its late echo, heard within `ECHO_WINDOW_MS` after the cut, starts nothing;
/// after the window a request starts again.
#[test]
fn sound_a_cut_answer_still_opens_the_echo_window() {
    for cut in [
        VoiceEvent::LockChanged { locked: true },
        VoiceEvent::ForegroundChanged { active: false },
    ] {
        let mut v = running(speak_answers());
        v.task(TaskEvent::FinishedOk {
            answer: Some("It is sunny in Paris today.".into()),
        });
        assert!(said_kinds(&v.log)
            .iter()
            .any(|(_, k)| *k == UtteranceKind::Answer));
        let out = v.send(cut.clone());
        assert!(has(&out, &VoiceEffect::StopSpeaking), "{cut:?}: {out:?}");
        v.open.clear();
        match cut {
            VoiceEvent::LockChanged { .. } => {
                v.send(VoiceEvent::LockChanged { locked: false });
            }
            _ => {
                v.send(VoiceEvent::ForegroundChanged { active: true });
            }
        }
        v.wait(SPEAKING_TAIL_MS);
        let out = v.final_("Hey Jarvis, open Mail");
        assert!(starts(&out).is_empty(), "{cut:?}: {out:?}");
        v.wait(ECHO_WINDOW_MS);
        let out = v.final_("Hey Jarvis, open Mail");
        assert_eq!(
            starts(&out),
            [("open Mail".to_owned(), TaskOrigin::Voice)],
            "{cut:?}"
        );
    }
}

/// The owner's "Jarvis, stop" during a read answer stops the reading at once, and push to
/// talk (a touch) still opens listening inside the window.
#[test]
fn sound_the_owner_can_stop_an_answer_and_use_push_to_talk_in_the_window() {
    let mut v = running(speak_answers());
    v.task(TaskEvent::FinishedOk {
        answer: Some("It is sunny in Paris today.".into()),
    });
    let out = v.final_("Jarvis, stop");
    assert_eq!(out.first(), Some(&VoiceEffect::StopSpeaking), "{out:?}");
    v.open.clear();
    let out = v.send(VoiceEvent::PushToTalk { pressed: true });
    assert!(has(&out, &VoiceEffect::StartRecognizer), "{out:?}");
    assert_eq!(v.session.status().phase, VoicePhase::Listening);
}

/// A second answer cannot be produced inside the window without a touch, so the window
/// cannot be extended by the room: after one window, a hands-free request starts.
#[test]
fn sound_the_room_cannot_extend_the_window() {
    let mut v = running(speak_answers());
    v.task(TaskEvent::FinishedOk {
        answer: Some("It is sunny in Paris today.".into()),
    });
    v.speech_done();
    for _ in 0..5 {
        assert!(v.final_("Hey Jarvis, what is the weather").is_empty());
        v.wait(500);
    }
    v.wait(ECHO_WINDOW_MS);
    let out = v.final_("Hey Jarvis, what is the weather");
    assert_eq!(starts(&out).len(), 1, "{out:?}");
}

/// While the voice task runs, a wake-phrase request says "I am busy." and starts nothing,
/// and a spoken "yes" to a pending question does nothing (not even "busy").
#[test]
fn sound_nothing_but_stop_acts_while_the_voice_task_runs() {
    let mut v = running(wake_on());
    v.task(TaskEvent::WaitingForApproval);
    v.speech_done();
    let out = v.final_("yes");
    assert!(out.iter().all(|e| !matches!(
        e,
        VoiceEffect::StartTask { .. } | VoiceEffect::StopTask | VoiceEffect::Speak { .. }
    )));
    v.wait(SPEAKING_TAIL_MS);
    let out = v.final_("Hey Jarvis, yes");
    assert_eq!(said(&out), [BUSY.to_owned()]);
    assert_eq!(v.starts().len(), 1);
}
