//! The end-to-end scenario shared by the Rust tests (and mirrored by the Swift smoke test in
//! `swift-smoke/`): Settings → Wi-Fi → switch off, with the Settings screen sent as a typed
//! tree and the Wi-Fi page as runner text, a scripted model that finds element numbers in
//! the prompt, and an approved confirmation.

#![allow(
    dead_code,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::collections::VecDeque;

use jarvis_core_ffi::*;

pub const SETTINGS: &str = "com.apple.Preferences";

pub fn rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
    Rect {
        x,
        y,
        width,
        height,
    }
}

pub fn ui(
    role: Role,
    label: Option<&str>,
    frame: Rect,
    hittable: bool,
    children: Vec<UiNode>,
) -> UiNode {
    UiNode {
        role,
        label: label.map(str::to_owned),
        value: None,
        identifier: None,
        placeholder: None,
        frame,
        enabled: true,
        visible: true,
        hittable,
        focused: false,
        selected: false,
        secure: false,
        children,
    }
}

pub fn size() -> Size {
    Size {
        width: 393.0,
        height: 852.0,
    }
}

/// The Settings root list as a typed tree.
pub fn settings_screen() -> Screen {
    let cell = |title: &str, detail: &str, y: f64| {
        ui(
            Role::Cell,
            None,
            rect(16.0, y, 361.0, 44.0),
            true,
            vec![
                ui(
                    Role::StaticText,
                    Some(title),
                    rect(60.0, y + 12.0, 200.0, 20.0),
                    false,
                    vec![],
                ),
                ui(
                    Role::StaticText,
                    Some(detail),
                    rect(260.0, y + 12.0, 100.0, 20.0),
                    false,
                    vec![],
                ),
            ],
        )
    };
    let nav = ui(
        Role::NavigationBar,
        None,
        rect(0.0, 59.0, 393.0, 81.0),
        false,
        vec![ui(
            Role::StaticText,
            Some("Settings"),
            rect(16.0, 96.0, 140.0, 41.0),
            false,
            vec![],
        )],
    );
    let table = ui(
        Role::Table,
        None,
        rect(0.0, 140.0, 393.0, 712.0),
        false,
        vec![
            cell("Wi-Fi", "HomeNet", 200.0),
            cell("Bluetooth", "On", 244.0),
        ],
    );
    let window = ui(
        Role::Window,
        None,
        rect(0.0, 0.0, 393.0, 852.0),
        false,
        vec![nav, table],
    );
    Screen {
        tree_cut: false,
        app_bundle_id: SETTINGS.to_owned(),
        app_name: Some("Settings".to_owned()),
        size: size(),
        root: ui(
            Role::Application,
            Some("Settings"),
            rect(0.0, 0.0, 393.0, 852.0),
            false,
            vec![window],
        ),
        keyboard_visible: false,
        orientation: Orientation::Portrait,
    }
}

/// The Wi-Fi page as the runner's text tree.
pub fn wifi_text(on: bool) -> String {
    format!(
        "Application, label: 'Settings', {{{{0, 0}}, {{393, 852}}}}\n\
         \x20 Window, {{{{0, 0}}, {{393, 852}}}}\n\
         \x20   NavigationBar, id: 'Wi-Fi', {{{{0, 59}}, {{393, 81}}}}\n\
         \x20     Button, label: 'Settings', id: 'BackButton', {{{{8, 70}}, {{90, 44}}}}\n\
         \x20   Table, {{{{0, 140}}, {{393, 712}}}}\n\
         \x20     Cell, {{{{16, 160}}, {{361, 44}}}}\n\
         \x20       Switch, label: 'Wi-Fi', value: '{}', {{{{312, 166}}, {{51, 31}}}}\n",
        if on { 1 } else { 0 }
    )
}

pub fn snapshot(source: String) -> RunnerSnapshot {
    RunnerSnapshot {
        source,
        app_bundle_id: SETTINGS.to_owned(),
        app_name: Some("Settings".to_owned()),
        size: size(),
        keyboard_visible: false,
        orientation: Orientation::Portrait,
        screenshot: None,
        device_locked: false,
    }
}

