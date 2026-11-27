//! What goes into and comes out of a [`crate::VoiceSession`].

use jarvis_proto::TaskOrigin;

/// When the task's answer text (which can hold screen content) may be read aloud.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SpeakAnswers {
    /// Never (the default): Jarvis says "Done." and the answer stays on the phone.
    #[default]
    Never,
    /// While the phone is unlocked. Never while it is locked, in any setting.
    WhenUnlocked,
}

/// The owner's voice settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VoiceSettings {
    /// Listen for the wake phrase ("Hey Jarvis"). Off by default; push to talk works
    /// without it.
    pub wake_word: bool,
    /// When the answer is read aloud.
    pub speak_answers: SpeakAnswers,
    /// Push to talk shows the transcript and waits for a touch on Start, which starts it as
    /// the owner's words (`TaskOrigin::Phone`). On by default. Off, a push-to-talk request
    /// starts without a touch, as `TaskOrigin::Voice`.
    pub confirm_push_to_talk: bool,
}

impl Default for VoiceSettings {
    fn default() -> Self {
        Self {
            wake_word: false,
            speak_answers: SpeakAnswers::Never,
            confirm_push_to_talk: true,
        }
    }
}

/// What the shell reports about the task Jarvis runs (whoever started it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskEvent {
    /// A task started. Right after [`VoiceEffect::StartTask`] it is the one voice asked for;
    /// any other start is a task started elsewhere (Ask, a workflow, a paired device).
    Started,
    /// The shell refused [`VoiceEffect::StartTask`]; `busy` when another task or a workflow
    /// holds the slot.
    StartRefused {
        /// Something else runs.
        busy: bool,
    },
    /// The task waits for the owner's approval or answer on the phone.
    WaitingForApproval,
    /// That question was answered or withdrawn (on the phone; never by voice).
    ApprovalAnswered,
    /// The task finished well. `answer` is its summary (it can hold screen content).
    FinishedOk {
        /// The model's final answer, if any.
        answer: Option<String>,
    },
    /// The task ended without doing what was asked.
    Failed,
    /// The task was stopped (by voice, the Stop button or a paired device).
    Stopped,
}

/// One input to [`crate::VoiceSession::step`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VoiceEvent {
    /// A dedicated wake-word engine heard the phrase (no text). The first implementation
    /// reads the wake phrase from the foreground recogniser's transcripts instead, which
    /// arrive as [`Self::PartialTranscript`]/[`Self::FinalTranscript`] while the wake word
    /// is armed.
    WakeDetected,
    /// The mic button: pressed starts listening, released ends the request.
    PushToTalk {
        /// `true` on press, `false` on release.
        pressed: bool,
    },
    /// A partial transcript from the listening stream (the recogniser or the wake word).
    PartialTranscript {
        /// The text so far.
        text: String,
    },
    /// The final transcript of one utterance. After it the recogniser has stopped by itself
    /// (the wake word keeps listening).
    FinalTranscript {
        /// The whole utterance.
        text: String,
    },
    /// The recogniser or the wake word failed; it is not listening any more.
    RecognizerFailed {
        /// Why (for the log; never spoken).
        reason: String,
    },
    /// An utterance from [`VoiceEffect::Speak`] ended (finished, or stopped).
    SpeechFinished {
        /// Its id.
        utterance_id: u64,
    },
    /// Time passed (the shell ticks a few times a second while voice is on).
    Tick,
    /// What the task did.
    Task(TaskEvent),
    /// The phone locked or unlocked.
    LockChanged {
        /// Locked now.
        locked: bool,
    },
    /// Jarvis came to the front or left it. The foreground recogniser (and so the first
    /// wake-word implementation) runs only while Jarvis is in front.
    ForegroundChanged {
        /// In front now.
        active: bool,
    },
    /// The audio session was interrupted (a phone call, Siri, an alarm) or the interruption
    /// ended.
    AudioInterrupted {
        /// `true` when it began, `false` when it ended.
        began: bool,
        /// On the end: the system says Jarvis may resume.
        should_resume: bool,
    },
    /// The audio route changed (headphones unplugged): speaking stops.
    RouteChanged,
    /// The owner changed the voice settings.
    SettingsChanged(VoiceSettings),
    /// The owner touched Start on the transcript row (possibly after editing it).
    TranscriptConfirmed {
        /// The text as it stands in the row.
        text: String,
    },
    /// The owner dismissed the transcript row.
    TranscriptDiscarded,
}

/// The sound played when Jarvis starts listening for a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CueKind {
    /// Listening for a request.
    Listening,
    /// Listening ended without a request.
    Ended,
}

/// What a spoken line is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UtteranceKind {
    /// One of the fixed status phrases.
    Status,
    /// Holds (part of) the task's answer.
    Answer,
}

/// One output of [`crate::VoiceSession::step`], carried out by the shell in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VoiceEffect {
    /// Start listening for the wake phrase.
    ArmWakeWord,
    /// Stop listening for the wake phrase.
    DisarmWakeWord,
    /// Start one recognition (one utterance; it ends with a final transcript).
    StartRecognizer,
    /// End the audio of the current recognition and deliver its final transcript (push to
    /// talk released).
    FinishRecognizer,
    /// Cancel the current recognition (no more transcripts from it).
    StopRecognizer,
    /// Play a cue.
    PlayCue(CueKind),
    /// Say `text` aloud.
    Speak {
        /// Id echoed by [`VoiceEvent::SpeechFinished`].
        utterance_id: u64,
        /// What to say (composed by the core; never contains the wake phrase).
        text: String,
        /// Status or answer.
        kind: UtteranceKind,
    },
    /// Stop everything being said.
    StopSpeaking,
    /// Show the transcript on the phone.
    ShowTranscript {
        /// The words.
        text: String,
        /// It is the request (not a partial).
        is_final: bool,
        /// The row waits for the owner's touch on Start (push to talk).
        needs_confirm: bool,
        /// The request is the last partial taken at the endpoint timeout, not a final
        /// transcript from the recogniser.
        endpointed: bool,
    },
    /// Remove the transcript row.
    ClearTranscript,
    /// Start a task through the shell's one task entry.
    StartTask {
        /// The request.
        text: String,
        /// `Voice` without a touch, `Phone` for a transcript the owner confirmed.
        origin: TaskOrigin,
    },
    /// Stop the running task (the same path as the Stop button).
    StopTask,
    /// Activate the audio session (before any listening or speaking).
    ActivateAudioSession,
    /// Deactivate it (nothing listens or speaks).
    DeactivateAudioSession,
}

/// Where a voice exchange stands (for the shell's UI).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VoicePhase {
    /// No exchange open.
    Idle,
    /// Listening for a request.
    Listening,
    /// A push-to-talk transcript waits for the owner's touch.
    Confirming {
        /// The transcript shown.
        text: String,
    },
    /// A task was asked for and has not started yet.
    Starting,
    /// The task voice started runs.
    TaskRunning {
        /// It waits for an answer on the phone.
        question_pending: bool,
    },
}

/// A summary of the session for the shell's UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceStatus {
    /// The exchange.
    pub phase: VoicePhase,
    /// The wake word listens.
    pub wake_armed: bool,
    /// The recogniser runs.
    pub recognizing: bool,
    /// Something is being said.
    pub speaking: bool,
    /// A task started elsewhere runs.
    pub other_task_running: bool,
    /// The audio session is interrupted.
    pub interrupted: bool,
}
