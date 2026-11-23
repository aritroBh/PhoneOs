//! proto → ffi → proto round trips for every mirror type and every enum variant.
//!
//! Variant coverage is enforced, not hoped for: each `*_variant` function below matches
//! exhaustively (no `_` arm), so a new proto variant fails to compile here, and each test
//! asserts its samples hit every variant index.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::collections::BTreeSet;
use std::fmt::Debug;

use jarvis_core_ffi as f;
use jarvis_proto as p;
use jarvis_proto::fixtures;
use serde_json::json;

/// proto → ffi → proto must be the identity.
fn rt<P, F>(value: P)
where
    P: Clone + PartialEq + Debug + Into<F>,
    F: Into<P> + Clone + Debug,
{
    let ffi: F = value.clone().into();
    let back: P = ffi.clone().into();
    assert_eq!(back, value, "via {ffi:?}");
}

/// The same for mirrors whose ffi → proto direction is fallible (JSON strings).
fn rt_try<P, F>(value: P)
where
    P: Clone + PartialEq + Debug + Into<F>,
    F: TryInto<P> + Clone + Debug,
    <F as TryInto<P>>::Error: Debug,
{
    let ffi: F = value.clone().into();
    let back: P = ffi.clone().try_into().unwrap();
    assert_eq!(back, value, "via {ffi:?}");
}

fn assert_covers(seen: impl IntoIterator<Item = usize>, count: usize, what: &str) {
    let seen: BTreeSet<usize> = seen.into_iter().collect();
    let all: BTreeSet<usize> = (0..count).collect();
    assert_eq!(seen, all, "{what}: not every variant has a round-trip case");
}

fn rect(i: f64) -> p::Rect {
    p::Rect::new(i, i + 0.5, 10.25 * i, 3.0)
}

fn image() -> p::ImageRef {
    p::ImageRef {
        id: "shot-1".to_owned(),
        width: 1179,
        height: 2556,
    }
}

// ---------------------------------------------------------------------------------------
// Exhaustive variant indices.
// ---------------------------------------------------------------------------------------

fn role_variant(r: &p::Role) -> usize {
    match r {
        p::Role::Button => 0,
        p::Role::Link => 1,
        p::Role::TextField => 2,
        p::Role::SecureTextField => 3,
        p::Role::TextView => 4,
        p::Role::SearchField => 5,
        p::Role::StaticText => 6,
        p::Role::Image => 7,
        p::Role::Cell => 8,
        p::Role::Switch => 9,
        p::Role::Slider => 10,
        p::Role::Picker => 11,
        p::Role::Tab => 12,
        p::Role::NavigationBar => 13,
        p::Role::TabBar => 14,
        p::Role::Toolbar => 15,
        p::Role::Alert => 16,
        p::Role::Sheet => 17,
        p::Role::Keyboard => 18,
        p::Role::Key => 19,
        p::Role::ScrollView => 20,
        p::Role::Table => 21,
        p::Role::CollectionView => 22,
        p::Role::WebView => 23,
        p::Role::Window => 24,
        p::Role::Application => 25,
        p::Role::Other(_) => 26,
    }
}

fn orientation_variant(o: &p::Orientation) -> usize {
    match o {
        p::Orientation::Portrait => 0,
        p::Orientation::PortraitUpsideDown => 1,
        p::Orientation::LandscapeLeft => 2,
        p::Orientation::LandscapeRight => 3,
    }
}

fn direction_variant(d: &p::Direction) -> usize {
    match d {
        p::Direction::Up => 0,
        p::Direction::Down => 1,
        p::Direction::Left => 2,
        p::Direction::Right => 3,
    }
}

fn action_variant(a: &p::Action) -> usize {
    match a {
        p::Action::Tap { .. } => 0,
        p::Action::LongPress { .. } => 1,
        p::Action::Type { .. } => 2,
        p::Action::Clear { .. } => 3,
        p::Action::Scroll { .. } => 4,
        p::Action::Swipe { .. } => 5,
        p::Action::TapPoint { .. } => 6,
        p::Action::PressHome => 7,
        p::Action::OpenApp { .. } => 8,
        p::Action::OpenUrl { .. } => 9,
        p::Action::Wait { .. } => 10,
        p::Action::CallTool { .. } => 11,
        p::Action::AskUser { .. } => 12,
        p::Action::Look => 13,
        p::Action::Done { .. } => 14,
        p::Action::Fail { .. } => 15,
    }
}

