#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! `JarvisVoice` and the voice task origin across the FFI.

use jarvis_core_ffi::{
    default_voice_settings, voice_origin_to_proto, voice_timings, JarvisError, JarvisVoice,
    TaskSpec, VoiceEffect, VoiceEvent, VoicePhase, VoiceSettings, VoiceSpeakAnswers,
    VoiceTaskEvent, VoiceTaskOrigin,
};
use jarvis_proto as p;

fn wake_on() -> VoiceSettings {
    VoiceSettings {
        wake_word: true,
        ..default_voice_settings()
    }
}

#[test]
fn defaults_and_timings_match_the_core() {
    let d = default_voice_settings();
    assert!(!d.wake_word);
    assert_eq!(d.speak_answers, VoiceSpeakAnswers::Never);
    assert!(d.confirm_push_to_talk);
    let t = voice_timings();
    assert_eq!(t.speaking_tail_ms, jarvis_voice::SPEAKING_TAIL_MS);
    assert_eq!(t.endpoint_silence_ms, jarvis_voice::ENDPOINT_SILENCE_MS);
}

#[test]
fn a_hands_free_request_starts_as_voice_and_a_spoken_yes_does_nothing() {
    let voice = JarvisVoice::new(wake_on());
    let out = voice.step(10, VoiceEvent::Tick).unwrap();
    assert_eq!(
        out,
        [VoiceEffect::ActivateAudioSession, VoiceEffect::ArmWakeWord]
    );
    let out = voice
        .step(
            20,
            VoiceEvent::FinalTranscript {
                text: "Hey Jarvis, open Settings".into(),
            },
        )
        .unwrap();
    let request = out
        .iter()
        .find_map(|e| match e {
            VoiceEffect::StartTask {
                text,
                origin: VoiceTaskOrigin::Voice,
                request,
            } if text == "open Settings" => Some(*request),
            _ => None,
        })
        .unwrap_or_else(|| panic!("{out:?}"));
    voice
        .step(
            30,
            VoiceEvent::RequestTask {
                request,
                event: VoiceTaskEvent::Started,
            },
        )
        .unwrap();
    voice
        .step(
            40,
            VoiceEvent::RequestTask {
                request,
                event: VoiceTaskEvent::WaitingForApproval,
            },
        )
        .unwrap();
    let out = voice
        .step(5_000, VoiceEvent::FinalTranscript { text: "yes".into() })
        .unwrap();
    assert!(
        !out.iter()
            .any(|e| matches!(e, VoiceEffect::StartTask { .. } | VoiceEffect::StopTask)),
        "{out:?}"
    );
    assert_eq!(
        voice.status().unwrap().phase,
        VoicePhase::TaskRunning {
            question_pending: true
        }
    );
}

#[test]
fn the_task_spec_carries_the_voice_origin() {
    let spec = TaskSpec {
        id: "t".into(),
        text: "open Settings".into(),
        remote: false,
        voice: true,
    };
    let core: p::TaskSpec = spec.clone().into();
    assert_eq!(core.origin, p::TaskOrigin::Voice);
    assert_eq!(TaskSpec::from(core), spec);
    let both = TaskSpec {
        remote: true,
        ..spec
    };
    let core: p::TaskSpec = both.into();
    assert_eq!(core.origin, p::TaskOrigin::Remote);
    assert_eq!(
        voice_origin_to_proto(VoiceTaskOrigin::Phone),
        p::TaskOrigin::Phone
    );
    assert_eq!(
        voice_origin_to_proto(VoiceTaskOrigin::Voice),
        p::TaskOrigin::Voice
    );
}

#[cfg(feature = "panic-hooks")]
#[test]
fn a_panic_poisons_the_voice_object() {
    let voice = JarvisVoice::new(default_voice_settings());
    voice.debug_panic_next_step();
    assert!(matches!(
        voice.step(1, VoiceEvent::Tick),
        Err(JarvisError::Internal { .. })
    ));
    assert!(matches!(
        voice.step(2, VoiceEvent::Tick),
        Err(JarvisError::Poisoned { .. })
    ));
    assert!(matches!(voice.status(), Err(JarvisError::Poisoned { .. })));
}

/// The request id crosses the FFI both ways: only a start reported with the id of the
/// `StartTask` is the voice task; a start without it is another task's, and a late start
/// of the request after it was answered is stopped.
#[test]
fn the_request_id_crosses_the_ffi_both_ways() {
    let voice = JarvisVoice::new(wake_on());
    voice.step(10, VoiceEvent::Tick).unwrap();
    let out = voice
        .step(
            20,
            VoiceEvent::FinalTranscript {
                text: "Hey Jarvis, open Settings".into(),
            },
        )
        .unwrap();
    let request = out
        .iter()
        .find_map(|e| match e {
            VoiceEffect::StartTask { request, .. } => Some(*request),
            _ => None,
        })
        .unwrap();
    let out = voice
        .step(
            30,
            VoiceEvent::Task {
                event: VoiceTaskEvent::Started,
            },
        )
        .unwrap();
    assert!(
        !out.iter().any(|e| matches!(e, VoiceEffect::Speak { .. })),
        "{out:?}"
    );
    assert_eq!(voice.status().unwrap().phase, VoicePhase::Starting);
    assert!(voice.status().unwrap().other_task_running);
    voice
        .step(
            40,
            VoiceEvent::RequestTask {
                request,
                event: VoiceTaskEvent::StartRefused { busy: true },
            },
        )
        .unwrap();
    assert_eq!(voice.status().unwrap().phase, VoicePhase::Idle);
    let late = voice
        .step(
            50,
            VoiceEvent::RequestTask {
                request,
                event: VoiceTaskEvent::Started,
            },
        )
        .unwrap();
    assert!(late.contains(&VoiceEffect::StopTask), "{late:?}");
    assert_eq!(voice.status().unwrap().phase, VoicePhase::Idle);
}
