#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! The second voice break-it fixes, each with variants the review did not try: the stop
//! grammar read on words in a stop context (f1-f4), one stop acting once, words read before
//! a task not read as its stop, a start with an id never issued (the review's open item),
//! and the covered (inactive) scene.

mod common;

use common::*;
use jarvis_proto::TaskOrigin;
use jarvis_voice::phrases::{BUSY, DONE_ANSWER_ON_PHONE, ON_IT, STOPPED};
use jarvis_voice::{
    only_stop_words, parse_utterance, wake_then_stop, AppScene, TaskEvent, UtteranceKind,
    VoiceEffect, VoiceEvent, VoicePhase, VoiceSettings, ENDPOINT_SILENCE_MS, SPEAKING_TAIL_MS,
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
    assert!(v.session.status().recognizing);
    v
}

/// Another task runs (typed on Ask) and the wake word's stream listens.
fn other_running() -> Voice {
    let mut v = Voice::new(wake_on());
    v.other(TaskEvent::Started);
    assert!(v.session.status().other_task_running && v.session.status().wake_armed);
    v
}

fn count(effects: &[VoiceEffect], wanted: &VoiceEffect) -> usize {
    effects.iter().filter(|e| *e == wanted).count()
}

fn scene(v: &mut Voice, scene: AppScene) -> Vec<VoiceEffect> {
    v.send(VoiceEvent::SceneChanged { scene })
}

fn words(t: &str) -> Vec<String> {
    parse_utterance(t).words
}

// ---- The grammar on words -----------------------------------------------------------

#[test]
fn the_grammar_reads_words() {
    for t in [
        "Jarvis, stop",
        "the news. Hey Jarvis, cancel and more",
        "OK JARVÍS: never mind",
        "jarvis jarvis stop",
    ] {
        assert!(wake_then_stop(&words(t)).is_some(), "{t}");
    }
    for t in [
        "stop",
        "Stop, Jarvis!",
        "and then I told him to stop",
        "Jarvis, please stop",
        "jarvisstop",
    ] {
        assert!(wake_then_stop(&words(t)).is_none(), "{t}");
    }
    for t in [
        "Stop, stop!",
        "Stop, Jarvis!",
        "Hey Jarvis, never mind. Abort.",
    ] {
        assert!(only_stop_words(&words(t)), "{t}");
    }
    for t in [
        "Jarvis",
        "",
        "stop it",
        "hey stop",
        "never",
        "stop the music",
    ] {
        assert!(!only_stop_words(&words(t)), "{t}");
    }
}

// ---- f1: repeated and trailing-wake stop forms ----------------------------------------

#[test]
fn stop_forms_made_only_of_stop_and_wake_words_stop_while_starting_too() {
    for t in [
        "Abort, abort, Jarvis",
        "never mind, never mind",
        "Hey Jarvis, stop. Stop!",
    ] {
        let mut v = Voice::new(wake_on());
        v.final_("Hey Jarvis, open Settings");
        assert_eq!(v.session.status().phase, VoicePhase::Starting);
        let out = v.final_(t);
        assert!(has(&out, &VoiceEffect::StopTask), "{t}: {out:?}");
        // The start that follows is stopped at once and says "Stopped." when it has.
        let out = v.task(TaskEvent::Started);
        assert!(has(&out, &VoiceEffect::StopTask), "{t}: {out:?}");
        assert!(!said(&out).contains(&ON_IT.to_owned()), "{t}");
        assert_eq!(said(&v.task(TaskEvent::Stopped)), [STOPPED], "{t}");
    }
}

#[test]
fn a_repeated_stop_without_the_wake_phrase_does_not_stop_another_task() {
    // A bare stop is not allowed for a task started elsewhere (Jarvis silent), repeated or
    // not, with the wake phrase after it or not.
    for t in ["Stop, stop!", "Stop, Jarvis!", "Cancel, cancel"] {
        let mut v = other_running();
        assert!(!has(&v.final_(t), &VoiceEffect::StopTask), "{t}");
    }
}

// ---- f2 + f3: the wake phrase and a stop phrase among other speech ---------------------

