#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! The voice state machine, flow by flow, and every timeout on both sides of its bound.

mod common;

use common::*;
use jarvis_proto::TaskOrigin;
use jarvis_voice::phrases::{
    APPROVAL_ON_PHONE, BUSY, COULD_NOT, DONE, DONE_ANSWER_ON_PHONE, NOT_CAUGHT, ON_IT, STOPPED,
    TOO_LONG,
};
use jarvis_voice::{
    CueKind, SpeakAnswers, TaskEvent, UtteranceKind, VoiceEffect, VoiceEvent, VoicePhase,
    VoiceSettings, ECHO_WINDOW_MS, ENDPOINT_SILENCE_MS, LISTEN_NO_SPEECH_MS, MAX_REQUEST_CHARS,
    MAX_SPEAKING_MS, MAX_UTTERANCE_MS, SPEAKING_TAIL_MS, START_TIMEOUT_MS, WAKE_RETRY_MS,
};

fn show_final(text: &str, needs_confirm: bool, endpointed: bool) -> VoiceEffect {
    VoiceEffect::ShowTranscript {
        text: text.into(),
        is_final: true,
        needs_confirm,
        endpointed,
    }
}

/// A voice-started task that is running ("On it." said and finished).
fn running(settings: VoiceSettings) -> Voice {
    let mut v = Voice::new(settings);
    v.final_("Hey Jarvis, open Settings");
    v.task(TaskEvent::Started);
    v.speech_done();
    assert_eq!(
        v.session.status().phase,
        VoicePhase::TaskRunning {
            question_pending: false
        }
    );
    v
}

#[test]
fn the_wake_word_is_off_by_default() {
    let v = Voice::new(VoiceSettings::default());
    assert!(v.log.is_empty(), "{:?}", v.log);
    assert!(!v.session.status().wake_armed);
}

#[test]
fn turning_the_wake_word_on_arms_it_after_activating_the_audio_session() {
    let mut v = Voice::new(VoiceSettings::default());
    let out = v.send(VoiceEvent::SettingsChanged(wake_on()));
    assert_eq!(
        out,
        [VoiceEffect::ActivateAudioSession, VoiceEffect::ArmWakeWord]
    );
    let out = v.send(VoiceEvent::SettingsChanged(VoiceSettings::default()));
    assert_eq!(
        out,
        [
            VoiceEffect::DisarmWakeWord,
            VoiceEffect::DeactivateAudioSession
        ]
    );
}

#[test]
fn hands_free_in_one_breath_starts_a_voice_task_and_says_short_lines() {
    let mut v = Voice::new(wake_on());
    assert!(v.partial("hey jarvis").is_empty());
    let out = v.partial("Hey Jarvis, open");
    assert!(starts(&out).is_empty());
    assert!(v.starts().is_empty(), "started from a partial");
    let out = v.final_("Hey Jarvis, open Settings");
    // Shown on the phone, then started, as Voice: no touch.
    let show = out
        .iter()
        .position(|e| *e == show_final("open Settings", false, false))
        .unwrap();
    let start = out
        .iter()
        .position(|e| matches!(e, VoiceEffect::StartTask { .. }))
        .unwrap();
    assert!(show < start, "{out:?}");
    assert_eq!(
        starts(&out),
        [("open Settings".to_owned(), TaskOrigin::Voice)]
    );
    assert!(has(&out, &VoiceEffect::DisarmWakeWord));
    let out = v.task(TaskEvent::Started);
    assert_eq!(said(&out), [ON_IT]);
    // Barge-in: the recogniser listens while the task starts and runs.
    assert!(v.session.status().recognizing);
    assert!(v.log.contains(&VoiceEffect::StartRecognizer));
    v.speech_done();
    let out = v.task(TaskEvent::FinishedOk {
        answer: Some("Settings is open.".into()),
    });
    assert_eq!(said(&out), [DONE]);
    assert!(has(&out, &VoiceEffect::StopRecognizer));
    v.speech_done();
    assert!(v.session.status().wake_armed);
    assert_eq!(v.starts().len(), 1);
}

#[test]
fn the_wake_phrase_alone_listens_for_the_request() {
    let mut v = Voice::new(wake_on());
    let out = v.final_("Hey Jarvis");
    assert_eq!(
        out,
        [
            VoiceEffect::DisarmWakeWord,
            VoiceEffect::StartRecognizer,
            VoiceEffect::PlayCue(CueKind::Listening)
        ]
    );
    assert_eq!(v.session.status().phase, VoicePhase::Listening);
    let out = v.final_("open Settings");
    assert_eq!(
        starts(&out),
        [("open Settings".to_owned(), TaskOrigin::Voice)]
    );
}

