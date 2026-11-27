//! A small driver for `VoiceSession` tests: a clock, the effects so far, and the shell's
//! usual replies (speech finishing).
#![allow(dead_code)]

use jarvis_proto::TaskOrigin;
use jarvis_voice::{
    SpeakAnswers, TaskEvent, UtteranceKind, VoiceEffect, VoiceEvent, VoiceSession, VoiceSettings,
    SPEAKING_TAIL_MS,
};

pub struct Voice {
    pub session: VoiceSession,
    pub now: u64,
    /// Every effect, in order.
    pub log: Vec<VoiceEffect>,
    /// Utterances spoken and not reported finished yet.
    pub open: Vec<u64>,
    /// The shell: the request id of the latest `StartTask` whose task has not been
    /// reported refused or ended yet (`task` passes it through, as the shell does).
    pub request: Option<u64>,
}

pub fn wake_on() -> VoiceSettings {
    VoiceSettings {
        wake_word: true,
        ..VoiceSettings::default()
    }
}

pub fn speak_answers() -> VoiceSettings {
    VoiceSettings {
        wake_word: true,
        speak_answers: SpeakAnswers::WhenUnlocked,
        ..VoiceSettings::default()
    }
}

impl Voice {
    pub fn new(settings: VoiceSettings) -> Self {
        let mut v = Self {
            session: VoiceSession::new(settings),
            now: 1_000,
            log: Vec::new(),
            open: Vec::new(),
            request: None,
        };
        v.send(VoiceEvent::Tick);
        v
    }

    /// Sends `event` at the current time and returns its effects.
    pub fn send(&mut self, event: VoiceEvent) -> Vec<VoiceEffect> {
        let out = self.session.step(self.now, event);
        for e in &out {
            if let VoiceEffect::Speak { utterance_id, .. } = e {
                self.open.push(*utterance_id);
            }
            if matches!(e, VoiceEffect::StopSpeaking) {
                self.open.clear();
            }
            if let VoiceEffect::StartTask { request, .. } = e {
                self.request = Some(*request);
            }
        }
        self.log.extend(out.iter().cloned());
        out
    }

    /// Advances the clock by `ms` and ticks.
    pub fn wait(&mut self, ms: u64) -> Vec<VoiceEffect> {
        self.now += ms;
        self.send(VoiceEvent::Tick)
    }

    pub fn partial(&mut self, text: &str) -> Vec<VoiceEffect> {
        self.send(VoiceEvent::PartialTranscript { text: text.into() })
    }

    pub fn final_(&mut self, text: &str) -> Vec<VoiceEffect> {
        self.send(VoiceEvent::FinalTranscript { text: text.into() })
    }

    /// An event of the task the shell started for the latest voice request (with its id),
    /// or, when there is none, of a task started elsewhere.
    pub fn task(&mut self, e: TaskEvent) -> Vec<VoiceEffect> {
        match self.request {
            Some(request) => {
                if matches!(
                    e,
                    TaskEvent::StartRefused { .. }
                        | TaskEvent::FinishedOk { .. }
                        | TaskEvent::Failed
                        | TaskEvent::Stopped
                ) {
                    self.request = None;
                }
                self.send(VoiceEvent::RequestTask { request, event: e })
            }
            None => self.other(e),
        }
    }

    /// An event of a task started elsewhere (no request id).
    pub fn other(&mut self, e: TaskEvent) -> Vec<VoiceEffect> {
        self.send(VoiceEvent::Task(e))
    }

    /// An event reported for request `request` (whatever the shell believes).
    pub fn for_request(&mut self, request: u64, e: TaskEvent) -> Vec<VoiceEffect> {
        self.send(VoiceEvent::RequestTask { request, event: e })
    }

    /// Every open utterance finishes, then the speaking tail passes.
    pub fn speech_done(&mut self) -> Vec<VoiceEffect> {
        let mut out = Vec::new();
        for id in std::mem::take(&mut self.open) {
            out.extend(self.send(VoiceEvent::SpeechFinished { utterance_id: id }));
        }
        out.extend(self.wait(SPEAKING_TAIL_MS));
        out
    }

    /// The texts spoken since the start.
    pub fn said(&self) -> Vec<String> {
        said(&self.log)
    }

    pub fn starts(&self) -> Vec<(String, TaskOrigin)> {
        starts(&self.log)
    }
}

pub fn said(effects: &[VoiceEffect]) -> Vec<String> {
    effects
        .iter()
        .filter_map(|e| match e {
            VoiceEffect::Speak { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect()
}

pub fn said_kinds(effects: &[VoiceEffect]) -> Vec<(String, UtteranceKind)> {
    effects
        .iter()
        .filter_map(|e| match e {
            VoiceEffect::Speak { text, kind, .. } => Some((text.clone(), *kind)),
            _ => None,
        })
        .collect()
}

pub fn starts(effects: &[VoiceEffect]) -> Vec<(String, TaskOrigin)> {
    effects
        .iter()
        .filter_map(|e| match e {
            VoiceEffect::StartTask { text, origin, .. } => Some((text.clone(), *origin)),
            _ => None,
        })
        .collect()
}

/// The request ids of the `StartTask`s in `effects`.
pub fn requests(effects: &[VoiceEffect]) -> Vec<u64> {
    effects
        .iter()
        .filter_map(|e| match e {
            VoiceEffect::StartTask { request, .. } => Some(*request),
            _ => None,
        })
        .collect()
}

pub fn has(effects: &[VoiceEffect], wanted: &VoiceEffect) -> bool {
    effects.contains(wanted)
}
