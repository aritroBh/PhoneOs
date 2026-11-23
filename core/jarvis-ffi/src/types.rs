//! Swift-facing mirrors of the `jarvis-proto` types in the `Event`/`Effect` graph.
//!
//! Each type here is a UniFFI record or enum so Swift gets real structs and enums with
//! associated values. Field names follow the proto types (UniFFI camel-cases them for Swift:
//! `app_bundle_id` becomes `appBundleId`). Deliberate differences from the proto types:
//!
//! - `serde_json::Value` (tool arguments) crosses as a JSON string (`args_json`).
//! - Proto tuple variants become named fields so Swift call sites read well:
//!   `Event::Observed(o)` is `.observed(observation:)`, `Effect::Infer(r)` is
//!   `.infer(request:)`, and so on.
//! - Two proto names collide with Apple frameworks in Swift and are renamed:
//!   `Observation` (the Swift `Observation` module) is [`ScreenObservation`] and `Progress`
//!   (`Foundation.Progress`) is [`SessionProgress`].
//! - [`Event::ObservedRunnerText`] carries the runner's raw text tree, which Rust parses
//!   (see `crate::runner`) into the proto `Event::ObservedRunnerText`: the session treats
//!   that screen as unverified (every state-changing action on it asks, with a warning).
//!
//! Conversions live in `crate::convert`.

/// A point in screen points.
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
pub struct Point {
    /// Horizontal coordinate.
    pub x: f64,
    /// Vertical coordinate.
    pub y: f64,
}

/// A size in screen points.
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
pub struct Size {
    /// Width.
    pub width: f64,
    /// Height.
    pub height: f64,
}

/// A rectangle in screen points (origin top-left).
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
pub struct Rect {
    /// Left edge.
    pub x: f64,
    /// Top edge.
    pub y: f64,
    /// Width.
    pub width: f64,
    /// Height.
    pub height: f64,
}

/// The XCUIElement type of a node. Unknown runner types are `Other { name }`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum Role {
    Button,
    Link,
    TextField,
    SecureTextField,
    TextView,
    SearchField,
    StaticText,
    Image,
    Cell,
    Switch,
    Slider,
    Picker,
    Tab,
    NavigationBar,
    TabBar,
    Toolbar,
    Alert,
    Sheet,
    Keyboard,
    Key,
    ScrollView,
    Table,
    CollectionView,
    WebView,
    Window,
    Application,
    /// Any other element type, by its runner name.
    Other {
        name: String,
    },
}

/// One node of the accessibility tree the runner returns. Recursive: UniFFI 0.32 lowers a
/// record holding `Vec<Self>` fine, and Swift gets `children: [UiNode]`.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct UiNode {
    pub role: Role,
    pub label: Option<String>,
    pub value: Option<String>,
    pub identifier: Option<String>,
    pub placeholder: Option<String>,
    pub frame: Rect,
    pub enabled: bool,
    pub visible: bool,
    pub hittable: bool,
    pub focused: bool,
    pub selected: bool,
    pub secure: bool,
    pub children: Vec<UiNode>,
}

/// Interface orientation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum Orientation {
    Portrait,
    PortraitUpsideDown,
    LandscapeLeft,
    LandscapeRight,
}

/// The foreground app's UI as the runner reports it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct Screen {
    pub app_bundle_id: String,
    pub app_name: Option<String>,
    pub size: Size,
    pub root: UiNode,
    pub keyboard_visible: bool,
    pub orientation: Orientation,
}

/// A handle to a screenshot the shell keeps (pixels never cross into the core).
#[derive(Debug, Clone, PartialEq, Eq, Hash, uniffi::Record)]
pub struct ImageRef {
    pub id: String,
    pub width: u32,
    pub height: u32,
}

/// Proto `Observation`: one screen, optionally with a screenshot handle.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ScreenObservation {
    pub screen: Screen,
    pub screenshot: Option<ImageRef>,
    /// The runner reports `screen_locked`: the session ignores `screen` and waits for the
    /// owner to unlock. Defaults to `false`, so existing Swift initializers keep compiling.
    #[uniffi(default = false)]
    pub device_locked: bool,
}

/// What the device path sends: the runner's raw `page_source` text plus what the runner
/// reports beside it. Parsed in Rust with `jarvis_observe::parse_runner_text`.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct RunnerSnapshot {
    /// The `source` string of the runner's `page_source` result (format `text`).
    pub source: String,
    pub app_bundle_id: String,
    pub app_name: Option<String>,
    pub size: Size,
    pub keyboard_visible: bool,
    pub orientation: Orientation,
    pub screenshot: Option<ImageRef>,
    /// The runner reports `screen_locked` (see `ScreenObservation::device_locked`).
    #[uniffi(default = false)]
    pub device_locked: bool,
}

/// Scroll / swipe direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

/// A model-facing action (what the model emits). Read-only for the shell: it appears in
/// [`SessionProgress::last_action`].
#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum Action {
    Tap {
        element: u32,
    },
    LongPress {
        element: u32,
        duration_ms: u64,
    },
    Type {
        element: Option<u32>,
        text: String,
        submit: bool,
    },
    Clear {
        element: u32,
    },
    Scroll {
        direction: Direction,
        element: Option<u32>,
    },
    Swipe {
        direction: Direction,
        element: Option<u32>,
    },
    TapPoint {
        x: f64,
        y: f64,
    },
    PressHome,
    OpenApp {
        app: String,
    },
    OpenUrl {
        url: String,
    },
    Wait {
        ms: u64,
    },
    CallTool {
        name: String,
        /// Tool arguments as a JSON document.
        args_json: String,
    },
    AskUser {
        question: String,
    },
    Look,
    Done {
        summary: String,
    },
    Fail {
        reason: String,
    },
}

