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
//!    │                                                       Started(its request id)
//!    │◀──── finished / failed / stopped ──────────────────────────────── TaskRunning
//! ```
//!
//! Speaking overlays every phase: while an utterance is in flight, and for
//! [`SPEAKING_TAIL_MS`] after the last one ended, transcripts are ignored except the stop
//! words; while a model-written answer is read and for [`ECHO_WINDOW_MS`] after it ended,
//! heard text can only stop. Who listens is part of the state too: the wake word
//! (`Mic::Wake`, armed only in Idle, or in a Listening exchange that it opened), the
//! recogniser (`Mic::Recognizer`, one utterance at a time, restarted for barge-in while the
//! voice-started task runs), or nothing. The audio session is active exactly while
//! something listens or speaks.
//!
//! Every `StartTask` carries a fresh request id; only a `Started` reported with that id
//! ([`VoiceEvent::RequestTask`]) is the voice task. Any other start is a task started
//! elsewhere, and a late start of an earlier request (after the start timeout) is stopped.

use jarvis_proto::TaskOrigin;

use crate::parse::{
    clean_request, contained_in, contains_wake_word, only_stop_words, parse_utterance,
    wake_then_stop, word_list, Utterance, MAX_TRANSCRIPT_CHARS,
};
use crate::phrases::{
    answer_line, clean_spoken, APPROVAL_ON_PHONE, BUSY, COULD_NOT, DONE, DONE_ANSWER_ON_PHONE,
    NOT_CAUGHT, ON_IT, STOPPED, TOO_LONG,
};
use crate::types::{
    AppScene, CueKind, SpeakAnswers, TaskEvent, UtteranceKind, VoiceEffect, VoiceEvent, VoicePhase,
    VoiceSettings, VoiceStatus,
};

/// Listening with no speech at all for this long ends the exchange ("I did not catch
/// that.").
pub const LISTEN_NO_SPEECH_MS: u64 = 8_000;
/// Silence after the last partial transcript for this long ends the utterance: the last
/// partial is taken as the request (marked `endpointed`). Push to talk: measured from the
/// release. Outside Listening (barge-in while the voice task starts and runs, the wake
/// word's stream while another task runs) the same endpoint applies to the stop words only.
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
/// ignored (its own voice, heard late). While an answer (model-written text) is read and
/// for this long after it ended, heard text can only stop: no task starts and no listening
/// exchange opens, whatever the words.
pub const ECHO_WINDOW_MS: u64 = 5_000;
/// An utterance with no `SpeechFinished` after this long counts as finished (a lost
/// callback must not mute the microphone for good).
pub const MAX_SPEAKING_MS: u64 = 60_000;
/// No `Started`/`StartRefused` for the request this long after `StartTask`: the start
/// counts as failed ("I could not do that.", once); a later start of that request is
/// stopped.
pub const START_TIMEOUT_MS: u64 = 10_000;
/// After the wake word or the barge-in recogniser failed, it is started again no sooner
/// than this (and again after every failure, for as long as it is wanted).
pub const WAKE_RETRY_MS: u64 = 5_000;

/// Requests remembered as "a stop was heard while it was starting" after their start
/// timed out (so their late start says "Stopped."): the most recent ones.
const MAX_REMEMBERED_STOPS: usize = 16;

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
    Starting { since_ms: u64, request: u64 },
    TaskRunning { question: bool, request: u64 },
}

/// A line being said.
#[derive(Debug, Clone, PartialEq, Eq)]
struct InFlight {
    id: u64,
    words: Vec<String>,
    kind: UtteranceKind,
    started_ms: u64,
}

/// A late start of an earlier voice request, being stopped (never adopted).
#[derive(Debug, Clone, PartialEq, Eq)]
struct Late {
    request: u64,
    /// A stop was heard while it was starting: "Stopped." when it has.
    say_stopped: bool,
}

/// A partial in a stop context made only of stop and wake phrases, waiting for the endpoint
/// (its words unchanged for [`ENDPOINT_SILENCE_MS`]).
#[derive(Debug, Clone, PartialEq, Eq)]
struct PendingStop {
    /// The unread words of the partial (compared as words: "Stop" and "Stop." are one).
    words: Vec<String>,
    /// A wake phrase comes right before a stop phrase in them.
    wake: bool,
    since_ms: u64,
}