fn key_variant(k: &p::Key) -> usize {
    match k {
        p::Key::Return => 0,
        p::Key::Delete => 1,
        p::Key::Tab => 2,
        p::Key::Space => 3,
        p::Key::Escape => 4,
        p::Key::Home => 5,
    }
}

fn command_variant(c: &p::RunnerCommand) -> usize {
    match c {
        p::RunnerCommand::Tap { .. } => 0,
        p::RunnerCommand::LongPress { .. } => 1,
        p::RunnerCommand::Swipe { .. } => 2,
        p::RunnerCommand::TypeText { .. } => 3,
        p::RunnerCommand::PressKey { .. } => 4,
        p::RunnerCommand::LaunchApp { .. } => 5,
        p::RunnerCommand::OpenUrl { .. } => 6,
        p::RunnerCommand::Wait { .. } => 7,
    }
}

fn param_kind_variant(k: &p::ParamKind) -> usize {
    match k {
        p::ParamKind::String => 0,
        p::ParamKind::Integer => 1,
        p::ParamKind::Number => 2,
        p::ParamKind::Boolean => 3,
    }
}

fn tier_variant(t: &p::Tier) -> usize {
    match t {
        p::Tier::Native => 0,
        p::Tier::Shortcut => 1,
        p::Tier::Ui => 2,
    }
}

fn request_kind_variant(k: &p::RequestKind) -> usize {
    match k {
        p::RequestKind::Text => 0,
        p::RequestKind::Vision => 1,
    }
}

fn risk_variant(r: &p::RiskCategory) -> usize {
    match r {
        p::RiskCategory::None => 0,
        p::RiskCategory::SendMessage => 1,
        p::RiskCategory::Payment => 2,
        p::RiskCategory::Purchase => 3,
        p::RiskCategory::Delete => 4,
        p::RiskCategory::PublicPost => 5,
        p::RiskCategory::AccountChange => 6,
        p::RiskCategory::SystemSetting => 7,
        p::RiskCategory::Credential => 8,
        p::RiskCategory::Unverified => 9,
        p::RiskCategory::Call => 10,
        p::RiskCategory::FirstUse => 11,
    }
}

fn mode_variant(m: &p::PolicyMode) -> usize {
    match m {
        p::PolicyMode::Guarded => 0,
        p::PolicyMode::StepByStep => 1,
    }
}

fn level_variant(l: &p::TrustLevel) -> usize {
    match l {
        p::TrustLevel::Trusted => 0,
        p::TrustLevel::Guarded => 1,
        p::TrustLevel::StepByStep => 2,
        p::TrustLevel::Denied => 3,
    }
}

fn event_variant(e: &p::Event) -> usize {
    match e {
        p::Event::Start => 0,
        p::Event::Observed(_) => 1,
        p::Event::ModelOutput(_) => 2,
        p::Event::ModelFailed { .. } => 3,
        p::Event::ActResult(_) => 4,
        p::Event::ToolResult(_) => 5,
        p::Event::UserDecision { .. } => 6,
        p::Event::UserAnswer { .. } => 7,
        p::Event::Cancel => 8,
        p::Event::DeviceLocked => 9,
        p::Event::DeviceUnlocked => 10,
        p::Event::Tick => 11,
        p::Event::ObserveFailed { .. } => 12,
        // No FFI round trip: its FFI form carries the runner's raw text (see
        // `runner_text_events_reach_the_session_marked_as_runner_text`).
        p::Event::ObservedRunnerText(_) => 13,
    }
}

fn status_variant(s: &p::OutcomeStatus) -> usize {
    match s {
        p::OutcomeStatus::Success => 0,
        p::OutcomeStatus::Failed => 1,
        p::OutcomeStatus::Cancelled => 2,
        p::OutcomeStatus::Blocked => 3,
        p::OutcomeStatus::Interrupted => 4,
    }
}

fn effect_variant(e: &p::Effect) -> usize {
    match e {
        p::Effect::Observe { .. } => 0,
        p::Effect::Infer(_) => 1,
        p::Effect::Act(_) => 2,
        p::Effect::CallTool(_) => 3,
        p::Effect::Confirm(_) => 4,
        p::Effect::AskUser { .. } => 5,
        p::Effect::Finished(_) => 6,
        p::Effect::AwaitUnlock { .. } => 7,
    }
}