#[test]
fn jarvis_stop_among_the_rooms_speech_stops_at_once_from_a_partial() {
    for t in [
        "the forecast for tomorrow. Hey Jarvis, cancel. Rain in the north",
        "OK Jarvis abort the weather tonight is mild",
        "and so on and so forth jarvis never mind",
    ] {
        let mut v = running(wake_on());
        v.partial("the forecast for tomorrow");
        let out = v.partial(t);
        assert!(has(&out, &VoiceEffect::StopTask), "{t}: {out:?}");
        let mut v = other_running();
        let out = v.partial(t);
        assert_eq!(out, [VoiceEffect::StopTask], "{t}");
        assert_eq!(said(&v.other(TaskEvent::Stopped)), [STOPPED], "{t}");
    }
}

#[test]
fn jarvis_stop_the_music_stops_a_running_task_and_is_a_request_only_in_idle() {
    let mut v = running(wake_on());
    let out = v.final_("Hey Jarvis, stop the music");
    assert!(has(&out, &VoiceEffect::StopTask), "{out:?}");
    assert!(!said(&out).contains(&BUSY.to_owned()));
    let mut v = other_running();
    assert_eq!(
        v.final_("Hey Jarvis, stop the music"),
        [VoiceEffect::StopTask]
    );
    // Nothing runs: a request, as before.
    let mut v = Voice::new(wake_on());
    v.final_("Hey Jarvis, stop the music");
    assert_eq!(
        v.starts(),
        [("stop the music".to_owned(), TaskOrigin::Voice)]
    );
}

#[test]
fn a_bare_stop_inside_other_speech_never_stops() {
    for t in [
        "and then I told him to stop",
        "stop the music",
        "please do not cancel my meeting",
        "Jarvis is a film, stop",
    ] {
        let mut v = running(wake_on());
        v.partial(t);
        v.wait(ENDPOINT_SILENCE_MS * 2);
        v.final_(t);
        v.wait(ENDPOINT_SILENCE_MS * 2);
        assert_eq!(count(&v.log, &VoiceEffect::StopTask), 0, "{t}: {:?}", v.log);
        let mut v = other_running();
        v.partial(t);
        v.wait(ENDPOINT_SILENCE_MS * 2);
        v.final_(t);
        assert_eq!(count(&v.log, &VoiceEffect::StopTask), 0, "{t}: {:?}", v.log);
    }
}

// ---- f4: the endpoint over words ----------------------------------------------------

#[test]
fn a_partial_stop_flapping_in_case_and_punctuation_stops_at_the_endpoint() {
    let mut v = running(wake_on());
    let at = v.now;
    for (i, t) in ["Cancel", "cancel.", "CANCEL!", "Cancel"]
        .iter()
        .cycle()
        .take(8)
        .enumerate()
    {
        v.partial(t);
        v.wait(400);
        if v.log.contains(&VoiceEffect::StopTask) {
            assert!(v.now - at >= ENDPOINT_SILENCE_MS, "step {i}: too early");
            break;
        }
    }
    assert_eq!(count(&v.log, &VoiceEffect::StopTask), 1, "{:?}", v.log);
}

#[test]
fn jarvis_stop_alone_as_a_partial_waits_for_the_endpoint_and_other_words_act_at_once() {
    // Only stop and wake words: the endpoint (the room is quiet, it comes).
    let mut v = other_running();
    assert!(v.partial("Jarvis, stop").is_empty());
    assert!(!has(
        &v.wait(ENDPOINT_SILENCE_MS - 1),
        &VoiceEffect::StopTask
    ));
    assert!(has(&v.wait(1), &VoiceEffect::StopTask));
    // The room goes on talking: at the next partial, with no wait.
    let mut v = other_running();
    v.partial("Jarvis, stop");
    v.wait(300);
    assert!(has(&v.partial("Jarvis, stop the"), &VoiceEffect::StopTask));
}

// ---- One stop acts once ---------------------------------------------------------------

#[test]
fn one_stop_acts_once_on_later_partials_and_the_final_of_the_same_recognition() {
    let mut v = running(wake_on());
    v.partial("music playing. Jarvis, stop");
    for t in [
        "music playing. Jarvis, stop",
        "music playing. Jarvis, stop and the band",
        "music playing. Jarvis, stop and the band plays on",
    ] {
        v.partial(t);
        v.wait(ENDPOINT_SILENCE_MS);
    }
    v.final_("music playing. Jarvis, stop and the band plays on");
    assert_eq!(count(&v.log, &VoiceEffect::StopTask), 1, "{:?}", v.log);
    // A bare stop at the endpoint, then the same words for seconds and the final.
    let mut v = running(wake_on());
    v.partial("stop");
    v.wait(ENDPOINT_SILENCE_MS);
    for _ in 0..4 {
        v.partial("Stop.");
        v.wait(ENDPOINT_SILENCE_MS);
    }
    v.final_("Stop.");
    assert_eq!(count(&v.log, &VoiceEffect::StopTask), 1, "{:?}", v.log);
}

