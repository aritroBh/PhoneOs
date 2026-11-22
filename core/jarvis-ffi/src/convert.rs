//! Conversions between the FFI mirrors (`crate::types`) and the `jarvis-proto` types.
//!
//! Written by hand with exhaustive matches and full struct destructuring (never `..` or a
//! `_` arm), so adding or renaming a field or variant in `jarvis-proto` is a compile error
//! here instead of silent drift. Conversions that can fail (tool arguments arrive from
//! Swift as JSON text) are `TryFrom`; everything else is `From`.

use jarvis_proto as p;

use crate::error::JarvisError;
use crate::runner::snapshot_to_observation;
use crate::types::*;

/// Serializes tool arguments. `serde_json::Value` always serializes (its map keys are
/// strings), so the fallback is unreachable in practice; it keeps this path panic-free.
fn args_to_json(args: &serde_json::Value) -> String {
    serde_json::to_string(args).unwrap_or_else(|_| "null".to_owned())
}

/// Parses tool arguments coming from Swift. An empty string means "no arguments" (`null`,
/// what the proto type defaults to).
fn args_from_json(field: &str, json: &str) -> Result<serde_json::Value, JarvisError> {
    if json.trim().is_empty() {
        return Ok(serde_json::Value::Null);
    }
    serde_json::from_str(json).map_err(|e| JarvisError::InvalidJson {
        field: field.to_owned(),
        message: e.to_string(),
    })
}

// ---------------------------------------------------------------------------------------
// Geometry.
// ---------------------------------------------------------------------------------------

impl From<p::Point> for Point {
    fn from(v: p::Point) -> Self {
        let p::Point { x, y } = v;
        Point { x, y }
    }
}

impl From<Point> for p::Point {
    fn from(v: Point) -> Self {
        let Point { x, y } = v;
        p::Point { x, y }
    }
}

impl From<p::Size> for Size {
    fn from(v: p::Size) -> Self {
        let p::Size { width, height } = v;
        Size { width, height }
    }
}

impl From<Size> for p::Size {
    fn from(v: Size) -> Self {
        let Size { width, height } = v;
        p::Size { width, height }
    }
}

impl From<p::Rect> for Rect {
    fn from(v: p::Rect) -> Self {
        let p::Rect {
            x,
            y,
            width,
            height,
        } = v;
        Rect {
            x,
            y,
            width,
            height,
        }
    }
}

impl From<Rect> for p::Rect {
    fn from(v: Rect) -> Self {
        let Rect {
            x,
            y,
            width,
            height,
        } = v;
        p::Rect {
            x,
            y,
            width,
            height,
        }
    }
}

// ---------------------------------------------------------------------------------------
// UI tree.
// ---------------------------------------------------------------------------------------

impl From<p::Role> for Role {
    fn from(v: p::Role) -> Self {
        match v {
            p::Role::Button => Role::Button,
            p::Role::Link => Role::Link,
            p::Role::TextField => Role::TextField,
            p::Role::SecureTextField => Role::SecureTextField,
            p::Role::TextView => Role::TextView,
            p::Role::SearchField => Role::SearchField,
            p::Role::StaticText => Role::StaticText,
            p::Role::Image => Role::Image,
            p::Role::Cell => Role::Cell,
            p::Role::Switch => Role::Switch,
            p::Role::Slider => Role::Slider,
            p::Role::Picker => Role::Picker,
            p::Role::Tab => Role::Tab,
            p::Role::NavigationBar => Role::NavigationBar,
            p::Role::TabBar => Role::TabBar,
            p::Role::Toolbar => Role::Toolbar,
            p::Role::Alert => Role::Alert,
            p::Role::Sheet => Role::Sheet,
            p::Role::Keyboard => Role::Keyboard,
            p::Role::Key => Role::Key,
            p::Role::ScrollView => Role::ScrollView,
            p::Role::Table => Role::Table,
            p::Role::CollectionView => Role::CollectionView,
            p::Role::WebView => Role::WebView,
            p::Role::Window => Role::Window,
            p::Role::Application => Role::Application,
            p::Role::Other(name) => Role::Other { name },
        }
    }
}