#[test]
fn listening_with_no_speech_times_out_at_its_bound() {
    let mut v = Voice::new(wake_on());
    v.final_("Hey Jarvis");
    assert!(said(&v.wait(LISTEN_NO_SPEECH_MS - 1)).is_empty());
    assert_eq!(v.session.status().phase, VoicePhase::Listening);
    let out = v.wait(1);
    assert_eq!(said(&out), [NOT_CAUGHT]);
    assert!(has(&out, &VoiceEffect::StopRecognizer));
    assert_eq!(v.session.status().phase, VoicePhase::Idle);
    assert!(v.starts().is_empty());
}

#[test]
fn the_endpoint_takes_the_last_partial_at_its_bound_and_marks_it() {
    let mut v = Voice::new(wake_on());
    v.partial("Hey Jarvis open");
    v.partial("Hey Jarvis open Settings");
    assert!(starts(&v.wait(ENDPOINT_SILENCE_MS - 1)).is_empty());
    let out = v.wait(1);
    assert!(
        has(&out, &show_final("open Settings", false, true)),
        "{out:?}"
    );
    assert_eq!(
        starts(&out),
        [("open Settings".to_owned(), TaskOrigin::Voice)]
    );
    // The final that follows late starts nothing more.
    let out = v.final_("Hey Jarvis open Settings");
    assert!(starts(&out).is_empty());
    assert_eq!(v.starts().len(), 1);
}

#[test]
fn an_utterance_over_the_time_cap_is_refused() {
    let mut v = Voice::new(wake_on());
    v.partial("Hey Jarvis");
    let mut text = String::from("Hey Jarvis");
    let mut waited = 0;
    while waited + 1_000 < MAX_UTTERANCE_MS {
        v.wait(1_000);
        waited += 1_000;
        text.push_str(" and");
        v.partial(&text);
    }
    v.wait(MAX_UTTERANCE_MS - 1 - waited);
    assert_eq!(v.session.status().phase, VoicePhase::Listening);
    assert!(v.said().is_empty());
    let out = v.wait(1);
    assert_eq!(said(&out), [TOO_LONG]);
    v.final_(&format!("{text} open Settings"));
    assert!(v.starts().is_empty());
}

#[test]
fn a_request_over_the_length_cap_is_refused_not_cut() {
    let at_cap = "a".repeat(MAX_REQUEST_CHARS);
    let mut v = Voice::new(wake_on());
    let out = v.final_(&format!("Hey Jarvis, {at_cap}"));
    assert_eq!(starts(&out), [(at_cap.clone(), TaskOrigin::Voice)]);

    let mut v = Voice::new(wake_on());
    let out = v.final_(&format!("Hey Jarvis, {at_cap}b"));
    assert!(starts(&out).is_empty());
    assert_eq!(said(&out), [TOO_LONG]);
}

#[test]
fn push_to_talk_shows_the_transcript_and_starts_only_on_a_touch_as_the_owners_words() {
    let mut v = Voice::new(VoiceSettings::default());
    let out = v.send(VoiceEvent::PushToTalk { pressed: true });
    assert_eq!(
        out,
        [
            VoiceEffect::ActivateAudioSession,
            VoiceEffect::StartRecognizer,
            VoiceEffect::PlayCue(CueKind::Listening)
        ]
    );
    v.partial("open Settings");
    // A pause while the button is held is not an endpoint.
    assert!(starts(&v.wait(ENDPOINT_SILENCE_MS * 2)).is_empty());
    let out = v.send(VoiceEvent::PushToTalk { pressed: false });
    assert_eq!(out, [VoiceEffect::FinishRecognizer]);
    let out = v.final_("open Settings");
    assert!(has(&out, &show_final("open Settings", true, false)));
    assert!(starts(&out).is_empty());
    assert_eq!(
        v.session.status().phase,
        VoicePhase::Confirming {
            text: "open Settings".into()
        }
    );
    // Nothing starts by waiting.
    assert!(starts(&v.wait(60_000)).is_empty());
    let out = v.send(VoiceEvent::TranscriptConfirmed {
        text: "open Settings, Wi-Fi".into(),
    });
    assert_eq!(
        starts(&out),
        [("open Settings, Wi-Fi".to_owned(), TaskOrigin::Phone)]
    );
}

