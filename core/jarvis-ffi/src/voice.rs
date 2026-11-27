//! `JarvisVoice`: the voice state machine (`jarvis_voice::VoiceSession`) for Swift.
//!
//! One session behind a `Mutex` (calls from any thread are serialized). Every call runs
//! inside [`guarded`]: a panic is a thrown `JarvisError.internal` and poisons the object,
//! after which every call throws `JarvisError.poisoned`. The shell then stops all voice IO
//! (recogniser, wake word, speech) and treats voice as unavailable; nothing it does is
//! decided outside the core.
//!
//! The records mirror `jarvis-voice`'s types one to one; their names carry a `Voice`
//! prefix so they never collide with the shell's own types.

use std::sync::{Arc, Mutex, MutexGuard};

use jarvis_proto::TaskOrigin;
use jarvis_voice as v;

use crate::error::JarvisError;
use crate::guard::guarded;

/// When the task's answer may be read aloud.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum VoiceSpeakAnswers {
    /// Never (the default).
    Never,
    /// While the phone is unlocked; never while locked.
    WhenUnlocked,
}

/// The owner's voice settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct VoiceSettings {
    /// Listen for "Hey Jarvis" (off by default).
    #[uniffi(default = false)]
    pub wake_word: bool,
    /// When the answer is read aloud.
    pub speak_answers: VoiceSpeakAnswers,
    /// Push to talk waits for a touch on Start (on by default).
    #[uniffi(default = true)]
    pub confirm_push_to_talk: bool,
}

/// What the task did (`jarvis_voice::TaskEvent`).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum VoiceTaskEvent {
    Started,
    StartRefused { busy: bool },
    WaitingForApproval,
    ApprovalAnswered,
    FinishedOk { answer: Option<String> },
    Failed,
    Stopped,
}

/// One input (`jarvis_voice::VoiceEvent`). `Task`: an event of a task that is not a voice
/// request's; `RequestTask`: an event of the task started for `StartTask` `request` (the id
/// passed through).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum VoiceEvent {
    WakeDetected,
    PushToTalk { pressed: bool },
    PartialTranscript { text: String },
    FinalTranscript { text: String },
    RecognizerFailed { reason: String },
    SpeechFinished { utterance_id: u64 },
    Tick,
    Task { event: VoiceTaskEvent },
    RequestTask { request: u64, event: VoiceTaskEvent },
    LockChanged { locked: bool },
    ForegroundChanged { active: bool },
    AudioInterrupted { began: bool, should_resume: bool },
    RouteChanged,
    SettingsChanged { settings: VoiceSettings },
    TranscriptConfirmed { text: String },
    TranscriptDiscarded,
}

/// The cue played.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum VoiceCue {
    Listening,
    Ended,
}

/// What a spoken line is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum VoiceUtteranceKind {
    Status,
    Answer,
}

/// Where a voice-started task came from: `voice` without a touch, `phone` for a transcript
/// the owner confirmed by touch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum VoiceTaskOrigin {
    Phone,
    Voice,
}

/// One output (`jarvis_voice::VoiceEffect`), carried out in order. `StartTask.request`: a fresh
/// id; every event of the task started for it goes back as `VoiceEvent.requestTask`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum VoiceEffect {
    ArmWakeWord,
    DisarmWakeWord,
    StartRecognizer,
    FinishRecognizer,
    StopRecognizer,
    PlayCue {
        cue: VoiceCue,
    },
    Speak {
        utterance_id: u64,
        text: String,
        kind: VoiceUtteranceKind,
    },
    StopSpeaking,
    ShowTranscript {
        text: String,
        is_final: bool,
        needs_confirm: bool,
        endpointed: bool,
    },
    ClearTranscript,
    StartTask {
        text: String,
        origin: VoiceTaskOrigin,
        request: u64,
    },
    StopTask,
    ActivateAudioSession,
    DeactivateAudioSession,
}

/// Where the exchange stands.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum VoicePhase {
    Idle,
    Listening,
    Confirming { text: String },
    Starting,
    TaskRunning { question_pending: bool },
}

