//! Builders shared by the Live view tests: trees, observations and the update stream a run
//! produces, effect by effect (the order the shell's driver reports).

#![allow(dead_code)]

use jarvis_proto as p;
use jarvis_viewer::{Reducer, ReducerConfig, Update, ViewerTimeline};

pub mod random;

pub const PHONE: p::Size = p::Size {
    width: 393.0,
    height: 852.0,
};

pub fn rect(x: f64, y: f64, w: f64, h: f64) -> p::Rect {
    p::Rect::new(x, y, w, h)
}

/// A node like the Swift tests' `node(...)` builder.
#[derive(Clone)]
pub struct N(pub p::UiNode);

pub fn node(role: p::Role, label: Option<&str>, frame: p::Rect) -> N {
    N(p::UiNode {
        role,
        label: label.map(str::to_owned),
        value: None,
        identifier: None,
        placeholder: None,
        frame,
        enabled: true,
        visible: true,
        hittable: true,
        focused: false,
        selected: false,
        secure: false,
        children: Vec::new(),
    })
}

impl N {
    pub fn value(mut self, v: &str) -> Self {
        self.0.value = Some(v.to_owned());
        self
    }
    pub fn placeholder(mut self, v: &str) -> Self {
        self.0.placeholder = Some(v.to_owned());
        self
    }
    pub fn identifier(mut self, v: &str) -> Self {
        self.0.identifier = Some(v.to_owned());
        self
    }
    pub fn focused(mut self, f: bool) -> Self {
        self.0.focused = f;
        self
    }
    pub fn secure(mut self, s: bool) -> Self {
        self.0.secure = s;
        self
    }
    pub fn visible(mut self, v: bool) -> Self {
        self.0.visible = v;
        self
    }
    pub fn children(mut self, c: Vec<N>) -> Self {
        self.0.children = c.into_iter().map(|n| n.0).collect();
        self
    }
    pub fn build(self) -> p::UiNode {
        self.0
    }
}

pub const EMAIL_FRAME: p::Rect = p::Rect::new(16.0, 200.0, 361.0, 44.0);
pub const PASSWORD_FRAME: p::Rect = p::Rect::new(16.0, 260.0, 361.0, 44.0);
pub const SEND_FRAME: p::Rect = p::Rect::new(16.0, 400.0, 361.0, 44.0);
pub const SECRET_VALUE: &str = "hunter2-SECRET";

/// A sign-in screen: Email (plain), Password (secure, holding a value), PIN (secure-flagged
/// text field), Send.
pub fn login_tree_sized(focus: Option<&str>, size: p::Size) -> p::UiNode {
    let full = rect(0.0, 0.0, size.width, size.height);
    node(p::Role::Application, Some("Mail"), full)
        .children(vec![node(p::Role::Window, None, full).children(vec![
            node(
                p::Role::NavigationBar,
                Some("Sign in"),
                rect(0.0, 59.0, size.width, 81.0),
            ),
            node(p::Role::TextField, Some("Email"), EMAIL_FRAME)
                .value("me@example.org")
                .placeholder("you@example.com")
                .focused(focus == Some("email")),
            node(p::Role::SecureTextField, Some("Password"), PASSWORD_FRAME)
                .value(SECRET_VALUE)
                .placeholder("Required")
                .focused(focus == Some("password"))
                .secure(true),
            node(
                p::Role::TextField,
                Some("PIN"),
                rect(16.0, 320.0, 361.0, 44.0),
            )
            .value("4321-PIN")
            .focused(focus == Some("pin"))
            .secure(true),
            node(p::Role::Button, Some("Send"), SEND_FRAME).children(vec![node(
                p::Role::StaticText,
                Some("Send"),
                rect(170.0, 410.0, 50.0, 20.0),
            )]),
        ])])
        .build()
}

pub fn login_tree(focus: Option<&str>) -> p::UiNode {
    login_tree_sized(focus, PHONE)
}

pub fn observed_full(
    root: p::UiNode,
    size: p::Size,
    locked: bool,
    app: Option<&str>,
    bundle: &str,
) -> p::Event {
    p::Event::Observed(p::Observation {
        screen: p::Screen {
            tree_cut: false,
            app_bundle_id: bundle.to_owned(),
            app_name: app.map(str::to_owned),
            size,
            root,
            keyboard_visible: false,
            orientation: if size.width > size.height {
                p::Orientation::LandscapeLeft
            } else {
                p::Orientation::Portrait
            },
        },
        screenshot: None,
        device_locked: locked,
    })
}

pub fn observed(root: p::UiNode) -> p::Event {
    observed_full(root, PHONE, false, Some("Mail"), "com.apple.mobilemail")
}