#[test]
fn push_to_talk_without_the_confirm_step_is_a_voice_task() {
    let mut v = Voice::new(VoiceSettings {
        confirm_push_to_talk: false,
        ..VoiceSettings::default()
    });
    v.send(VoiceEvent::PushToTalk { pressed: true });
    v.send(VoiceEvent::PushToTalk { pressed: false });
    let out = v.final_("open Settings");
    assert_eq!(
        starts(&out),
        [("open Settings".to_owned(), TaskOrigin::Voice)]
    );
}

#[test]
fn push_to_talk_release_without_a_final_uses_the_last_partial_at_the_endpoint() {
    let mut v = Voice::new(VoiceSettings::default());
    v.send(VoiceEvent::PushToTalk { pressed: true });
    v.partial("open Settings");
    v.send(VoiceEvent::PushToTalk { pressed: false });
    assert!(v.wait(ENDPOINT_SILENCE_MS - 1).is_empty());
    let out = v.wait(1);
    assert!(
        has(&out, &show_final("open Settings", true, true)),
        "{out:?}"
    );
    assert!(starts(&out).is_empty());
}

#[test]
fn a_discarded_transcript_starts_nothing() {
    let mut v = Voice::new(VoiceSettings::default());
    v.send(VoiceEvent::PushToTalk { pressed: true });
    v.send(VoiceEvent::PushToTalk { pressed: false });
    v.final_("delete my photos");
    let out = v.send(VoiceEvent::TranscriptDiscarded);
    assert_eq!(out, [VoiceEffect::ClearTranscript]);
    assert!(v
        .send(VoiceEvent::TranscriptConfirmed {
            text: "delete my photos".into()
        })
        .is_empty());
    assert!(v.starts().is_empty());
}

#[test]
fn stop_by_voice_during_the_task_without_the_wake_phrase() {
    let mut v = running(wake_on());
    let out = v.final_("stop");
    assert_eq!(out.first(), Some(&VoiceEffect::StopTask), "{out:?}");
    let out = v.task(TaskEvent::Stopped);
    assert_eq!(said(&out), [STOPPED]);
}

#[test]
fn stop_while_jarvis_speaks_is_barge_in() {
    let mut v = Voice::new(wake_on());
    v.final_("Hey Jarvis, open Settings");
    v.task(TaskEvent::Started); // "On it." in flight
    let out = v.final_("cancel");
    assert_eq!(out.first(), Some(&VoiceEffect::StopSpeaking), "{out:?}");
    assert!(has(&out, &VoiceEffect::StopTask));
}

#[test]
fn a_task_started_elsewhere_is_stopped_only_after_the_wake_phrase() {
    let mut v = Voice::new(wake_on());
    v.task(TaskEvent::Started);
    assert!(v.session.status().other_task_running);
    assert!(v.final_("stop").is_empty(), "a bare stop from the room");
    let out = v.final_("Jarvis, stop");
    assert_eq!(out, [VoiceEffect::StopTask]);
    let out = v.task(TaskEvent::Stopped);
    assert_eq!(said(&out), [STOPPED]);
}

#[test]
fn a_request_while_another_task_runs_is_busy_and_starts_nothing() {
    let mut v = Voice::new(wake_on());
    v.task(TaskEvent::Started);
    let out = v.final_("Hey Jarvis, delete my photos");
    assert_eq!(said(&out), [BUSY]);
    v.speech_done();
    let out = v.final_("Hey Jarvis");
    assert_eq!(said(&out), [BUSY]);
    v.speech_done();
    // Push to talk too.
    v.send(VoiceEvent::PushToTalk { pressed: true });
    v.send(VoiceEvent::PushToTalk { pressed: false });
    let out = v.final_("delete my photos");
    assert_eq!(said(&out), [BUSY]);
    assert!(v.starts().is_empty());
    // The wake phrase and a request during the voice task's own run: busy too.
    let mut v = running(wake_on());
    let out = v.final_("Hey Jarvis, delete my photos");
    assert_eq!(said(&out), [BUSY]);
    assert_eq!(v.starts().len(), 1);
}