/// A summary for the UI.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct VoiceStatus {
    pub phase: VoicePhase,
    pub wake_armed: bool,
    pub recognizing: bool,
    pub speaking: bool,
    pub other_task_running: bool,
    pub interrupted: bool,
}

/// The core's timing constants, in milliseconds (for the shell's tick and its tests).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct VoiceTimings {
    pub listen_no_speech_ms: u64,
    pub endpoint_silence_ms: u64,
    pub max_utterance_ms: u64,
    pub max_request_chars: u64,
    pub speaking_tail_ms: u64,
    pub echo_window_ms: u64,
    pub max_speaking_ms: u64,
    pub start_timeout_ms: u64,
    pub wake_retry_ms: u64,
}

/// The core's voice timing constants.
#[uniffi::export]
pub fn voice_timings() -> VoiceTimings {
    VoiceTimings {
        listen_no_speech_ms: v::LISTEN_NO_SPEECH_MS,
        endpoint_silence_ms: v::ENDPOINT_SILENCE_MS,
        max_utterance_ms: v::MAX_UTTERANCE_MS,
        max_request_chars: u64::try_from(v::MAX_REQUEST_CHARS).unwrap_or(u64::MAX),
        speaking_tail_ms: v::SPEAKING_TAIL_MS,
        echo_window_ms: v::ECHO_WINDOW_MS,
        max_speaking_ms: v::MAX_SPEAKING_MS,
        start_timeout_ms: v::START_TIMEOUT_MS,
        wake_retry_ms: v::WAKE_RETRY_MS,
    }
}

/// The default voice settings (wake word off, answers never spoken, push to talk confirms).
#[uniffi::export]
pub fn default_voice_settings() -> VoiceSettings {
    v::VoiceSettings::default().into()
}

impl From<v::VoiceSettings> for VoiceSettings {
    fn from(s: v::VoiceSettings) -> Self {
        Self {
            wake_word: s.wake_word,
            speak_answers: match s.speak_answers {
                v::SpeakAnswers::Never => VoiceSpeakAnswers::Never,
                v::SpeakAnswers::WhenUnlocked => VoiceSpeakAnswers::WhenUnlocked,
            },
            confirm_push_to_talk: s.confirm_push_to_talk,
        }
    }
}

impl From<VoiceSettings> for v::VoiceSettings {
    fn from(s: VoiceSettings) -> Self {
        Self {
            wake_word: s.wake_word,
            speak_answers: match s.speak_answers {
                VoiceSpeakAnswers::Never => v::SpeakAnswers::Never,
                VoiceSpeakAnswers::WhenUnlocked => v::SpeakAnswers::WhenUnlocked,
            },
            confirm_push_to_talk: s.confirm_push_to_talk,
        }
    }
}

impl From<VoiceTaskEvent> for v::TaskEvent {
    fn from(e: VoiceTaskEvent) -> Self {
        match e {
            VoiceTaskEvent::Started => v::TaskEvent::Started,
            VoiceTaskEvent::StartRefused { busy } => v::TaskEvent::StartRefused { busy },
            VoiceTaskEvent::WaitingForApproval => v::TaskEvent::WaitingForApproval,
            VoiceTaskEvent::ApprovalAnswered => v::TaskEvent::ApprovalAnswered,
            VoiceTaskEvent::FinishedOk { answer } => v::TaskEvent::FinishedOk { answer },
            VoiceTaskEvent::Failed => v::TaskEvent::Failed,
            VoiceTaskEvent::Stopped => v::TaskEvent::Stopped,
        }
    }
}