fn phase_variant(ph: &p::Phase) -> usize {
    match ph {
        p::Phase::Idle => 0,
        p::Phase::Observing => 1,
        p::Phase::Thinking => 2,
        p::Phase::Acting => 3,
        p::Phase::Verifying => 4,
        p::Phase::CallingTool => 5,
        p::Phase::WaitingForUser => 6,
        p::Phase::Finished => 7,
        p::Phase::WaitingForUnlock => 8,
    }
}

// ---------------------------------------------------------------------------------------
// Samples.
// ---------------------------------------------------------------------------------------

fn all_roles() -> Vec<p::Role> {
    let names = [
        "button",
        "link",
        "text_field",
        "secure_text_field",
        "text_view",
        "search_field",
        "static_text",
        "image",
        "cell",
        "switch",
        "slider",
        "picker",
        "tab",
        "navigation_bar",
        "tab_bar",
        "toolbar",
        "alert",
        "sheet",
        "keyboard",
        "key",
        "scroll_view",
        "table",
        "collection_view",
        "web_view",
        "window",
        "application",
        "Type<47>",
    ];
    let mut roles: Vec<p::Role> = names.iter().map(|n| p::Role::from_name(n)).collect();
    // An `Other` whose name is also a known role must stay `Other`.
    roles.push(p::Role::Other("button".to_owned()));
    roles.push(p::Role::Other(String::new()));
    roles
}

fn fixture_screens() -> Vec<p::Screen> {
    vec![
        fixtures::settings_screen(),
        fixtures::login_screen(),
        fixtures::compose_screen(),
    ]
}

/// A node using every role, every optional field and every flag in both states.
fn kitchen_sink_tree() -> p::UiNode {
    let children = all_roles()
        .into_iter()
        .enumerate()
        .map(|(i, role)| {
            let i_f = i as f64;
            let mut n = p::UiNode::new(role, rect(i_f));
            n.label = (i % 2 == 0).then(|| format!("label {i} \"quoted\" ünïcødé 👋"));
            n.value = (i % 3 == 0).then(|| format!("value {i}"));
            n.identifier = (i % 4 == 0).then(|| format!("id.{i}"));
            n.placeholder = (i % 5 == 0).then(String::new);
            n.enabled = i % 2 == 0;
            n.visible = i % 3 != 0;
            n.hittable = i % 4 == 0;
            n.focused = i % 5 == 0;
            n.selected = i % 6 == 0;
            n.secure = i % 7 == 0;
            n
        })
        .collect();
    // And some depth.
    let mut deep = p::UiNode::new(p::Role::StaticText, rect(-1.0)).with_label("leaf");
    for d in 0..40 {
        deep = p::UiNode::new(p::Role::Other(format!("level{d}")), rect(d as f64)).push_child(deep);
    }
    p::UiNode::new(p::Role::Application, rect(0.0))
        .with_children(children)
        .push_child(deep)
}

fn all_actions() -> Vec<p::Action> {
    vec![
        p::Action::Tap { element: 17 },
        p::Action::LongPress {
            element: 3,
            duration_ms: 800,
        },
        p::Action::Type {
            element: Some(4),
            text: "hello {email}".to_owned(),
            submit: true,
        },
        p::Action::Type {
            element: None,
            text: String::new(),
            submit: false,
        },
        p::Action::Clear { element: 2 },
        p::Action::Scroll {
            direction: p::Direction::Down,
            element: None,
        },
        p::Action::Scroll {
            direction: p::Direction::Up,
            element: Some(9),
        },
        p::Action::Swipe {
            direction: p::Direction::Left,
            element: Some(1),
        },
        p::Action::Swipe {
            direction: p::Direction::Right,
            element: None,
        },
        p::Action::TapPoint { x: 0.25, y: 0.75 },
        p::Action::PressHome,
        p::Action::OpenApp {
            app: "Messages".to_owned(),
        },
        p::Action::OpenUrl {
            url: "https://example.com/a?b=c".to_owned(),
        },
        p::Action::Wait { ms: u64::MAX },
        p::Action::CallTool {
            name: "calendar.create".to_owned(),
            args: json!({"title": "Dinner", "at": 1_700_000_000u64, "attendees": ["a", "b"], "nested": {"x": 1.5, "y": null, "z": true}}),
        },
        p::Action::CallTool {
            name: "noop".to_owned(),
            args: serde_json::Value::Null,
        },
        p::Action::AskUser {
            question: "Which Alex?".to_owned(),
        },
        p::Action::Look,
        p::Action::Done {
            summary: "Done.".to_owned(),
        },
        p::Action::Fail {
            reason: "Blocked by Face ID".to_owned(),
        },
    ]
}