/// A runner-text observation (its tree is not looked at by the viewer).
pub fn observed_text(bundle: &str, app: &str, locked: bool) -> p::Event {
    p::Event::ObservedRunnerText(p::Observation {
        screen: p::Screen {
            tree_cut: false,
            app_bundle_id: bundle.to_owned(),
            app_name: Some(app.to_owned()),
            size: PHONE,
            root: p::UiNode::new(p::Role::Application, PHONE.to_rect()),
            keyboard_visible: true,
            orientation: p::Orientation::Portrait,
        },
        screenshot: None,
        device_locked: locked,
    })
}

pub fn progress(step: u32, action: Option<p::Action>) -> p::Progress {
    p::Progress {
        step,
        phase: p::Phase::Acting,
        last_action: action,
        app: Some("Mail".to_owned()),
        ..p::Progress::default()
    }
}

pub fn type_action(element: Option<u32>, text: &str) -> p::Action {
    p::Action::Type {
        element,
        text: text.to_owned(),
        submit: false,
    }
}

pub fn tap(x: f64, y: f64) -> p::RunnerCommand {
    p::RunnerCommand::Tap { x, y, expect: None }
}

pub fn type_text(text: &str) -> p::RunnerCommand {
    p::RunnerCommand::TypeText {
        text: text.to_owned(),
        expect: None,
    }
}

pub fn infer() -> p::Effect {
    p::Effect::Infer(p::ModelRequest {
        system: String::new(),
        user: String::new(),
        image: None,
        marks: Vec::new(),
        max_tokens: 1,
        grammar: None,
        json_schema: None,
        kind: p::RequestKind::Text,
    })
}

/// The apps the shell names (`KnownApps.common` + the demo app).
pub fn known_apps() -> Vec<p::AppInfo> {
    [
        ("com.apple.Preferences", "Settings"),
        ("com.apple.mobilesafari", "Safari"),
        ("com.apple.MobileSMS", "Messages"),
        ("com.apple.mobilenotes", "Notes"),
        ("com.apple.mobilemail", "Mail"),
        ("dev.jarvis.demo.settings", "Demo Settings"),
    ]
    .into_iter()
    .map(|(b, n)| p::AppInfo::new(b, n))
    .collect()
}

pub fn config(max_steps: usize, frames: bool) -> ReducerConfig {
    ReducerConfig {
        max_steps,
        frames_enabled: frames,
        known_apps: known_apps(),
    }
}

/// The update stream a run produces, effect by effect.
#[derive(Clone)]
pub struct Run {
    pub updates: Vec<Update>,
    pub number: u64,
    pub core_step: u32,
}

impl Run {
    pub fn new(task_id: &str) -> Run {
        Run {
            updates: vec![Update::Started {
                task_id: task_id.to_owned(),
            }],
            number: 0,
            core_step: 0,
        }
    }

    pub fn observe(&mut self, event: p::Event) {
        self.number += 1;
        let effect = p::Effect::Observe { screenshot: false };
        self.updates.push(Update::Effect {
            effect: effect.clone(),
            number: self.number,
        });
        self.updates.push(Update::EffectCompleted { effect, event });
    }

    pub fn prompt(&mut self, text: &str) {
        self.updates.push(Update::Prompt(text.to_owned()));
    }

    pub fn act_with(
        &mut self,
        command: p::RunnerCommand,
        action: Option<p::Action>,
        result: p::RunnerResult,
    ) {
        self.number += 1;
        self.core_step += 1;
        let effect = p::Effect::Act(command);
        self.updates.push(Update::Effect {
            effect: effect.clone(),
            number: self.number,
        });
        self.updates
            .push(Update::Progress(progress(self.core_step, action)));
        self.updates.push(Update::EffectCompleted {
            effect,
            event: p::Event::ActResult(result),
        });
    }

    pub fn act(&mut self, command: p::RunnerCommand) {
        self.act_with(command, None, p::RunnerResult::ok());
    }

    pub fn act_typed(&mut self, command: p::RunnerCommand, action: p::Action) {
        self.act_with(command, Some(action), p::RunnerResult::ok());
    }

    pub fn add(&mut self, update: Update) {
        self.updates.push(update);
    }

    pub fn reducer(&self, config: ReducerConfig) -> Reducer {
        let mut r = Reducer::new(config);
        for u in &self.updates {
            r.apply(u.clone());
        }
        r
    }

    pub fn reduce(&self) -> Reducer {
        self.reducer(config(200, false))
    }

    pub fn timeline(&self) -> ViewerTimeline {
        self.reduce().timeline().clone()
    }
}

/// The encoded timeline as text.
pub fn json(t: &ViewerTimeline) -> String {
    String::from_utf8(t.encode()).unwrap()
}

pub fn close(a: f64, b: f64, tolerance: f64) -> bool {
    (a - b).abs() <= tolerance
}