impl From<Role> for p::Role {
    fn from(v: Role) -> Self {
        match v {
            Role::Button => p::Role::Button,
            Role::Link => p::Role::Link,
            Role::TextField => p::Role::TextField,
            Role::SecureTextField => p::Role::SecureTextField,
            Role::TextView => p::Role::TextView,
            Role::SearchField => p::Role::SearchField,
            Role::StaticText => p::Role::StaticText,
            Role::Image => p::Role::Image,
            Role::Cell => p::Role::Cell,
            Role::Switch => p::Role::Switch,
            Role::Slider => p::Role::Slider,
            Role::Picker => p::Role::Picker,
            Role::Tab => p::Role::Tab,
            Role::NavigationBar => p::Role::NavigationBar,
            Role::TabBar => p::Role::TabBar,
            Role::Toolbar => p::Role::Toolbar,
            Role::Alert => p::Role::Alert,
            Role::Sheet => p::Role::Sheet,
            Role::Keyboard => p::Role::Keyboard,
            Role::Key => p::Role::Key,
            Role::ScrollView => p::Role::ScrollView,
            Role::Table => p::Role::Table,
            Role::CollectionView => p::Role::CollectionView,
            Role::WebView => p::Role::WebView,
            Role::Window => p::Role::Window,
            Role::Application => p::Role::Application,
            // `Role::from_name` would map e.g. "button" back to `Button`; keep the Swift
            // value exactly as given instead, which is what the proto variant means.
            Role::Other { name } => p::Role::Other(name),
        }
    }
}