fn all_commands() -> Vec<p::RunnerCommand> {
    let mut v = vec![
        p::RunnerCommand::Tap { x: 196.5, y: 400.0 },
        p::RunnerCommand::LongPress {
            x: 1.0,
            y: 2.0,
            duration_ms: 600,
        },
        p::RunnerCommand::Swipe {
            from: p::Point::new(200.0, 700.0),
            to: p::Point::new(200.0, 200.0),
            duration_ms: 300,
        },
        p::RunnerCommand::TypeText {
            text: "See you at 7".to_owned(),
        },
        p::RunnerCommand::LaunchApp {
            bundle_id: "com.apple.MobileSMS".to_owned(),
        },
        p::RunnerCommand::OpenUrl {
            url: "https://example.com".to_owned(),
        },
        p::RunnerCommand::Wait { ms: 500 },
    ];
    for key in [
        p::Key::Return,
        p::Key::Delete,
        p::Key::Tab,
        p::Key::Space,
        p::Key::Escape,
        p::Key::Home,
    ] {
        v.push(p::RunnerCommand::PressKey { key });
    }
    v
}

fn all_tools() -> Vec<p::ToolSpec> {
    let kinds = [
        p::ParamKind::String,
        p::ParamKind::Integer,
        p::ParamKind::Number,
        p::ParamKind::Boolean,
    ];
    let tiers = [p::Tier::Native, p::Tier::Shortcut, p::Tier::Ui];
    p::RiskCategory::ALL
        .iter()
        .enumerate()
        .map(|(i, risk)| p::ToolSpec {
            name: format!("tool{i}"),
            description: format!("does thing {i}"),
            params: kinds
                .iter()
                .enumerate()
                .map(|(j, kind)| p::ToolParam {
                    name: format!("p{j}"),
                    description: String::new(),
                    kind: *kind,
                    required: j % 2 == 0,
                })
                .take(i % 5)
                .collect(),
            tier: tiers[i % tiers.len()],
            risk: *risk,
            app: (i % 2 == 0).then(|| "com.apple.MobileSMS".to_owned()),
        })
        .collect()
}

fn model_requests() -> Vec<p::ModelRequest> {
    vec![
        p::ModelRequest::default(),
        p::ModelRequest {
            system: jarvis_agent::system_prompt(),
            user: "Task: …\n[1] Button \"OK\"".to_owned(),
            image: Some(image()),
            marks: vec![
                p::Mark {
                    index: 1,
                    frame: rect(1.0),
                },
                p::Mark {
                    index: 2,
                    frame: rect(2.0),
                },
            ],
            max_tokens: 256,
            grammar: Some(jarvis_agent::action_gbnf()),
            json_schema: Some(jarvis_agent::action_json_schema()),
            kind: p::RequestKind::Vision,
        },
    ]
}

fn outcomes() -> Vec<p::Outcome> {
    [
        p::OutcomeStatus::Success,
        p::OutcomeStatus::Failed,
        p::OutcomeStatus::Cancelled,
        p::OutcomeStatus::Blocked,
        p::OutcomeStatus::Interrupted,
    ]
    .into_iter()
    .enumerate()
    .map(|(i, status)| p::Outcome {
        status,
        summary: format!("summary {i}"),
        steps: i as u32,
        duration_ms: 1000 * i as u64,
    })
    .collect()
}