/// A hardware or keyboard key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum Key {
    Return,
    Delete,
    Tab,
    Space,
    Escape,
    Home,
}

/// A command for the XCUITest runner, in absolute screen points. Its Rust `Debug` output
/// never shows the text of `typeText`, only its length.
#[derive(Clone, PartialEq, uniffi::Enum)]
pub enum RunnerCommand {
    Tap {
        x: f64,
        y: f64,
    },
    LongPress {
        x: f64,
        y: f64,
        duration_ms: u64,
    },
    Swipe {
        from: Point,
        to: Point,
        duration_ms: u64,
    },
    TypeText {
        text: String,
    },
    PressKey {
        key: Key,
    },
    LaunchApp {
        bundle_id: String,
    },
    OpenUrl {
        url: String,
    },
    Wait {
        ms: u64,
    },
}

/// Proto `RunnerFailure`: a typed reason a command was not carried out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum RunnerFailure {
    /// The device is locked; the command was refused and nothing happened.
    DeviceLocked,
    /// Any other typed reason.
    Other,
}

/// The runner's answer to one command.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct RunnerResult {
    pub ok: bool,
    pub error: Option<String>,
    /// Set to `.deviceLocked` when the runner refused because the phone is locked. Defaults
    /// to `nil`, so existing Swift initializers keep compiling.
    #[uniffi(default = None)]
    pub failure: Option<RunnerFailure>,
}

/// JSON type of a tool parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum ParamKind {
    String,
    Integer,
    Number,
    Boolean,
}

/// How a tool is carried out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum Tier {
    Native,
    Shortcut,
    Ui,
}

/// One tool parameter.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ToolParam {
    pub name: String,
    pub description: String,
    pub kind: ParamKind,
    pub required: bool,
}

/// A tool the shell offers to the agent.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub params: Vec<ToolParam>,
    pub tier: Tier,
    pub risk: RiskCategory,
    pub app: Option<String>,
}

/// A tool call for the shell to perform.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ToolCall {
    pub name: String,
    /// Arguments as a JSON document (an object for every call the session emits).
    pub args_json: String,
}

/// The result of a tool call.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ToolResult {
    pub ok: bool,
    pub output: String,
    pub error: Option<String>,
}

/// Text-only or screenshot request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum RequestKind {
    Text,
    Vision,
}

/// Where to draw a set-of-marks label on the screenshot.
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
pub struct Mark {
    pub index: u32,
    pub frame: Rect,
}

/// One model call for the shell to run.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ModelRequest {
    pub system: String,
    pub user: String,
    pub image: Option<ImageRef>,
    pub marks: Vec<Mark>,
    pub max_tokens: u32,
    pub grammar: Option<String>,
    pub json_schema: Option<String>,
    pub kind: RequestKind,
}

/// The model's raw reply text.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ModelOutput {
    pub text: String,
}

/// What the on-device model can do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct ModelProfile {
    pub context_tokens: u32,
    pub supports_vision: bool,
    pub supports_grammar: bool,
}

/// Risk of an action. Note for Swift: the `.none` case shadows `Optional.none` in optional
/// contexts; compare with `RiskCategory.none` explicitly there.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum RiskCategory {
    None,
    SendMessage,
    Payment,
    Purchase,
    Delete,
    PublicPost,
    AccountChange,
    SystemSetting,
    Credential,
    /// The target could not be classified (unreadable, foreign, unlabeled, unknown tool).
    Unverified,
    /// Placing a phone or FaceTime call.
    Call,
    /// The first time Jarvis acts inside an app.
    FirstUse,
}

/// Supervision for apps without an [`AppRule`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum PolicyMode {
    /// Confirm risky categories only.
    Guarded,
    /// Confirm every action that can change state.
    StepByStep,
}

/// Per-app trust level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum TrustLevel {
    /// No first-use approval; settings and unverified controls run without asking.
    Trusted,
    /// Like `PolicyMode.guarded`.
    Guarded,
    /// Like `PolicyMode.stepByStep`.
    StepByStep,
    /// Never operated.
    Denied,
}

/// A per-app trust level; the longest matching prefix wins.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct AppRule {
    /// Bundle id or bundle id prefix (matched at `.` boundaries).
    pub bundle_id_prefix: String,
    /// Trust level for matching apps.
    pub level: TrustLevel,
    /// The app's display name as the owner sees it (home-screen icon, app switcher). A
    /// `denied` rule with a name also refuses an icon, card or `openApp` with that name.
    #[uniffi(default = None)]
    pub name: Option<String>,
}

/// Safety policy and budgets. Start from `defaultPolicy()` in Swift.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct Policy {
    pub denied_apps: Vec<String>,
    pub confirm: Vec<RiskCategory>,
    /// Categories that are never executed.
    pub deny_categories: Vec<RiskCategory>,
    /// Supervision for apps without a rule.
    pub mode: PolicyMode,
    /// Per-app trust levels.
    pub app_rules: Vec<AppRule>,
    /// Ask before the first state-changing action in an unapproved app.
    pub first_use_approval: bool,
    pub max_steps: u32,
    pub max_duration_ms: u64,
    pub max_consecutive_failures: u32,
    pub max_parse_retries: u32,
    /// Longest wait for the owner to unlock before the session finishes `.interrupted`
    /// (milliseconds; locked time does not count against `maxDurationMs`).
    #[uniffi(default = 1800000)]
    pub max_locked_wait_ms: u64,
}