impl From<p::UiNode> for UiNode {
    fn from(v: p::UiNode) -> Self {
        let p::UiNode {
            role,
            label,
            value,
            identifier,
            placeholder,
            frame,
            enabled,
            visible,
            hittable,
            focused,
            selected,
            secure,
            children,
        } = v;
        UiNode {
            role: role.into(),
            label,
            value,
            identifier,
            placeholder,
            frame: frame.into(),
            enabled,
            visible,
            hittable,
            focused,
            selected,
            secure,
            children: children.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<UiNode> for p::UiNode {
    fn from(v: UiNode) -> Self {
        let UiNode {
            role,
            label,
            value,
            identifier,
            placeholder,
            frame,
            enabled,
            visible,
            hittable,
            focused,
            selected,
            secure,
            children,
        } = v;
        p::UiNode {
            role: role.into(),
            label,
            value,
            identifier,
            placeholder,
            frame: frame.into(),
            enabled,
            visible,
            hittable,
            focused,
            selected,
            secure,
            children: children.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<p::Orientation> for Orientation {
    fn from(v: p::Orientation) -> Self {
        match v {
            p::Orientation::Portrait => Orientation::Portrait,
            p::Orientation::PortraitUpsideDown => Orientation::PortraitUpsideDown,
            p::Orientation::LandscapeLeft => Orientation::LandscapeLeft,
            p::Orientation::LandscapeRight => Orientation::LandscapeRight,
        }
    }
}

impl From<Orientation> for p::Orientation {
    fn from(v: Orientation) -> Self {
        match v {
            Orientation::Portrait => p::Orientation::Portrait,
            Orientation::PortraitUpsideDown => p::Orientation::PortraitUpsideDown,
            Orientation::LandscapeLeft => p::Orientation::LandscapeLeft,
            Orientation::LandscapeRight => p::Orientation::LandscapeRight,
        }
    }
}

impl From<p::Screen> for Screen {
    fn from(v: p::Screen) -> Self {
        let p::Screen {
            app_bundle_id,
            app_name,
            size,
            root,
            keyboard_visible,
            orientation,
        } = v;
        Screen {
            app_bundle_id,
            app_name,
            size: size.into(),
            root: root.into(),
            keyboard_visible,
            orientation: orientation.into(),
        }
    }
}

impl From<Screen> for p::Screen {
    fn from(v: Screen) -> Self {
        let Screen {
            app_bundle_id,
            app_name,
            size,
            root,
            keyboard_visible,
            orientation,
        } = v;
        p::Screen {
            app_bundle_id,
            app_name,
            size: size.into(),
            root: root.into(),
            keyboard_visible,
            orientation: orientation.into(),
        }
    }
}

impl From<p::ImageRef> for ImageRef {
    fn from(v: p::ImageRef) -> Self {
        let p::ImageRef { id, width, height } = v;
        ImageRef { id, width, height }
    }
}

impl From<ImageRef> for p::ImageRef {
    fn from(v: ImageRef) -> Self {
        let ImageRef { id, width, height } = v;
        p::ImageRef { id, width, height }
    }
}

impl From<p::Observation> for ScreenObservation {
    fn from(v: p::Observation) -> Self {
        let p::Observation { screen, screenshot } = v;
        ScreenObservation {
            screen: screen.into(),
            screenshot: screenshot.map(Into::into),
        }
    }
}

impl From<ScreenObservation> for p::Observation {
    fn from(v: ScreenObservation) -> Self {
        let ScreenObservation { screen, screenshot } = v;
        p::Observation {
            screen: screen.into(),
            screenshot: screenshot.map(Into::into),
        }
    }
}

// ---------------------------------------------------------------------------------------
// Actions and runner commands.
// ---------------------------------------------------------------------------------------

impl From<p::Direction> for Direction {
    fn from(v: p::Direction) -> Self {
        match v {
            p::Direction::Up => Direction::Up,
            p::Direction::Down => Direction::Down,
            p::Direction::Left => Direction::Left,
            p::Direction::Right => Direction::Right,
        }
    }
}

impl From<Direction> for p::Direction {
    fn from(v: Direction) -> Self {
        match v {
            Direction::Up => p::Direction::Up,
            Direction::Down => p::Direction::Down,
            Direction::Left => p::Direction::Left,
            Direction::Right => p::Direction::Right,
        }
    }
}

impl From<p::Action> for Action {
    fn from(v: p::Action) -> Self {
        match v {
            p::Action::Tap { element } => Action::Tap { element },
            p::Action::LongPress {
                element,
                duration_ms,
            } => Action::LongPress {
                element,
                duration_ms,
            },
            p::Action::Type {
                element,
                text,
                submit,
            } => Action::Type {
                element,
                text,
                submit,
            },
            p::Action::Clear { element } => Action::Clear { element },
            p::Action::Scroll { direction, element } => Action::Scroll {
                direction: direction.into(),
                element,
            },
            p::Action::Swipe { direction, element } => Action::Swipe {
                direction: direction.into(),
                element,
            },
            p::Action::TapPoint { x, y } => Action::TapPoint { x, y },
            p::Action::PressHome => Action::PressHome,
            p::Action::OpenApp { app } => Action::OpenApp { app },
            p::Action::OpenUrl { url } => Action::OpenUrl { url },
            p::Action::Wait { ms } => Action::Wait { ms },
            p::Action::CallTool { name, args } => Action::CallTool {
                name,
                args_json: args_to_json(&args),
            },
            p::Action::AskUser { question } => Action::AskUser { question },
            p::Action::Look => Action::Look,
            p::Action::Done { summary } => Action::Done { summary },
            p::Action::Fail { reason } => Action::Fail { reason },
        }
    }
}

impl TryFrom<Action> for p::Action {
    type Error = JarvisError;

    fn try_from(v: Action) -> Result<Self, JarvisError> {
        Ok(match v {
            Action::Tap { element } => p::Action::Tap { element },
            Action::LongPress {
                element,
                duration_ms,
            } => p::Action::LongPress {
                element,
                duration_ms,
            },
            Action::Type {
                element,
                text,
                submit,
            } => p::Action::Type {
                element,
                text,
                submit,
            },
            Action::Clear { element } => p::Action::Clear { element },
            Action::Scroll { direction, element } => p::Action::Scroll {
                direction: direction.into(),
                element,
            },
            Action::Swipe { direction, element } => p::Action::Swipe {
                direction: direction.into(),
                element,
            },
            Action::TapPoint { x, y } => p::Action::TapPoint { x, y },
            Action::PressHome => p::Action::PressHome,
            Action::OpenApp { app } => p::Action::OpenApp { app },
            Action::OpenUrl { url } => p::Action::OpenUrl { url },
            Action::Wait { ms } => p::Action::Wait { ms },
            Action::CallTool { name, args_json } => p::Action::CallTool {
                args: args_from_json("Action.CallTool.args_json", &args_json)?,
                name,
            },
            Action::AskUser { question } => p::Action::AskUser { question },
            Action::Look => p::Action::Look,
            Action::Done { summary } => p::Action::Done { summary },
            Action::Fail { reason } => p::Action::Fail { reason },
        })
    }
}

impl From<p::Key> for Key {
    fn from(v: p::Key) -> Self {
        match v {
            p::Key::Return => Key::Return,
            p::Key::Delete => Key::Delete,
            p::Key::Tab => Key::Tab,
            p::Key::Space => Key::Space,
            p::Key::Escape => Key::Escape,
            p::Key::Home => Key::Home,
        }
    }
}

impl From<Key> for p::Key {
    fn from(v: Key) -> Self {
        match v {
            Key::Return => p::Key::Return,
            Key::Delete => p::Key::Delete,
            Key::Tab => p::Key::Tab,
            Key::Space => p::Key::Space,
            Key::Escape => p::Key::Escape,
            Key::Home => p::Key::Home,
        }
    }
}

impl From<p::RunnerCommand> for RunnerCommand {
    fn from(v: p::RunnerCommand) -> Self {
        match v {
            p::RunnerCommand::Tap { x, y } => RunnerCommand::Tap { x, y },
            p::RunnerCommand::LongPress { x, y, duration_ms } => {
                RunnerCommand::LongPress { x, y, duration_ms }
            }
            p::RunnerCommand::Swipe {
                from,
                to,
                duration_ms,
            } => RunnerCommand::Swipe {
                from: from.into(),
                to: to.into(),
                duration_ms,
            },
            p::RunnerCommand::TypeText { text } => RunnerCommand::TypeText { text },
            p::RunnerCommand::PressKey { key } => RunnerCommand::PressKey { key: key.into() },
            p::RunnerCommand::LaunchApp { bundle_id } => RunnerCommand::LaunchApp { bundle_id },
            p::RunnerCommand::OpenUrl { url } => RunnerCommand::OpenUrl { url },
            p::RunnerCommand::Wait { ms } => RunnerCommand::Wait { ms },
        }
    }
}

impl From<RunnerCommand> for p::RunnerCommand {
    fn from(v: RunnerCommand) -> Self {
        match v {
            RunnerCommand::Tap { x, y } => p::RunnerCommand::Tap { x, y },
            RunnerCommand::LongPress { x, y, duration_ms } => {
                p::RunnerCommand::LongPress { x, y, duration_ms }
            }
            RunnerCommand::Swipe {
                from,
                to,
                duration_ms,
            } => p::RunnerCommand::Swipe {
                from: from.into(),
                to: to.into(),
                duration_ms,
            },
            RunnerCommand::TypeText { text } => p::RunnerCommand::TypeText { text },
            RunnerCommand::PressKey { key } => p::RunnerCommand::PressKey { key: key.into() },
            RunnerCommand::LaunchApp { bundle_id } => p::RunnerCommand::LaunchApp { bundle_id },
            RunnerCommand::OpenUrl { url } => p::RunnerCommand::OpenUrl { url },
            RunnerCommand::Wait { ms } => p::RunnerCommand::Wait { ms },
        }
    }
}

impl From<p::RunnerResult> for RunnerResult {
    fn from(v: p::RunnerResult) -> Self {
        let p::RunnerResult { ok, error } = v;
        RunnerResult { ok, error }
    }
}

impl From<RunnerResult> for p::RunnerResult {
    fn from(v: RunnerResult) -> Self {
        let RunnerResult { ok, error } = v;
        p::RunnerResult { ok, error }
    }
}

// ---------------------------------------------------------------------------------------
// Tools.
// ---------------------------------------------------------------------------------------

impl From<p::ParamKind> for ParamKind {
    fn from(v: p::ParamKind) -> Self {
        match v {
            p::ParamKind::String => ParamKind::String,
            p::ParamKind::Integer => ParamKind::Integer,
            p::ParamKind::Number => ParamKind::Number,
            p::ParamKind::Boolean => ParamKind::Boolean,
        }
    }
}

impl From<ParamKind> for p::ParamKind {
    fn from(v: ParamKind) -> Self {
        match v {
            ParamKind::String => p::ParamKind::String,
            ParamKind::Integer => p::ParamKind::Integer,
            ParamKind::Number => p::ParamKind::Number,
            ParamKind::Boolean => p::ParamKind::Boolean,
        }
    }
}

impl From<p::Tier> for Tier {
    fn from(v: p::Tier) -> Self {
        match v {
            p::Tier::Native => Tier::Native,
            p::Tier::Shortcut => Tier::Shortcut,
            p::Tier::Ui => Tier::Ui,
        }
    }
}

impl From<Tier> for p::Tier {
    fn from(v: Tier) -> Self {
        match v {
            Tier::Native => p::Tier::Native,
            Tier::Shortcut => p::Tier::Shortcut,
            Tier::Ui => p::Tier::Ui,
        }
    }
}

impl From<p::ToolParam> for ToolParam {
    fn from(v: p::ToolParam) -> Self {
        let p::ToolParam {
            name,
            description,
            kind,
            required,
        } = v;
        ToolParam {
            name,
            description,
            kind: kind.into(),
            required,
        }
    }
}

impl From<ToolParam> for p::ToolParam {
    fn from(v: ToolParam) -> Self {
        let ToolParam {
            name,
            description,
            kind,
            required,
        } = v;
        p::ToolParam {
            name,
            description,
            kind: kind.into(),
            required,
        }
    }
}

impl From<p::ToolSpec> for ToolSpec {
    fn from(v: p::ToolSpec) -> Self {
        let p::ToolSpec {
            name,
            description,
            params,
            tier,
            risk,
            app,
        } = v;
        ToolSpec {
            name,
            description,
            params: params.into_iter().map(Into::into).collect(),
            tier: tier.into(),
            risk: risk.into(),
            app,
        }
    }
}

impl From<ToolSpec> for p::ToolSpec {
    fn from(v: ToolSpec) -> Self {
        let ToolSpec {
            name,
            description,
            params,
            tier,
            risk,
            app,
        } = v;
        p::ToolSpec {
            name,
            description,
            params: params.into_iter().map(Into::into).collect(),
            tier: tier.into(),
            risk: risk.into(),
            app,
        }
    }
}

impl From<p::ToolCall> for ToolCall {
    fn from(v: p::ToolCall) -> Self {
        let p::ToolCall { name, args } = v;
        ToolCall {
            name,
            args_json: args_to_json(&args),
        }
    }
}

impl TryFrom<ToolCall> for p::ToolCall {
    type Error = JarvisError;

    fn try_from(v: ToolCall) -> Result<Self, JarvisError> {
        let ToolCall { name, args_json } = v;
        Ok(p::ToolCall {
            name,
            args: args_from_json("ToolCall.args_json", &args_json)?,
        })
    }
}

impl From<p::ToolResult> for ToolResult {
    fn from(v: p::ToolResult) -> Self {
        let p::ToolResult { ok, output, error } = v;
        ToolResult { ok, output, error }
    }
}

impl From<ToolResult> for p::ToolResult {
    fn from(v: ToolResult) -> Self {
        let ToolResult { ok, output, error } = v;
        p::ToolResult { ok, output, error }
    }
}

// ---------------------------------------------------------------------------------------
// Model IO.
// ---------------------------------------------------------------------------------------

impl From<p::RequestKind> for RequestKind {
    fn from(v: p::RequestKind) -> Self {
        match v {
            p::RequestKind::Text => RequestKind::Text,
            p::RequestKind::Vision => RequestKind::Vision,
        }
    }
}

impl From<RequestKind> for p::RequestKind {
    fn from(v: RequestKind) -> Self {
        match v {
            RequestKind::Text => p::RequestKind::Text,
            RequestKind::Vision => p::RequestKind::Vision,
        }
    }
}

impl From<p::Mark> for Mark {
    fn from(v: p::Mark) -> Self {
        let p::Mark { index, frame } = v;
        Mark {
            index,
            frame: frame.into(),
        }
    }
}

impl From<Mark> for p::Mark {
    fn from(v: Mark) -> Self {
        let Mark { index, frame } = v;
        p::Mark {
            index,
            frame: frame.into(),
        }
    }
}

impl From<p::ModelRequest> for ModelRequest {
    fn from(v: p::ModelRequest) -> Self {
        let p::ModelRequest {
            system,
            user,
            image,
            marks,
            max_tokens,
            grammar,
            json_schema,
            kind,
        } = v;
        ModelRequest {
            system,
            user,
            image: image.map(Into::into),
            marks: marks.into_iter().map(Into::into).collect(),
            max_tokens,
            grammar,
            json_schema,
            kind: kind.into(),
        }
    }
}

impl From<ModelRequest> for p::ModelRequest {
    fn from(v: ModelRequest) -> Self {
        let ModelRequest {
            system,
            user,
            image,
            marks,
            max_tokens,
            grammar,
            json_schema,
            kind,
        } = v;
        p::ModelRequest {
            system,
            user,
            image: image.map(Into::into),
            marks: marks.into_iter().map(Into::into).collect(),
            max_tokens,
            grammar,
            json_schema,
            kind: kind.into(),
        }
    }
}

impl From<p::ModelOutput> for ModelOutput {
    fn from(v: p::ModelOutput) -> Self {
        let p::ModelOutput { text } = v;
        ModelOutput { text }
    }
}

impl From<ModelOutput> for p::ModelOutput {
    fn from(v: ModelOutput) -> Self {
        let ModelOutput { text } = v;
        p::ModelOutput { text }
    }
}

impl From<p::ModelProfile> for ModelProfile {
    fn from(v: p::ModelProfile) -> Self {
        let p::ModelProfile {
            context_tokens,
            supports_vision,
            supports_grammar,
        } = v;
        ModelProfile {
            context_tokens,
            supports_vision,
            supports_grammar,
        }
    }
}

impl From<ModelProfile> for p::ModelProfile {
    fn from(v: ModelProfile) -> Self {
        let ModelProfile {
            context_tokens,
            supports_vision,
            supports_grammar,
        } = v;
        p::ModelProfile {
            context_tokens,
            supports_vision,
            supports_grammar,
        }
    }
}

// ---------------------------------------------------------------------------------------
// Safety.
// ---------------------------------------------------------------------------------------

impl From<p::RiskCategory> for RiskCategory {
    fn from(v: p::RiskCategory) -> Self {
        match v {
            p::RiskCategory::None => RiskCategory::None,
            p::RiskCategory::SendMessage => RiskCategory::SendMessage,
            p::RiskCategory::Payment => RiskCategory::Payment,
            p::RiskCategory::Purchase => RiskCategory::Purchase,
            p::RiskCategory::Delete => RiskCategory::Delete,
            p::RiskCategory::PublicPost => RiskCategory::PublicPost,
            p::RiskCategory::AccountChange => RiskCategory::AccountChange,
            p::RiskCategory::SystemSetting => RiskCategory::SystemSetting,
            p::RiskCategory::Credential => RiskCategory::Credential,
        }
    }
}

impl From<RiskCategory> for p::RiskCategory {
    fn from(v: RiskCategory) -> Self {
        match v {
            RiskCategory::None => p::RiskCategory::None,
            RiskCategory::SendMessage => p::RiskCategory::SendMessage,
            RiskCategory::Payment => p::RiskCategory::Payment,
            RiskCategory::Purchase => p::RiskCategory::Purchase,
            RiskCategory::Delete => p::RiskCategory::Delete,
            RiskCategory::PublicPost => p::RiskCategory::PublicPost,
            RiskCategory::AccountChange => p::RiskCategory::AccountChange,
            RiskCategory::SystemSetting => p::RiskCategory::SystemSetting,
            RiskCategory::Credential => p::RiskCategory::Credential,
        }
    }
}

impl From<p::Policy> for Policy {
    fn from(v: p::Policy) -> Self {
        let p::Policy {
            denied_apps,
            confirm,
            max_steps,
            max_duration_ms,
            max_consecutive_failures,
            max_parse_retries,
        } = v;
        Policy {
            denied_apps,
            confirm: confirm.into_iter().map(Into::into).collect(),
            max_steps,
            max_duration_ms,
            max_consecutive_failures,
            max_parse_retries,
        }
    }
}

impl From<Policy> for p::Policy {
    fn from(v: Policy) -> Self {
        let Policy {
            denied_apps,
            confirm,
            max_steps,
            max_duration_ms,
            max_consecutive_failures,
            max_parse_retries,
        } = v;
        p::Policy {
            denied_apps,
            confirm: confirm.into_iter().map(Into::into).collect(),
            max_steps,
            max_duration_ms,
            max_consecutive_failures,
            max_parse_retries,
        }
    }
}

// ---------------------------------------------------------------------------------------
// Session IO.
// ---------------------------------------------------------------------------------------

impl From<p::Event> for Event {
    fn from(v: p::Event) -> Self {
        match v {
            p::Event::Start => Event::Start,
            p::Event::Observed(observation) => Event::Observed {
                observation: observation.into(),
            },
            p::Event::ModelOutput(output) => Event::ModelOutput {
                output: output.into(),
            },
            p::Event::ModelFailed { error } => Event::ModelFailed { error },
            p::Event::ActResult(result) => Event::ActResult {
                result: result.into(),
            },
            p::Event::ToolResult(result) => Event::ToolResult {
                result: result.into(),
            },
            p::Event::UserDecision { approved } => Event::UserDecision { approved },
            p::Event::UserAnswer { text } => Event::UserAnswer { text },
            p::Event::Cancel => Event::Cancel,
        }
    }
}

/// Converts an FFI event for the session. The second value is a note about the runner text
/// when [`Event::ObservedRunnerText`] could not be parsed cleanly (see `crate::runner`).
pub(crate) fn event_into_proto(v: Event) -> (p::Event, Option<String>) {
    match v {
        Event::Start => (p::Event::Start, None),
        Event::Observed { observation } => (p::Event::Observed(observation.into()), None),
        Event::ObservedRunnerText { snapshot } => {
            let (observation, issue) = snapshot_to_observation(snapshot);
            (p::Event::Observed(observation), issue)
        }
        Event::ModelOutput { output } => (p::Event::ModelOutput(output.into()), None),
        Event::ModelFailed { error } => (p::Event::ModelFailed { error }, None),
        Event::ActResult { result } => (p::Event::ActResult(result.into()), None),
        Event::ToolResult { result } => (p::Event::ToolResult(result.into()), None),
        Event::UserDecision { approved } => (p::Event::UserDecision { approved }, None),
        Event::UserAnswer { text } => (p::Event::UserAnswer { text }, None),
        Event::Cancel => (p::Event::Cancel, None),
    }
}

impl From<p::ConfirmRequest> for ConfirmRequest {
    fn from(v: p::ConfirmRequest) -> Self {
        let p::ConfirmRequest {
            category,
            summary,
            app,
        } = v;
        ConfirmRequest {
            category: category.into(),
            summary,
            app,
        }
    }
}

impl From<ConfirmRequest> for p::ConfirmRequest {
    fn from(v: ConfirmRequest) -> Self {
        let ConfirmRequest {
            category,
            summary,
            app,
        } = v;
        p::ConfirmRequest {
            category: category.into(),
            summary,
            app,
        }
    }
}

impl From<p::OutcomeStatus> for OutcomeStatus {
    fn from(v: p::OutcomeStatus) -> Self {
        match v {
            p::OutcomeStatus::Success => OutcomeStatus::Success,
            p::OutcomeStatus::Failed => OutcomeStatus::Failed,
            p::OutcomeStatus::Cancelled => OutcomeStatus::Cancelled,
            p::OutcomeStatus::Blocked => OutcomeStatus::Blocked,
        }
    }
}

impl From<OutcomeStatus> for p::OutcomeStatus {
    fn from(v: OutcomeStatus) -> Self {
        match v {
            OutcomeStatus::Success => p::OutcomeStatus::Success,
            OutcomeStatus::Failed => p::OutcomeStatus::Failed,
            OutcomeStatus::Cancelled => p::OutcomeStatus::Cancelled,
            OutcomeStatus::Blocked => p::OutcomeStatus::Blocked,
        }
    }
}

impl From<p::Outcome> for Outcome {
    fn from(v: p::Outcome) -> Self {
        let p::Outcome {
            status,
            summary,
            steps,
            duration_ms,
        } = v;
        Outcome {
            status: status.into(),
            summary,
            steps,
            duration_ms,
        }
    }
}

impl From<Outcome> for p::Outcome {
    fn from(v: Outcome) -> Self {
        let Outcome {
            status,
            summary,
            steps,
            duration_ms,
        } = v;
        p::Outcome {
            status: status.into(),
            summary,
            steps,
            duration_ms,
        }
    }
}

impl From<p::Effect> for Effect {
    fn from(v: p::Effect) -> Self {
        match v {
            p::Effect::Observe { screenshot } => Effect::Observe { screenshot },
            p::Effect::Infer(request) => Effect::Infer {
                request: request.into(),
            },
            p::Effect::Act(command) => Effect::Act {
                command: command.into(),
            },
            p::Effect::CallTool(call) => Effect::CallTool { call: call.into() },
            p::Effect::Confirm(request) => Effect::Confirm {
                request: request.into(),
            },
            p::Effect::AskUser { question } => Effect::AskUser { question },
            p::Effect::Finished(outcome) => Effect::Finished {
                outcome: outcome.into(),
            },
        }
    }
}

impl TryFrom<Effect> for p::Effect {
    type Error = JarvisError;

    fn try_from(v: Effect) -> Result<Self, JarvisError> {
        Ok(match v {
            Effect::Observe { screenshot } => p::Effect::Observe { screenshot },
            Effect::Infer { request } => p::Effect::Infer(request.into()),
            Effect::Act { command } => p::Effect::Act(command.into()),
            Effect::CallTool { call } => p::Effect::CallTool(call.try_into()?),
            Effect::Confirm { request } => p::Effect::Confirm(request.into()),
            Effect::AskUser { question } => p::Effect::AskUser { question },
            Effect::Finished { outcome } => p::Effect::Finished(outcome.into()),
        })
    }
}

impl From<p::Phase> for Phase {
    fn from(v: p::Phase) -> Self {
        match v {
            p::Phase::Idle => Phase::Idle,
            p::Phase::Observing => Phase::Observing,
            p::Phase::Thinking => Phase::Thinking,
            p::Phase::Acting => Phase::Acting,
            p::Phase::Verifying => Phase::Verifying,
            p::Phase::CallingTool => Phase::CallingTool,
            p::Phase::WaitingForUser => Phase::WaitingForUser,
            p::Phase::Finished => Phase::Finished,
        }
    }
}

impl From<Phase> for p::Phase {
    fn from(v: Phase) -> Self {
        match v {
            Phase::Idle => p::Phase::Idle,
            Phase::Observing => p::Phase::Observing,
            Phase::Thinking => p::Phase::Thinking,
            Phase::Acting => p::Phase::Acting,
            Phase::Verifying => p::Phase::Verifying,
            Phase::CallingTool => p::Phase::CallingTool,
            Phase::WaitingForUser => p::Phase::WaitingForUser,
            Phase::Finished => p::Phase::Finished,
        }
    }
}

impl From<p::Progress> for SessionProgress {
    fn from(v: p::Progress) -> Self {
        let p::Progress {
            step,
            phase,
            last_action,
            app,
        } = v;
        SessionProgress {
            step,
            phase: phase.into(),
            last_action: last_action.map(Into::into),
            app,
        }
    }
}

impl TryFrom<SessionProgress> for p::Progress {
    type Error = JarvisError;

    fn try_from(v: SessionProgress) -> Result<Self, JarvisError> {
        let SessionProgress {
            step,
            phase,
            last_action,
            app,
        } = v;
        Ok(p::Progress {
            step,
            phase: phase.into(),
            last_action: last_action.map(TryInto::try_into).transpose()?,
            app,
        })
    }
}

// ---------------------------------------------------------------------------------------
// Task and configuration.
// ---------------------------------------------------------------------------------------

impl From<p::TaskSpec> for TaskSpec {
    fn from(v: p::TaskSpec) -> Self {
        let p::TaskSpec { id, text } = v;
        TaskSpec { id, text }
    }
}

impl From<TaskSpec> for p::TaskSpec {
    fn from(v: TaskSpec) -> Self {
        let TaskSpec { id, text } = v;
        p::TaskSpec { id, text }
    }
}

impl From<p::AppInfo> for AppInfo {
    fn from(v: p::AppInfo) -> Self {
        let p::AppInfo { bundle_id, name } = v;
        AppInfo { bundle_id, name }
    }
}

impl From<AppInfo> for p::AppInfo {
    fn from(v: AppInfo) -> Self {
        let AppInfo { bundle_id, name } = v;
        p::AppInfo { bundle_id, name }
    }
}

impl From<p::SessionConfig> for SessionConfig {
    fn from(v: p::SessionConfig) -> Self {
        let p::SessionConfig {
            policy,
            tools,
            model,
            known_apps,
        } = v;
        SessionConfig {
            policy: policy.into(),
            tools: tools.into_iter().map(Into::into).collect(),
            model: model.into(),
            known_apps: known_apps.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<SessionConfig> for p::SessionConfig {
    fn from(v: SessionConfig) -> Self {
        let SessionConfig {
            policy,
            tools,
            model,
            known_apps,
        } = v;
        p::SessionConfig {
            policy: policy.into(),
            tools: tools.into_iter().map(Into::into).collect(),
            model: model.into(),
            known_apps: known_apps.into_iter().map(Into::into).collect(),
        }
    }
}