fn all_events() -> Vec<p::Event> {
    let mut v = vec![
        p::Event::Start,
        p::Event::ModelOutput(p::ModelOutput {
            text: r#"{"action":"tap","element":1}"#.to_owned(),
        }),
        p::Event::ModelFailed {
            error: "out of memory".to_owned(),
        },
        p::Event::ActResult(p::RunnerResult::ok()),
        p::Event::ActResult(p::RunnerResult::err("element not hittable")),
        p::Event::ToolResult(p::ToolResult {
            ok: true,
            output: "created".to_owned(),
            error: None,
        }),
        p::Event::ToolResult(p::ToolResult {
            ok: false,
            output: String::new(),
            error: Some("denied".to_owned()),
        }),
        p::Event::UserDecision { approved: true },
        p::Event::UserDecision { approved: false },
        p::Event::UserAnswer {
            text: "the one from work".to_owned(),
        },
        p::Event::Cancel,
        p::Event::ActResult(p::RunnerResult::device_locked()),
        p::Event::ActResult(p::RunnerResult {
            ok: false,
            error: None,
            failure: Some(p::RunnerFailure::Other),
        }),
        p::Event::DeviceLocked,
        p::Event::DeviceUnlocked,
        p::Event::Tick,
        p::Event::ObserveFailed {
            reason: "snapshot timed out".to_owned(),
        },
    ];
    for screen in fixture_screens() {
        v.push(p::Event::Observed(fixtures::observation(screen.clone())));
        v.push(p::Event::Observed(p::Observation {
            screen: screen.clone(),
            screenshot: Some(image()),
            device_locked: false,
        }));
        v.push(p::Event::Observed(p::Observation {
            device_locked: true,
            ..fixtures::observation(screen)
        }));
    }
    v
}

fn all_effects() -> Vec<p::Effect> {
    let mut v = vec![
        p::Effect::Observe { screenshot: false },
        p::Effect::Observe { screenshot: true },
        p::Effect::CallTool(p::ToolCall {
            name: "reminders.add".to_owned(),
            args: json!({"title": "milk"}),
        }),
        p::Effect::Confirm(p::ConfirmRequest {
            category: p::RiskCategory::Payment,
            summary: "Pay $12".to_owned(),
            app: "com.example.shop".to_owned(),
            phone_only: false,
        }),
        p::Effect::AskUser {
            question: "Which one?".to_owned(),
        },
        p::Effect::AwaitUnlock {
            reason: "Unlock to continue".to_owned(),
            waited_ms: 61_000,
        },
    ];
    v.extend(model_requests().into_iter().map(p::Effect::Infer));
    v.extend(all_commands().into_iter().map(p::Effect::Act));
    v.extend(outcomes().into_iter().map(p::Effect::Finished));
    v
}

// ---------------------------------------------------------------------------------------
// Tests.
// ---------------------------------------------------------------------------------------

#[test]
fn geometry_round_trips() {
    rt::<_, f::Point>(p::Point::new(-1.5, 1e9));
    rt::<_, f::Size>(p::Size::new(393.0, 852.0));
    rt::<_, f::Rect>(p::Rect::new(0.1, 0.2, 0.3, 0.4));
    rt::<_, f::Rect>(p::Rect::default());
}

#[test]
fn every_role_round_trips() {
    let roles = all_roles();
    assert_covers(roles.iter().map(role_variant), 27, "Role");
    for r in roles {
        rt::<_, f::Role>(r);
    }
}

#[test]
fn ui_trees_round_trip() {
    for screen in fixture_screens() {
        rt::<_, f::UiNode>(screen.root.clone());
        rt::<_, f::Screen>(screen);
    }
    let sink = kitchen_sink_tree();
    let ffi: f::UiNode = sink.clone().into();
    assert_eq!(ffi.children.len(), sink.children.len());
    rt::<_, f::UiNode>(sink);
}

#[test]
fn screens_observations_and_orientations_round_trip() {
    let orientations = [
        p::Orientation::Portrait,
        p::Orientation::PortraitUpsideDown,
        p::Orientation::LandscapeLeft,
        p::Orientation::LandscapeRight,
    ];
    assert_covers(
        orientations.iter().map(orientation_variant),
        4,
        "Orientation",
    );
    for (o, base) in orientations
        .into_iter()
        .zip(fixture_screens().into_iter().cycle())
    {
        rt::<_, f::Orientation>(o);
        let mut s = base;
        s.orientation = o;
        s.app_name = None;
        rt::<_, f::Screen>(s.clone());
        rt::<_, f::ScreenObservation>(fixtures::observation(s.clone()));
        rt::<_, f::ScreenObservation>(p::Observation {
            screen: s,
            screenshot: Some(image()),
            device_locked: true,
        });
    }
    rt::<_, f::ImageRef>(image());
}