impl From<VoiceEvent> for v::VoiceEvent {
    fn from(e: VoiceEvent) -> Self {
        match e {
            VoiceEvent::WakeDetected => v::VoiceEvent::WakeDetected,
            VoiceEvent::PushToTalk { pressed } => v::VoiceEvent::PushToTalk { pressed },
            VoiceEvent::PartialTranscript { text } => v::VoiceEvent::PartialTranscript { text },
            VoiceEvent::FinalTranscript { text } => v::VoiceEvent::FinalTranscript { text },
            VoiceEvent::RecognizerFailed { reason } => v::VoiceEvent::RecognizerFailed { reason },
            VoiceEvent::SpeechFinished { utterance_id } => {
                v::VoiceEvent::SpeechFinished { utterance_id }
            }
            VoiceEvent::Tick => v::VoiceEvent::Tick,
            VoiceEvent::Task { event } => v::VoiceEvent::Task(event.into()),
            VoiceEvent::RequestTask { request, event } => v::VoiceEvent::RequestTask {
                request,
                event: event.into(),
            },
            VoiceEvent::LockChanged { locked } => v::VoiceEvent::LockChanged { locked },
            VoiceEvent::ForegroundChanged { active } => v::VoiceEvent::ForegroundChanged { active },
            VoiceEvent::AudioInterrupted {
                began,
                should_resume,
            } => v::VoiceEvent::AudioInterrupted {
                began,
                should_resume,
            },
            VoiceEvent::RouteChanged => v::VoiceEvent::RouteChanged,
            VoiceEvent::SettingsChanged { settings } => {
                v::VoiceEvent::SettingsChanged(settings.into())
            }
            VoiceEvent::TranscriptConfirmed { text } => v::VoiceEvent::TranscriptConfirmed { text },
            VoiceEvent::TranscriptDiscarded => v::VoiceEvent::TranscriptDiscarded,
        }
    }
}

impl From<v::VoiceEffect> for VoiceEffect {
    fn from(e: v::VoiceEffect) -> Self {
        match e {
            v::VoiceEffect::ArmWakeWord => VoiceEffect::ArmWakeWord,
            v::VoiceEffect::DisarmWakeWord => VoiceEffect::DisarmWakeWord,
            v::VoiceEffect::StartRecognizer => VoiceEffect::StartRecognizer,
            v::VoiceEffect::FinishRecognizer => VoiceEffect::FinishRecognizer,
            v::VoiceEffect::StopRecognizer => VoiceEffect::StopRecognizer,
            v::VoiceEffect::PlayCue(cue) => VoiceEffect::PlayCue {
                cue: match cue {
                    v::CueKind::Listening => VoiceCue::Listening,
                    v::CueKind::Ended => VoiceCue::Ended,
                },
            },
            v::VoiceEffect::Speak {
                utterance_id,
                text,
                kind,
            } => VoiceEffect::Speak {
                utterance_id,
                text,
                kind: match kind {
                    v::UtteranceKind::Status => VoiceUtteranceKind::Status,
                    v::UtteranceKind::Answer => VoiceUtteranceKind::Answer,
                },
            },
            v::VoiceEffect::StopSpeaking => VoiceEffect::StopSpeaking,
            v::VoiceEffect::ShowTranscript {
                text,
                is_final,
                needs_confirm,
                endpointed,
            } => VoiceEffect::ShowTranscript {
                text,
                is_final,
                needs_confirm,
                endpointed,
            },
            v::VoiceEffect::ClearTranscript => VoiceEffect::ClearTranscript,
            v::VoiceEffect::StartTask {
                text,
                origin,
                request,
            } => VoiceEffect::StartTask {
                text,
                request,
                // Only a transcript confirmed by touch is the owner's; anything else the
                // core could ever say is at least as strict as voice.
                origin: if origin.is_owner_words() {
                    VoiceTaskOrigin::Phone
                } else {
                    VoiceTaskOrigin::Voice
                },
            },
            v::VoiceEffect::StopTask => VoiceEffect::StopTask,
            v::VoiceEffect::ActivateAudioSession => VoiceEffect::ActivateAudioSession,
            v::VoiceEffect::DeactivateAudioSession => VoiceEffect::DeactivateAudioSession,
        }
    }
}

