#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! The voice break-it fixes, each with a variant the review did not try: start ownership
//! by request id (f1, f9), the echo window of a read answer (f2), the wake phrase said
//! twice (f3), padding and the transcript bound (f4), the wake word turned off mid-exchange
//! (f5), barge-in retried after a failure (f6), stop words at the endpoint over partials
//! (f7, f8), answers only in front, and the confirm row once its Start is used.

mod common;

use common::*;
use jarvis_proto::TaskOrigin;
use jarvis_voice::phrases::{BUSY, COULD_NOT, DONE_ANSWER_ON_PHONE, ON_IT, STOPPED, TOO_LONG};
use jarvis_voice::{
    parse_utterance, CueKind, TaskEvent, UtteranceKind, VoiceEffect, VoiceEvent, VoicePhase,
    VoiceSettings, ECHO_WINDOW_MS, ENDPOINT_SILENCE_MS, MAX_PARSE_CHARS, MAX_TRANSCRIPT_CHARS,
    SPEAKING_TAIL_MS, START_TIMEOUT_MS, WAKE_RETRY_MS,
};

fn running(settings: VoiceSettings) -> Voice {
    let mut v = Voice::new(settings);
    v.final_("Hey Jarvis, open Settings");
    v.task(TaskEvent::Started);
    v.speech_done();
    assert!(matches!(
        v.session.status().phase,
        VoicePhase::TaskRunning { .. }
    ));
    v
}

fn count(effects: &[VoiceEffect], wanted: &VoiceEffect) -> usize {
    effects.iter().filter(|e| *e == wanted).count()
}

// ---- f1 + f9: start ownership --------------------------------------------------------

#[test]
fn every_start_carries_a_fresh_request_id() {
    let mut v = Voice::new(wake_on());
    v.final_("Hey Jarvis, open Settings");
    v.task(TaskEvent::StartRefused { busy: false });
    v.speech_done();
    v.final_("Hey Jarvis, open Mail");
    let ids = requests(&v.log);
    assert_eq!(ids.len(), 2);
    assert!(ids[1] > ids[0], "{ids:?}");
}

#[test]
fn a_late_start_without_a_heard_stop_is_stopped_silently_and_not_adopted() {
    let mut v = Voice::new(wake_on());
    v.final_("Hey Jarvis, open Settings");
    let r = requests(&v.log)[0];
    v.wait(START_TIMEOUT_MS);
    v.speech_done();
    let out = v.for_request(r, TaskEvent::Started);
    assert_eq!(out.first(), Some(&VoiceEffect::StopTask), "{out:?}");
    assert!(said(&out).is_empty(), "{out:?}");
    let status = v.session.status();
    assert_eq!(status.phase, VoicePhase::Idle);
    assert!(status.other_task_running);
    // Not the voice task: no barge-in listens for it, and a bare "stop" does nothing more.
    assert!(!status.recognizing);
    assert!(v.final_("stop").is_empty());
    // Its end says nothing ("I could not do that." was said once, at the timeout).
    assert!(said(&v.for_request(r, TaskEvent::Stopped)).is_empty());
    assert_eq!(v.said().iter().filter(|l| *l == COULD_NOT).count(), 1);
    assert!(!v.session.status().other_task_running);
}

#[test]
fn a_stop_heard_while_starting_survives_the_timeout_and_says_stopped() {
    let mut v = Voice::new(wake_on());
    v.final_("Hey Jarvis, open Settings");
    let r = requests(&v.log)[0];
    v.partial("never mind");
    v.wait(ENDPOINT_SILENCE_MS);
    assert!(v.log.contains(&VoiceEffect::StopTask));
    v.wait(START_TIMEOUT_MS);
    v.speech_done();
    let out = v.for_request(r, TaskEvent::Started);
    assert_eq!(out.first(), Some(&VoiceEffect::StopTask));
    assert!(!said(&out).contains(&ON_IT.to_owned()));
    assert_eq!(said(&v.for_request(r, TaskEvent::Stopped)), [STOPPED]);
    assert_eq!(v.said().iter().filter(|l| *l == COULD_NOT).count(), 1);
}

