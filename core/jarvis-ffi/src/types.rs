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
//! - [`Event::ObservedRunnerText`] has no proto counterpart: it carries the runner's raw
//!   text tree, which Rust parses (see `crate::runner`).
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

/// A command for the XCUITest runner, in absolute screen points.
#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
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

/// One input to the session.
#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum Event {
    Start,
    /// A typed screen (tests, simulators, or a shell that builds the tree itself).
    Observed {
        observation: ScreenObservation,
    },
    /// The device path: the runner's raw text tree, parsed in Rust. A tree that cannot be
    /// parsed becomes an observation of an empty screen of that app (see `crate::runner`).
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
