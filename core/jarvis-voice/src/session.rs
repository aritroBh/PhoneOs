//! [`VoiceSession`]: the voice state machine.
//!
//! ```text
//!            wake phrase (alone) / WakeDetected / push to talk
//!   Idle ───────────────────────────────────────────────▶ Listening
//!    ▲  ◀── no speech, too long, not caught, stop, busy ──┘   │ final transcript, or
//!    │                                                        │ the endpoint over the
//!    │        "Hey Jarvis, <request>" (one final)             │ last partial
//!    │  ─────────────────────────────────────────┐            ▼
//!    │                          push to talk ─▶ Confirming ── Start (touch) ─┐
//!    │                                          (transcript row)             │
//!    │                                                  hands-free ──────────┤
//!    │                                                                       ▼
//!    │◀──────────── refused / start timeout ───────────────────────────── Starting
//!    │                                                                       │ Started
//!    │◀──── finished / failed / stopped ──────────────────────────────── TaskRunning
//! ```
//!
//! Speaking overlays every phase: while an utterance is in flight, and for
//! [`SPEAKING_TAIL_MS`] after the last one ended, transcripts are ignored except the stop
//! words. Who listens is part of the state too: the wake word (`Mic::Wake`, armed only in
//! Idle, or in a Listening exchange that it opened), the recogniser (`Mic::Recognizer`,
//! one utterance at a time, restarted for barge-in while the voice-started task runs), or
//! nothing. The audio session is active exactly while something listens or speaks.

use jarvis_proto::TaskOrigin;

use crate::parse::{contained_in, contains_wake_word, parse_utterance, word_list, Utterance};
use crate::phrases::{
    answer_line, clean_spoken, APPROVAL_ON_PHONE, BUSY, COULD_NOT, DONE, DONE_ANSWER_ON_PHONE,
    NOT_CAUGHT, ON_IT, STOPPED, TOO_LONG,
};
use crate::types::{
    CueKind, SpeakAnswers, TaskEvent, UtteranceKind, VoiceEffect, VoiceEvent, VoicePhase,
    VoiceSettings, VoiceStatus,
};

/// Listening with no speech at all for this long ends the exchange ("I did not catch
/// that.").
pub const LISTEN_NO_SPEECH_MS: u64 = 8_000;
/// Silence after the last partial transcript for this long ends the utterance: the last
/// partial is taken as the request (marked `endpointed`). Push to talk: measured from the
/// release.
pub const ENDPOINT_SILENCE_MS: u64 = 1_500;
/// An utterance (from its first partial, or from the press for push to talk) longer than
/// this is refused ("That was too long."), never cut.
pub const MAX_UTTERANCE_MS: u64 = 20_000;
/// A request longer than this many characters is refused ("That was too long."), never
/// cut.
pub const MAX_REQUEST_CHARS: usize = 300;
/// After Jarvis stops speaking, transcripts are still ignored (except the stop words) for
/// this long: the room's echo of its own voice.
pub const SPEAKING_TAIL_MS: u64 = 400;
/// A transcript contained in a line Jarvis finished saying less than this long ago is
/// ignored (its own voice, heard late).
pub const ECHO_WINDOW_MS: u64 = 5_000;
/// An utterance with no `SpeechFinished` after this long counts as finished (a lost
/// callback must not mute the microphone for good).
pub const MAX_SPEAKING_MS: u64 = 60_000;
/// No `Started`/`StartRefused` this long after `StartTask`: the start counts as failed.
pub const START_TIMEOUT_MS: u64 = 10_000;
/// After the wake word failed, it is armed again no sooner than this.
pub const WAKE_RETRY_MS: u64 = 5_000;

/// Who listens now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mic {
    Off,
    Wake,
    Recognizer,
}

/// An open listening exchange.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Listen {
    /// Push to talk (else hands-free).
    ptt: bool,
    /// Push to talk: the button is still held.
    pressed: bool,
    /// When listening for this request began.
    since_ms: u64,
    /// When the first speech of the request was heard.
    speech_ms: Option<u64>,
    /// The latest partial transcript.
    last_partial: Option<String>,
    /// When the latest partial arrived.
    heard_ms: u64,
    /// Push to talk: when the button was released.
    released_ms: Option<u64>,
}