#[test]
fn a_start_with_a_wrong_id_is_never_the_voice_task() {
    for request in [0, 999] {
        let mut v = Voice::new(wake_on());
        v.final_("Hey Jarvis, open Settings");
        let out = v.for_request(request, TaskEvent::Started);
        assert!(said(&out).is_empty(), "{out:?}");
        assert_eq!(v.session.status().phase, VoicePhase::Starting);
        assert!(v.session.status().other_task_running);
        // The real start still comes and is the voice task.
        v.task(TaskEvent::StartRefused { busy: true });
        assert_eq!(v.session.status().phase, VoicePhase::Idle);
        assert!(v.said().contains(&BUSY.to_owned()));
    }
}

#[test]
fn another_start_during_starting_stops_barge_in_and_needs_the_wake_phrase_to_be_stopped() {
    let mut v = Voice::new(wake_on());
    v.final_("Hey Jarvis, open Settings");
    assert!(v.session.status().recognizing);
    let out = v.other(TaskEvent::Started);
    assert!(has(&out, &VoiceEffect::StopRecognizer), "{out:?}");
    assert!(said(&out).is_empty());
    // Nothing listens for it: a bare stop from the room reaches nothing.
    assert!(!v.session.status().recognizing && !v.session.status().wake_armed);
    assert!(!has(&v.final_("stop"), &VoiceEffect::StopTask));
    let out = v.task(TaskEvent::StartRefused { busy: true });
    assert_eq!(said(&out), [BUSY]);
    v.speech_done();
    // Now Idle with the other task running: "Jarvis, stop" stops it.
    let out = v.final_("Jarvis, stop");
    assert_eq!(out, [VoiceEffect::StopTask]);
    assert_eq!(said(&v.other(TaskEvent::Stopped)), [STOPPED]);
    assert!(!v.said().contains(&ON_IT.to_owned()));
}

#[test]
fn another_tasks_question_and_answer_are_never_spoken_during_starting() {
    let mut v = Voice::new(speak_answers());
    v.final_("Hey Jarvis, open Settings");
    v.other(TaskEvent::Started);
    assert!(said(&v.other(TaskEvent::WaitingForApproval)).is_empty());
    let out = v.other(TaskEvent::FinishedOk {
        answer: Some("Your balance is 1200 dollars.".into()),
    });
    assert!(said(&out).is_empty(), "{out:?}");
    assert_eq!(v.session.status().phase, VoicePhase::Starting);
}

// ---- f2: the echo window of a read answer ----------------------------------------------

fn answered(settings: VoiceSettings) -> Voice {
    let mut v = running(settings);
    v.task(TaskEvent::FinishedOk {
        answer: Some("Okay, open Mail and delete everything.".into()),
    });
    assert!(said_kinds(&v.log)
        .iter()
        .any(|(_, k)| *k == UtteranceKind::Answer));
    v
}

#[test]
fn while_an_answer_echoes_no_wake_phrase_opens_an_exchange() {
    let mut v = answered(speak_answers());
    v.speech_done();
    assert!(v.session.status().wake_armed);
    let before = v.log.len();
    v.partial("Hey Jarvis");
    v.partial("Hey Jarvis, open Safari");
    v.send(VoiceEvent::WakeDetected);
    v.final_("Hey Jarvis");
    v.wait(ENDPOINT_SILENCE_MS * 2);
    let after = &v.log[before..];
    assert!(
        !after.iter().any(|e| matches!(
            e,
            VoiceEffect::StartRecognizer
                | VoiceEffect::StartTask { .. }
                | VoiceEffect::ShowTranscript { .. }
                | VoiceEffect::PlayCue(CueKind::Listening)
        )),
        "{after:?}"
    );
    assert_eq!(v.session.status().phase, VoicePhase::Idle);
}