#[test]
fn every_action_round_trips() {
    let actions = all_actions();
    assert_covers(actions.iter().map(action_variant), 16, "Action");
    let dirs: Vec<p::Direction> = actions
        .iter()
        .filter_map(|a| match a {
            p::Action::Scroll { direction, .. } | p::Action::Swipe { direction, .. } => {
                Some(*direction)
            }
            _ => None,
        })
        .collect();
    assert_covers(dirs.iter().map(direction_variant), 4, "Direction");
    for a in actions {
        rt_try::<_, f::Action>(a);
    }
}

#[test]
fn every_runner_command_and_key_round_trips() {
    let commands = all_commands();
    assert_covers(commands.iter().map(command_variant), 8, "RunnerCommand");
    let keys: Vec<p::Key> = commands
        .iter()
        .filter_map(|c| match c {
            p::RunnerCommand::PressKey { key } => Some(*key),
            _ => None,
        })
        .collect();
    assert_covers(keys.iter().map(key_variant), 6, "Key");
    for c in commands {
        rt::<_, f::RunnerCommand>(c);
    }
    rt::<_, f::RunnerResult>(p::RunnerResult::ok());
    rt::<_, f::RunnerResult>(p::RunnerResult::err("nope"));
    rt::<_, f::RunnerResult>(p::RunnerResult::device_locked());
    let locked: f::RunnerResult = p::RunnerResult::device_locked().into();
    assert_eq!(locked.failure, Some(f::RunnerFailure::DeviceLocked));
}

#[test]
fn tools_round_trip() {
    let tools = all_tools();
    assert_covers(
        tools.iter().map(|t| risk_variant(&t.risk)),
        12,
        "RiskCategory",
    );
    assert_covers(tools.iter().map(|t| tier_variant(&t.tier)), 3, "Tier");
    assert_covers(
        tools
            .iter()
            .flat_map(|t| t.params.iter().map(|p| param_kind_variant(&p.kind))),
        4,
        "ParamKind",
    );
    for t in tools {
        for param in &t.params {
            rt::<_, f::ToolParam>(param.clone());
        }
        rt::<_, f::RiskCategory>(t.risk);
        rt::<_, f::Tier>(t.tier);
        rt::<_, f::ToolSpec>(t);
    }
    rt_try::<_, f::ToolCall>(p::ToolCall {
        name: "x".to_owned(),
        args: json!({"a": [1, 2, {"b": "c"}]}),
    });
    rt::<_, f::ToolResult>(p::ToolResult::default());
}

#[test]
fn model_io_round_trips() {
    let requests = model_requests();
    assert_covers(
        requests.iter().map(|r| request_kind_variant(&r.kind)),
        2,
        "RequestKind",
    );
    for r in requests {
        rt::<_, f::ModelRequest>(r);
    }
    rt::<_, f::ModelOutput>(p::ModelOutput {
        text: "```json\n{}\n```".to_owned(),
    });
    rt::<_, f::ModelProfile>(p::ModelProfile::default());
    rt::<_, f::ModelProfile>(p::ModelProfile {
        context_tokens: 128_000,
        supports_vision: true,
        supports_grammar: true,
    });
    rt::<_, f::Mark>(p::Mark {
        index: 7,
        frame: rect(7.0),
    });
}

