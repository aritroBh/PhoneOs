//! Conversions between the FFI mirrors (`crate::types`) and the `jarvis-proto` types.
//!
//! Written by hand with exhaustive matches and full struct destructuring (never `..` or a
//! `_` arm), so adding or renaming a field or variant in `jarvis-proto` is a compile error
//! here instead of silent drift. Conversions that can fail (tool arguments arrive from
//! Swift as JSON text) are `TryFrom`; everything else is `From`.

use jarvis_proto as p;
use jarvis_remote as r;
use jarvis_skills as sk;
use zeroize::Zeroizing;

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
        let p::Observation {
            screen,
            screenshot,
            device_locked,
        } = v;
        ScreenObservation {
            device_locked,
            screen: screen.into(),
            screenshot: screenshot.map(Into::into),
        }
    }
}

impl From<ScreenObservation> for p::Observation {
    fn from(v: ScreenObservation) -> Self {
        let ScreenObservation {
            screen,
            screenshot,
            device_locked,
        } = v;
        p::Observation {
            device_locked,
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

impl From<p::RunnerFailure> for RunnerFailure {
    fn from(v: p::RunnerFailure) -> Self {
        match v {
            p::RunnerFailure::DeviceLocked => RunnerFailure::DeviceLocked,
            p::RunnerFailure::Other => RunnerFailure::Other,
        }
    }
}

impl From<RunnerFailure> for p::RunnerFailure {
    fn from(v: RunnerFailure) -> Self {
        match v {
            RunnerFailure::DeviceLocked => p::RunnerFailure::DeviceLocked,
            RunnerFailure::Other => p::RunnerFailure::Other,
        }
    }
}

impl From<p::RunnerResult> for RunnerResult {
    fn from(v: p::RunnerResult) -> Self {
        let p::RunnerResult { ok, error, failure } = v;
        RunnerResult {
            ok,
            error,
            failure: failure.map(Into::into),
        }
    }
}

impl From<RunnerResult> for p::RunnerResult {
    fn from(v: RunnerResult) -> Self {
        let RunnerResult { ok, error, failure } = v;
        p::RunnerResult {
            ok,
            error,
            failure: failure.map(Into::into),
        }
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
            p::RiskCategory::Unverified => RiskCategory::Unverified,
            p::RiskCategory::Call => RiskCategory::Call,
            p::RiskCategory::FirstUse => RiskCategory::FirstUse,
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
            RiskCategory::Unverified => p::RiskCategory::Unverified,
            RiskCategory::Call => p::RiskCategory::Call,
            RiskCategory::FirstUse => p::RiskCategory::FirstUse,
        }
    }
}

impl From<p::PolicyMode> for PolicyMode {
    fn from(v: p::PolicyMode) -> Self {
        match v {
            p::PolicyMode::Guarded => PolicyMode::Guarded,
            p::PolicyMode::StepByStep => PolicyMode::StepByStep,
        }
    }
}

impl From<PolicyMode> for p::PolicyMode {
    fn from(v: PolicyMode) -> Self {
        match v {
            PolicyMode::Guarded => p::PolicyMode::Guarded,
            PolicyMode::StepByStep => p::PolicyMode::StepByStep,
        }
    }
}

impl From<p::TrustLevel> for TrustLevel {
    fn from(v: p::TrustLevel) -> Self {
        match v {
            p::TrustLevel::Trusted => TrustLevel::Trusted,
            p::TrustLevel::Guarded => TrustLevel::Guarded,
            p::TrustLevel::StepByStep => TrustLevel::StepByStep,
            p::TrustLevel::Denied => TrustLevel::Denied,
        }
    }
}

impl From<TrustLevel> for p::TrustLevel {
    fn from(v: TrustLevel) -> Self {
        match v {
            TrustLevel::Trusted => p::TrustLevel::Trusted,
            TrustLevel::Guarded => p::TrustLevel::Guarded,
            TrustLevel::StepByStep => p::TrustLevel::StepByStep,
            TrustLevel::Denied => p::TrustLevel::Denied,
        }
    }
}

impl From<p::AppRule> for AppRule {
    fn from(v: p::AppRule) -> Self {
        let p::AppRule {
            bundle_id_prefix,
            level,
            name,
        } = v;
        AppRule {
            bundle_id_prefix,
            level: level.into(),
            name,
        }
    }
}

impl From<AppRule> for p::AppRule {
    fn from(v: AppRule) -> Self {
        let AppRule {
            bundle_id_prefix,
            level,
            name,
        } = v;
        p::AppRule {
            bundle_id_prefix,
            level: level.into(),
            name,
        }
    }
}

impl From<p::Policy> for Policy {
    fn from(v: p::Policy) -> Self {
        let p::Policy {
            denied_apps,
            confirm,
            deny_categories,
            mode,
            app_rules,
            first_use_approval,
            max_steps,
            max_duration_ms,
            max_consecutive_failures,
            max_parse_retries,
            max_locked_wait_ms,
        } = v;
        Policy {
            denied_apps,
            confirm: confirm.into_iter().map(Into::into).collect(),
            deny_categories: deny_categories.into_iter().map(Into::into).collect(),
            mode: mode.into(),
            app_rules: app_rules.into_iter().map(Into::into).collect(),
            first_use_approval,
            max_steps,
            max_duration_ms,
            max_consecutive_failures,
            max_parse_retries,
            max_locked_wait_ms,
        }
    }
}

impl From<Policy> for p::Policy {
    fn from(v: Policy) -> Self {
        let Policy {
            denied_apps,
            confirm,
            deny_categories,
            mode,
            app_rules,
            first_use_approval,
            max_steps,
            max_duration_ms,
            max_consecutive_failures,
            max_parse_retries,
            max_locked_wait_ms,
        } = v;
        p::Policy {
            denied_apps,
            confirm: confirm.into_iter().map(Into::into).collect(),
            deny_categories: deny_categories.into_iter().map(Into::into).collect(),
            mode: mode.into(),
            app_rules: app_rules.into_iter().map(Into::into).collect(),
            first_use_approval,
            max_steps,
            max_duration_ms,
            max_consecutive_failures,
            max_parse_retries,
            max_locked_wait_ms,
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
            // The runner's text is gone once parsed: the FFI side gets the parsed tree as a
            // plain observation (this direction is only used for inspection and tests).
            p::Event::Observed(observation) | p::Event::ObservedRunnerText(observation) => {
                Event::Observed {
                    observation: observation.into(),
                }
            }
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
            p::Event::DeviceLocked => Event::DeviceLocked,
            p::Event::DeviceUnlocked => Event::DeviceUnlocked,
            p::Event::Tick => Event::Tick,
            p::Event::ObserveFailed { reason } => Event::ObserveFailed { reason },
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
            // Marked as runner text: the session treats the screen as unverified.
            (p::Event::ObservedRunnerText(observation), issue)
        }
        Event::ModelOutput { output } => (p::Event::ModelOutput(output.into()), None),
        Event::ModelFailed { error } => (p::Event::ModelFailed { error }, None),
        Event::ActResult { result } => (p::Event::ActResult(result.into()), None),
        Event::ToolResult { result } => (p::Event::ToolResult(result.into()), None),
        Event::UserDecision { approved } => (p::Event::UserDecision { approved }, None),
        Event::UserAnswer { text } => (p::Event::UserAnswer { text }, None),
        Event::Cancel => (p::Event::Cancel, None),
        Event::DeviceLocked => (p::Event::DeviceLocked, None),
        Event::DeviceUnlocked => (p::Event::DeviceUnlocked, None),
        Event::Tick => (p::Event::Tick, None),
        Event::ObserveFailed { reason } => (p::Event::ObserveFailed { reason }, None),
    }
}

impl From<p::ConfirmRequest> for ConfirmRequest {
    fn from(v: p::ConfirmRequest) -> Self {
        let p::ConfirmRequest {
            category,
            summary,
            app,
            phone_only,
            rows,
            stage,
            verb,
            target,
        } = v;
        ConfirmRequest {
            category: category.into(),
            summary,
            app,
            phone_only,
            rows: rows.into_iter().map(Into::into).collect(),
            stage: stage.into(),
            verb: verb.into(),
            target: target.map(|t| (*t).into()),
        }
    }
}

impl From<ConfirmRequest> for p::ConfirmRequest {
    fn from(v: ConfirmRequest) -> Self {
        let ConfirmRequest {
            category,
            summary,
            app,
            phone_only,
            rows,
            stage,
            verb,
            target,
        } = v;
        p::ConfirmRequest {
            category: category.into(),
            summary,
            app,
            phone_only,
            rows: rows.into_iter().map(Into::into).collect(),
            stage: stage.into(),
            verb: verb.into(),
            target: target.map(|t| Box::new(t.into())),
        }
    }
}

impl From<p::QuestionRow> for QuestionRow {
    fn from(v: p::QuestionRow) -> Self {
        QuestionRow {
            display: v.display_text().to_owned(),
            kind: v.kind.into(),
            text: v.text,
        }
    }
}

impl From<QuestionRow> for p::QuestionRow {
    fn from(v: QuestionRow) -> Self {
        p::QuestionRow {
            kind: v.kind.into(),
            text: v.text,
        }
    }
}

macro_rules! same_enum {
    ($ffi:ident, $proto:path, [$($v:ident),* $(,)?]) => {
        impl From<$proto> for $ffi {
            fn from(v: $proto) -> Self {
                use $proto as P;
                match v {
                    $(P::$v => $ffi::$v,)*
                }
            }
        }
        impl From<$ffi> for $proto {
            fn from(v: $ffi) -> Self {
                use $proto as P;
                match v {
                    $($ffi::$v => P::$v,)*
                }
            }
        }
    };
}

same_enum!(
    QuestionRowKind,
    p::QuestionRowKind,
    [
        Action,
        Warning,
        Sends,
        Typed,
        Amount,
        Recipient,
        NotVerified,
        ScreenSays,
        Selected,
        Fields,
        Note,
        Other,
    ]
);
same_enum!(ConfirmStage, p::ConfirmStage, [Whole, Typing, Return]);
same_enum!(
    ConfirmVerb,
    p::ConfirmVerb,
    [
        Other, Tap, LongPress, Type, Clear, Scroll, Swipe, TapPoint, PressHome, OpenApp, OpenUrl,
        CallTool,
    ]
);

impl From<p::ConfirmTarget> for ConfirmTarget {
    fn from(v: p::ConfirmTarget) -> Self {
        let p::ConfirmTarget {
            element,
            node,
            frame,
            point,
            digest,
        } = v;
        ConfirmTarget {
            element,
            node,
            frame: frame.map(Into::into),
            point: point.map(Into::into),
            digest,
        }
    }
}

impl From<ConfirmTarget> for p::ConfirmTarget {
    fn from(v: ConfirmTarget) -> Self {
        let ConfirmTarget {
            element,
            node,
            frame,
            point,
            digest,
        } = v;
        p::ConfirmTarget {
            element,
            node,
            frame: frame.map(Into::into),
            point: point.map(Into::into),
            digest,
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
            p::OutcomeStatus::Interrupted => OutcomeStatus::Interrupted,
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
            OutcomeStatus::Interrupted => p::OutcomeStatus::Interrupted,
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
            p::Effect::AwaitUnlock { reason, waited_ms } => {
                Effect::AwaitUnlock { reason, waited_ms }
            }
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
            Effect::AwaitUnlock { reason, waited_ms } => {
                p::Effect::AwaitUnlock { reason, waited_ms }
            }
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
            p::Phase::WaitingForUnlock => Phase::WaitingForUnlock,
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
            Phase::WaitingForUnlock => p::Phase::WaitingForUnlock,
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
            model_calls,
            parse_retries,
            waiting_for_unlock,
            locked_wait_ms,
            locked_total_ms,
        } = v;
        SessionProgress {
            step,
            phase: phase.into(),
            last_action: last_action.map(Into::into),
            app,
            model_calls,
            parse_retries,
            waiting_for_unlock,
            locked_wait_ms,
            locked_total_ms,
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
            model_calls,
            parse_retries,
            waiting_for_unlock,
            locked_wait_ms,
            locked_total_ms,
        } = v;
        Ok(p::Progress {
            step,
            phase: phase.into(),
            last_action: last_action.map(TryInto::try_into).transpose()?,
            app,
            model_calls,
            parse_retries,
            waiting_for_unlock,
            locked_wait_ms,
            locked_total_ms,
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
            agent_bundle_id,
            skill,
        } = v;
        SessionConfig {
            policy: policy.into(),
            tools: tools.into_iter().map(Into::into).collect(),
            model: model.into(),
            known_apps: known_apps.into_iter().map(Into::into).collect(),
            agent_bundle_id,
            skill: skill.map(Into::into),
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
            agent_bundle_id,
            skill,
        } = v;
        p::SessionConfig {
            policy: policy.into(),
            tools: tools.into_iter().map(Into::into).collect(),
            model: model.into(),
            known_apps: known_apps.into_iter().map(Into::into).collect(),
            agent_bundle_id,
            skill: skill.map(Into::into),
        }
    }
}

// ---------------------------------------------------------------------------------------
// Skills.
// ---------------------------------------------------------------------------------------

impl From<p::SkillContext> for SkillContext {
    fn from(v: p::SkillContext) -> Self {
        let p::SkillContext {
            id,
            name,
            instructions,
            apps,
            confine_to_apps,
            tools,
            args,
        } = v;
        SkillContext {
            id,
            name,
            instructions,
            apps,
            confine_to_apps,
            args: args.into_iter().collect(),
            tools,
        }
    }
}

impl From<SkillContext> for p::SkillContext {
    fn from(v: SkillContext) -> Self {
        let SkillContext {
            id,
            name,
            instructions,
            apps,
            confine_to_apps,
            args,
            tools,
        } = v;
        p::SkillContext {
            id,
            name,
            instructions,
            apps,
            confine_to_apps,
            tools,
            args: args.into_iter().collect(),
        }
    }
}

impl From<sk::SkillKind> for SkillKind {
    fn from(v: sk::SkillKind) -> Self {
        match v {
            sk::SkillKind::Learned => SkillKind::Learned,
            sk::SkillKind::Written => SkillKind::Written,
            sk::SkillKind::BuiltIn => SkillKind::BuiltIn,
        }
    }
}

impl From<SkillKind> for sk::SkillKind {
    fn from(v: SkillKind) -> Self {
        match v {
            SkillKind::Learned => sk::SkillKind::Learned,
            SkillKind::Written => sk::SkillKind::Written,
            SkillKind::BuiltIn => sk::SkillKind::BuiltIn,
        }
    }
}

impl From<sk::SkillParam> for SkillParam {
    fn from(v: sk::SkillParam) -> Self {
        let sk::SkillParam {
            name,
            description,
            required,
        } = v;
        SkillParam {
            name,
            description,
            required,
        }
    }
}

impl From<SkillParam> for sk::SkillParam {
    fn from(v: SkillParam) -> Self {
        let SkillParam {
            name,
            description,
            required,
        } = v;
        sk::SkillParam {
            name,
            description,
            required,
        }
    }
}

impl From<sk::Skill> for Skill {
    fn from(v: sk::Skill) -> Self {
        let sk::Skill {
            id,
            name,
            summary,
            kind,
            triggers,
            instructions,
            apps,
            confine_to_apps,
            tools,
            params,
            flow_keys,
            enabled,
            created_ms,
            updated_ms,
            runs,
            last_run_ms,
        } = v;
        Skill {
            id,
            name,
            summary,
            kind: kind.into(),
            triggers,
            instructions,
            apps,
            confine_to_apps,
            tools,
            params: params.into_iter().map(Into::into).collect(),
            flow_keys,
            enabled,
            created_ms,
            updated_ms,
            runs,
            last_run_ms,
        }
    }
}

impl From<Skill> for sk::Skill {
    fn from(v: Skill) -> Self {
        let Skill {
            id,
            name,
            summary,
            kind,
            triggers,
            instructions,
            apps,
            confine_to_apps,
            tools,
            params,
            flow_keys,
            enabled,
            created_ms,
            updated_ms,
            runs,
            last_run_ms,
        } = v;
        sk::Skill {
            id,
            name,
            summary,
            kind: kind.into(),
            triggers,
            instructions,
            apps,
            confine_to_apps,
            tools,
            params: params.into_iter().map(Into::into).collect(),
            flow_keys,
            enabled,
            created_ms,
            updated_ms,
            runs,
            last_run_ms,
        }
    }
}

impl From<sk::SkillMatch> for SkillMatch {
    fn from(v: sk::SkillMatch) -> Self {
        let sk::SkillMatch {
            skill_id,
            score,
            args,
            matched_trigger,
            missing,
            exact,
        } = v;
        SkillMatch {
            skill_id,
            score,
            args: args.into_iter().collect(),
            matched_trigger,
            missing,
            exact,
        }
    }
}

impl From<SkillMatch> for sk::SkillMatch {
    fn from(v: SkillMatch) -> Self {
        let SkillMatch {
            skill_id,
            score,
            args,
            matched_trigger,
            missing,
            exact,
        } = v;
        sk::SkillMatch {
            skill_id,
            score,
            args: args.into_iter().collect(),
            matched_trigger,
            missing,
            exact,
        }
    }
}

// ---------------------------------------------------------------------------------------
// Workflows.
// ---------------------------------------------------------------------------------------

impl From<sk::Weekday> for Weekday {
    fn from(v: sk::Weekday) -> Self {
        match v {
            sk::Weekday::Mon => Weekday::Mon,
            sk::Weekday::Tue => Weekday::Tue,
            sk::Weekday::Wed => Weekday::Wed,
            sk::Weekday::Thu => Weekday::Thu,
            sk::Weekday::Fri => Weekday::Fri,
            sk::Weekday::Sat => Weekday::Sat,
            sk::Weekday::Sun => Weekday::Sun,
        }
    }
}

impl From<Weekday> for sk::Weekday {
    fn from(v: Weekday) -> Self {
        match v {
            Weekday::Mon => sk::Weekday::Mon,
            Weekday::Tue => sk::Weekday::Tue,
            Weekday::Wed => sk::Weekday::Wed,
            Weekday::Thu => sk::Weekday::Thu,
            Weekday::Fri => sk::Weekday::Fri,
            Weekday::Sat => sk::Weekday::Sat,
            Weekday::Sun => sk::Weekday::Sun,
        }
    }
}

impl From<sk::Schedule> for Schedule {
    fn from(v: sk::Schedule) -> Self {
        let sk::Schedule { days, hour, minute } = v;
        Schedule {
            days: days.into_iter().map(Into::into).collect(),
            hour,
            minute,
        }
    }
}

impl From<Schedule> for sk::Schedule {
    fn from(v: Schedule) -> Self {
        let Schedule { days, hour, minute } = v;
        sk::Schedule {
            days: days.into_iter().map(Into::into).collect(),
            hour,
            minute,
        }
    }
}

impl From<sk::Trigger> for Trigger {
    fn from(v: sk::Trigger) -> Self {
        match v {
            sk::Trigger::Manual => Trigger::Manual,
            sk::Trigger::Phrase { text } => Trigger::Phrase { text },
            sk::Trigger::Schedule(s) => Trigger::Schedule { schedule: s.into() },
            sk::Trigger::External { key } => Trigger::External { key },
        }
    }
}

impl From<Trigger> for sk::Trigger {
    fn from(v: Trigger) -> Self {
        match v {
            Trigger::Manual => sk::Trigger::Manual,
            Trigger::Phrase { text } => sk::Trigger::Phrase { text },
            Trigger::Schedule { schedule } => sk::Trigger::Schedule(schedule.into()),
            Trigger::External { key } => sk::Trigger::External { key },
        }
    }
}

impl From<sk::Condition> for Condition {
    fn from(v: sk::Condition) -> Self {
        match v {
            sk::Condition::LastSucceeded => Condition::LastSucceeded,
            sk::Condition::LastSummaryContains { text } => Condition::LastSummaryContains { text },
            sk::Condition::OwnerSaidYes => Condition::OwnerSaidYes,
        }
    }
}

impl From<Condition> for sk::Condition {
    fn from(v: Condition) -> Self {
        match v {
            Condition::LastSucceeded => sk::Condition::LastSucceeded,
            Condition::LastSummaryContains { text } => sk::Condition::LastSummaryContains { text },
            Condition::OwnerSaidYes => sk::Condition::OwnerSaidYes,
        }
    }
}

impl From<sk::Step> for Step {
    fn from(v: sk::Step) -> Self {
        match v {
            sk::Step::RunSkill { skill_id, args } => Step::RunSkill {
                skill_id,
                args: args.into_iter().collect(),
            },
            sk::Step::RunTask { text } => Step::RunTask { text },
            sk::Step::AskOwner { question } => Step::AskOwner { question },
            sk::Step::Notify { text } => Step::Notify { text },
            sk::Step::Wait { ms } => Step::Wait { ms },
            sk::Step::StopUnless { condition } => Step::StopUnless {
                condition: condition.into(),
            },
        }
    }
}

impl From<Step> for sk::Step {
    fn from(v: Step) -> Self {
        match v {
            Step::RunSkill { skill_id, args } => sk::Step::RunSkill {
                skill_id,
                args: args.into_iter().collect(),
            },
            Step::RunTask { text } => sk::Step::RunTask { text },
            Step::AskOwner { question } => sk::Step::AskOwner { question },
            Step::Notify { text } => sk::Step::Notify { text },
            Step::Wait { ms } => sk::Step::Wait { ms },
            Step::StopUnless { condition } => sk::Step::StopUnless {
                condition: condition.into(),
            },
        }
    }
}

impl From<sk::OnFailure> for OnFailure {
    fn from(v: sk::OnFailure) -> Self {
        match v {
            sk::OnFailure::Stop => OnFailure::Stop,
            sk::OnFailure::Continue => OnFailure::Continue,
        }
    }
}

impl From<OnFailure> for sk::OnFailure {
    fn from(v: OnFailure) -> Self {
        match v {
            OnFailure::Stop => sk::OnFailure::Stop,
            OnFailure::Continue => sk::OnFailure::Continue,
        }
    }
}

impl From<sk::WorkflowStatus> for WorkflowStatus {
    fn from(v: sk::WorkflowStatus) -> Self {
        match v {
            sk::WorkflowStatus::Succeeded => WorkflowStatus::Succeeded,
            sk::WorkflowStatus::CompletedWithFailures => WorkflowStatus::CompletedWithFailures,
            sk::WorkflowStatus::Failed => WorkflowStatus::Failed,
            sk::WorkflowStatus::Stopped => WorkflowStatus::Stopped,
            sk::WorkflowStatus::Cancelled => WorkflowStatus::Cancelled,
            sk::WorkflowStatus::Blocked => WorkflowStatus::Blocked,
            sk::WorkflowStatus::Interrupted => WorkflowStatus::Interrupted,
        }
    }
}

impl From<WorkflowStatus> for sk::WorkflowStatus {
    fn from(v: WorkflowStatus) -> Self {
        match v {
            WorkflowStatus::Succeeded => sk::WorkflowStatus::Succeeded,
            WorkflowStatus::CompletedWithFailures => sk::WorkflowStatus::CompletedWithFailures,
            WorkflowStatus::Failed => sk::WorkflowStatus::Failed,
            WorkflowStatus::Stopped => sk::WorkflowStatus::Stopped,
            WorkflowStatus::Cancelled => sk::WorkflowStatus::Cancelled,
            WorkflowStatus::Blocked => sk::WorkflowStatus::Blocked,
            WorkflowStatus::Interrupted => sk::WorkflowStatus::Interrupted,
        }
    }
}

impl From<sk::WorkflowOutcome> for WorkflowOutcome {
    fn from(v: sk::WorkflowOutcome) -> Self {
        let sk::WorkflowOutcome {
            status,
            steps_run,
            summary,
            failed_step,
        } = v;
        WorkflowOutcome {
            status: status.into(),
            steps_run,
            summary,
            failed_step,
        }
    }
}

impl From<WorkflowOutcome> for sk::WorkflowOutcome {
    fn from(v: WorkflowOutcome) -> Self {
        let WorkflowOutcome {
            status,
            steps_run,
            summary,
            failed_step,
        } = v;
        sk::WorkflowOutcome {
            status: status.into(),
            steps_run,
            summary,
            failed_step,
        }
    }
}

impl From<sk::Workflow> for Workflow {
    fn from(v: sk::Workflow) -> Self {
        let sk::Workflow {
            id,
            name,
            enabled,
            trigger,
            steps,
            on_failure,
            created_ms,
            updated_ms,
            runs,
            last_run_ms,
            last_outcome,
        } = v;
        Workflow {
            id,
            name,
            enabled,
            trigger: trigger.into(),
            steps: steps.into_iter().map(Into::into).collect(),
            on_failure: on_failure.into(),
            created_ms,
            updated_ms,
            runs,
            last_run_ms,
            last_outcome: last_outcome.map(Into::into),
        }
    }
}

impl From<Workflow> for sk::Workflow {
    fn from(v: Workflow) -> Self {
        let Workflow {
            id,
            name,
            enabled,
            trigger,
            steps,
            on_failure,
            created_ms,
            updated_ms,
            runs,
            last_run_ms,
            last_outcome,
        } = v;
        sk::Workflow {
            id,
            name,
            enabled,
            trigger: trigger.into(),
            steps: steps.into_iter().map(Into::into).collect(),
            on_failure: on_failure.into(),
            created_ms,
            updated_ms,
            runs,
            last_run_ms,
            last_outcome: last_outcome.map(Into::into),
        }
    }
}

impl From<sk::RunLimits> for RunLimits {
    fn from(v: sk::RunLimits) -> Self {
        let sk::RunLimits {
            max_steps,
            max_total_ms,
            max_wait_ms,
        } = v;
        RunLimits {
            max_steps,
            max_total_ms,
            max_wait_ms,
        }
    }
}

impl From<RunLimits> for sk::RunLimits {
    fn from(v: RunLimits) -> Self {
        let RunLimits {
            max_steps,
            max_total_ms,
            max_wait_ms,
        } = v;
        sk::RunLimits {
            max_steps,
            max_total_ms,
            max_wait_ms,
        }
    }
}

impl From<sk::WorkflowEvent> for WorkflowEvent {
    fn from(v: sk::WorkflowEvent) -> Self {
        match v {
            sk::WorkflowEvent::Start => WorkflowEvent::Start,
            sk::WorkflowEvent::SessionFinished { status, summary } => {
                WorkflowEvent::SessionFinished {
                    status: status.into(),
                    summary,
                }
            }
            sk::WorkflowEvent::OwnerAnswered { yes, text } => {
                WorkflowEvent::OwnerAnswered { yes, text }
            }
            sk::WorkflowEvent::Notified => WorkflowEvent::Notified,
            sk::WorkflowEvent::WaitElapsed => WorkflowEvent::WaitElapsed,
            sk::WorkflowEvent::Cancel => WorkflowEvent::Cancel,
        }
    }
}

impl From<WorkflowEvent> for sk::WorkflowEvent {
    fn from(v: WorkflowEvent) -> Self {
        match v {
            WorkflowEvent::Start => sk::WorkflowEvent::Start,
            WorkflowEvent::SessionFinished { status, summary } => {
                sk::WorkflowEvent::SessionFinished {
                    status: status.into(),
                    summary,
                }
            }
            WorkflowEvent::OwnerAnswered { yes, text } => {
                sk::WorkflowEvent::OwnerAnswered { yes, text }
            }
            WorkflowEvent::Notified => sk::WorkflowEvent::Notified,
            WorkflowEvent::WaitElapsed => sk::WorkflowEvent::WaitElapsed,
            WorkflowEvent::Cancel => sk::WorkflowEvent::Cancel,
        }
    }
}

impl From<sk::WorkflowEffect> for WorkflowEffect {
    fn from(v: sk::WorkflowEffect) -> Self {
        match v {
            sk::WorkflowEffect::StartSession { task, skill } => WorkflowEffect::StartSession {
                task,
                skill: skill.map(Into::into),
            },
            sk::WorkflowEffect::AskOwner { question } => WorkflowEffect::AskOwner { question },
            sk::WorkflowEffect::Notify { text } => WorkflowEffect::Notify { text },
            sk::WorkflowEffect::Wait { until_ms } => WorkflowEffect::Wait { until_ms },
            sk::WorkflowEffect::Finished { outcome } => WorkflowEffect::Finished {
                outcome: outcome.into(),
            },
        }
    }
}

impl From<WorkflowEffect> for sk::WorkflowEffect {
    fn from(v: WorkflowEffect) -> Self {
        match v {
            WorkflowEffect::StartSession { task, skill } => sk::WorkflowEffect::StartSession {
                task,
                skill: skill.map(Into::into),
            },
            WorkflowEffect::AskOwner { question } => sk::WorkflowEffect::AskOwner { question },
            WorkflowEffect::Notify { text } => sk::WorkflowEffect::Notify { text },
            WorkflowEffect::Wait { until_ms } => sk::WorkflowEffect::Wait { until_ms },
            WorkflowEffect::Finished { outcome } => sk::WorkflowEffect::Finished {
                outcome: outcome.into(),
            },
        }
    }
}

impl From<sk::Awaiting> for Awaiting {
    fn from(v: sk::Awaiting) -> Self {
        match v {
            sk::Awaiting::Start => Awaiting::Start,
            sk::Awaiting::Session => Awaiting::Session,
            sk::Awaiting::Answer => Awaiting::Answer,
            sk::Awaiting::Notified => Awaiting::Notified,
            sk::Awaiting::WaitElapsed => Awaiting::WaitElapsed,
            sk::Awaiting::Nothing => Awaiting::Nothing,
        }
    }
}

impl From<Awaiting> for sk::Awaiting {
    fn from(v: Awaiting) -> Self {
        match v {
            Awaiting::Start => sk::Awaiting::Start,
            Awaiting::Session => sk::Awaiting::Session,
            Awaiting::Answer => sk::Awaiting::Answer,
            Awaiting::Notified => sk::Awaiting::Notified,
            Awaiting::WaitElapsed => sk::Awaiting::WaitElapsed,
            Awaiting::Nothing => sk::Awaiting::Nothing,
        }
    }
}

impl From<sk::Resolution> for Resolution {
    fn from(v: sk::Resolution) -> Self {
        match v {
            sk::Resolution::RunWorkflow { workflow_id } => Resolution::RunWorkflow { workflow_id },
            sk::Resolution::RunSkill(m) => Resolution::RunSkill {
                skill_match: m.into(),
            },
            sk::Resolution::OwnerChoice => Resolution::OwnerChoice,
            sk::Resolution::NoMatch => Resolution::NoMatch,
        }
    }
}

impl From<Resolution> for sk::Resolution {
    fn from(v: Resolution) -> Self {
        match v {
            Resolution::RunWorkflow { workflow_id } => sk::Resolution::RunWorkflow { workflow_id },
            Resolution::RunSkill { skill_match } => sk::Resolution::RunSkill(skill_match.into()),
            Resolution::OwnerChoice => sk::Resolution::OwnerChoice,
            Resolution::NoMatch => sk::Resolution::NoMatch,
        }
    }
}

impl From<sk::UtteranceMatch> for UtteranceMatch {
    fn from(v: sk::UtteranceMatch) -> Self {
        let sk::UtteranceMatch {
            workflows,
            skills,
            resolution,
        } = v;
        UtteranceMatch {
            needs_owner_choice: resolution == sk::Resolution::OwnerChoice,
            workflows,
            skills: skills.into_iter().map(Into::into).collect(),
            resolution: resolution.into(),
        }
    }
}

impl From<UtteranceMatch> for sk::UtteranceMatch {
    /// `needsOwnerChoice` is derived from `resolution`, so it is not carried back.
    fn from(v: UtteranceMatch) -> Self {
        let UtteranceMatch {
            workflows,
            skills,
            resolution,
            needs_owner_choice: _,
        } = v;
        sk::UtteranceMatch {
            workflows,
            skills: skills.into_iter().map(Into::into).collect(),
            resolution: resolution.into(),
        }
    }
}

// ---------------------------------------------------------------------------------------
// Remote control.
// ---------------------------------------------------------------------------------------

/// Parses a device id from Swift (32 hex characters). Anything else is an unknown device.
pub(crate) fn device_id_from(s: &str) -> Result<r::DeviceId, JarvisError> {
    r::DeviceId::from_hex(s.trim()).ok_or_else(|| JarvisError::RemoteUnknownDevice {
        device_id: s.chars().take(64).collect(),
    })
}

fn opt_device_id_from(s: Option<String>) -> Result<Option<r::DeviceId>, JarvisError> {
    s.as_deref().map(device_id_from).transpose()
}

impl From<r::EntropyError> for EntropyProblem {
    fn from(v: r::EntropyError) -> Self {
        match v {
            r::EntropyError::TooShort => EntropyProblem::TooShort,
            r::EntropyError::NotRandom => EntropyProblem::NotRandom,
        }
    }
}

impl From<EntropyProblem> for r::EntropyError {
    fn from(v: EntropyProblem) -> Self {
        match v {
            EntropyProblem::TooShort => r::EntropyError::TooShort,
            EntropyProblem::NotRandom => r::EntropyError::NotRandom,
        }
    }
}

impl From<r::RemotePermissions> for RemotePermissions {
    fn from(v: r::RemotePermissions) -> Self {
        let r::RemotePermissions {
            start_tasks,
            see_progress,
            see_screen_text,
            approve,
        } = v;
        RemotePermissions {
            start_tasks,
            see_progress,
            see_screen_text,
            approve,
        }
    }
}

impl From<RemotePermissions> for r::RemotePermissions {
    fn from(v: RemotePermissions) -> Self {
        let RemotePermissions {
            start_tasks,
            see_progress,
            see_screen_text,
            approve,
        } = v;
        r::RemotePermissions {
            start_tasks,
            see_progress,
            see_screen_text,
            approve,
        }
    }
}

impl From<r::DeviceInfo> for DeviceInfo {
    fn from(v: r::DeviceInfo) -> Self {
        let r::DeviceInfo {
            id,
            name,
            permissions,
            paired_at_ms,
        } = v;
        DeviceInfo {
            id: id.to_hex(),
            name,
            permissions: permissions.into(),
            paired_at_ms,
            // Not part of the core record: `JarvisRemoteHub.devices()` fills it in.
            watch_screen: false,
        }
    }
}

impl TryFrom<DeviceInfo> for r::DeviceInfo {
    type Error = JarvisError;

    fn try_from(v: DeviceInfo) -> Result<Self, JarvisError> {
        let DeviceInfo {
            id,
            name,
            permissions,
            paired_at_ms,
            watch_screen: _,
        } = v;
        Ok(r::DeviceInfo {
            id: device_id_from(&id)?,
            name,
            permissions: permissions.into(),
            paired_at_ms,
        })
    }
}

impl From<r::CatalogEntry> for CatalogEntry {
    fn from(v: r::CatalogEntry) -> Self {
        let r::CatalogEntry { id, name, summary } = v;
        CatalogEntry { id, name, summary }
    }
}

impl From<CatalogEntry> for r::CatalogEntry {
    fn from(v: CatalogEntry) -> Self {
        let CatalogEntry { id, name, summary } = v;
        r::CatalogEntry { id, name, summary }
    }
}

impl From<r::PairingOffer> for PairingOffer {
    /// Copies the secret out of its zeroizing wrappers (which zeroize on drop); the copy is
    /// what Swift receives.
    fn from(v: r::PairingOffer) -> Self {
        let r::PairingOffer {
            code,
            qr_payload,
            expires_at_ms,
        } = v;
        PairingOffer {
            code: code.as_str().to_owned(),
            qr_payload: qr_payload.as_str().to_owned(),
            expires_at_ms,
        }
    }
}

impl From<PairingOffer> for r::PairingOffer {
    fn from(v: PairingOffer) -> Self {
        let PairingOffer {
            code,
            qr_payload,
            expires_at_ms,
        } = v;
        r::PairingOffer {
            code: Zeroizing::new(code),
            qr_payload: Zeroizing::new(qr_payload),
            expires_at_ms,
        }
    }
}

impl From<r::CloseReason> for CloseReason {
    fn from(v: r::CloseReason) -> Self {
        match v {
            r::CloseReason::AuthFailed => CloseReason::AuthFailed,
            r::CloseReason::Replayed => CloseReason::Replayed,
            r::CloseReason::LockedOut => CloseReason::LockedOut,
            r::CloseReason::HandshakeTimeout => CloseReason::HandshakeTimeout,
            r::CloseReason::IdleTimeout => CloseReason::IdleTimeout,
            r::CloseReason::TooManyConnections => CloseReason::TooManyConnections,
            r::CloseReason::DuplicateConnection => CloseReason::DuplicateConnection,
            r::CloseReason::TooLarge => CloseReason::TooLarge,
            r::CloseReason::Revoked => CloseReason::Revoked,
            r::CloseReason::TooManyDevices => CloseReason::TooManyDevices,
            r::CloseReason::BadEntropy => CloseReason::BadEntropy,
            r::CloseReason::PeerClosed => CloseReason::PeerClosed,
            r::CloseReason::Internal => CloseReason::Internal,
        }
    }
}

impl From<CloseReason> for r::CloseReason {
    fn from(v: CloseReason) -> Self {
        match v {
            CloseReason::AuthFailed => r::CloseReason::AuthFailed,
            CloseReason::Replayed => r::CloseReason::Replayed,
            CloseReason::LockedOut => r::CloseReason::LockedOut,
            CloseReason::HandshakeTimeout => r::CloseReason::HandshakeTimeout,
            CloseReason::IdleTimeout => r::CloseReason::IdleTimeout,
            CloseReason::TooManyConnections => r::CloseReason::TooManyConnections,
            CloseReason::DuplicateConnection => r::CloseReason::DuplicateConnection,
            CloseReason::TooLarge => r::CloseReason::TooLarge,
            CloseReason::Revoked => r::CloseReason::Revoked,
            CloseReason::TooManyDevices => r::CloseReason::TooManyDevices,
            CloseReason::BadEntropy => r::CloseReason::BadEntropy,
            CloseReason::PeerClosed => r::CloseReason::PeerClosed,
            CloseReason::Internal => r::CloseReason::Internal,
        }
    }
}

impl From<r::DenyReason> for DenyReason {
    fn from(v: r::DenyReason) -> Self {
        match v {
            r::DenyReason::NotPermitted => DenyReason::NotPermitted,
            r::DenyReason::Busy => DenyReason::Busy,
            r::DenyReason::NotRunning => DenyReason::NotRunning,
            r::DenyReason::StaleRequest => DenyReason::StaleRequest,
            r::DenyReason::NotRemotelyApprovable => DenyReason::NotRemotelyApprovable,
            r::DenyReason::UnknownSkill => DenyReason::UnknownSkill,
        }
    }
}

impl From<DenyReason> for r::DenyReason {
    fn from(v: DenyReason) -> Self {
        match v {
            DenyReason::NotPermitted => r::DenyReason::NotPermitted,
            DenyReason::Busy => r::DenyReason::Busy,
            DenyReason::NotRunning => r::DenyReason::NotRunning,
            DenyReason::StaleRequest => r::DenyReason::StaleRequest,
            DenyReason::NotRemotelyApprovable => r::DenyReason::NotRemotelyApprovable,
            DenyReason::UnknownSkill => r::DenyReason::UnknownSkill,
        }
    }
}

impl From<r::ErrorCode> for RemoteErrorCode {
    fn from(v: r::ErrorCode) -> Self {
        match v {
            r::ErrorCode::Malformed => RemoteErrorCode::Malformed,
            r::ErrorCode::UnknownMessage => RemoteErrorCode::UnknownMessage,
            r::ErrorCode::RateLimited => RemoteErrorCode::RateLimited,
            r::ErrorCode::InvalidRequest => RemoteErrorCode::InvalidRequest,
            r::ErrorCode::TooLarge => RemoteErrorCode::TooLarge,
        }
    }
}

impl From<RemoteErrorCode> for r::ErrorCode {
    fn from(v: RemoteErrorCode) -> Self {
        match v {
            RemoteErrorCode::Malformed => r::ErrorCode::Malformed,
            RemoteErrorCode::UnknownMessage => r::ErrorCode::UnknownMessage,
            RemoteErrorCode::RateLimited => r::ErrorCode::RateLimited,
            RemoteErrorCode::InvalidRequest => r::ErrorCode::InvalidRequest,
            RemoteErrorCode::TooLarge => r::ErrorCode::TooLarge,
        }
    }
}

impl From<r::AuditEvent> for AuditEvent {
    fn from(v: r::AuditEvent) -> Self {
        match v {
            r::AuditEvent::PairingExpired => AuditEvent::PairingExpired,
            r::AuditEvent::Paired { name } => AuditEvent::Paired { name },
            r::AuditEvent::PairingAbandoned => AuditEvent::PairingAbandoned,
            r::AuditEvent::Connected => AuditEvent::Connected,
            r::AuditEvent::Closed { reason } => AuditEvent::Closed {
                reason: reason.into(),
            },
            r::AuditEvent::ConnectionRefused { reason } => AuditEvent::ConnectionRefused {
                reason: reason.into(),
            },
            r::AuditEvent::HandshakeFailed { failures } => AuditEvent::HandshakeFailed { failures },
            r::AuditEvent::PreAuthSuppressed { count, since_ms } => {
                AuditEvent::PreAuthSuppressed { count, since_ms }
            }
            r::AuditEvent::LockedOut { until_ms } => AuditEvent::LockedOut { until_ms },
            r::AuditEvent::Accepted { request, detail } => AuditEvent::Accepted { request, detail },
            r::AuditEvent::Denied { request, reason } => AuditEvent::Denied {
                request,
                reason: reason.into(),
            },
            r::AuditEvent::Rejected { request, code } => AuditEvent::Rejected {
                request,
                code: code.into(),
            },
            r::AuditEvent::Revoked => AuditEvent::Revoked,
            r::AuditEvent::PermissionsChanged { permissions } => AuditEvent::PermissionsChanged {
                permissions: permissions.into(),
            },
        }
    }
}

impl From<AuditEvent> for r::AuditEvent {
    fn from(v: AuditEvent) -> Self {
        match v {
            AuditEvent::PairingExpired => r::AuditEvent::PairingExpired,
            AuditEvent::Paired { name } => r::AuditEvent::Paired { name },
            AuditEvent::PairingAbandoned => r::AuditEvent::PairingAbandoned,
            AuditEvent::Connected => r::AuditEvent::Connected,
            AuditEvent::Closed { reason } => r::AuditEvent::Closed {
                reason: reason.into(),
            },
            AuditEvent::ConnectionRefused { reason } => r::AuditEvent::ConnectionRefused {
                reason: reason.into(),
            },
            AuditEvent::HandshakeFailed { failures } => r::AuditEvent::HandshakeFailed { failures },
            AuditEvent::PreAuthSuppressed { count, since_ms } => {
                r::AuditEvent::PreAuthSuppressed { count, since_ms }
            }
            AuditEvent::LockedOut { until_ms } => r::AuditEvent::LockedOut { until_ms },
            AuditEvent::Accepted { request, detail } => r::AuditEvent::Accepted { request, detail },
            AuditEvent::Denied { request, reason } => r::AuditEvent::Denied {
                request,
                reason: reason.into(),
            },
            AuditEvent::Rejected { request, code } => r::AuditEvent::Rejected {
                request,
                code: code.into(),
            },
            AuditEvent::Revoked => r::AuditEvent::Revoked,
            AuditEvent::PermissionsChanged { permissions } => r::AuditEvent::PermissionsChanged {
                permissions: permissions.into(),
            },
        }
    }
}

impl From<r::AuditEntry> for AuditEntry {
    fn from(v: r::AuditEntry) -> Self {
        let r::AuditEntry {
            at_ms,
            conn_id,
            device_id,
            event,
        } = v;
        AuditEntry {
            at_ms,
            conn_id,
            device_id: device_id.map(|d| d.to_hex()),
            event: event.into(),
        }
    }
}

impl TryFrom<AuditEntry> for r::AuditEntry {
    type Error = JarvisError;

    fn try_from(v: AuditEntry) -> Result<Self, JarvisError> {
        let AuditEntry {
            at_ms,
            conn_id,
            device_id,
            event,
        } = v;
        Ok(r::AuditEntry {
            at_ms,
            conn_id,
            device_id: opt_device_id_from(device_id)?,
            event: event.into(),
        })
    }
}

impl From<r::HubEffect> for HubEffect {
    fn from(v: r::HubEffect) -> Self {
        match v {
            r::HubEffect::Send { conn_id, bytes } => HubEffect::Send { conn_id, bytes },
            r::HubEffect::Close { conn_id, reason } => HubEffect::Close {
                conn_id,
                reason: reason.into(),
            },
            r::HubEffect::StoreChanged => HubEffect::StoreChanged,
            r::HubEffect::StartTask {
                task_id,
                device_id,
                text,
            } => HubEffect::StartTask {
                task_id,
                device_id: device_id.to_hex(),
                text,
            },
            r::HubEffect::RunSkill {
                task_id,
                device_id,
                id,
                args,
            } => HubEffect::RunSkill {
                task_id,
                device_id: device_id.to_hex(),
                id,
                args: args.into_iter().collect(),
            },
            r::HubEffect::RunWorkflow {
                task_id,
                device_id,
                id,
            } => HubEffect::RunWorkflow {
                task_id,
                device_id: device_id.to_hex(),
                id,
            },
            r::HubEffect::CancelTask { device_id, task_id } => HubEffect::CancelTask {
                device_id: device_id.to_hex(),
                task_id,
            },
            r::HubEffect::Decide {
                request_id,
                approved,
                category,
                device_id,
            } => HubEffect::Decide {
                request_id,
                approved,
                category: category.into(),
                device_id: device_id.to_hex(),
            },
            r::HubEffect::DevicePaired { device } => HubEffect::DevicePaired {
                device: device.into(),
            },
            r::HubEffect::Audit { entry } => HubEffect::Audit {
                entry: entry.into(),
            },
        }
    }
}

impl TryFrom<HubEffect> for r::HubEffect {
    type Error = JarvisError;

    fn try_from(v: HubEffect) -> Result<Self, JarvisError> {
        Ok(match v {
            HubEffect::Send { conn_id, bytes } => r::HubEffect::Send { conn_id, bytes },
            HubEffect::Close { conn_id, reason } => r::HubEffect::Close {
                conn_id,
                reason: reason.into(),
            },
            HubEffect::StoreChanged => r::HubEffect::StoreChanged,
            HubEffect::StartTask {
                task_id,
                device_id,
                text,
            } => r::HubEffect::StartTask {
                task_id,
                device_id: device_id_from(&device_id)?,
                text,
            },
            HubEffect::RunSkill {
                task_id,
                device_id,
                id,
                args,
            } => r::HubEffect::RunSkill {
                task_id,
                device_id: device_id_from(&device_id)?,
                id,
                args: args.into_iter().collect(),
            },
            HubEffect::RunWorkflow {
                task_id,
                device_id,
                id,
            } => r::HubEffect::RunWorkflow {
                task_id,
                device_id: device_id_from(&device_id)?,
                id,
            },
            HubEffect::CancelTask { device_id, task_id } => r::HubEffect::CancelTask {
                device_id: device_id_from(&device_id)?,
                task_id,
            },
            HubEffect::Decide {
                request_id,
                approved,
                category,
                device_id,
            } => r::HubEffect::Decide {
                request_id,
                approved,
                category: category.into(),
                device_id: device_id_from(&device_id)?,
            },
            HubEffect::DevicePaired { device } => r::HubEffect::DevicePaired {
                device: device.try_into()?,
            },
            HubEffect::Audit { entry } => r::HubEffect::Audit {
                entry: entry.try_into()?,
            },
        })
    }
}

/// The text remotes with `see_screen_text` get for a screen: built from the compacted,
/// redacted screen (`jarvis_safety::redact_screen`: secure fields emptied, credential
/// fields masked, secrets in labels and values masked), one `label: value` item per
/// element. The hub normalises, redacts again and caps it before sending.
pub(crate) fn remote_screen_text(screen: &p::Screen) -> String {
    let safe = jarvis_safety::redact_screen(&jarvis_observe::compact(screen));
    let mut items: Vec<String> = Vec::new();
    for e in &safe.elements {
        let label = e.label.trim();
        let value = e.value.as_deref().map(str::trim).unwrap_or_default();
        let item = match (label.is_empty(), value.is_empty()) {
            (true, true) => continue,
            (false, true) => label.to_owned(),
            (true, false) => value.to_owned(),
            (false, false) => format!("{label}: {value}"),
        };
        items.push(item);
    }
    items.join(" · ")
}

impl TryFrom<LocalEvent> for r::LocalEvent {
    type Error = JarvisError;

    fn try_from(v: LocalEvent) -> Result<Self, JarvisError> {
        Ok(match v {
            LocalEvent::SessionStarted { task_id } => r::LocalEvent::SessionStarted { task_id },
            LocalEvent::Progress { progress, screen } => r::LocalEvent::Progress {
                progress: progress.try_into()?,
                screen_text: screen.map(|s| remote_screen_text(&s.into())),
            },
            LocalEvent::ConfirmRequested { request } => r::LocalEvent::ConfirmRequested {
                request: request.into(),
            },
            LocalEvent::ConfirmResolved {
                request_id,
                approved,
            } => r::LocalEvent::ConfirmResolved {
                request_id,
                approved,
            },
            LocalEvent::SessionFinished { task_id, outcome } => r::LocalEvent::SessionFinished {
                task_id,
                outcome: outcome.into(),
            },
            LocalEvent::DeviceRevoked { device_id } => r::LocalEvent::DeviceRevoked {
                device_id: device_id_from(&device_id)?,
            },
            LocalEvent::PermissionsChanged {
                device_id,
                permissions,
            } => r::LocalEvent::PermissionsChanged {
                device_id: device_id_from(&device_id)?,
                permissions: permissions.into(),
            },
            LocalEvent::CatalogChanged { skills, workflows } => r::LocalEvent::CatalogChanged {
                skills: skills.into_iter().map(Into::into).collect(),
                workflows: workflows.into_iter().map(Into::into).collect(),
            },
        })
    }
}

impl From<r::ClientMessage> for ClientMessage {
    fn from(v: r::ClientMessage) -> Self {
        match v {
            r::ClientMessage::Hello => ClientMessage::Hello,
            r::ClientMessage::StartTask { text } => ClientMessage::StartTask { text },
            r::ClientMessage::RunSkill { id, args } => ClientMessage::RunSkill {
                id,
                args: args.into_iter().collect(),
            },
            r::ClientMessage::RunWorkflow { id } => ClientMessage::RunWorkflow { id },
            r::ClientMessage::Cancel => ClientMessage::Cancel,
            r::ClientMessage::Decision {
                request_id,
                approved,
            } => ClientMessage::Decision {
                request_id,
                approved,
            },
            r::ClientMessage::Subscribe => ClientMessage::Subscribe,
            r::ClientMessage::Unsubscribe => ClientMessage::Unsubscribe,
            r::ClientMessage::ListSkills => ClientMessage::ListSkills,
            r::ClientMessage::Ping => ClientMessage::Ping,
        }
    }
}

impl From<ClientMessage> for r::ClientMessage {
    fn from(v: ClientMessage) -> Self {
        match v {
            ClientMessage::Hello => r::ClientMessage::Hello,
            ClientMessage::StartTask { text } => r::ClientMessage::StartTask { text },
            ClientMessage::RunSkill { id, args } => r::ClientMessage::RunSkill {
                id,
                args: args.into_iter().collect(),
            },
            ClientMessage::RunWorkflow { id } => r::ClientMessage::RunWorkflow { id },
            ClientMessage::Cancel => r::ClientMessage::Cancel,
            ClientMessage::Decision {
                request_id,
                approved,
            } => r::ClientMessage::Decision {
                request_id,
                approved,
            },
            ClientMessage::Subscribe => r::ClientMessage::Subscribe,
            ClientMessage::Unsubscribe => r::ClientMessage::Unsubscribe,
            ClientMessage::ListSkills => r::ClientMessage::ListSkills,
            ClientMessage::Ping => r::ClientMessage::Ping,
        }
    }
}

impl From<r::ServerMessage> for ServerMessage {
    fn from(v: r::ServerMessage) -> Self {
        match v {
            r::ServerMessage::Welcome {
                protocol_version,
                permissions,
                phone_name,
                busy,
            } => ServerMessage::Welcome {
                protocol_version,
                permissions: permissions.into(),
                phone_name,
                busy,
            },
            r::ServerMessage::TaskStarted => ServerMessage::TaskStarted,
            r::ServerMessage::Progress {
                step,
                phase,
                app,
                last_action,
                screen_text,
                waiting_for_unlock,
            } => ServerMessage::Progress {
                step,
                phase: phase.into(),
                app,
                last_action,
                screen_text,
                waiting_for_unlock,
            },
            r::ServerMessage::ConfirmPending {
                request_id,
                category,
                summary,
                can_approve,
            } => ServerMessage::ConfirmPending {
                request_id,
                category: category.into(),
                summary,
                can_approve,
            },
            r::ServerMessage::ConfirmResolved { request_id } => {
                ServerMessage::ConfirmResolved { request_id }
            }
            r::ServerMessage::Finished { status, summary } => ServerMessage::Finished {
                status: status.into(),
                summary,
            },
            r::ServerMessage::Skills { skills, workflows } => ServerMessage::Skills {
                skills: skills.into_iter().map(Into::into).collect(),
                workflows: workflows.into_iter().map(Into::into).collect(),
            },
            r::ServerMessage::PermissionsChanged { permissions } => {
                ServerMessage::PermissionsChanged {
                    permissions: permissions.into(),
                }
            }
            r::ServerMessage::Denied { reason } => ServerMessage::Denied {
                reason: reason.into(),
            },
            r::ServerMessage::Error { code } => ServerMessage::Error { code: code.into() },
            r::ServerMessage::Pong => ServerMessage::Pong,
        }
    }
}

impl From<ServerMessage> for r::ServerMessage {
    fn from(v: ServerMessage) -> Self {
        match v {
            ServerMessage::Welcome {
                protocol_version,
                permissions,
                phone_name,
                busy,
            } => r::ServerMessage::Welcome {
                protocol_version,
                permissions: permissions.into(),
                phone_name,
                busy,
            },
            ServerMessage::TaskStarted => r::ServerMessage::TaskStarted,
            ServerMessage::Progress {
                step,
                phase,
                app,
                last_action,
                screen_text,
                waiting_for_unlock,
            } => r::ServerMessage::Progress {
                step,
                phase: phase.into(),
                app,
                last_action,
                screen_text,
                waiting_for_unlock,
            },
            ServerMessage::ConfirmPending {
                request_id,
                category,
                summary,
                can_approve,
            } => r::ServerMessage::ConfirmPending {
                request_id,
                category: category.into(),
                summary,
                can_approve,
            },
            ServerMessage::ConfirmResolved { request_id } => {
                r::ServerMessage::ConfirmResolved { request_id }
            }
            ServerMessage::Finished { status, summary } => r::ServerMessage::Finished {
                status: status.into(),
                summary,
            },
            ServerMessage::Skills { skills, workflows } => r::ServerMessage::Skills {
                skills: skills.into_iter().map(Into::into).collect(),
                workflows: workflows.into_iter().map(Into::into).collect(),
            },
            ServerMessage::PermissionsChanged { permissions } => {
                r::ServerMessage::PermissionsChanged {
                    permissions: permissions.into(),
                }
            }
            ServerMessage::Denied { reason } => r::ServerMessage::Denied {
                reason: reason.into(),
            },
            ServerMessage::Error { code } => r::ServerMessage::Error { code: code.into() },
            ServerMessage::Pong => r::ServerMessage::Pong,
        }
    }
}

impl From<r::ViewerEndReason> for RemoteViewerEndReason {
    fn from(v: r::ViewerEndReason) -> Self {
        match v {
            r::ViewerEndReason::Stopped => RemoteViewerEndReason::Stopped,
            r::ViewerEndReason::PermissionRevoked => RemoteViewerEndReason::PermissionRevoked,
            r::ViewerEndReason::Other => RemoteViewerEndReason::Other,
        }
    }
}

impl From<r::ViewerEvent> for RemoteViewerEvent {
    fn from(v: r::ViewerEvent) -> Self {
        match v {
            r::ViewerEvent::Delta { bytes } => RemoteViewerEvent::Delta { bytes },
            r::ViewerEvent::Snapshot { bytes, part, parts } => {
                RemoteViewerEvent::Snapshot { bytes, part, parts }
            }
            r::ViewerEvent::ResyncRequested => RemoteViewerEvent::ResyncRequested,
            r::ViewerEvent::Ended { reason } => RemoteViewerEvent::Ended {
                reason: reason.into(),
            },
            r::ViewerEvent::Denied { reason } => RemoteViewerEvent::Denied {
                reason: reason.into(),
            },
            r::ViewerEvent::Incompatible { version } => RemoteViewerEvent::Incompatible { version },
        }
    }
}

impl From<r::ClientEvent> for ClientEvent {
    /// `paired` copies the credentials JSON out of its zeroizing wrapper.
    fn from(v: r::ClientEvent) -> Self {
        match v {
            r::ClientEvent::Send { bytes } => ClientEvent::Send { bytes },
            r::ClientEvent::Paired { credentials } => ClientEvent::Paired {
                device_id: credentials.device_id.to_hex(),
                credentials_json: credentials.to_json().as_str().to_owned(),
            },
            r::ClientEvent::Connected => ClientEvent::Connected,
            r::ClientEvent::Message(m) => ClientEvent::Message { message: m.into() },
            r::ClientEvent::Undecodable(e) => ClientEvent::Undecodable {
                reason: e.to_string(),
            },
        }
    }
}

impl TryFrom<ClientEvent> for r::ClientEvent {
    type Error = JarvisError;

    /// `undecodable` comes back as `MessageError::Malformed` with the reason text.
    fn try_from(v: ClientEvent) -> Result<Self, JarvisError> {
        Ok(match v {
            ClientEvent::Send { bytes } => r::ClientEvent::Send { bytes },
            ClientEvent::Paired {
                device_id: _,
                credentials_json,
            } => {
                let json = Zeroizing::new(credentials_json);
                r::ClientEvent::Paired {
                    credentials: r::ClientCredentials::from_json(&json)
                        .map_err(crate::remote::persist_error)?,
                }
            }
            ClientEvent::Connected => r::ClientEvent::Connected,
            ClientEvent::Message { message } => r::ClientEvent::Message(message.into()),
            ClientEvent::Undecodable { reason } => {
                r::ClientEvent::Undecodable(r::MessageError::Malformed(reason))
            }
        })
    }
}