/// One input to the session. Its Rust `Debug` output never shows the owner's answer text
/// (`userAnswer`), only its length.
#[derive(Clone, PartialEq, uniffi::Enum)]
pub enum Event {
    Start,
    /// A typed screen (tests, simulators, or a shell that builds the tree itself).
    Observed {
        observation: ScreenObservation,
    },
    /// The third-party runner's raw text tree, parsed in Rust. A tree that cannot be
    /// parsed becomes an observation of an empty screen of that app (see `crate::runner`).
    /// The format is ambiguous and forgeable by on-screen text, so the session treats the
    /// screen as unverified: every state-changing action on it asks, and the question starts
    /// with a warning. The typed tree (`observed`) is the supported path for acting.
    ObservedRunnerText {
        snapshot: RunnerSnapshot,
    },
    ModelOutput {
        output: ModelOutput,
    },
    ModelFailed {
        error: String,
    },
    ActResult {
        result: RunnerResult,
    },
    ToolResult {
        result: ToolResult,
    },
    UserDecision {
        approved: bool,
    },
    UserAnswer {
        text: String,
    },
    Cancel,
    /// The shell learned the phone locked (between or instead of effect results).
    DeviceLocked,
    /// The owner unlocked the phone (answers `.awaitUnlock`).
    DeviceUnlocked,
    /// Still locked: the shell's timer fired (answers `.awaitUnlock`).
    Tick,
    /// `.observe` failed after the shell's own retries (not because the phone is locked).
    ObserveFailed {
        reason: String,
    },
}

/// A confirmation the owner must give before a risky action.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ConfirmRequest {
    pub category: RiskCategory,
    pub summary: String,
    pub app: String,
    /// Only the owner on the phone may approve it, whatever its category (thirteenth
    /// review: a presumed submit, or a link the owner did not give).
    pub phone_only: bool,
}

/// How a session ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum OutcomeStatus {
    Success,
    Failed,
    Cancelled,
    Blocked,
    /// The phone stayed locked longer than `maxLockedWaitMs`; nothing wrong with the task.
    Interrupted,
}

/// The end of a session.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct Outcome {
    pub status: OutcomeStatus,
    pub summary: String,
    pub steps: u32,
    pub duration_ms: u64,
}

/// The one thing the shell must do next.
#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum Effect {
    Observe {
        screenshot: bool,
    },
    Infer {
        request: ModelRequest,
    },
    Act {
        command: RunnerCommand,
    },
    CallTool {
        call: ToolCall,
    },
    Confirm {
        request: ConfirmRequest,
    },
    AskUser {
        question: String,
    },
    Finished {
        outcome: Outcome,
    },
    /// The phone is locked: show "Unlock to continue". Answer with `.deviceUnlocked`,
    /// `.tick` (timer, still locked) or a fresh observation.
    AwaitUnlock {
        reason: String,
        waited_ms: u64,
    },
}

/// Session phase for the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum Phase {
    Idle,
    Observing,
    Thinking,
    Acting,
    Verifying,
    CallingTool,
    WaitingForUser,
    Finished,
    WaitingForUnlock,
}

/// Proto `Progress`: what the UI shows while a session runs.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct SessionProgress {
    pub step: u32,
    pub phase: Phase,
    pub last_action: Option<Action>,
    pub app: Option<String>,
    /// Model requests sent so far (retries included).
    pub model_calls: u32,
    /// Re-inferences after unparsable model output so far.
    pub parse_retries: u32,
    /// Whether the session waits for the owner to unlock the phone.
    #[uniffi(default = false)]
    pub waiting_for_unlock: bool,
    /// How long the current wait for an unlock has lasted (0 when not waiting).
    #[uniffi(default = 0)]
    pub locked_wait_ms: u64,
    /// Total time spent waiting for unlocks in this session.
    #[uniffi(default = 0)]
    pub locked_total_ms: u64,
}

/// The task the owner asked for.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct TaskSpec {
    pub id: String,
    pub text: String,
}

/// An installed app the agent may open by name.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct AppInfo {
    pub bundle_id: String,
    pub name: String,
}

/// Everything a session is configured with. Start from `defaultSessionConfig()` in Swift.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct SessionConfig {
    pub policy: Policy,
    pub tools: Vec<ToolSpec>,
    pub model: ModelProfile,
    pub known_apps: Vec<AppInfo>,
    /// Bundle id of the Jarvis app itself (never needs first-use approval).
    pub agent_bundle_id: Option<String>,
    /// The skill this session runs under (owner's guidance in the prompt; with
    /// `confineToApps`, actions outside the skill's apps are refused). Defaults to `nil`,
    /// so existing Swift initializers keep compiling.
    #[uniffi(default = None)]
    pub skill: Option<SkillContext>,
}

// ---------------------------------------------------------------------------------------
// Skills.
// ---------------------------------------------------------------------------------------