#[test]
fn policy_task_and_config_round_trip() {
    rt::<_, f::Policy>(p::Policy::default());
    rt::<_, f::Policy>(p::Policy {
        denied_apps: vec![],
        confirm: vec![],
        deny_categories: vec![],
        mode: p::PolicyMode::Guarded,
        app_rules: vec![],
        first_use_approval: false,
        max_steps: 0,
        max_duration_ms: u64::MAX,
        max_consecutive_failures: 1,
        max_parse_retries: 0,
        max_locked_wait_ms: 0,
    });
    // Every mode, trust level and category, in every list.
    let modes = [p::PolicyMode::Guarded, p::PolicyMode::StepByStep];
    let levels = [
        p::TrustLevel::Trusted,
        p::TrustLevel::Guarded,
        p::TrustLevel::StepByStep,
        p::TrustLevel::Denied,
    ];
    assert_covers(modes.iter().map(mode_variant), 2, "PolicyMode");
    assert_covers(levels.iter().map(level_variant), 4, "TrustLevel");
    for mode in modes {
        rt::<_, f::Policy>(p::Policy {
            denied_apps: vec!["com.examplebank".to_owned()],
            confirm: p::RiskCategory::ALL.to_vec(),
            deny_categories: vec![p::RiskCategory::Payment, p::RiskCategory::FirstUse],
            mode,
            app_rules: levels
                .iter()
                .enumerate()
                .map(|(i, l)| {
                    let rule = p::AppRule::new(format!("com.example.app{i}"), *l);
                    // Display names cross the bridge both ways (and may be absent).
                    if i % 2 == 0 {
                        rule.with_name(format!("App {i}"))
                    } else {
                        rule
                    }
                })
                .collect(),
            first_use_approval: true,
            ..p::Policy::default()
        });
    }
    let fp: f::Policy = p::Policy::default().into();
    assert!(fp.first_use_approval);
    assert_eq!(fp.mode, f::PolicyMode::Guarded);
    assert!(fp.confirm.contains(&f::RiskCategory::Unverified));
    assert!(fp.confirm.contains(&f::RiskCategory::Call));
    assert!(fp.confirm.contains(&f::RiskCategory::FirstUse));
    assert_eq!(fp.max_locked_wait_ms, 1_800_000);
    rt::<_, f::TaskSpec>(p::TaskSpec::new("id-1", "Text Alex I'm late"));
    rt::<_, f::AppInfo>(p::AppInfo::new("com.apple.MobileSMS", "Messages"));
    rt::<_, f::SessionConfig>(p::SessionConfig::default());
    rt::<_, f::SessionConfig>(p::SessionConfig {
        policy: p::Policy::default(),
        tools: all_tools(),
        model: p::ModelProfile {
            context_tokens: 8192,
            supports_vision: true,
            supports_grammar: false,
        },
        known_apps: vec![
            p::AppInfo::new("com.apple.Preferences", "Settings"),
            p::AppInfo::new("com.apple.MobileSMS", "Messages"),
        ],
        agent_bundle_id: Some("com.example.jarvis".to_owned()),
        skill: Some(p::SkillContext {
            id: "text-someone".to_owned(),
            name: "Text someone".to_owned(),
            instructions: "Use the existing thread.\nKeep it short.".to_owned(),
            apps: vec!["com.apple.MobileSMS".to_owned()],
            confine_to_apps: true,
            tools: vec!["open_link".to_owned()],
            args: [("contact", "Mom"), ("message", "on my way")]
                .into_iter()
                .map(|(k, v)| (k.to_owned(), v.to_owned()))
                .collect(),
        }),
    });
}

#[test]
fn every_event_round_trips() {
    let events = all_events();
    assert_covers(events.iter().map(event_variant), 13, "Event");
    for e in events {
        let ffi: f::Event = e.clone().into();
        let (back, issue) = f::event_to_proto(ffi.clone());
        assert_eq!(back, e, "via {ffi:?}");
        assert_eq!(issue, None);
    }
}

/// The runner's text becomes `ObservedRunnerText` for the session (fifth review: the
/// session treats such a screen as unverified), and maps back to a plain `Observed`.
#[test]
fn runner_text_events_reach_the_session_marked_as_runner_text() {
    let snapshot = f::RunnerSnapshot {
        source: "Application, label: 'Shop', {{0, 0}, {393, 852}}\n  Button, label: 'Pay', {{16, 700}, {361, 50}}\n".to_owned(),
        app_bundle_id: "com.example.shop".to_owned(),
        app_name: Some("Shop".to_owned()),
        size: p::Size::new(393.0, 852.0).into(),
        keyboard_visible: false,
        orientation: p::Orientation::Portrait.into(),
        screenshot: None,
        device_locked: false,
    };
    let (e, issue) = f::event_to_proto(f::Event::ObservedRunnerText { snapshot });
    assert_eq!(issue, None);
    let p::Event::ObservedRunnerText(obs) = e.clone() else {
        panic!("{e:?}")
    };
    assert_eq!(obs.screen.root.children.len(), 1);
    let back: f::Event = e.into();
    assert!(matches!(back, f::Event::Observed { .. }), "{back:?}");
}