impl From<v::VoiceStatus> for VoiceStatus {
    fn from(s: v::VoiceStatus) -> Self {
        Self {
            phase: match s.phase {
                v::VoicePhase::Idle => VoicePhase::Idle,
                v::VoicePhase::Listening => VoicePhase::Listening,
                v::VoicePhase::Confirming { text } => VoicePhase::Confirming { text },
                v::VoicePhase::Starting => VoicePhase::Starting,
                v::VoicePhase::TaskRunning { question_pending } => {
                    VoicePhase::TaskRunning { question_pending }
                }
            },
            wake_armed: s.wake_armed,
            recognizing: s.recognizing,
            speaking: s.speaking,
            other_task_running: s.other_task_running,
            interrupted: s.interrupted,
        }
    }
}

/// `VoiceTaskOrigin` as the core's task origin (for tests and tools).
pub fn voice_origin_to_proto(origin: VoiceTaskOrigin) -> TaskOrigin {
    match origin {
        VoiceTaskOrigin::Phone => TaskOrigin::Phone,
        VoiceTaskOrigin::Voice => TaskOrigin::Voice,
    }
}

struct Inner {
    session: v::VoiceSession,
    poisoned: Option<String>,
    /// Test hook: the next `step` panics inside the guarded region.
    #[cfg(any(test, feature = "panic-hooks"))]
    panic_next_step: bool,
}

/// The voice state machine. Swift: `JarvisVoice`.
#[derive(uniffi::Object)]
pub struct JarvisVoice {
    inner: Mutex<Inner>,
}

impl JarvisVoice {
    fn lock(&self) -> MutexGuard<'_, Inner> {
        match self.inner.lock() {
            Ok(g) => g,
            Err(poison) => {
                self.inner.clear_poison();
                let mut g = poison.into_inner();
                g.poisoned
                    .get_or_insert_with(|| "a call panicked while holding the lock".to_owned());
                g
            }
        }
    }

    fn with<T>(
        &self,
        what: &str,
        f: impl FnOnce(&mut Inner) -> Result<T, JarvisError>,
    ) -> Result<T, JarvisError> {
        let result = guarded(what, || {
            let mut inner = self.lock();
            if let Some(message) = &inner.poisoned {
                return Err(JarvisError::Poisoned {
                    message: message.clone(),
                });
            }
            f(&mut inner)
        });
        if let Err(JarvisError::Internal { message }) = &result {
            self.lock().poisoned = Some(message.clone());
        }
        result
    }
}

#[uniffi::export]
impl JarvisVoice {
    /// A voice session with `settings` (nothing listening yet; send `tick` to arm the wake
    /// word when it is on).
    #[uniffi::constructor]
    pub fn new(settings: VoiceSettings) -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(Inner {
                session: v::VoiceSession::new(settings.into()),
                poisoned: None,
                #[cfg(any(test, feature = "panic-hooks"))]
                panic_next_step: false,
            }),
        })
    }

    /// Feeds one event at `now_ms` (monotonic) and returns the effects, in order. Throws
    /// `internal` when a panic was caught (the object is poisoned from then on) and
    /// `poisoned` afterwards.
    pub fn step(&self, now_ms: u64, event: VoiceEvent) -> Result<Vec<VoiceEffect>, JarvisError> {
        self.with("JarvisVoice.step", |inner| {
            #[cfg(any(test, feature = "panic-hooks"))]
            if std::mem::take(&mut inner.panic_next_step) {
                #[allow(clippy::panic)]
                {
                    panic!("deliberate panic in JarvisVoice.step");
                }
            }
            Ok(inner
                .session
                .step(now_ms, event.into())
                .into_iter()
                .map(Into::into)
                .collect())
        })
    }

    /// Where the session stands.
    pub fn status(&self) -> Result<VoiceStatus, JarvisError> {
        self.with("JarvisVoice.status", |inner| {
            Ok(inner.session.status().into())
        })
    }

    /// The settings in use.
    pub fn settings(&self) -> Result<VoiceSettings, JarvisError> {
        self.with("JarvisVoice.settings", |inner| {
            Ok(inner.session.settings().into())
        })
    }
}

#[cfg(any(test, feature = "panic-hooks"))]
#[uniffi::export]
impl JarvisVoice {
    /// Test hook (feature `panic-hooks`): the next `step` panics inside the guarded region.
    pub fn debug_panic_next_step(&self) {
        self.lock().panic_next_step = true;
    }
}