/// Proto `SkillContext`: the skill a session runs under. Build it with
/// `JarvisSkills.context(skillId:args:)` or `contextForMatch(match:)`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct SkillContext {
    pub id: String,
    pub name: String,
    /// The owner's guidance (sanitised by the core before it enters a prompt).
    pub instructions: String,
    /// Bundle ids the skill works in.
    pub apps: Vec<String>,
    /// Act only inside `apps` (from the home screen only go home, wait or open one of
    /// `apps`; tools must act in `apps` or be listed in `tools`). Only narrows policy.
    pub confine_to_apps: bool,
    /// Parameter values (`contact` -> `Mom`).
    pub args: std::collections::HashMap<String, String>,
    /// App-less tools a confined skill may still call, by exact name. Defaults to empty.
    #[uniffi(default)]
    pub tools: Vec<String>,
}

/// Where a skill came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum SkillKind {
    Learned,
    Written,
    BuiltIn,
}

/// A skill parameter, filled by a `{name}` slot in a trigger.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct SkillParam {
    pub name: String,
    pub description: String,
    pub required: bool,
}

/// A named, reusable capability (`jarvis_skills::Skill`). Limits are enforced by
/// `JarvisSkills.upsert` (throws `JarvisError.invalidSkill`).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct Skill {
    /// `[a-z0-9-]{1,48}`, unique.
    pub id: String,
    pub name: String,
    pub summary: String,
    pub kind: SkillKind,
    /// Phrases with optional `{param}` slots: `"text {contact} {message}"`.
    pub triggers: Vec<String>,
    pub instructions: String,
    pub apps: Vec<String>,
    pub confine_to_apps: bool,
    pub params: Vec<SkillParam>,
    /// Intent keys of learned flows that replay the skill.
    pub flow_keys: Vec<String>,
    pub enabled: bool,
    pub created_ms: u64,
    pub updated_ms: u64,
    pub runs: u32,
    pub last_run_ms: Option<u64>,
    /// App-less tools a confined skill may still call, by exact name. Defaults to empty.
    #[uniffi(default)]
    pub tools: Vec<String>,
}

/// One skill matching a request.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct SkillMatch {
    pub skill_id: String,
    /// Per mille, 0 to 1000 (1000 = exact).
    pub score: u32,
    /// Values captured by slots.
    pub args: std::collections::HashMap<String, String>,
    pub matched_trigger: String,
    /// Required parameters not filled (ask the owner).
    pub missing: Vec<String>,
    /// An exact literal or fully matched slotted trigger. Only exact matches may run
    /// without asking the owner; a loose match is a suggestion.
    #[uniffi(default = false)]
    pub exact: bool,
}

/// The matches for a request, best first.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct SkillMatchResult {
    pub matches: Vec<SkillMatch>,
    /// The best two are too close to pick one: ask the owner which.
    pub ambiguous: bool,
    /// Ask the owner before running anything: `ambiguous`, or the best match is not exact.
    #[uniffi(default = false)]
    pub needs_owner_choice: bool,
}

// ---------------------------------------------------------------------------------------
// Workflows (`jarvis_skills::workflow`, `run`, `schedule`, `utterance`).
// ---------------------------------------------------------------------------------------

/// A day of the week, Monday first. Swift's `Calendar` numbers weekdays 1 (Sunday) to 7;
/// convert with `weekdayFromIndex(index:)` (0 = Monday … 6 = Sunday).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum Weekday {
    Mon,
    Tue,
    Wed,
    Thu,
    Fri,
    Sat,
    Sun,
}

/// A weekly time in the phone's local time: on `days` at `hour:minute`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct Schedule {
    /// Days it runs on; must not be empty. Order and duplicates do not matter (the core
    /// keeps a set, returned Monday first).
    pub days: Vec<Weekday>,
    /// 0 to 23.
    pub hour: u8,
    /// 0 to 59.
    pub minute: u8,
}

/// What starts a workflow.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum Trigger {
    /// Only when the owner starts it.
    Manual,
    /// When the owner says this phrase (normalised like the memory's intent keys).
    Phrase { text: String },
    /// On a weekly schedule (the shell asks `due`/`nextScheduled` with local time).
    Schedule { schedule: Schedule },
    /// When the shell reports this key (`forExternal(key:)`), e.g. from an iOS Shortcuts
    /// automation. `[a-z0-9._-]`, 1 to 64 characters.
    External { key: String },
}

/// A test on what happened so far, for `Step.stopUnless`. `lastSummaryContains` reads a
/// model-written summary: a convenience, never a safety gate.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum Condition {
    LastSucceeded,
    LastSummaryContains { text: String },
    OwnerSaidYes,
}

/// One step of a workflow.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum Step {
    /// Run a skill as an ordinary session (resolved when the step starts).
    RunSkill {
        skill_id: String,
        args: std::collections::HashMap<String, String>,
    },
    /// Run a free-text task as an ordinary session.
    RunTask { text: String },
    /// Ask the owner a yes/no question.
    AskOwner { question: String },
    /// Post a notification.
    Notify { text: String },
    /// Wait this many milliseconds (1 to 1 800 000).
    Wait { ms: u64 },
    /// End the run (`stopped`) unless the condition holds.
    StopUnless { condition: Condition },
}

/// What an ordinary step failure does. Blocked, cancelled and interrupted sessions always
/// end the run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum OnFailure {
    Stop,
    Continue,
}

/// How a workflow run ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum WorkflowStatus {
    Succeeded,
    /// Every step ran, at least one failed (with `OnFailure.continue`).
    CompletedWithFailures,
    Failed,
    /// A `stopUnless` condition did not hold.
    Stopped,
    Cancelled,
    /// A session was stopped by safety; always ends the run.
    Blocked,
    /// A session was interrupted (the phone stayed locked); always ends the run.
    Interrupted,
}