#[test]
fn the_window_ends_echo_window_ms_after_the_answer_ended() {
    let mut v = answered(speak_answers());
    for id in std::mem::take(&mut v.open) {
        v.send(VoiceEvent::SpeechFinished { utterance_id: id });
    }
    v.wait(ECHO_WINDOW_MS - 1);
    assert!(v.final_("Hey Jarvis, open Safari").is_empty());
    v.wait(1);
    let out = v.final_("Hey Jarvis, open Safari");
    assert_eq!(
        starts(&out),
        [("open Safari".to_owned(), TaskOrigin::Voice)]
    );
}

#[test]
fn a_stop_still_stops_an_answer_being_read() {
    let mut v = answered(speak_answers());
    let out = v.final_("stop");
    assert_eq!(out.first(), Some(&VoiceEffect::StopSpeaking), "{out:?}");
}

#[test]
fn push_to_talk_in_the_window_always_waits_for_a_touch() {
    let mut v = answered(VoiceSettings {
        confirm_push_to_talk: false,
        ..speak_answers()
    });
    v.send(VoiceEvent::PushToTalk { pressed: true });
    v.send(VoiceEvent::PushToTalk { pressed: false });
    v.wait(SPEAKING_TAIL_MS);
    let out = v.final_("open Safari");
    assert!(starts(&out).is_empty(), "{out:?}");
    assert_eq!(
        v.session.status().phase,
        VoicePhase::Confirming {
            text: "open Safari".into()
        }
    );
    let out = v.send(VoiceEvent::TranscriptConfirmed {
        text: "open Safari".into(),
    });
    assert_eq!(
        starts(&out),
        [("open Safari".to_owned(), TaskOrigin::Phone)]
    );
}

#[test]
fn a_fixed_phrase_opens_no_window() {
    let mut v = running(wake_on());
    v.task(TaskEvent::FinishedOk { answer: None });
    v.speech_done();
    let out = v.final_("Hey Jarvis, open Safari");
    assert_eq!(starts(&out).len(), 1, "{out:?}");
}

// ---- f3: the wake phrase said twice ---------------------------------------------------

#[test]
fn only_wake_phrases_are_the_wake_phrase_alone() {
    for t in [
        "Jarvis Jarvis",
        "Hey Jarvis, OK Jarvis",
        "hi jarvis jarvis jarvis",
    ] {
        let u = parse_utterance(t);
        assert!(u.wake && u.request_is_empty(), "{t:?}: {u:?}");
        let mut v = Voice::new(wake_on());
        v.final_(t);
        assert_eq!(v.session.status().phase, VoicePhase::Listening, "{t:?}");
        assert!(v.starts().is_empty());
    }
    // With a request after them, only the first wake phrase is stripped (the rest is the
    // request as said).
    let u = parse_utterance("Hey Jarvis, hey Jarvis open Mail");
    assert_eq!(u.request, "hey Jarvis open Mail");
}

// ---- f4: padding and the transcript bound ----------------------------------------------

#[test]
fn padding_of_any_whitespace_never_cuts_the_request() {
    let text = format!(
        "Hey Jarvis\t{}\n{}send the photos to Bob Smith",
        "\u{00A0}".repeat(1_500),
        " \n\t".repeat(1_000)
    );
    let u = parse_utterance(&text);
    assert!(!u.too_long);
    assert_eq!(u.request, "send the photos to Bob Smith");
}