/// The voice state machine. Create one per app run; feed it every [`VoiceEvent`] with the
/// shell's monotonic `now_ms` and carry out the effects in order.
#[derive(Debug, Clone)]
pub struct VoiceSession {
    settings: VoiceSettings,
    now: u64,
    locked: bool,
    /// Jarvis active, covered (inactive) or in the background.
    scene: AppScene,
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
    /// Heard text can only stop until this time (an answer was read; see
    /// [`ECHO_WINDOW_MS`]).
    answer_until: u64,
    next_utterance: u64,
    /// The id the next `StartTask` carries.
    next_request: u64,
    /// A task started elsewhere (or a late start being stopped) runs.
    other_task: bool,
    /// Voice asked to stop the voice task (Starting or TaskRunning); "Stopped." is said when
    /// it has.
    stop_requested: bool,
    /// Voice asked to stop a task started elsewhere; "Stopped." is said when it has.
    other_stop_requested: bool,
    /// Timed-out requests a stop was heard for (most recent last).
    stopped_requests: Vec<u64>,
    /// A late start of an earlier request runs and is being stopped.
    late: Option<Late>,
    /// The barge-in recogniser may not start before this time (it failed).
    barge_in_retry_at: u64,
    /// The wake word may not be armed before this time (it failed).
    wake_retry_at: u64,
    /// A partial stop outside Listening, waiting for the endpoint.
    pending_stop: Option<PendingStop>,
    /// The words of the current listening stream's latest partial (one recognition, or the
    /// wake word's stream until its next final): on-device transcripts keep everything
    /// heard since the recognition started.
    stream_words: usize,
    /// The words of the current stream before this index are read already (a stop acted on,
    /// or heard before the stop context began): they are never read as a stop again, and a
    /// stream with read words starts nothing.
    stop_from: usize,
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
            scene: AppScene::Active,
            interrupted: false,
            resume_blocked: false,
            phase: Phase::Idle,
            mic: Mic::Off,
            audio_active: false,
            speaking: Vec::new(),
            recent: Vec::new(),
            quiet_until: 0,
            answer_until: 0,
            next_utterance: 1,
            next_request: 1,
            other_task: false,
            stop_requested: false,
            other_stop_requested: false,
            stopped_requests: Vec::new(),
            late: None,
            barge_in_retry_at: 0,
            wake_retry_at: 0,
            pending_stop: None,
            stream_words: 0,
            stop_from: 0,
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
                Phase::TaskRunning { question, .. } => VoicePhase::TaskRunning {
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
        let was_stop_context = self.stop_context();
        match event {
            VoiceEvent::WakeDetected => self.on_wake_detected(&mut out),
            VoiceEvent::PushToTalk { pressed } => self.on_push_to_talk(&mut out, pressed),
            VoiceEvent::PartialTranscript { text } => self.on_partial(&mut out, &text),
            VoiceEvent::FinalTranscript { text } => self.on_final(&mut out, &text),
            VoiceEvent::RecognizerFailed { .. } => self.on_recognizer_failed(&mut out),
            VoiceEvent::SpeechFinished { utterance_id } => self.on_speech_finished(utterance_id),
            VoiceEvent::Tick => self.on_tick(&mut out),
            VoiceEvent::Task(task) => self.on_task(&mut out, None, task),
            VoiceEvent::RequestTask { request, event } => {
                self.on_task(&mut out, Some(request), event);
            }
            VoiceEvent::LockChanged { locked } => self.on_lock(&mut out, locked),
            VoiceEvent::ForegroundChanged { active } => self.on_scene(
                &mut out,
                if active {
                    AppScene::Active
                } else {
                    AppScene::Background
                },
            ),
            VoiceEvent::SceneChanged { scene } => self.on_scene(&mut out, scene),
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
        if !was_stop_context && self.stop_context() {
            // What the current stream heard before the stop context began is not read as a
            // stop for it.
            self.consume_stream();
        }
        self.reconcile(&mut out);
        if out
            .0
            .iter()
            .any(|e| matches!(e, VoiceEffect::StartRecognizer | VoiceEffect::ArmWakeWord))
        {
            // A new recognition (or arming) starts with nothing heard.
            self.reset_stream();
        }
        out.0
    }

    /// The current stream's words so far are read: none is read as a stop again.
    fn consume_stream(&mut self) {
        self.stop_from = self.stream_words;
        self.pending_stop = None;
    }

    /// The current stream ended (a final, a failure) or a new one starts.
    fn reset_stream(&mut self) {
        self.stream_words = 0;
        self.stop_from = 0;
        self.pending_stop = None;
    }

    // -----------------------------------------------------------------------------------
    // Listening.
    // -----------------------------------------------------------------------------------

    fn on_wake_detected(&mut self, out: &mut Out) {
        // Only the armed wake word reports it; a late report after disarming is stale.
        if self.mic != Mic::Wake
            || self.phase != Phase::Idle
            || self.quiet()
            || self.answer_window()
            || !self.active()
        {
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
        // A new exchange only while Jarvis is active (a touch cannot land on it while it is
        // covered or in the background).
        if self.locked || self.interrupted || !self.active() {
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
        self.pending_stop = None;
        out.push(VoiceEffect::PlayCue(CueKind::Listening));
        self.phase = Phase::Listening(Listen::new(ptt, self.now));
    }

    fn on_partial(&mut self, out: &mut Out, text: &str) {
        if self.mic == Mic::Off {
            return;
        }
        let heard = parse_utterance(text);
        self.stream_words = heard.words.len();
        if self.stop_context() {
            self.read_stop_partial(out, &heard);
            return;
        }
        if self.quiet() || self.is_echo(&heard) {
            return;
        }
        let now = self.now;
        let answer_window = self.answer_window();
        let opens = self.active() && self.stop_from == 0;
        match &mut self.phase {
            Phase::Idle => {
                // The wake word's own stream: a wake phrase opens the exchange and the same
                // stream goes on carrying the request. Not while an answer echoes, not while
                // Jarvis is covered, and not on a stream whose start was read already.
                if !heard.wake || answer_window || !opens {
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
        if heard.too_long || heard.request.chars().count() > MAX_REQUEST_CHARS {
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
        let from = self.stop_from.min(heard.words.len());
        // The recognition is over (the wake word's stream starts a new one).
        self.reset_stream();
        // The stop words come first: while Jarvis speaks, and even when they repeat what it
        // says (stopping is the safe direction). In a stop context they are read on the
        // words not read yet (see `read_stop_partial`).
        if self.stop_context() {
            let fresh = heard.words.get(from..).unwrap_or(&[]);
            let wake_stop = wake_then_stop(fresh).is_some();
            if wake_stop || (self.bare_stop_allowed() && only_stop_words(fresh)) {
                self.stop(out, wake_stop);
                return;
            }
        } else if from == 0 && self.is_stop_for_now(&heard) {
            self.stop(out, heard.wake);
            return;
        }
        if from > 0 {
            // Its start was read already (a stop acted on): nothing more comes of it.
            return;
        }
        if self.quiet() || self.is_echo(&heard) {
            return;
        }
        // While an answer echoes, heard text only stops; a push-to-talk exchange goes on to
        // the row (a touch decides).
        if self.answer_window() && !matches!(&self.phase, Phase::Listening(l) if l.ptt) {
            return;
        }
        self.on_request_heard(out, &heard, false);
    }

    /// The stop words, in a form that acts now outside a stop context: after the wake
    /// phrase, or while an exchange is open or Jarvis speaks.
    fn is_stop_for_now(&self, heard: &Utterance) -> bool {
        heard.is_stop() && (heard.wake || self.bare_stop_allowed())
    }

    /// A stop without the wake phrase acts: while an exchange is open (barge-in on the voice
    /// task) or Jarvis speaks.
    fn bare_stop_allowed(&self) -> bool {
        self.exchange_open() || self.quiet()
    }

    /// A partial in a stop context (barge-in while the voice task starts or runs, the wake
    /// word's stream while another task runs), read on its unread words. Stopping is the
    /// safe direction, so neither the echo rule nor the speaking tail holds it:
    ///
    /// - a wake phrase right before a stop phrase, with other words around it ("...the
    ///   weather tonight. Jarvis, stop", "Jarvis, stop the weather..."): stops at once (a
    ///   playing video never gives the silence an endpoint needs);
    /// - only stop and wake phrases ("Stop, stop!", "Jarvis, stop"), where a wake phrase
    ///   comes right before a stop or a bare stop is allowed: stops once its words have not
    ///   changed for [`ENDPOINT_SILENCE_MS`] (or on the final);
    /// - anything else ("...and then I told him to stop"): nothing, and a waiting stop is
    ///   dropped.
    ///
    /// A stop acted on marks the stream's words read, so it acts once.
    fn read_stop_partial(&mut self, out: &mut Out, heard: &Utterance) {
        let from = self.stop_from.min(heard.words.len());
        let fresh = heard.words.get(from..).unwrap_or(&[]);
        let wake_stop = wake_then_stop(fresh).is_some();
        if only_stop_words(fresh) && (wake_stop || self.bare_stop_allowed()) {
            let same = self.pending_stop.as_ref().is_some_and(|p| p.words == fresh);
            if !same {
                self.pending_stop = Some(PendingStop {
                    words: fresh.to_vec(),
                    wake: wake_stop,
                    since_ms: self.now,
                });
            }
        } else if wake_stop {
            self.consume_stream();
            self.stop(out, true);
        } else {
            self.pending_stop = None;
        }
    }

    /// Where only the stop words are read (from finals, and from partials at the endpoint).
    fn stop_context(&self) -> bool {
        match self.phase {
            Phase::Starting { .. } | Phase::TaskRunning { .. } => true,
            Phase::Idle => self.other_task,
            Phase::Listening(_) | Phase::Confirming { .. } => false,
        }
    }

    /// A whole utterance (a final transcript, or the last partial at the endpoint).
    fn on_request_heard(&mut self, out: &mut Out, heard: &Utterance, endpointed: bool) {
        // Covered (or in the background): the wake phrase gets no exchange and nothing is
        // said ("I am busy." included). A push-to-talk exchange already open goes on to its
        // row.
        if !self.active() && !matches!(&self.phase, Phase::Listening(l) if l.ptt) {
            return;
        }
        match &self.phase {
            Phase::Idle => {
                if !heard.wake {
                    return;
                }
                if heard.too_long {
                    self.say(out, TOO_LONG, UtteranceKind::Status);
                } else if heard.request_is_empty() {
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
                if heard.too_long {
                    self.end_exchange(out, Some(TOO_LONG));
                } else if heard.request_is_empty() {
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
        let answer_window = self.answer_window();
        if !ptt && (answer_window || !self.active()) {
            // Never reached (hands-free listening does not open in the window); kept so no
            // path starts a task from what the room may be echoing.
            self.end_exchange(out, None);
            return;
        }
        self.stop_recognizer(out);
        // Covered or in the window: even with confirmation off, a push-to-talk request waits
        // for a touch on Start.
        if ptt && (self.settings.confirm_push_to_talk || answer_window || !self.active()) {
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
        let request = self.next_request;
        self.next_request = self.next_request.saturating_add(1);
        out.push(VoiceEffect::StartTask {
            text: text.to_owned(),
            origin,
            request,
        });
        self.stop_requested = false;
        self.barge_in_retry_at = 0;
        self.pending_stop = None;
        self.phase = Phase::Starting {
            since_ms: self.now,
            request,
        };
    }

    fn on_confirmed(&mut self, out: &mut Out, text: &str) {
        // A touch on Start lands only while Jarvis is active; a confirmation reported while
        // it is covered or in the background leaves the row waiting.
        if !matches!(self.phase, Phase::Confirming { .. }) || !self.active() {
            return;
        }
        if text.chars().nth(MAX_TRANSCRIPT_CHARS).is_some() {
            self.end_exchange(out, Some(TOO_LONG));
            return;
        }
        let request = clean_request(text);
        if request.is_empty() {
            out.push(VoiceEffect::ClearTranscript);
            self.phase = Phase::Idle;
        } else if request.chars().count() > MAX_REQUEST_CHARS {
            self.end_exchange(out, Some(TOO_LONG));
        } else if self.other_task {
            self.end_exchange(out, Some(BUSY));
        } else {
            // The row's Start is used: it shows what runs, with no Start any more.
            out.push(show(&request, true, false, false));
            // Confirmed by touch: the owner's own words.
            self.start_task(out, &request, TaskOrigin::Phone);
        }
    }

    fn on_recognizer_failed(&mut self, out: &mut Out) {
        match self.mic {
            Mic::Off => {}
            Mic::Wake => {
                self.mic = Mic::Off;
                self.reset_stream();
                self.wake_retry_at = self.now.saturating_add(WAKE_RETRY_MS);
                if matches!(self.phase, Phase::Listening(_)) {
                    self.end_exchange(out, Some(NOT_CAUGHT));
                }
            }
            Mic::Recognizer => {
                self.mic = Mic::Off;
                self.reset_stream();
                match self.phase {
                    Phase::Listening(_) => self.end_exchange(out, Some(NOT_CAUGHT)),
                    Phase::TaskRunning { .. } | Phase::Starting { .. } => {
                        // Barge-in comes back after the delay, as often as it fails.
                        self.barge_in_retry_at = self.now.saturating_add(WAKE_RETRY_MS);
                    }
                    Phase::Idle | Phase::Confirming { .. } => {}
                }
            }
        }
    }

    /// Stop words: whatever is open ends; a running task is asked to stop. `wake`: said
    /// after the wake phrase.
    fn stop(&mut self, out: &mut Out, wake: bool) {
        self.stop_speaking(out);
        self.pending_stop = None;
        match self.phase {
            Phase::Listening(_) => self.end_exchange(out, None),
            Phase::Confirming { .. } => {
                out.push(VoiceEffect::ClearTranscript);
                self.phase = Phase::Idle;
            }
            Phase::Starting { .. } => {
                // Kept for the voice task: its start is stopped at once when it arrives.
                self.stop_requested = true;
                if !self.other_task {
                    out.push(VoiceEffect::StopTask);
                } else if wake {
                    // What runs is another task: only "Jarvis, stop" stops it (rule 8).
                    out.push(VoiceEffect::StopTask);
                    self.other_stop_requested = true;
                }
            }
            Phase::TaskRunning { .. } => {
                out.push(VoiceEffect::StopTask);
                self.stop_requested = true;
            }
            Phase::Idle => {
                if self.other_task {
                    out.push(VoiceEffect::StopTask);
                    self.other_stop_requested = true;
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
                self.retire(u);
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
        // The endpoint over a partial stop (barge-in, the wake word's stream while another
        // task runs).
        if let Some(p) = &self.pending_stop {
            if now.saturating_sub(p.since_ms) >= ENDPOINT_SILENCE_MS {
                let wake = p.wake;
                self.pending_stop = None;
                if self.stop_context() {
                    self.consume_stream();
                    self.stop(out, wake);
                }
            }
        }
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
            Phase::Starting { since_ms, request } => {
                if now.saturating_sub(*since_ms) >= START_TIMEOUT_MS {
                    let request = *request;
                    if self.stop_requested {
                        // Its late start, if it comes, is stopped and says "Stopped.".
                        self.stopped_requests.push(request);
                        if self.stopped_requests.len() > MAX_REMEMBERED_STOPS {
                            self.stopped_requests.remove(0);
                        }
                    }
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
            self.stop(out, heard.wake);
            return;
        }
        self.on_request_heard(out, &heard, true);
    }

    // -----------------------------------------------------------------------------------
    // The task.
    // -----------------------------------------------------------------------------------

    /// The request the voice task holds (Starting or TaskRunning).
    fn current_request(&self) -> Option<u64> {
        match self.phase {
            Phase::Starting { request, .. } | Phase::TaskRunning { request, .. } => Some(request),
            Phase::Idle | Phase::Listening(_) | Phase::Confirming { .. } => None,
        }
    }

    /// `request` is one this session issued.
    fn issued(&self, request: Option<u64>) -> bool {
        request.is_some_and(|r| r >= 1 && r < self.next_request)
    }

    fn on_task(&mut self, out: &mut Out, request: Option<u64>, event: TaskEvent) {
        let ours = request.is_some() && request == self.current_request();
        let late = !ours && request.is_some() && self.late.as_ref().map(|l| l.request) == request;
        // An event for an earlier request that is neither the voice task nor the late start
        // being stopped (a start after it was refused, an end after it ended): stale.
        let stale = !ours && !late && self.issued(request);
        match event {
            TaskEvent::Started => {
                if ours {
                    if let Phase::Starting { request, .. } = self.phase {
                        self.phase = Phase::TaskRunning {
                            question: false,
                            request,
                        };
                        // The shell runs one task at a time: once the voice task has started
                        // nothing else runs, whatever an earlier start (with an id never
                        // issued, or a late one being stopped) made this believe.
                        self.other_task = false;
                        self.other_stop_requested = false;
                        self.late = None;
                        if self.stop_requested {
                            out.push(VoiceEffect::StopTask);
                        } else {
                            self.say(out, ON_IT, UtteranceKind::Status);
                        }
                    }
                } else if let Some(r) = request.filter(|_| self.issued(request)) {
                    // A late start of an earlier voice request (after its start timed out, or
                    // reported again): stopped at once, never adopted as running.
                    out.push(VoiceEffect::StopTask);
                    let say_stopped = match self.stopped_requests.iter().position(|x| *x == r) {
                        Some(i) => {
                            self.stopped_requests.remove(i);
                            true
                        }
                        None => false,
                    };
                    self.late = Some(Late {
                        request: r,
                        say_stopped,
                    });
                    self.other_task = true;
                    self.other_stop_requested = false;
                } else {
                    // Started elsewhere (no request id of ours): never the voice task, never
                    // spoken for, never listened for.
                    self.other_task = true;
                    self.other_stop_requested = false;
                }
            }
            TaskEvent::StartRefused { busy } => {
                if ours && matches!(self.phase, Phase::Starting { .. }) {
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
                if !ours {
                    return;
                }
                if let Phase::TaskRunning { question, .. } = &mut self.phase {
                    if !*question {
                        *question = true;
                        if !self.stop_requested {
                            self.say(out, APPROVAL_ON_PHONE, UtteranceKind::Status);
                        }
                    }
                }
            }
            TaskEvent::ApprovalAnswered => {
                if !ours {
                    return;
                }
                if let Phase::TaskRunning { question, .. } = &mut self.phase {
                    *question = false;
                }
            }
            TaskEvent::FinishedOk { .. } | TaskEvent::Failed | TaskEvent::Stopped => {
                if ours {
                    self.task_ended(out);
                    match event {
                        TaskEvent::FinishedOk { answer } => {
                            self.say_finished(out, answer.as_deref());
                        }
                        TaskEvent::Stopped => self.say(out, STOPPED, UtteranceKind::Status),
                        _ => self.say(out, COULD_NOT, UtteranceKind::Status),
                    }
                } else if late {
                    let asked = self.late.take().is_some_and(|l| l.say_stopped)
                        || self.other_stop_requested;
                    self.other_ended();
                    if asked && event == TaskEvent::Stopped {
                        self.say(out, STOPPED, UtteranceKind::Status);
                    }
                } else if !stale {
                    let asked = self.other_stop_requested;
                    self.other_ended();
                    if asked && event == TaskEvent::Stopped {
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
        self.barge_in_retry_at = 0;
        self.pending_stop = None;
        self.other_task = false;
        self.late = None;
    }

    fn other_ended(&mut self) {
        self.other_task = false;
        self.other_stop_requested = false;
        self.late = None;
        self.pending_stop = None;
    }

    /// "Done." and, when the setting, the lock and the front allow it, the answer.
    fn say_finished(&mut self, out: &mut Out, answer: Option<&str>) {
        let answer = answer.map(clean_spoken).filter(|a| !a.is_empty());
        let Some(answer) = answer else {
            self.say(out, DONE, UtteranceKind::Status);
            return;
        };
        if self.locked || !self.active() {
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
        self.retire(u);
        if self.speaking.is_empty() {
            self.quiet_until = self.now.saturating_add(SPEAKING_TAIL_MS);
        }
    }

    /// A line has ended (finished, stopped or expired): into the echo memory; an answer opens
    /// the window in which heard text can only stop.
    fn retire(&mut self, u: InFlight) {
        if u.kind == UtteranceKind::Answer {
            self.answer_until = self
                .answer_until
                .max(self.now.saturating_add(ECHO_WINDOW_MS));
        }
        self.recent.push((u.words, self.now));
    }

    fn stop_speaking(&mut self, out: &mut Out) {
        if self.speaking.is_empty() {
            return;
        }
        out.push(VoiceEffect::StopSpeaking);
        let now = self.now;
        for u in std::mem::take(&mut self.speaking) {
            self.retire(u);
        }
        self.quiet_until = now.saturating_add(SPEAKING_TAIL_MS);
    }

    /// Jarvis speaks, or just stopped.
    fn quiet(&self) -> bool {
        !self.speaking.is_empty() || self.now < self.quiet_until
    }

    /// An answer is being read, or ended less than [`ECHO_WINDOW_MS`] ago.
    fn answer_window(&self) -> bool {
        self.now < self.answer_until
            || self
                .speaking
                .iter()
                .any(|u| u.kind == UtteranceKind::Answer)
    }

    fn speaking_answer(&self) -> bool {
        self.speaking
            .iter()
            .any(|u| u.kind == UtteranceKind::Answer)
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
        if self.speaking_answer() {
            self.stop_speaking(out);
        }
        if matches!(self.phase, Phase::Listening(_)) {
            self.end_exchange(out, None);
        }
    }

    fn on_scene(&mut self, out: &mut Out, scene: AppScene) {
        self.scene = scene;
        if scene == AppScene::Active {
            self.resume_blocked = false;
            return;
        }
        // An answer is read only while Jarvis is active (fixed phrases may finish).
        if self.speaking_answer() {
            self.stop_speaking(out);
        }
        if scene == AppScene::Inactive {
            // Covered: the microphone stays on (the stop words still work) but a hands-free
            // exchange starts nothing; push to talk is the owner's touch and goes on.
            if matches!(&self.phase, Phase::Listening(l) if !l.ptt) {
                self.end_exchange(out, None);
            }
            return;
        }
        if matches!(self.phase, Phase::Listening(_)) {
            self.end_exchange(out, None);
        }
        self.stop_recognizer(out);
    }

    /// Jarvis is active (in front and not covered).
    fn active(&self) -> bool {
        self.scene == AppScene::Active
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
        if settings.speak_answers == SpeakAnswers::Never && self.speaking_answer() {
            self.stop_speaking(out);
        }
        // Hands-free off: an open hands-free exchange is discarded (nothing starts at its
        // endpoint). Push to talk is the owner's touch and goes on.
        if !settings.wake_word {
            if let Phase::Listening(l) = &self.phase {
                if !l.ptt {
                    self.end_exchange(out, None);
                }
            }
        }
    }

    /// The microphone may run at all.
    fn can_listen(&self) -> bool {
        self.scene != AppScene::Background
            && !self.locked
            && !self.interrupted
            && !self.resume_blocked
    }

    /// While a task started elsewhere runs, the wake phrase alone does not open an exchange
    /// on the wake word's stream (only "Jarvis, stop" acts).
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
        // Barge-in listens for the voice task only: not while what runs is another task.
        let task_phase = matches!(
            self.phase,
            Phase::Starting { .. } | Phase::TaskRunning { .. }
        );
        let barge_in_wanted = task_phase && self.can_listen() && !self.other_task;
        if self.mic == Mic::Wake && !wake_wanted {
            out.push(VoiceEffect::DisarmWakeWord);
            self.mic = Mic::Off;
        }
        // The recogniser runs only for a request (Listening) or for barge-in.
        let recognizer_wanted = match self.phase {
            Phase::Listening(_) => self.can_listen(),
            Phase::Starting { .. } | Phase::TaskRunning { .. } => barge_in_wanted,
            Phase::Idle | Phase::Confirming { .. } => false,
        };
        if self.mic == Mic::Recognizer && !recognizer_wanted {
            out.push(VoiceEffect::StopRecognizer);
            self.mic = Mic::Off;
        }
        if self.mic == Mic::Off {
            if wake_wanted && self.phase == Phase::Idle {
                out.push(VoiceEffect::ArmWakeWord);
                self.mic = Mic::Wake;
            } else if barge_in_wanted && self.now >= self.barge_in_retry_at {
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