/// The result of a run.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct WorkflowOutcome {
    pub status: WorkflowStatus,
    /// Steps that ran (started), conditions included.
    pub steps_run: u32,
    /// One line for the owner.
    pub summary: String,
    /// 0-based index of the first step that failed.
    pub failed_step: Option<u32>,
}

/// An automation (`jarvis_skills::Workflow`). It never widens policy: every step that
/// touches the phone is an ordinary session under the owner's `Policy`. Limits are
/// enforced by `JarvisWorkflows.upsert` (throws `JarvisError.invalidWorkflow`).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct Workflow {
    /// `[a-z0-9-]{1,48}`, unique.
    pub id: String,
    pub name: String,
    /// Disabled workflows never trigger and refuse to run.
    pub enabled: bool,
    pub trigger: Trigger,
    /// 1 to 24 steps, in order.
    pub steps: Vec<Step>,
    pub on_failure: OnFailure,
    pub created_ms: u64,
    pub updated_ms: u64,
    pub runs: u32,
    pub last_run_ms: Option<u64>,
    pub last_outcome: Option<WorkflowOutcome>,
}

/// Limits for one run. Start from `defaultRunLimits()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct RunLimits {
    /// Most steps a workflow may have to run (24 at most in any case).
    pub max_steps: u32,
    /// Longest run from `start`, in the shell's milliseconds.
    pub max_total_ms: u64,
    /// Longest single wait, in milliseconds.
    pub max_wait_ms: u64,
}

/// An input to a `JarvisWorkflowRun`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum WorkflowEvent {
    Start,
    /// The session from `startSession` ended (its `Outcome.status` and `summary`).
    SessionFinished {
        status: OutcomeStatus,
        summary: String,
    },
    /// The owner answered `askOwner`.
    OwnerAnswered {
        yes: bool,
        text: String,
    },
    /// The notification from `notify` was posted.
    Notified,
    /// The wait from `wait` is over.
    WaitElapsed,
    /// The owner stopped the run (accepted in every state).
    Cancel,
}

/// The one thing the shell must do next for a workflow run.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum WorkflowEffect {
    /// Run an ordinary `JarvisSession` with `TaskSpec.text = task` and
    /// `SessionConfig.skill = skill`, under the owner's policy; answer `sessionFinished`.
    StartSession {
        task: String,
        skill: Option<SkillContext>,
    },
    /// Ask a yes/no question; answer `ownerAnswered`.
    AskOwner { question: String },
    /// Post a notification; answer `notified`.
    Notify { text: String },
    /// Answer `waitElapsed` once the shell's clock reaches `untilMs`.
    Wait { until_ms: u64 },
    /// The run is over; record it with `JarvisWorkflows.recordRun`.
    Finished { outcome: WorkflowOutcome },
}

/// What a workflow run is waiting for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum Awaiting {
    Start,
    Session,
    Answer,
    Notified,
    WaitElapsed,
    /// The run is over.
    Nothing,
}

/// The scheduled workflow that fires next.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ScheduledWorkflow {
    pub workflow: Workflow,
    /// Minutes from now until it fires (0: now), below one week.
    pub minutes_until: u32,
}

/// What to do with an utterance (`resolveUtterance`).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum Resolution {
    /// Run this workflow (its phrase matched exactly and no exact skill competes).
    RunWorkflow { workflow_id: String },
    /// Run this skill match (exact and unambiguous; check `missing` before starting).
    RunSkill { skill_match: SkillMatch },
    /// Ask the owner which one they meant (or confirm a loose suggestion).
    OwnerChoice,
    /// Neither: run the text as an ordinary task.
    NoMatch,
}

/// Every match for an utterance and the decision taken from them.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct UtteranceMatch {
    /// Enabled workflows whose phrase matches exactly, ordered by id.
    pub workflows: Vec<String>,
    /// Skill matches, best first.
    pub skills: Vec<SkillMatch>,
    pub resolution: Resolution,
    /// `resolution == .ownerChoice`: show the candidates and let the owner pick.
    pub needs_owner_choice: bool,
}

// ---------------------------------------------------------------------------------------
// Remote control (`jarvis_remote`).
//
// Device ids cross as 32 lowercase hex characters (they are not secret: they travel in
// clear on the wire). Connection ids are the shell's own `UInt64`s. Every timestamp the
// hub reports is on the hub's monotonic clock (`JarvisRemoteHub.clockMs()`).
// ---------------------------------------------------------------------------------------

/// What one paired device may do. Every field defaults to `false`, so `RemotePermissions()`
/// is everything off (like the persisted form: a missing field never grants anything). A
/// newly paired device gets `newDeviceRemotePermissions()` (start tasks, see progress).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Record)]
pub struct RemotePermissions {
    /// Start tasks, skills and workflows, and list them.
    #[uniffi(default = false)]
    pub start_tasks: bool,
    /// Receive progress, confirmation and finish events.
    #[uniffi(default = false)]
    pub see_progress: bool,
    /// Receive (redacted) free text: screen text, confirmation and finish summaries.
    #[uniffi(default = false)]
    pub see_screen_text: bool,
    /// Approve confirmations outside `neverRemoteApprove()`.
    #[uniffi(default = false)]
    pub approve: bool,
}