pub fn config() -> SessionConfig {
    let mut c = api::default_session_config().unwrap();
    c.known_apps = vec![AppInfo {
        bundle_id: SETTINGS.to_owned(),
        name: "Settings".to_owned(),
    }];
    c
}

pub fn task() -> TaskSpec {
    TaskSpec {
        id: "t1".to_owned(),
        text: "Turn off Wi-Fi".to_owned(),
        remote: false,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Settings,
    WifiOn,
    WifiOff,
}

/// The element number of the line `[N] <role> "<label>"` in a prompt.
pub fn index_in_prompt(user: &str, role: &str, label: &str) -> Option<u32> {
    let needle = format!("] {role} \"{label}\"");
    user.lines().find(|l| l.contains(&needle)).and_then(|l| {
        let l = l.trim_start();
        let inner = l.strip_prefix('[')?;
        inner.split(']').next()?.parse().ok()
    })
}

/// What the scripted model answers, in order: (role, label) to tap, then done.
pub fn model_script() -> VecDeque<Option<(&'static str, &'static str)>> {
    VecDeque::from(vec![
        Some(("Cell", "Wi-Fi, HomeNet")),
        Some(("Switch", "Wi-Fi")),
        None,
    ])
}

pub fn contains(r: Rect, x: f64, y: f64) -> bool {
    x >= r.x && x < r.x + r.width && y >= r.y && y < r.y + r.height
}

/// Everything one run did.
#[derive(Debug, Default)]
pub struct Run {
    pub outcome: Option<Outcome>,
    pub infers: usize,
    pub acts: usize,
    pub confirms: Vec<ConfirmRequest>,
    pub typed_observations: usize,
    pub text_observations: usize,
    pub memory_json: String,
    pub final_page: Option<Page>,
}

/// Drives one session through the FFI object until `Finished`, then calls `finish()`.
pub fn drive(memory_json: &str) -> Run {
    drive_with(config(), memory_json, |_| true)
}

/// [`drive`] with an explicit config and owner.
pub fn drive_with(
    config: SessionConfig,
    memory_json: &str,
    decide: impl Fn(&ConfirmRequest) -> bool,
) -> Run {
    let session = JarvisSession::new(config, task(), memory_json.to_owned()).unwrap();
    let mut page = Page::Settings;
    let mut script = model_script();
    let mut run = Run::default();
    let mut now = 1_000u64;
    let mut event = Event::Start;
    for _ in 0..200 {
        let effect = session.step(now, event);
        now += 50;
        event = match effect {
            Effect::Observe { .. } => match page {
                Page::Settings => {
                    run.typed_observations += 1;
                    Event::Observed {
                        observation: ScreenObservation {
                            screen: settings_screen(),
                            screenshot: None,
                            device_locked: false,
                        },
                    }
                }
                Page::WifiOn | Page::WifiOff => {
                    run.text_observations += 1;
                    Event::ObservedRunnerText {
                        snapshot: snapshot(wifi_text(page == Page::WifiOn)),
                    }
                }
            },
            Effect::Infer { request } => {
                run.infers += 1;
                let text = match script.pop_front().flatten() {
                    Some((role, label)) => {
                        let i = index_in_prompt(&request.user, role, label)
                            .unwrap_or_else(|| panic!("no {role} {label:?} in\n{}", request.user));
                        format!(r#"{{"thought":"tap {label}","action":"tap","element":{i}}}"#)
                    }
                    None => {
                        r#"{"thought":"done","action":"done","summary":"Wi-Fi is off"}"#.to_owned()
                    }
                };
                Event::ModelOutput {
                    output: ModelOutput { text },
                }
            }
            Effect::Act { command, .. } => {
                run.acts += 1;
                if let RunnerCommand::Tap { x, y } = command {
                    page = match page {
                        Page::Settings if contains(rect(16.0, 200.0, 361.0, 44.0), x, y) => {
                            Page::WifiOn
                        }
                        Page::WifiOn if contains(rect(312.0, 166.0, 51.0, 31.0), x, y) => {
                            Page::WifiOff
                        }
                        other => other,
                    };
                }
                Event::ActResult {
                    result: RunnerResult {
                        ok: true,
                        error: None,
                        failure: None,
                    },
                }
            }
            Effect::Confirm { request } => {
                let approved = decide(&request);
                run.confirms.push(request);
                Event::UserDecision {
                    approved,
                    by_phone: true,
                }
            }
            Effect::CallTool { call } => panic!("unexpected tool call {call:?}"),
            Effect::AskUser { question } => panic!("unexpected question {question:?}"),
            Effect::AwaitUnlock { reason, .. } => panic!("unexpected wait for unlock: {reason}"),
            Effect::Finished { outcome } => {
                run.outcome = Some(outcome);
                run.final_page = Some(page);
                run.memory_json = session.finish().unwrap();
                return run;
            }
        };
    }
    panic!("session did not finish");
}

/// Drives the Settings → Wi-Fi session and hands every update to `sink` the way the
/// shell's driver reports them: `.started`, then per effect `.effect`, `.progress`, the
/// effect's own update, `.effectCompleted`; `.finished` last. `on_confirm` runs right after
/// each `.confirmationPending`. Returns the update count.
pub fn drive_updates(
    mut sink: impl FnMut(CoreViewerUpdate),
    mut on_confirm: impl FnMut(),
) -> usize {
    let session = JarvisSession::new(config(), task(), String::new()).unwrap();
    let mut feeds = 0;
    let mut feed = |u: CoreViewerUpdate| {
        feeds += 1;
        sink(u);
    };
    feed(CoreViewerUpdate::Started { task_id: task().id });
    let mut page = Page::Settings;
    let mut script = model_script();
    let mut now = 1_000u64;
    let mut event = Event::Start;
    for number in 1..200u64 {
        let effect = session.step(now, event);
        now += 50;
        feed(CoreViewerUpdate::Effect {
            effect: effect.clone(),
            number,
        });
        feed(CoreViewerUpdate::Progress {
            progress: session.progress().unwrap(),
        });
        event = match &effect {
            Effect::Observe { .. } => match page {
                Page::Settings => Event::Observed {
                    observation: ScreenObservation {
                        screen: settings_screen(),
                        screenshot: None,
                        device_locked: false,
                    },
                },
                _ => Event::ObservedRunnerText {
                    snapshot: snapshot(wifi_text(page == Page::WifiOn)),
                },
            },
            Effect::Infer { request } => {
                feed(CoreViewerUpdate::Prompt {
                    text: request.user.clone(),
                });
                let text = match script.pop_front().flatten() {
                    Some((role, label)) => {
                        let i = index_in_prompt(&request.user, role, label).unwrap();
                        format!(r#"{{"thought":"tap","action":"tap","element":{i}}}"#)
                    }
                    None => {
                        r#"{"thought":"done","action":"done","summary":"Wi-Fi is off"}"#.to_owned()
                    }
                };
                Event::ModelOutput {
                    output: ModelOutput { text },
                }
            }
            Effect::Act { command, .. } => {
                if let RunnerCommand::Tap { x, y } = command {
                    page = match page {
                        Page::Settings if contains(rect(16.0, 200.0, 361.0, 44.0), *x, *y) => {
                            Page::WifiOn
                        }
                        Page::WifiOn if contains(rect(312.0, 166.0, 51.0, 31.0), *x, *y) => {
                            Page::WifiOff
                        }
                        other => other,
                    };
                }
                Event::ActResult {
                    result: RunnerResult {
                        ok: true,
                        error: None,
                        failure: None,
                    },
                }
            }
            Effect::Confirm { request } => {
                feed(CoreViewerUpdate::ConfirmationPending {
                    request: request.clone(),
                });
                on_confirm();
                Event::UserDecision {
                    approved: true,
                    by_phone: true,
                }
            }
            Effect::Finished { outcome } => {
                feed(CoreViewerUpdate::Finished {
                    status: outcome.status,
                });
                return feeds;
            }
            other => panic!("unexpected {other:?}"),
        };
        feed(CoreViewerUpdate::EffectCompleted {
            effect,
            event: event.clone(),
        });
    }
    panic!("session did not finish");
}