#[test]
fn a_new_stop_after_the_read_words_still_acts() {
    let mut v = other_running();
    v.partial("news. Jarvis, stop");
    assert_eq!(count(&v.log, &VoiceEffect::StopTask), 1);
    // The stop did not take (the task goes on): the owner says it again, in the same
    // recognition.
    let out = v.partial("news. Jarvis, stop. more news. Jarvis, stop now");
    assert!(has(&out, &VoiceEffect::StopTask), "{out:?}");
    assert_eq!(count(&v.log, &VoiceEffect::StopTask), 2);
}

#[test]
fn words_heard_before_a_task_started_are_not_its_stop() {
    let mut v = Voice::new(wake_on());
    // Nothing runs: the wake phrase counts at the start only, so this opens nothing.
    v.partial("I said Jarvis stop it");
    assert_eq!(v.session.status().phase, VoicePhase::Idle);
    v.other(TaskEvent::Started);
    v.partial("I said Jarvis stop it now");
    v.wait(ENDPOINT_SILENCE_MS * 2);
    assert_eq!(count(&v.log, &VoiceEffect::StopTask), 0, "{:?}", v.log);
    // New words after the start do.
    let out = v.partial("I said Jarvis stop it now. Jarvis, stop please");
    assert!(has(&out, &VoiceEffect::StopTask), "{out:?}");
    // And a new recognition starts from nothing.
    v.final_("I said Jarvis stop it now. Jarvis, stop please");
    v.other(TaskEvent::Stopped);
    v.other(TaskEvent::Started);
    v.speech_done();
    assert_eq!(v.final_("Jarvis, stop"), [VoiceEffect::StopTask]);
}

#[test]
fn the_stop_read_for_one_task_is_not_read_again_for_the_next() {
    let mut v = other_running();
    v.partial("Jarvis, stop");
    v.wait(ENDPOINT_SILENCE_MS);
    assert_eq!(count(&v.log, &VoiceEffect::StopTask), 1);
    v.other(TaskEvent::Stopped);
    v.speech_done();
    // The wake word's stream goes on (no final yet); the next task starts.
    v.other(TaskEvent::Started);
    v.partial("Jarvis, stop");
    v.wait(ENDPOINT_SILENCE_MS * 2);
    v.partial("Jarvis, stop and so on");
    assert_eq!(count(&v.log, &VoiceEffect::StopTask), 1, "{:?}", v.log);
}

// ---- A start with an id never issued (the review's open item) --------------------------

#[test]
fn a_start_with_an_id_never_issued_does_not_outlive_the_real_start() {
    let mut v = Voice::new(wake_on());
    v.final_("Hey Jarvis, open Settings");
    v.for_request(99, TaskEvent::Started);
    assert!(v.session.status().other_task_running);
    let out = v.task(TaskEvent::Started);
    assert!(said(&out).contains(&ON_IT.to_owned()), "{out:?}");
    let status = v.session.status();
    assert!(!status.other_task_running, "{status:?}");
    assert!(status.recognizing, "barge-in does not listen: {status:?}");
    v.speech_done();
    // The voice task's own barge-in: a bare stop stops it.
    assert!(has(&v.final_("stop"), &VoiceEffect::StopTask));
    assert_eq!(said(&v.task(TaskEvent::Stopped)), [STOPPED]);
    // A late end of the never-issued id changes nothing.
    assert!(v.for_request(99, TaskEvent::Stopped).is_empty());
    assert_eq!(v.session.status().phase, VoicePhase::Idle);
    assert!(!v.session.status().other_task_running);
}

// ---- Covered (inactive) -----------------------------------------------------------------