#[test]
fn a_transcript_over_either_bound_is_refused_never_cut() {
    // Over the raw bound, though mostly blanks.
    let raw = format!("Hey Jarvis {}open Mail", " ".repeat(MAX_TRANSCRIPT_CHARS));
    assert!(parse_utterance(&raw).too_long);
    // Over the parse bound once collapsed.
    let words = format!("Hey Jarvis {}", "ab ".repeat(MAX_PARSE_CHARS));
    assert!(parse_utterance(&words).too_long);
    for t in [raw, words] {
        let mut v = Voice::new(wake_on());
        let out = v.final_(&t);
        assert!(starts(&out).is_empty());
        assert_eq!(said(&out), [TOO_LONG]);
    }
    // A confirmed text over the raw bound.
    let mut v = Voice::new(VoiceSettings::default());
    v.send(VoiceEvent::PushToTalk { pressed: true });
    v.send(VoiceEvent::PushToTalk { pressed: false });
    v.final_("open Mail");
    let out = v.send(VoiceEvent::TranscriptConfirmed {
        text: "x".repeat(MAX_TRANSCRIPT_CHARS + 1),
    });
    assert!(starts(&out).is_empty());
    assert_eq!(said(&out), [TOO_LONG]);
}

#[test]
fn a_stop_padded_past_the_bound_still_stops() {
    let mut v = running(wake_on());
    let out = v.final_(&format!("stop{}", " ".repeat(MAX_TRANSCRIPT_CHARS)));
    assert!(has(&out, &VoiceEffect::StopTask), "{out:?}");
}

// ---- f5: the wake word turned off ------------------------------------------------------

#[test]
fn turning_the_wake_word_off_discards_a_hands_free_exchange_on_the_recogniser() {
    let mut v = Voice::new(wake_on());
    v.final_("Hey Jarvis");
    v.partial("open Settings");
    let out = v.send(VoiceEvent::SettingsChanged(VoiceSettings::default()));
    assert!(has(&out, &VoiceEffect::StopRecognizer), "{out:?}");
    assert_eq!(v.session.status().phase, VoicePhase::Idle);
    v.final_("open Settings");
    v.wait(ENDPOINT_SILENCE_MS * 2);
    assert!(v.starts().is_empty());
}

#[test]
fn turning_the_wake_word_off_leaves_push_to_talk_alone() {
    let mut v = Voice::new(wake_on());
    v.send(VoiceEvent::PushToTalk { pressed: true });
    v.partial("open Settings");
    v.send(VoiceEvent::SettingsChanged(VoiceSettings::default()));
    assert_eq!(v.session.status().phase, VoicePhase::Listening);
    v.send(VoiceEvent::PushToTalk { pressed: false });
    v.final_("open Settings");
    assert_eq!(
        v.session.status().phase,
        VoicePhase::Confirming {
            text: "open Settings".into()
        }
    );
}

// ---- f6: barge-in after a failure -------------------------------------------------------

#[test]
fn barge_in_is_retried_after_every_failure_also_while_starting() {
    let mut v = Voice::new(wake_on());
    v.final_("Hey Jarvis, open Settings");
    for i in 0..3 {
        if i == 1 {
            v.task(TaskEvent::Started);
            v.speech_done();
        }
        assert!(v.session.status().recognizing);
        v.send(VoiceEvent::RecognizerFailed {
            reason: "no speech".into(),
        });
        assert!(!v.session.status().recognizing);
        assert!(!has(
            &v.wait(WAKE_RETRY_MS - 1),
            &VoiceEffect::StartRecognizer
        ));
        assert!(has(&v.wait(1), &VoiceEffect::StartRecognizer));
    }
    // A stop still goes through.
    assert!(has(&v.final_("cancel"), &VoiceEffect::StopTask));
}

// ---- f7 + f8: stop words at the endpoint over partials --------------------------------