#[test]
fn every_effect_outcome_and_confirm_round_trips() {
    let effects = all_effects();
    assert_covers(effects.iter().map(effect_variant), 8, "Effect");
    let statuses: Vec<usize> = effects
        .iter()
        .filter_map(|e| match e {
            p::Effect::Finished(o) => Some(status_variant(&o.status)),
            _ => None,
        })
        .collect();
    assert_covers(statuses, 5, "OutcomeStatus");
    for e in effects {
        rt_try::<_, f::Effect>(e);
    }
    for o in outcomes() {
        rt::<_, f::Outcome>(o);
    }
    rt::<_, f::ConfirmRequest>(p::ConfirmRequest {
        category: p::RiskCategory::Delete,
        summary: "Delete 3 photos".to_owned(),
        app: "com.apple.mobileslideshow".to_owned(),
        phone_only: false,
    });
    // Thirteenth review: the phone-only flag crosses the FFI.
    rt::<_, f::ConfirmRequest>(p::ConfirmRequest {
        category: p::RiskCategory::SendMessage,
        summary: "Tap \"Continue\" in Forum. Sends: Note: \"hi\"".to_owned(),
        app: "com.example.forum".to_owned(),
        phone_only: true,
    });
}

#[test]
fn every_phase_and_progress_round_trips() {
    let phases = [
        p::Phase::Idle,
        p::Phase::Observing,
        p::Phase::Thinking,
        p::Phase::Acting,
        p::Phase::Verifying,
        p::Phase::CallingTool,
        p::Phase::WaitingForUser,
        p::Phase::Finished,
        p::Phase::WaitingForUnlock,
    ];
    assert_covers(phases.iter().map(phase_variant), 9, "Phase");
    for (phase, action) in phases.into_iter().zip(all_actions()) {
        rt::<_, f::Phase>(phase);
        rt_try::<_, f::SessionProgress>(p::Progress {
            step: 3,
            phase,
            last_action: Some(action),
            app: Some("com.apple.Preferences".to_owned()),
            model_calls: 7,
            parse_retries: 2,
            waiting_for_unlock: phase == p::Phase::WaitingForUnlock,
            locked_wait_ms: 5,
            locked_total_ms: 50,
        });
    }
    rt_try::<_, f::SessionProgress>(p::Progress::default());
}

#[test]
fn json_strings_from_swift_are_validated() {
    let bad = f::ToolCall {
        name: "x".to_owned(),
        args_json: "{not json".to_owned(),
    };
    let err = p::ToolCall::try_from(bad).unwrap_err();
    assert!(
        matches!(&err, f::JarvisError::InvalidJson { field, .. } if field == "ToolCall.args_json"),
        "{err:?}"
    );
    let bad = f::Action::CallTool {
        name: "x".to_owned(),
        args_json: "[1,".to_owned(),
    };
    assert!(p::Action::try_from(bad).is_err());
    // Empty means no arguments.
    let empty = f::ToolCall {
        name: "x".to_owned(),
        args_json: String::new(),
    };
    assert_eq!(
        p::ToolCall::try_from(empty).unwrap().args,
        serde_json::Value::Null
    );
}

#[test]
fn ffi_values_round_trip_through_proto() {
    // The other direction for the types Swift constructs.
    let screen: f::Screen = fixtures::compose_screen().into();
    let back: f::Screen = p::Screen::from(screen.clone()).into();
    assert_eq!(back, screen);
    let role = f::Role::Other {
        name: "Type<12>".to_owned(),
    };
    assert_eq!(f::Role::from(p::Role::from(role.clone())), role);
}

/// `Debug` of the FFI records that carry typed text or the owner's answer shows its length
/// only (the text may hold substituted profile values).
#[test]
fn ffi_debug_never_shows_typed_text_or_answers() {
    let secret = "4111 1111 1111 1111 secret-note";
    let cmd = f::RunnerCommand::TypeText {
        text: secret.to_owned(),
    };
    let effect = f::Effect::Act {
        command: cmd.clone(),
    };
    let answer = f::Event::UserAnswer {
        text: secret.to_owned(),
    };
    for shown in [
        format!("{cmd:?}"),
        format!("{effect:?}"),
        format!("{answer:?}"),
    ] {
        assert!(
            !shown.contains("4111") && !shown.contains("secret"),
            "{shown}"
        );
        assert!(shown.contains("text_chars"), "{shown}");
    }
    // The proto types behave the same.
    let p_cmd = p::RunnerCommand::TypeText {
        text: secret.to_owned(),
    };
    let p_effect = p::Effect::Act(p_cmd.clone());
    for shown in [format!("{p_cmd:?}"), format!("{p_effect:?}")] {
        assert!(!shown.contains("4111"), "{shown}");
    }
    // Other commands still show their fields.
    assert!(format!("{:?}", f::RunnerCommand::Tap { x: 1.0, y: 2.0 }).contains("x: 1.0"));
}