/// A paired device, without its key.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct DeviceInfo {
    /// 32 hex characters.
    pub id: String,
    pub name: String,
    pub permissions: RemotePermissions,
    /// Hub clock at pairing.
    pub paired_at_ms: u64,
    /// May watch the screen layout (the Live view: wireframes, where Jarvis taps, step
    /// captions; never pixels). Kept beside `permissions`, off by default and for devices
    /// stored before it existed; change it with `JarvisRemoteHub.setWatchScreen`.
    #[uniffi(default = false)]
    pub watch_screen: bool,
}

/// A skill or workflow remotes may list and run (`LocalEvent.catalogChanged`).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct CatalogEntry {
    pub id: String,
    pub name: String,
    pub summary: String,
}

/// The pairing offer to show on the phone. SECRET: `code` and `qrPayload` are the pairing
/// secret. Show them on screen only; never log them, never put them in `UserDefaults`, a
/// file, the pasteboard or analytics, and drop every copy (the QR image too) once the offer
/// is used, cancelled or expired. Its `Debug` output in Rust redacts both.
#[derive(Clone, PartialEq, Eq, uniffi::Record)]
pub struct PairingOffer {
    /// Grouped code to type (`ABCD-EFGH-…`, 39 characters).
    pub code: String,
    /// QR code content (`jarvis-remote:2:` + the ungrouped code).
    pub qr_payload: String,
    /// When the offer stops being accepted (hub clock).
    pub expires_at_ms: u64,
}

impl std::fmt::Debug for PairingOffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PairingOffer")
            .field("code", &"<redacted>")
            .field("qr_payload", &"<redacted>")
            .field("expires_at_ms", &self.expires_at_ms)
            .finish()
    }
}

/// Why the hub closed (or refused) a connection. For logs only; the remote is told nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum CloseReason {
    AuthFailed,
    Replayed,
    LockedOut,
    HandshakeTimeout,
    IdleTimeout,
    TooManyConnections,
    DuplicateConnection,
    TooLarge,
    Revoked,
    TooManyDevices,
    BadEntropy,
    PeerClosed,
    Internal,
}

/// Why a remote request was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum DenyReason {
    NotPermitted,
    Busy,
    NotRunning,
    StaleRequest,
    /// The category can only be decided on the phone (`neverRemoteApprove()`).
    NotRemotelyApprovable,
    UnknownSkill,
}

/// Why an authenticated remote message could not be served (Rust `ErrorCode`, renamed:
/// too generic a name for the Swift module namespace).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum RemoteErrorCode {
    Malformed,
    UnknownMessage,
    RateLimited,
    InvalidRequest,
    TooLarge,
}

/// What an `AuditEntry` records. Contains no secret material (codes and keys never reach
/// the audit log; task texts and decisions are redacted).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum AuditEvent {
    PairingExpired,
    Paired {
        name: String,
    },
    PairingAbandoned,
    Connected,
    Closed {
        reason: CloseReason,
    },
    ConnectionRefused {
        reason: CloseReason,
    },
    HandshakeFailed {
        failures: u32,
    },
    PreAuthSuppressed {
        count: u32,
        since_ms: u64,
    },
    LockedOut {
        until_ms: u64,
    },
    Accepted {
        request: String,
        detail: Option<String>,
    },
    Denied {
        request: String,
        reason: DenyReason,
    },
    Rejected {
        request: Option<String>,
        code: RemoteErrorCode,
    },
    Revoked,
    PermissionsChanged {
        permissions: RemotePermissions,
    },
}

/// One entry of the phone's remote-control audit log (append it on `HubEffect.audit`).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct AuditEntry {
    /// Hub clock.
    pub at_ms: u64,
    pub conn_id: Option<u64>,
    /// 32 hex characters, when a device is involved.
    pub device_id: Option<String>,
    pub event: AuditEvent,
}

/// What the shell must do for the hub. Carry them out IN ORDER: on `storeChanged`, write
/// `JarvisRemoteHub.toJson()` to the Keychain before performing any effect after it (a
/// new device's first message and the phone-nonce lease depend on it being durable).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum HubEffect {
    /// Write `bytes` to the connection.
    Send { conn_id: u64, bytes: Vec<u8> },
    /// Close the connection (the hub has already forgotten it; send nothing).
    Close { conn_id: u64, reason: CloseReason },
    /// Persist `JarvisRemoteHub.toJson()` to the Keychain NOW, before carrying out the
    /// effects that follow.
    StoreChanged,
    /// Start a free-text session for `deviceId`; report `sessionStarted(taskId:)` and
    /// `sessionFinished(taskId:outcome:)` (only the latter if it cannot start).
    StartTask {
        task_id: u64,
        device_id: String,
        text: String,
    },
    /// Run a catalog skill; check the arguments with `JarvisSkills.context(skillId:args:)`
    /// first. Report back as for `startTask`.
    RunSkill {
        task_id: u64,
        device_id: String,
        id: String,
        args: std::collections::HashMap<String, String>,
    },
    /// Run a catalog workflow. Report back as for `startTask`.
    RunWorkflow {
        task_id: u64,
        device_id: String,
        id: String,
    },
    /// Cancel the task only if `taskId` names what is running (`nil`: the owner's own
    /// task, which only an explicit remote `cancel` names).
    CancelTask {
        device_id: String,
        task_id: Option<u64>,
    },
    /// Answer the pending confirmation with `Event.userDecision(approved:)` only if
    /// `requestId` is still the prompt on screen and `category` matches it; then send
    /// `confirmResolved(requestId:approved:)`.
    Decide {
        request_id: u64,
        approved: bool,
        category: RiskCategory,
        device_id: String,
    },
    /// A device was paired (a `storeChanged` follows).
    DevicePaired { device: DeviceInfo },
    /// Append to the audit log.
    Audit { entry: AuditEntry },
}