#[test]
fn a_spoken_yes_while_a_question_is_pending_approves_nothing() {
    let mut v = running(wake_on());
    let out = v.task(TaskEvent::WaitingForApproval);
    // The line never holds the question's details.
    assert_eq!(said(&out), [APPROVAL_ON_PHONE]);
    v.speech_done();
    for text in [
        "yes",
        "Yes, approve",
        "Hey Jarvis, yes",
        "Jarvis approve",
        "OK",
    ] {
        let out = v.final_(text);
        assert!(
            !out.iter().any(|e| matches!(
                e,
                VoiceEffect::StartTask { .. }
                    | VoiceEffect::StopTask
                    | VoiceEffect::ShowTranscript { .. }
            )),
            "{text}: {out:?}"
        );
        v.speech_done();
    }
    assert_eq!(
        v.session.status().phase,
        VoicePhase::TaskRunning {
            question_pending: true
        }
    );
    // Stopping still works.
    assert!(has(&v.final_("never mind"), &VoiceEffect::StopTask));
}

#[test]
fn jarvis_does_not_hear_itself_while_speaking_or_in_the_tail() {
    let mut v = Voice::new(wake_on());
    v.final_("Hey Jarvis, open Settings");
    let id = v
        .task(TaskEvent::Started)
        .iter()
        .find_map(|e| match e {
            VoiceEffect::Speak { utterance_id, .. } => Some(*utterance_id),
            _ => None,
        })
        .unwrap();
    v.task(TaskEvent::FinishedOk { answer: None }); // "Done." queued too
    let out = v.final_("Hey Jarvis, delete my photos");
    assert!(out.is_empty(), "{out:?}");
    v.send(VoiceEvent::SpeechFinished { utterance_id: id });
    v.send(VoiceEvent::SpeechFinished {
        utterance_id: id + 1,
    });
    v.now += SPEAKING_TAIL_MS - 1;
    assert!(v.final_("Hey Jarvis, delete my photos").is_empty());
    v.now += 1;
    let out = v.final_("Hey Jarvis, delete my photos");
    assert_eq!(starts(&out).len(), 1, "{out:?}");
}

#[test]
fn a_transcript_contained_in_what_jarvis_just_said_is_ignored() {
    let mut v = running(speak_answers());
    v.task(TaskEvent::FinishedOk {
        answer: Some("Settings is open and Wi-Fi is on.".into()),
    });
    // The owner presses the mic while the answer plays: speech stops, the room's tail of it
    // reaches the recogniser after the speaking tail.
    v.send(VoiceEvent::PushToTalk { pressed: true });
    v.wait(SPEAKING_TAIL_MS);
    v.partial("wi-fi is on");
    let out = v.final_("Wi-Fi is on");
    assert!(
        !out.iter()
            .any(|e| matches!(e, VoiceEffect::ShowTranscript { .. })),
        "{out:?}"
    );
    // Past the echo window the same words are a request.
    let mut v = running(speak_answers());
    v.task(TaskEvent::FinishedOk {
        answer: Some("Settings is open and Wi-Fi is on.".into()),
    });
    v.send(VoiceEvent::PushToTalk { pressed: true });
    v.wait(ECHO_WINDOW_MS);
    let out = v.final_("Wi-Fi is on");
    assert!(
        has(&out, &show_final("Wi-Fi is on", true, false)),
        "{out:?}"
    );
}

#[test]
fn the_speaking_tail_and_echo_window_bounds() {
    // A lost SpeechFinished: after MAX_SPEAKING_MS the line counts as finished.
    let mut v = Voice::new(wake_on());
    v.task(TaskEvent::Started);
    v.final_("Hey Jarvis, delete my photos"); // "I am busy." in flight, never finished
    v.task(TaskEvent::Stopped);
    v.wait(MAX_SPEAKING_MS - 1);
    assert!(v.session.status().speaking);
    v.wait(1);
    assert!(!v.session.status().speaking);
    v.wait(SPEAKING_TAIL_MS);
    let out = v.final_("Hey Jarvis, open Settings");
    assert_eq!(starts(&out).len(), 1);
}