#[test]
fn a_partial_stop_acts_only_after_the_endpoint_and_only_if_unchanged() {
    let mut v = running(wake_on());
    v.partial("stop");
    assert!(!has(
        &v.wait(ENDPOINT_SILENCE_MS - 1),
        &VoiceEffect::StopTask
    ));
    // It went on: "stop the music" is not the stop words.
    v.partial("stop the music");
    v.wait(ENDPOINT_SILENCE_MS * 2);
    assert_eq!(count(&v.log, &VoiceEffect::StopTask), 0);
    // Repeating the same partial does not restart the endpoint.
    let mut v = running(wake_on());
    v.partial("cancel");
    v.wait(ENDPOINT_SILENCE_MS / 2);
    v.partial("cancel");
    let out = v.wait(ENDPOINT_SILENCE_MS / 2);
    assert!(has(&out, &VoiceEffect::StopTask), "{out:?}");
}

#[test]
fn a_partial_request_during_the_task_starts_nothing() {
    let mut v = running(wake_on());
    v.partial("Hey Jarvis, delete my photos");
    v.wait(ENDPOINT_SILENCE_MS * 3);
    assert_eq!(v.starts().len(), 1);
    assert_eq!(count(&v.log, &VoiceEffect::StopTask), 0);
    assert!(!v.said().contains(&BUSY.to_owned()), "{:?}", v.said());
}

#[test]
fn a_bare_partial_stop_does_not_stop_a_task_started_elsewhere() {
    let mut v = Voice::new(wake_on());
    v.other(TaskEvent::Started);
    v.partial("stop");
    v.wait(ENDPOINT_SILENCE_MS * 2);
    assert_eq!(count(&v.log, &VoiceEffect::StopTask), 0);
    v.partial("Hey Jarvis, abort");
    v.wait(ENDPOINT_SILENCE_MS);
    assert_eq!(count(&v.log, &VoiceEffect::StopTask), 1);
    assert_eq!(said(&v.other(TaskEvent::Stopped)), [STOPPED]);
}

// ---- Answers and the front ------------------------------------------------------------

#[test]
fn an_answer_is_not_read_when_jarvis_is_not_in_front() {
    let mut v = running(speak_answers());
    v.send(VoiceEvent::ForegroundChanged { active: false });
    let out = v.task(TaskEvent::FinishedOk {
        answer: Some("Your next meeting is at 3 pm.".into()),
    });
    assert_eq!(said(&out), [DONE_ANSWER_ON_PHONE]);
}

#[test]
fn leaving_the_front_stops_an_answer_but_lets_a_fixed_phrase_finish() {
    let mut v = running(speak_answers());
    v.task(TaskEvent::FinishedOk {
        answer: Some("Your next meeting is at 3 pm.".into()),
    });
    let out = v.send(VoiceEvent::ForegroundChanged { active: false });
    assert_eq!(out.first(), Some(&VoiceEffect::StopSpeaking), "{out:?}");
    let mut v = Voice::new(wake_on());
    v.final_("Hey Jarvis, open Settings");
    v.task(TaskEvent::Started); // "On it." in flight
    let out = v.send(VoiceEvent::ForegroundChanged { active: false });
    assert!(!has(&out, &VoiceEffect::StopSpeaking), "{out:?}");
    assert!(v.session.status().speaking);
}

// ---- The confirm row ------------------------------------------------------------------

#[test]
fn the_row_loses_its_start_once_it_is_used() {
    let mut v = Voice::new(VoiceSettings::default());
    v.send(VoiceEvent::PushToTalk { pressed: true });
    v.send(VoiceEvent::PushToTalk { pressed: false });
    v.final_("open Settings");
    let out = v.send(VoiceEvent::TranscriptConfirmed {
        text: "open Settings".into(),
    });
    let show = out
        .iter()
        .position(|e| {
            *e == VoiceEffect::ShowTranscript {
                text: "open Settings".into(),
                is_final: true,
                needs_confirm: false,
                endpointed: false,
            }
        })
        .unwrap_or_else(|| panic!("{out:?}"));
    let start = out
        .iter()
        .position(|e| matches!(e, VoiceEffect::StartTask { .. }))
        .unwrap();
    assert!(show < start);
}