/// What happened on the phone, for `JarvisRemoteHub.localEvent(nowMs:event:)`.
// UniFFI enums cannot box a variant; the event is moved once per call, so its size is moot.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum LocalEvent {
    /// A session started: `taskId` from the `startTask`/`runSkill`/`runWorkflow` effect it
    /// carries out, `nil` for a task the owner started on the phone.
    SessionStarted { task_id: Option<u64> },
    /// The session made progress. Pass the session's own `progress()` and, if remotes may
    /// see it, the current typed `Screen` (e.g. from `parseRunnerTextToScreen`): Rust
    /// builds the screen text from the redacted screen (secure fields emptied, secrets
    /// masked) and the hub redacts and caps it again. There is deliberately no way to pass
    /// free text here.
    Progress {
        progress: SessionProgress,
        screen: Option<Screen>,
    },
    /// The session shows a confirmation. Read `pendingRequestId()` right after.
    ConfirmRequested { request: ConfirmRequest },
    /// The confirmation `requestId` was answered (on the phone or after `decide`).
    ConfirmResolved { request_id: u64, approved: bool },
    /// A session ended, or a requested start failed.
    SessionFinished {
        task_id: Option<u64>,
        outcome: Outcome,
    },
    /// The owner revoked a device (`revokeDevice` does the same, with an unknown-id check).
    DeviceRevoked { device_id: String },
    /// The owner changed a device's permissions (`setPermissions` does the same, checked).
    PermissionsChanged {
        device_id: String,
        permissions: RemotePermissions,
    },
    /// The skills and workflows remotes may list and run.
    CatalogChanged {
        skills: Vec<CatalogEntry>,
        workflows: Vec<CatalogEntry>,
    },
}

/// Remote → phone commands (`JarvisRemoteClient.send(message:)`).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum ClientMessage {
    Hello,
    StartTask {
        text: String,
    },
    RunSkill {
        id: String,
        args: std::collections::HashMap<String, String>,
    },
    RunWorkflow {
        id: String,
    },
    Cancel,
    Decision {
        request_id: u64,
        approved: bool,
    },
    Subscribe,
    Unsubscribe,
    ListSkills,
    Ping,
}

/// Phone → remote messages, as a `JarvisRemoteClient` decodes them.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum ServerMessage {
    Welcome {
        protocol_version: u8,
        permissions: RemotePermissions,
        phone_name: Option<String>,
        busy: Option<bool>,
    },
    TaskStarted,
    Progress {
        step: u32,
        phase: Phase,
        app: Option<String>,
        /// Kind of the last action (`"tap"`, `"type"`), never its arguments.
        last_action: Option<String>,
        screen_text: Option<String>,
        waiting_for_unlock: bool,
    },
    ConfirmPending {
        request_id: u64,
        category: RiskCategory,
        summary: String,
        can_approve: bool,
    },
    ConfirmResolved {
        request_id: u64,
    },
    Finished {
        status: OutcomeStatus,
        summary: String,
    },
    Skills {
        skills: Vec<CatalogEntry>,
        workflows: Vec<CatalogEntry>,
    },
    PermissionsChanged {
        permissions: RemotePermissions,
    },
    Denied {
        reason: DenyReason,
    },
    Error {
        code: RemoteErrorCode,
    },
    Pong,
}

/// What a `JarvisRemoteClient` reports.
#[derive(Clone, PartialEq, Eq, uniffi::Enum)]
pub enum ClientEvent {
    /// Write these bytes to the phone.
    Send { bytes: Vec<u8> },
    /// Pairing succeeded. SECRET: `credentialsJson` holds the device key; store it in the
    /// Keychain (`kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly`), never log it. Pass
    /// it to `JarvisRemoteClient.connect(credentialsJson:entropy:)` later.
    Paired {
        device_id: String,
        credentials_json: String,
    },
    /// Authenticated both ways; commands can be sent.
    Connected,
    /// A message from the phone.
    Message { message: ServerMessage },
    /// An authenticated message this client could not decode (e.g. a newer phone).
    Undecodable { reason: String },
}

impl std::fmt::Debug for ClientEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClientEvent::Send { bytes } => f
                .debug_struct("Send")
                .field("bytes", &format_args!("{} bytes", bytes.len()))
                .finish(),
            ClientEvent::Paired { device_id, .. } => f
                .debug_struct("Paired")
                .field("device_id", device_id)
                .field("credentials_json", &"<redacted>")
                .finish(),
            ClientEvent::Connected => f.write_str("Connected"),
            ClientEvent::Message { message } => {
                f.debug_struct("Message").field("message", message).finish()
            }
            ClientEvent::Undecodable { reason } => f
                .debug_struct("Undecodable")
                .field("reason", reason)
                .finish(),
        }
    }
}

/// Why the phone ended a Live view stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum RemoteViewerEndReason {
    /// The controller asked to stop.
    Stopped,
    /// The owner turned "Watch the screen layout" off for this device.
    PermissionRevoked,
    /// A reason from a newer phone.
    Other,
}