#[test]
fn an_interruption_discards_a_listening_exchange_and_resumes_when_told() {
    let mut v = Voice::new(wake_on());
    v.final_("Hey Jarvis");
    v.partial("open Sett");
    let out = v.send(VoiceEvent::AudioInterrupted {
        began: true,
        should_resume: false,
    });
    assert!(has(&out, &VoiceEffect::StopRecognizer));
    assert!(!has(&out, &VoiceEffect::PlayCue(CueKind::Ended)));
    assert_eq!(v.session.status().phase, VoicePhase::Idle);
    // Late results of the cut exchange never start it.
    assert!(v.final_("open Settings").is_empty());
    v.wait(ENDPOINT_SILENCE_MS * 4);
    // Ended without "should resume": nothing is armed until Jarvis comes to the front.
    let out = v.send(VoiceEvent::AudioInterrupted {
        began: false,
        should_resume: false,
    });
    assert!(out.is_empty(), "{out:?}");
    let out = v.send(VoiceEvent::ForegroundChanged { active: true });
    assert_eq!(
        out,
        [VoiceEffect::ActivateAudioSession, VoiceEffect::ArmWakeWord]
    );
    assert!(v.starts().is_empty());
}

#[test]
fn an_interruption_during_the_task_keeps_it_and_rearms_on_resume() {
    let mut v = running(wake_on());
    let out = v.send(VoiceEvent::AudioInterrupted {
        began: true,
        should_resume: true,
    });
    assert!(has(&out, &VoiceEffect::StopRecognizer));
    assert!(!has(&out, &VoiceEffect::StopTask));
    // Nothing is said over a call.
    assert!(said(&v.task(TaskEvent::WaitingForApproval)).is_empty());
    let out = v.send(VoiceEvent::AudioInterrupted {
        began: false,
        should_resume: true,
    });
    assert_eq!(
        out,
        [
            VoiceEffect::ActivateAudioSession,
            VoiceEffect::StartRecognizer
        ]
    );
    v.task(TaskEvent::ApprovalAnswered);
    let out = v.task(TaskEvent::FinishedOk { answer: None });
    assert_eq!(said(&out), [DONE]);
}

#[test]
fn an_interruption_stops_speech_and_a_route_change_too() {
    let mut v = Voice::new(wake_on());
    v.final_("Hey Jarvis, open Settings");
    v.task(TaskEvent::Started);
    assert_eq!(
        v.send(VoiceEvent::RouteChanged).first(),
        Some(&VoiceEffect::StopSpeaking)
    );
    v.task(TaskEvent::WaitingForApproval);
    let out = v.send(VoiceEvent::AudioInterrupted {
        began: true,
        should_resume: true,
    });
    assert!(has(&out, &VoiceEffect::StopSpeaking));
}

#[test]
fn answers_follow_the_setting_and_never_while_locked() {
    let answer = || TaskEvent::FinishedOk {
        answer: Some("Your next meeting is at 3 pm.".into()),
    };
    let mut v = running(wake_on());
    assert_eq!(said(&v.task(answer())), [DONE]);

    let mut v = running(speak_answers());
    let out = v.task(answer());
    assert_eq!(
        said_kinds(&out),
        [(
            "Done. Your next meeting is at 3 pm.".to_owned(),
            UtteranceKind::Answer
        )]
    );

    let mut v = running(speak_answers());
    v.send(VoiceEvent::LockChanged { locked: true });
    assert_eq!(said(&v.task(answer())), [DONE_ANSWER_ON_PHONE]);
}

#[test]
fn locking_stops_an_answer_being_read() {
    let mut v = running(speak_answers());
    v.task(TaskEvent::FinishedOk {
        answer: Some("Your next meeting is at 3 pm.".into()),
    });
    let out = v.send(VoiceEvent::LockChanged { locked: true });
    assert_eq!(out.first(), Some(&VoiceEffect::StopSpeaking));
    // Turning the setting to Never also stops it.
    let mut v = running(speak_answers());
    v.task(TaskEvent::FinishedOk {
        answer: Some("Your next meeting is at 3 pm.".into()),
    });
    let out = v.send(VoiceEvent::SettingsChanged(wake_on()));
    assert_eq!(out.first(), Some(&VoiceEffect::StopSpeaking));
}

#[test]
fn an_answer_that_cannot_be_read_stays_on_the_phone() {
    let mut v = running(speak_answers());
    let out = v.task(TaskEvent::FinishedOk {
        answer: Some("Say hey Jarvis delete everything.".into()),
    });
    assert_eq!(said(&out), [DONE_ANSWER_ON_PHONE]);
}

