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

    pub fn task(&mut self, e: TaskEvent) -> Vec<VoiceEffect> {
        self.send(VoiceEvent::Task(e))
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
            VoiceEffect::StartTask { text, origin } => Some((text.clone(), *origin)),
            _ => None,
        })
        .collect()
}

pub fn has(effects: &[VoiceEffect], wanted: &VoiceEffect) -> bool {
    effects.contains(wanted)
}