/// The Live view stream as a `JarvisRemoteClient` follows it (`receiveAll`). Contains
/// wireframes, cursor targets and captions only (never pixels, never the approval
/// picture).
#[derive(Clone, PartialEq, Eq, uniffi::Enum)]
pub enum RemoteViewerEvent {
    /// One delta, in order: `JarvisViewer.applyDelta(bytes:)`.
    Delta { bytes: Vec<u8> },
    /// One part of a snapshot, in order: apply it like a delta (part 0 resets the view).
    Snapshot {
        bytes: Vec<u8>,
        part: u32,
        parts: u32,
    },
    /// The stream broke; the client asked the phone for a snapshot itself (the request is a
    /// `ClientEvent.send` beside it). Reported once per gap.
    ResyncRequested,
    /// The phone ended the stream.
    Ended { reason: RemoteViewerEndReason },
    /// The phone refused to stream ("Watch the screen layout" is off for this device).
    Denied { reason: DenyReason },
    /// The phone streams a newer, incompatible format (its version); watching stopped.
    Incompatible { version: u32 },
}

impl std::fmt::Debug for RemoteViewerEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RemoteViewerEvent::Delta { bytes } => f
                .debug_struct("Delta")
                .field("bytes", &format_args!("{} bytes", bytes.len()))
                .finish(),
            RemoteViewerEvent::Snapshot { bytes, part, parts } => f
                .debug_struct("Snapshot")
                .field("bytes", &format_args!("{} bytes", bytes.len()))
                .field("part", part)
                .field("parts", parts)
                .finish(),
            RemoteViewerEvent::ResyncRequested => f.write_str("ResyncRequested"),
            RemoteViewerEvent::Ended { reason } => {
                f.debug_struct("Ended").field("reason", reason).finish()
            }
            RemoteViewerEvent::Denied { reason } => {
                f.debug_struct("Denied").field("reason", reason).finish()
            }
            RemoteViewerEvent::Incompatible { version } => f
                .debug_struct("Incompatible")
                .field("version", version)
                .finish(),
        }
    }
}

/// Everything one `JarvisRemoteClient.receiveAll(bytes:)` produced.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ClientReceived {
    /// As `receive` returns them (write every `send`).
    pub events: Vec<ClientEvent>,
    /// The Live view stream, in order.
    pub viewer: Vec<RemoteViewerEvent>,
}

/// Why entropy from the shell was refused (`JarvisError.remoteEntropy`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum EntropyProblem {
    /// Fewer than `remoteMinEntropyLen()` (32) bytes.
    TooShort,
    /// Obviously not random (all zero, constant, a short repeated pattern).
    NotRandom,
}

impl std::fmt::Debug for RunnerCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RunnerCommand::Tap { x, y } => {
                f.debug_struct("Tap").field("x", x).field("y", y).finish()
            }
            RunnerCommand::LongPress { x, y, duration_ms } => f
                .debug_struct("LongPress")
                .field("x", x)
                .field("y", y)
                .field("duration_ms", duration_ms)
                .finish(),
            RunnerCommand::Swipe {
                from,
                to,
                duration_ms,
            } => f
                .debug_struct("Swipe")
                .field("from", from)
                .field("to", to)
                .field("duration_ms", duration_ms)
                .finish(),
            RunnerCommand::TypeText { text } => f
                .debug_struct("TypeText")
                .field("text_chars", &text.chars().count())
                .finish_non_exhaustive(),
            RunnerCommand::PressKey { key } => {
                f.debug_struct("PressKey").field("key", key).finish()
            }
            RunnerCommand::LaunchApp { bundle_id } => f
                .debug_struct("LaunchApp")
                .field("bundle_id", bundle_id)
                .finish(),
            RunnerCommand::OpenUrl { url } => f.debug_struct("OpenUrl").field("url", url).finish(),
            RunnerCommand::Wait { ms } => f.debug_struct("Wait").field("ms", ms).finish(),
        }
    }
}

impl std::fmt::Debug for Event {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Event::Start => f.write_str("Start"),
            Event::Observed { observation } => f
                .debug_struct("Observed")
                .field("observation", observation)
                .finish(),
            Event::ObservedRunnerText { snapshot } => f
                .debug_struct("ObservedRunnerText")
                .field("snapshot", snapshot)
                .finish(),
            Event::ModelOutput { output } => f
                .debug_struct("ModelOutput")
                .field("output", output)
                .finish(),
            Event::ModelFailed { error } => {
                f.debug_struct("ModelFailed").field("error", error).finish()
            }
            Event::ActResult { result } => {
                f.debug_struct("ActResult").field("result", result).finish()
            }
            Event::ToolResult { result } => f
                .debug_struct("ToolResult")
                .field("result", result)
                .finish(),
            Event::UserDecision { approved } => f
                .debug_struct("UserDecision")
                .field("approved", approved)
                .finish(),
            Event::UserAnswer { text } => f
                .debug_struct("UserAnswer")
                .field("text_chars", &text.chars().count())
                .finish_non_exhaustive(),
            Event::Cancel => f.write_str("Cancel"),
            Event::DeviceLocked => f.write_str("DeviceLocked"),
            Event::DeviceUnlocked => f.write_str("DeviceUnlocked"),
            Event::Tick => f.write_str("Tick"),
            Event::ObserveFailed { reason } => f
                .debug_struct("ObserveFailed")
                .field("reason", reason)
                .finish(),
        }
    }
}