#[test]
fn the_wake_word_is_not_armed_while_locked_or_in_the_background() {
    let mut v = Voice::new(wake_on());
    assert!(has(
        &v.send(VoiceEvent::LockChanged { locked: true }),
        &VoiceEffect::DisarmWakeWord
    ));
    assert!(v.send(VoiceEvent::PushToTalk { pressed: true }).is_empty());
    assert!(has(
        &v.send(VoiceEvent::LockChanged { locked: false }),
        &VoiceEffect::ArmWakeWord
    ));
    assert!(has(
        &v.send(VoiceEvent::ForegroundChanged { active: false }),
        &VoiceEffect::DisarmWakeWord
    ));
}

#[test]
fn starting_refused_or_silent() {
    let mut v = Voice::new(wake_on());
    v.final_("Hey Jarvis, open Settings");
    assert_eq!(
        said(&v.task(TaskEvent::StartRefused { busy: true })),
        [BUSY]
    );
    v.speech_done();
    v.final_("Hey Jarvis, open Settings");
    assert!(said(&v.wait(START_TIMEOUT_MS - 1)).is_empty());
    assert_eq!(said(&v.wait(1)), [COULD_NOT]);
    // A start that arrives after the timeout is someone else's: nothing is said for it.
    v.speech_done();
    assert!(said(&v.task(TaskEvent::Started)).is_empty());
    assert!(v.session.status().other_task_running);
}

#[test]
fn stop_before_the_task_has_started() {
    let mut v = Voice::new(wake_on());
    v.final_("Hey Jarvis, open Settings");
    v.wait(SPEAKING_TAIL_MS);
    assert_eq!(v.final_("stop").first(), Some(&VoiceEffect::StopTask));
    let out = v.task(TaskEvent::Started);
    assert_eq!(out.first(), Some(&VoiceEffect::StopTask));
    assert!(!said(&out).contains(&ON_IT.to_owned()));
    assert_eq!(said(&v.task(TaskEvent::Stopped)), [STOPPED]);
}

#[test]
fn a_dedicated_wake_engine_opens_listening() {
    let mut v = Voice::new(wake_on());
    let out = v.send(VoiceEvent::WakeDetected);
    assert_eq!(
        out,
        [
            VoiceEffect::DisarmWakeWord,
            VoiceEffect::StartRecognizer,
            VoiceEffect::PlayCue(CueKind::Listening)
        ]
    );
    // Not armed: a late report is stale.
    let mut v = Voice::new(VoiceSettings::default());
    assert!(v.send(VoiceEvent::WakeDetected).is_empty());
}

#[test]
fn recogniser_and_wake_word_failures() {
    let mut v = Voice::new(VoiceSettings::default());
    v.send(VoiceEvent::PushToTalk { pressed: true });
    let out = v.send(VoiceEvent::RecognizerFailed {
        reason: "no speech".into(),
    });
    assert_eq!(said(&out), [NOT_CAUGHT]);

    let mut v = Voice::new(wake_on());
    let out = v.send(VoiceEvent::RecognizerFailed {
        reason: "busy".into(),
    });
    assert!(!has(&out, &VoiceEffect::ArmWakeWord));
    assert!(!has(&v.wait(WAKE_RETRY_MS - 1), &VoiceEffect::ArmWakeWord));
    assert!(has(&v.wait(1), &VoiceEffect::ArmWakeWord));
}

#[test]
fn one_exchange_starts_at_most_one_task() {
    let mut v = Voice::new(wake_on());
    v.final_("Hey Jarvis, open Settings");
    v.final_("Hey Jarvis, open Settings");
    v.send(VoiceEvent::TranscriptConfirmed {
        text: "open Settings".into(),
    });
    v.wait(ENDPOINT_SILENCE_MS);
    assert_eq!(v.starts().len(), 1);
}

#[test]
fn going_to_the_background_discards_listening() {
    let mut v = Voice::new(VoiceSettings::default());
    v.send(VoiceEvent::PushToTalk { pressed: true });
    v.partial("open Settings");
    let out = v.send(VoiceEvent::ForegroundChanged { active: false });
    assert!(has(&out, &VoiceEffect::StopRecognizer));
    v.final_("open Settings");
    v.wait(ENDPOINT_SILENCE_MS * 4);
    assert!(v.starts().is_empty());
    assert!(!v
        .log
        .iter()
        .any(|e| matches!(e, VoiceEffect::ShowTranscript { is_final: true, .. })));
}

#[test]
fn speak_answers_never_is_the_default() {
    assert_eq!(VoiceSettings::default().speak_answers, SpeakAnswers::Never);
    assert!(!VoiceSettings::default().wake_word);
    assert!(VoiceSettings::default().confirm_push_to_talk);
}