#[test]
fn while_covered_the_wake_phrase_gets_no_exchange_and_nothing_is_said() {
    let mut v = Voice::new(wake_on());
    scene(&mut v, AppScene::Inactive);
    assert!(v.session.status().wake_armed, "the microphone stays on");
    for t in [
        "Hey Jarvis, open Mail",
        "Hey Jarvis",
        "Hey Jarvis, delete my photos",
    ] {
        assert!(v.final_(t).is_empty(), "{t}");
        assert!(v.partial(t).is_empty(), "{t}");
        v.wait(ENDPOINT_SILENCE_MS * 2);
        assert_eq!(v.session.status().phase, VoicePhase::Idle, "{t}");
    }
    assert!(v.send(VoiceEvent::WakeDetected).is_empty());
    assert!(v.send(VoiceEvent::PushToTalk { pressed: true }).is_empty());
    assert!(v.starts().is_empty() && v.said().is_empty(), "{:?}", v.log);
    // Active again: a request starts.
    scene(&mut v, AppScene::Active);
    v.final_("Hey Jarvis, open Mail");
    assert_eq!(v.starts(), [("open Mail".to_owned(), TaskOrigin::Voice)]);
}

#[test]
fn while_covered_the_stop_words_still_stop_and_busy_is_not_said() {
    let mut v = running(wake_on());
    scene(&mut v, AppScene::Inactive);
    assert!(v.session.status().recognizing, "barge-in goes on");
    let out = v.final_("Hey Jarvis, open Mail");
    assert!(said(&out).is_empty() && starts(&out).is_empty(), "{out:?}");
    assert!(has(&v.final_("stop"), &VoiceEffect::StopTask));
    let mut v = other_running();
    scene(&mut v, AppScene::Inactive);
    assert!(v.final_("Hey Jarvis, open Mail").is_empty());
    assert_eq!(v.final_("Jarvis, stop"), [VoiceEffect::StopTask]);
}

#[test]
fn covering_ends_a_hands_free_exchange_but_not_push_to_talk() {
    let mut v = Voice::new(wake_on());
    v.final_("Hey Jarvis");
    assert_eq!(v.session.status().phase, VoicePhase::Listening);
    scene(&mut v, AppScene::Inactive);
    assert_eq!(v.session.status().phase, VoicePhase::Idle);
    v.partial("open Mail");
    v.wait(ENDPOINT_SILENCE_MS * 2);
    assert!(v.starts().is_empty());
    // Push to talk opened while active goes on to its row, even with confirmation off; a
    // confirmation while covered starts nothing; once active the touch starts it.
    let mut v = Voice::new(VoiceSettings {
        confirm_push_to_talk: false,
        ..VoiceSettings::default()
    });
    v.send(VoiceEvent::PushToTalk { pressed: true });
    v.partial("open Mail");
    scene(&mut v, AppScene::Inactive);
    assert_eq!(v.session.status().phase, VoicePhase::Listening);
    v.send(VoiceEvent::PushToTalk { pressed: false });
    v.final_("open Mail");
    assert_eq!(
        v.session.status().phase,
        VoicePhase::Confirming {
            text: "open Mail".into()
        }
    );
    assert!(v
        .send(VoiceEvent::TranscriptConfirmed {
            text: "open Mail".into()
        })
        .is_empty());
    assert!(v.starts().is_empty());
    scene(&mut v, AppScene::Active);
    v.send(VoiceEvent::TranscriptConfirmed {
        text: "open Mail".into(),
    });
    assert_eq!(v.starts(), [("open Mail".to_owned(), TaskOrigin::Phone)]);
}

#[test]
fn answers_are_read_only_while_active() {
    let mut v = running(speak_answers());
    scene(&mut v, AppScene::Inactive);
    let out = v.task(TaskEvent::FinishedOk {
        answer: Some("Your next meeting is at 3 pm.".into()),
    });
    assert_eq!(said(&out), [DONE_ANSWER_ON_PHONE]);
    // Covered while an answer is read: it stops.
    let mut v = running(speak_answers());
    v.task(TaskEvent::FinishedOk {
        answer: Some("Your next meeting is at 3 pm.".into()),
    });
    assert!(said_kinds(&v.log)
        .iter()
        .any(|(_, k)| *k == UtteranceKind::Answer));
    let out = scene(&mut v, AppScene::Inactive);
    assert_eq!(out.first(), Some(&VoiceEffect::StopSpeaking), "{out:?}");
}

#[test]
fn the_background_is_what_leaving_the_front_was() {
    let mut v = running(wake_on());
    let out = scene(&mut v, AppScene::Background);
    assert!(has(&out, &VoiceEffect::StopRecognizer), "{out:?}");
    assert!(!v.session.status().recognizing);
    let out = scene(&mut v, AppScene::Active);
    assert!(has(&out, &VoiceEffect::StartRecognizer), "{out:?}");
    v.wait(SPEAKING_TAIL_MS);
}