impl Listen {
    fn new(ptt: bool, now: u64) -> Self {
        Self {
            ptt,
            pressed: ptt,
            since_ms: now,
            speech_ms: None,
            last_partial: None,
            heard_ms: now,
            released_ms: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Phase {
    Idle,
    Listening(Listen),
    Confirming { text: String },
    Starting { since_ms: u64 },
    TaskRunning { question: bool },
}

/// A line being said.
#[derive(Debug, Clone, PartialEq, Eq)]
struct InFlight {
    id: u64,
    words: Vec<String>,
    kind: UtteranceKind,
    started_ms: u64,
}

/// The voice state machine. Create one per app run; feed it every [`VoiceEvent`] with the
/// shell's monotonic `now_ms` and carry out the effects in order.
#[derive(Debug, Clone)]
pub struct VoiceSession {
    settings: VoiceSettings,
    now: u64,
    locked: bool,
    foreground: bool,
    /// Between an interruption's begin and end.
    interrupted: bool,
    /// An interruption ended without "should resume": nothing is armed again until Jarvis
    /// comes to the front, the settings change or the mic button is pressed.
    resume_blocked: bool,
    phase: Phase,
    mic: Mic,
    audio_active: bool,
    speaking: Vec<InFlight>,
    /// Lines finished recently (their words, when they ended), for the echo check.
    recent: Vec<(Vec<String>, u64)>,
    /// Transcripts are ignored (but the stop words) until this time.
    quiet_until: u64,
    next_utterance: u64,
    /// A task started elsewhere runs.
    other_task: bool,
    /// Voice asked to stop the running task; "Stopped." is said when it has.
    stop_requested: bool,
    /// The recogniser failed during the voice-started task: no barge-in restart until the
    /// next exchange.
    barge_in_failed: bool,
    /// The wake word may not be armed before this time (it failed).
    wake_retry_at: u64,
}

/// Effects of one step.
#[derive(Default)]
struct Out(Vec<VoiceEffect>);

impl Out {
    fn push(&mut self, e: VoiceEffect) {
        self.0.push(e);
    }
}

impl Default for VoiceSession {
    fn default() -> Self {
        Self::new(VoiceSettings::default())
    }
}

impl VoiceSession {
    /// A session with `settings`, nothing listening, the phone unlocked and Jarvis in front
    /// (the shell reports otherwise with the first events). Call [`Self::step`] with
    /// [`VoiceEvent::Tick`] once to arm the wake word if the settings say so.
    pub fn new(settings: VoiceSettings) -> Self {
        Self {
            settings,
            now: 0,
            locked: false,
            foreground: true,
            interrupted: false,
            resume_blocked: false,
            phase: Phase::Idle,
            mic: Mic::Off,
            audio_active: false,
            speaking: Vec::new(),
            recent: Vec::new(),
            quiet_until: 0,
            next_utterance: 1,
            other_task: false,
            stop_requested: false,
            barge_in_failed: false,
            wake_retry_at: 0,
        }
    }

    /// The settings in use.
    pub fn settings(&self) -> VoiceSettings {
        self.settings
    }

    /// Where the session stands, for the UI.
    pub fn status(&self) -> VoiceStatus {
        VoiceStatus {
            phase: match &self.phase {
                Phase::Idle => VoicePhase::Idle,
                Phase::Listening(_) => VoicePhase::Listening,
                Phase::Confirming { text } => VoicePhase::Confirming { text: text.clone() },
                Phase::Starting { .. } => VoicePhase::Starting,
                Phase::TaskRunning { question } => VoicePhase::TaskRunning {
                    question_pending: *question,
                },
            },
            wake_armed: self.mic == Mic::Wake,
            recognizing: self.mic == Mic::Recognizer,
            speaking: !self.speaking.is_empty(),
            other_task_running: self.other_task,
            interrupted: self.interrupted,
        }
    }

    /// Feeds one event at `now_ms` (monotonic; a time earlier than one already seen counts
    /// as that one) and returns the effects to carry out, in order. Total: never panics.
    pub fn step(&mut self, now_ms: u64, event: VoiceEvent) -> Vec<VoiceEffect> {
        self.now = self.now.max(now_ms);
        let mut out = Out::default();
        self.expire();
        match event {
            VoiceEvent::WakeDetected => self.on_wake_detected(&mut out),
            VoiceEvent::PushToTalk { pressed } => self.on_push_to_talk(&mut out, pressed),
            VoiceEvent::PartialTranscript { text } => self.on_partial(&mut out, &text),
            VoiceEvent::FinalTranscript { text } => self.on_final(&mut out, &text),
            VoiceEvent::RecognizerFailed { .. } => self.on_recognizer_failed(&mut out),
            VoiceEvent::SpeechFinished { utterance_id } => self.on_speech_finished(utterance_id),
            VoiceEvent::Tick => self.on_tick(&mut out),
            VoiceEvent::Task(task) => self.on_task(&mut out, task),
            VoiceEvent::LockChanged { locked } => self.on_lock(&mut out, locked),
            VoiceEvent::ForegroundChanged { active } => self.on_foreground(&mut out, active),
            VoiceEvent::AudioInterrupted {
                began,
                should_resume,
            } => self.on_interruption(&mut out, began, should_resume),
            VoiceEvent::RouteChanged => self.stop_speaking(&mut out),
            VoiceEvent::SettingsChanged(settings) => self.on_settings(&mut out, settings),
            VoiceEvent::TranscriptConfirmed { text } => self.on_confirmed(&mut out, &text),
            VoiceEvent::TranscriptDiscarded => {
                if matches!(self.phase, Phase::Confirming { .. }) {
                    out.push(VoiceEffect::ClearTranscript);
                    self.phase = Phase::Idle;
                }
            }
        }
        self.reconcile(&mut out);
        out.0
    }

    // -----------------------------------------------------------------------------------
    // Listening.
    // -----------------------------------------------------------------------------------

    fn on_wake_detected(&mut self, out: &mut Out) {
        // Only the armed wake word reports it; a late report after disarming is stale.
        if self.mic != Mic::Wake || self.phase != Phase::Idle || self.quiet() {
            return;
        }
        self.listen_for_request(out, false);
    }

    fn on_push_to_talk(&mut self, out: &mut Out, pressed: bool) {
        if !pressed {
            if let Phase::Listening(l) = &mut self.phase {
                if l.ptt && l.pressed {
                    l.pressed = false;
                    l.released_ms = Some(self.now);
                    if self.mic == Mic::Recognizer {
                        out.push(VoiceEffect::FinishRecognizer);
                    }
                }
            }
            return;
        }
        if self.locked || self.interrupted || !self.foreground {
            return;
        }
        match self.phase {
            Phase::Idle | Phase::Confirming { .. } => {
                if matches!(self.phase, Phase::Confirming { .. }) {
                    out.push(VoiceEffect::ClearTranscript);
                }
                // A touch is the owner's own action: it may also clear a blocked resume.
                self.resume_blocked = false;
                self.stop_speaking(out);
                self.listen_for_request(out, true);
            }
            Phase::Listening(_) | Phase::Starting { .. } | Phase::TaskRunning { .. } => {}
        }
    }

    /// Opens (or reopens) Listening for a request on the recogniser, with the cue.
    fn listen_for_request(&mut self, out: &mut Out, ptt: bool) {
        if self.mic == Mic::Wake {
            out.push(VoiceEffect::DisarmWakeWord);
            self.mic = Mic::Off;
        }
        if self.mic == Mic::Recognizer {
            out.push(VoiceEffect::StopRecognizer);
        }
        out.push(VoiceEffect::StartRecognizer);
        self.mic = Mic::Recognizer;
        out.push(VoiceEffect::PlayCue(CueKind::Listening));
        self.phase = Phase::Listening(Listen::new(ptt, self.now));
    }

    fn on_partial(&mut self, out: &mut Out, text: &str) {
        if self.mic == Mic::Off || self.quiet() {
            return;
        }
        let heard = parse_utterance(text);
        if self.is_echo(&heard) {
            return;
        }
        let now = self.now;
        let blocked = self.other_task_blocks_listening();
        match &mut self.phase {
            Phase::Idle => {
                // The wake word's own stream: a wake phrase opens the exchange and the same
                // stream goes on carrying the request.
                if !heard.wake || blocked {
                    return;
                }
                let mut l = Listen::new(false, now);
                l.speech_ms = Some(now);
                l.last_partial = Some(text.to_owned());
                self.phase = Phase::Listening(l);
            }
            Phase::Listening(l) => {
                l.speech_ms.get_or_insert(now);
                l.last_partial = Some(text.to_owned());
                l.heard_ms = now;
            }
            Phase::Confirming { .. } | Phase::Starting { .. } | Phase::TaskRunning { .. } => {
                return;
            }
        }
        if heard.request.chars().count() > MAX_REQUEST_CHARS {
            self.end_exchange(out, Some(TOO_LONG));
        } else if !heard.request_is_empty() {
            out.push(show(&heard.request, false, false, false));
        }
    }

    fn on_final(&mut self, out: &mut Out, text: &str) {
        if self.mic == Mic::Off {
            return;
        }
        if self.mic == Mic::Recognizer {
            // One utterance per recognition: it has ended by itself.
            self.mic = Mic::Off;
        }
        let heard = parse_utterance(text);
        // The stop words come first: while Jarvis speaks, and even when they repeat what it
        // says (stopping is the safe direction).
        if heard.is_stop() && (heard.wake || self.exchange_open() || self.quiet()) {
            self.stop(out);
            return;
        }
        if self.quiet() || self.is_echo(&heard) {
            return;
        }
        self.on_request_heard(out, &heard, false);
    }

    /// A whole utterance (a final transcript, or the last partial at the endpoint).
    fn on_request_heard(&mut self, out: &mut Out, heard: &Utterance, endpointed: bool) {
        match &self.phase {
            Phase::Idle => {
                if !heard.wake {
                    return;
                }
                if heard.request_is_empty() {
                    if self.other_task_blocks_listening() {
                        self.say(out, BUSY, UtteranceKind::Status);
                    } else {
                        self.listen_for_request(out, false);
                    }
                } else {
                    self.handle_request(out, &heard.request, false, endpointed);
                }
            }
            Phase::Listening(l) => {
                let ptt = l.ptt;
                if heard.request_is_empty() {
                    if heard.wake && !ptt {
                        // "Hey Jarvis" and a pause: listen for the request itself.
                        self.listen_for_request(out, false);
                    } else {
                        self.end_exchange(out, Some(NOT_CAUGHT));
                    }
                } else {
                    self.handle_request(out, &heard.request, ptt, endpointed);
                }
            }
            Phase::Starting { .. } | Phase::TaskRunning { .. } => {
                // Rule 8, and rule 2: nothing heard while a task runs does anything but stop
                // it; a spoken "yes" to a pending question is ignored like any other words.
                if heard.wake && !heard.request_is_empty() {
                    self.say(out, BUSY, UtteranceKind::Status);
                }
            }
            Phase::Confirming { .. } => {}
        }
    }

    /// The request is heard in full: refuse it, show it for a touch, or start it.
    fn handle_request(&mut self, out: &mut Out, request: &str, ptt: bool, endpointed: bool) {
        if request.chars().count() > MAX_REQUEST_CHARS {
            self.end_exchange(out, Some(TOO_LONG));
            return;
        }
        if self.other_task {
            self.end_exchange(out, Some(BUSY));
            return;
        }
        self.stop_recognizer(out);
        if ptt && self.settings.confirm_push_to_talk {
            out.push(show(request, true, true, endpointed));
            self.phase = Phase::Confirming {
                text: request.to_owned(),
            };
            return;
        }
        out.push(show(request, true, false, endpointed));
        self.start_task(out, request, TaskOrigin::Voice);
    }

    fn start_task(&mut self, out: &mut Out, text: &str, origin: TaskOrigin) {
        out.push(VoiceEffect::StartTask {
            text: text.to_owned(),
            origin,
        });
        self.stop_requested = false;
        self.barge_in_failed = false;
        self.phase = Phase::Starting { since_ms: self.now };
    }

    fn on_confirmed(&mut self, out: &mut Out, text: &str) {
        if !matches!(self.phase, Phase::Confirming { .. }) {
            return;
        }
        let request = crate::parse::clean_request(text);
        if request.is_empty() {
            out.push(VoiceEffect::ClearTranscript);
            self.phase = Phase::Idle;
        } else if request.chars().count() > MAX_REQUEST_CHARS {
            self.end_exchange(out, Some(TOO_LONG));
        } else if self.other_task {
            self.end_exchange(out, Some(BUSY));
        } else {
            // Confirmed by touch: the owner's own words.
            self.start_task(out, &request, TaskOrigin::Phone);
        }
    }

    fn on_recognizer_failed(&mut self, out: &mut Out) {
        match self.mic {
            Mic::Off => {}
            Mic::Wake => {
                self.mic = Mic::Off;
                self.wake_retry_at = self.now.saturating_add(WAKE_RETRY_MS);
                if matches!(self.phase, Phase::Listening(_)) {
                    self.end_exchange(out, Some(NOT_CAUGHT));
                }
            }
            Mic::Recognizer => {
                self.mic = Mic::Off;
                match self.phase {
                    Phase::Listening(_) => self.end_exchange(out, Some(NOT_CAUGHT)),
                    Phase::TaskRunning { .. } | Phase::Starting { .. } => {
                        self.barge_in_failed = true;
                    }
                    Phase::Idle | Phase::Confirming { .. } => {}
                }
            }
        }
    }

    /// Stop words: whatever is open ends; a running task is asked to stop.
    fn stop(&mut self, out: &mut Out) {
        self.stop_speaking(out);
        match self.phase {
            Phase::Listening(_) => self.end_exchange(out, None),
            Phase::Confirming { .. } => {
                out.push(VoiceEffect::ClearTranscript);
                self.phase = Phase::Idle;
            }
            Phase::Starting { .. } | Phase::TaskRunning { .. } => {
                out.push(VoiceEffect::StopTask);
                self.stop_requested = true;
            }
            Phase::Idle => {
                if self.other_task {
                    out.push(VoiceEffect::StopTask);
                    self.stop_requested = true;
                }
            }
        }
    }

    /// Closes the exchange (back to Idle), saying `line` if given.
    fn end_exchange(&mut self, out: &mut Out, line: Option<&str>) {
        if matches!(self.phase, Phase::Listening(_)) {
            self.stop_recognizer(out);
            out.push(VoiceEffect::ClearTranscript);
            if line.is_none() && !self.interrupted {
                out.push(VoiceEffect::PlayCue(CueKind::Ended));
            }
        } else if matches!(self.phase, Phase::Confirming { .. }) {
            out.push(VoiceEffect::ClearTranscript);
        }
        self.phase = Phase::Idle;
        if let Some(line) = line {
            self.say(out, line, UtteranceKind::Status);
        }
    }

    fn stop_recognizer(&mut self, out: &mut Out) {
        if self.mic == Mic::Recognizer {
            out.push(VoiceEffect::StopRecognizer);
            self.mic = Mic::Off;
        }
    }

    // -----------------------------------------------------------------------------------
    // Time.
    // -----------------------------------------------------------------------------------

    /// Lines past [`MAX_SPEAKING_MS`] count as finished; old lines leave the echo memory.
    fn expire(&mut self) {
        let now = self.now;
        let (done, still): (Vec<InFlight>, Vec<InFlight>) = std::mem::take(&mut self.speaking)
            .into_iter()
            .partition(|u| now.saturating_sub(u.started_ms) >= MAX_SPEAKING_MS);
        self.speaking = still;
        if !done.is_empty() {
            for u in done {
                self.recent.push((u.words, now));
            }
            if self.speaking.is_empty() {
                self.quiet_until = now.saturating_add(SPEAKING_TAIL_MS);
            }
        }
        self.recent
            .retain(|(_, ended)| now.saturating_sub(*ended) < ECHO_WINDOW_MS);
    }

    fn on_tick(&mut self, out: &mut Out) {
        let now = self.now;
        match &self.phase {
            Phase::Listening(l) => {
                let l = l.clone();
                let since_speech = l.speech_ms.map(|s| now.saturating_sub(s));
                if since_speech.is_some_and(|d| d >= MAX_UTTERANCE_MS)
                    || (l.ptt && now.saturating_sub(l.since_ms) >= MAX_UTTERANCE_MS)
                {
                    self.end_exchange(out, Some(TOO_LONG));
                } else if l.ptt {
                    if let Some(released) = l.released_ms {
                        if now.saturating_sub(released) >= ENDPOINT_SILENCE_MS {
                            self.endpoint(out, l.last_partial.as_deref());
                        }
                    }
                } else if l.speech_ms.is_none() {
                    if now.saturating_sub(l.since_ms) >= LISTEN_NO_SPEECH_MS {
                        self.end_exchange(out, Some(NOT_CAUGHT));
                    }
                } else if now.saturating_sub(l.heard_ms) >= ENDPOINT_SILENCE_MS {
                    self.endpoint(out, l.last_partial.as_deref());
                }
            }
            Phase::Starting { since_ms } => {
                if now.saturating_sub(*since_ms) >= START_TIMEOUT_MS {
                    self.phase = Phase::Idle;
                    self.stop_requested = false;
                    self.say(out, COULD_NOT, UtteranceKind::Status);
                }
            }
            Phase::Idle | Phase::Confirming { .. } | Phase::TaskRunning { .. } => {}
        }
    }

    /// The endpoint: the last partial is the utterance.
    fn endpoint(&mut self, out: &mut Out, last_partial: Option<&str>) {
        let Some(text) = last_partial else {
            self.end_exchange(out, Some(NOT_CAUGHT));
            return;
        };
        let heard = parse_utterance(text);
        if heard.is_stop() {
            self.stop(out);
            return;
        }
        self.on_request_heard(out, &heard, true);
    }

    // -----------------------------------------------------------------------------------
    // The task.
    // -----------------------------------------------------------------------------------

    fn on_task(&mut self, out: &mut Out, event: TaskEvent) {
        let ours = matches!(
            self.phase,
            Phase::Starting { .. } | Phase::TaskRunning { .. }
        );
        match event {
            TaskEvent::Started => match self.phase {
                Phase::Starting { .. } => {
                    self.phase = Phase::TaskRunning { question: false };
                    if self.stop_requested {
                        out.push(VoiceEffect::StopTask);
                    } else {
                        self.say(out, ON_IT, UtteranceKind::Status);
                    }
                }
                Phase::TaskRunning { .. } => {}
                Phase::Idle | Phase::Listening(_) | Phase::Confirming { .. } => {
                    self.other_task = true;
                    self.stop_requested = false;
                }
            },
            TaskEvent::StartRefused { busy } => {
                if matches!(self.phase, Phase::Starting { .. }) {
                    self.phase = Phase::Idle;
                    self.stop_requested = false;
                    self.say(
                        out,
                        if busy { BUSY } else { COULD_NOT },
                        UtteranceKind::Status,
                    );
                }
            }
            TaskEvent::WaitingForApproval => {
                if let Phase::TaskRunning { question } = &mut self.phase {
                    if !*question {
                        *question = true;
                        if !self.stop_requested {
                            self.say(out, APPROVAL_ON_PHONE, UtteranceKind::Status);
                        }
                    }
                }
            }
            TaskEvent::ApprovalAnswered => {
                if let Phase::TaskRunning { question } = &mut self.phase {
                    *question = false;
                }
            }
            TaskEvent::FinishedOk { answer } => {
                if ours {
                    self.task_ended(out);
                    self.say_finished(out, answer.as_deref());
                } else {
                    self.other_ended();
                }
            }
            TaskEvent::Failed => {
                if ours {
                    self.task_ended(out);
                    self.say(out, COULD_NOT, UtteranceKind::Status);
                } else {
                    self.other_ended();
                }
            }
            TaskEvent::Stopped => {
                if ours {
                    self.task_ended(out);
                    self.say(out, STOPPED, UtteranceKind::Status);
                } else {
                    let asked = self.stop_requested;
                    self.other_ended();
                    if asked {
                        self.say(out, STOPPED, UtteranceKind::Status);
                    }
                }
            }
        }
    }

    fn task_ended(&mut self, out: &mut Out) {
        self.stop_recognizer(out);
        self.phase = Phase::Idle;
        self.stop_requested = false;
        self.barge_in_failed = false;
        self.other_task = false;
    }

    fn other_ended(&mut self) {
        self.other_task = false;
        self.stop_requested = false;
    }

    /// "Done." and, when the setting and the lock allow it, the answer.
    fn say_finished(&mut self, out: &mut Out, answer: Option<&str>) {
        let answer = answer.map(clean_spoken).filter(|a| !a.is_empty());
        let Some(answer) = answer else {
            self.say(out, DONE, UtteranceKind::Status);
            return;
        };
        if self.locked {
            self.say(out, DONE_ANSWER_ON_PHONE, UtteranceKind::Status);
            return;
        }
        match self.settings.speak_answers {
            SpeakAnswers::Never => self.say(out, DONE, UtteranceKind::Status),
            SpeakAnswers::WhenUnlocked => match answer_line(&answer) {
                Some(line) => self.say(out, &line, UtteranceKind::Answer),
                None => self.say(out, DONE_ANSWER_ON_PHONE, UtteranceKind::Status),
            },
        }
    }

    // -----------------------------------------------------------------------------------
    // Speaking.
    // -----------------------------------------------------------------------------------

    fn say(&mut self, out: &mut Out, text: &str, kind: UtteranceKind) {
        // Not over a phone call, and never a line that holds the wake word (every line is
        // composed here, so this only guards against a mistake).
        if self.interrupted || contains_wake_word(text) {
            return;
        }
        let id = self.next_utterance;
        self.next_utterance = self.next_utterance.saturating_add(1);
        self.speaking.push(InFlight {
            id,
            words: word_list(text),
            kind,
            started_ms: self.now,
        });
        out.push(VoiceEffect::Speak {
            utterance_id: id,
            text: text.to_owned(),
            kind,
        });
    }

    fn on_speech_finished(&mut self, id: u64) {
        let Some(pos) = self.speaking.iter().position(|u| u.id == id) else {
            return;
        };
        let u = self.speaking.remove(pos);
        self.recent.push((u.words, self.now));
        if self.speaking.is_empty() {
            self.quiet_until = self.now.saturating_add(SPEAKING_TAIL_MS);
        }
    }

    fn stop_speaking(&mut self, out: &mut Out) {
        if self.speaking.is_empty() {
            return;
        }
        out.push(VoiceEffect::StopSpeaking);
        let now = self.now;
        for u in std::mem::take(&mut self.speaking) {
            self.recent.push((u.words, now));
        }
        self.quiet_until = now.saturating_add(SPEAKING_TAIL_MS);
    }

    /// Jarvis speaks, or just stopped.
    fn quiet(&self) -> bool {
        !self.speaking.is_empty() || self.now < self.quiet_until
    }

    /// The transcript is (part of) a line Jarvis is saying or just said.
    fn is_echo(&self, heard: &Utterance) -> bool {
        self.speaking
            .iter()
            .map(|u| &u.words)
            .chain(self.recent.iter().map(|(w, _)| w))
            .any(|spoken| contained_in(&heard.words, spoken))
    }

    // -----------------------------------------------------------------------------------
    // The phone and the audio session.
    // -----------------------------------------------------------------------------------

    fn on_lock(&mut self, out: &mut Out, locked: bool) {
        self.locked = locked;
        if !locked {
            return;
        }
        // Never an answer while locked, not even one that began before.
        if self
            .speaking
            .iter()
            .any(|u| u.kind == UtteranceKind::Answer)
        {
            self.stop_speaking(out);
        }
        if matches!(self.phase, Phase::Listening(_)) {
            self.end_exchange(out, None);
        }
    }

    fn on_foreground(&mut self, out: &mut Out, active: bool) {
        self.foreground = active;
        if active {
            self.resume_blocked = false;
            return;
        }
        if matches!(self.phase, Phase::Listening(_)) {
            self.end_exchange(out, None);
        }
        self.stop_recognizer(out);
    }

    fn on_interruption(&mut self, out: &mut Out, began: bool, should_resume: bool) {
        if !began {
            self.interrupted = false;
            if !should_resume {
                self.resume_blocked = true;
            }
            return;
        }
        self.interrupted = true;
        // A Listening exchange cut by an interruption is discarded, never started later.
        if matches!(self.phase, Phase::Listening(_)) {
            self.end_exchange(out, None);
        }
        self.stop_recognizer(out);
        if self.mic == Mic::Wake {
            out.push(VoiceEffect::DisarmWakeWord);
            self.mic = Mic::Off;
        }
        self.stop_speaking(out);
        // The system deactivated the session.
        self.audio_active = false;
    }

    fn on_settings(&mut self, out: &mut Out, settings: VoiceSettings) {
        self.settings = settings;
        self.resume_blocked = false;
        if settings.speak_answers == SpeakAnswers::Never
            && self
                .speaking
                .iter()
                .any(|u| u.kind == UtteranceKind::Answer)
        {
            self.stop_speaking(out);
        }
    }

    /// The microphone may run at all.
    fn can_listen(&self) -> bool {
        self.foreground && !self.locked && !self.interrupted && !self.resume_blocked
    }

    /// While a task started elsewhere runs, the wake phrase alone does not open an exchange
    /// on the wake word's stream (only "Jarvis, stop" in one utterance acts).
    fn other_task_blocks_listening(&self) -> bool {
        self.other_task
    }

    fn exchange_open(&self) -> bool {
        self.phase != Phase::Idle
    }

    /// Brings the wake word, the barge-in recogniser and the audio session in line with the
    /// state. The activation goes first, the deactivation last.
    fn reconcile(&mut self, out: &mut Out) {
        let wake_wanted = self.settings.wake_word
            && self.can_listen()
            && self.now >= self.wake_retry_at
            && match &self.phase {
                Phase::Idle => true,
                // An exchange the wake word's own stream opened goes on on that stream.
                Phase::Listening(l) => !l.ptt && self.mic == Mic::Wake,
                Phase::Confirming { .. } | Phase::Starting { .. } | Phase::TaskRunning { .. } => {
                    false
                }
            };
        if self.mic == Mic::Wake && !wake_wanted {
            out.push(VoiceEffect::DisarmWakeWord);
            self.mic = Mic::Off;
        }
        if self.mic == Mic::Recognizer && !self.can_listen() {
            out.push(VoiceEffect::StopRecognizer);
            self.mic = Mic::Off;
        }
        if self.mic == Mic::Off {
            if wake_wanted && self.phase == Phase::Idle {
                out.push(VoiceEffect::ArmWakeWord);
                self.mic = Mic::Wake;
            } else if matches!(
                self.phase,
                Phase::Starting { .. } | Phase::TaskRunning { .. }
            ) && self.can_listen()
                && !self.barge_in_failed
            {
                // Barge-in while the voice-started task starts and runs: only the stop
                // words act.
                out.push(VoiceEffect::StartRecognizer);
                self.mic = Mic::Recognizer;
            }
        }
        let audio_wanted = self.mic != Mic::Off || !self.speaking.is_empty();
        if audio_wanted && !self.audio_active && !self.interrupted {
            out.0.insert(0, VoiceEffect::ActivateAudioSession);
            self.audio_active = true;
        } else if !audio_wanted && self.audio_active {
            out.push(VoiceEffect::DeactivateAudioSession);
            self.audio_active = false;
        }
    }
}

fn show(text: &str, is_final: bool, needs_confirm: bool, endpointed: bool) -> VoiceEffect {
    VoiceEffect::ShowTranscript {
        text: text.to_owned(),
        is_final,
        needs_confirm,
        endpointed,
    }
}
