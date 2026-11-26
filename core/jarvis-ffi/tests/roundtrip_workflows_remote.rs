//! Rust → ffi → Rust round trips for every workflow and remote-control mirror and every
//! enum variant (the `*_variant` functions match exhaustively, so a new variant in
//! `jarvis-skills` or `jarvis-remote` fails to compile here), plus the ffi → Rust
//! conversion of every `LocalEvent` variant, and redacted `Debug` for the secret records.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Debug;

use jarvis_core_ffi as f;
use jarvis_proto as p;
use jarvis_remote as r;
use jarvis_skills as sk;

fn rt<R, F>(value: R)
where
    R: Clone + PartialEq + Debug + Into<F>,
    F: Into<R> + Clone + Debug,
{
    let ffi: F = value.clone().into();
    let back: R = ffi.clone().into();
    assert_eq!(back, value, "via {ffi:?}");
}

fn rt_try<R, F>(value: R)
where
    R: Clone + PartialEq + Debug + Into<F>,
    F: TryInto<R> + Clone + Debug,
    <F as TryInto<R>>::Error: Debug,
{
    let ffi: F = value.clone().into();
    let back: R = ffi.clone().try_into().unwrap();
    assert_eq!(back, value, "via {ffi:?}");
}

fn assert_covers(seen: impl IntoIterator<Item = usize>, count: usize, what: &str) {
    let seen: BTreeSet<usize> = seen.into_iter().collect();
    let all: BTreeSet<usize> = (0..count).collect();
    assert_eq!(seen, all, "{what}: not every variant has a round-trip case");
}

const OUTCOMES: [p::OutcomeStatus; 5] = [
    p::OutcomeStatus::Success,
    p::OutcomeStatus::Failed,
    p::OutcomeStatus::Cancelled,
    p::OutcomeStatus::Blocked,
    p::OutcomeStatus::Interrupted,
];

const PHASES: [p::Phase; 9] = [
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

fn args() -> BTreeMap<String, String> {
    [
        ("contact".to_owned(), "Mom".to_owned()),
        ("n".to_owned(), "2".to_owned()),
    ]
    .into_iter()
    .collect()
}

// ---------------------------------------------------------------------------------------
// Workflows.
// ---------------------------------------------------------------------------------------

fn weekday_variant(v: &sk::Weekday) -> usize {
    match v {
        sk::Weekday::Mon => 0,
        sk::Weekday::Tue => 1,
        sk::Weekday::Wed => 2,
        sk::Weekday::Thu => 3,
        sk::Weekday::Fri => 4,
        sk::Weekday::Sat => 5,
        sk::Weekday::Sun => 6,
    }
}

fn schedule() -> sk::Schedule {
    sk::Schedule {
        days: [sk::Weekday::Mon, sk::Weekday::Sun].into_iter().collect(),
        hour: 7,
        minute: 45,
    }
}

fn trigger_variant(v: &sk::Trigger) -> usize {
    match v {
        sk::Trigger::Manual => 0,
        sk::Trigger::Phrase { .. } => 1,
        sk::Trigger::Schedule(_) => 2,
        sk::Trigger::External { .. } => 3,
    }
}

fn triggers() -> Vec<sk::Trigger> {
    vec![
        sk::Trigger::Manual,
        sk::Trigger::Phrase {
            text: "good night".into(),
        },
        sk::Trigger::Schedule(schedule()),
        sk::Trigger::External {
            key: "charger.on".into(),
        },
    ]
}

fn condition_variant(v: &sk::Condition) -> usize {
    match v {
        sk::Condition::LastSucceeded => 0,
        sk::Condition::LastSummaryContains { .. } => 1,
        sk::Condition::OwnerSaidYes => 2,
    }
}

fn conditions() -> Vec<sk::Condition> {
    vec![
        sk::Condition::LastSucceeded,
        sk::Condition::LastSummaryContains {
            text: "sent".into(),
        },
        sk::Condition::OwnerSaidYes,
    ]
}

fn step_variant(v: &sk::Step) -> usize {
    match v {
        sk::Step::RunSkill { .. } => 0,
        sk::Step::RunTask { .. } => 1,
        sk::Step::AskOwner { .. } => 2,
        sk::Step::Notify { .. } => 3,
        sk::Step::Wait { .. } => 4,
        sk::Step::StopUnless { .. } => 5,
    }
}

fn steps() -> Vec<sk::Step> {
    let mut v = vec![
        sk::Step::RunSkill {
            skill_id: "text-someone".into(),
            args: args(),
        },
        sk::Step::RunTask {
            text: "Turn on Do Not Disturb".into(),
        },
        sk::Step::AskOwner {
            question: "Go on?".into(),
        },
        sk::Step::Notify {
            text: "Done".into(),
        },
        sk::Step::Wait { ms: 60_000 },
    ];
    v.extend(
        conditions()
            .into_iter()
            .map(|condition| sk::Step::StopUnless { condition }),
    );
    v
}

fn status_variant(v: &sk::WorkflowStatus) -> usize {
    match v {
        sk::WorkflowStatus::Succeeded => 0,
        sk::WorkflowStatus::CompletedWithFailures => 1,
        sk::WorkflowStatus::Failed => 2,
        sk::WorkflowStatus::Stopped => 3,
        sk::WorkflowStatus::Cancelled => 4,
        sk::WorkflowStatus::Blocked => 5,
        sk::WorkflowStatus::Interrupted => 6,
    }
}

const STATUSES: [sk::WorkflowStatus; 7] = [
    sk::WorkflowStatus::Succeeded,
    sk::WorkflowStatus::CompletedWithFailures,
    sk::WorkflowStatus::Failed,
    sk::WorkflowStatus::Stopped,
    sk::WorkflowStatus::Cancelled,
    sk::WorkflowStatus::Blocked,
    sk::WorkflowStatus::Interrupted,
];

fn outcome(status: sk::WorkflowStatus) -> sk::WorkflowOutcome {
    sk::WorkflowOutcome {
        status,
        steps_run: 3,
        summary: "Step 2 failed: no network.".into(),
        failed_step: Some(1),
    }
}

fn on_failure_variant(v: &sk::OnFailure) -> usize {
    match v {
        sk::OnFailure::Stop => 0,
        sk::OnFailure::Continue => 1,
    }
}

fn workflow(trigger: sk::Trigger, on_failure: sk::OnFailure) -> sk::Workflow {
    sk::Workflow {
        id: "evening".into(),
        name: "Evening".into(),
        enabled: false,
        trigger,
        steps: steps(),
        on_failure,
        created_ms: 1,
        updated_ms: 2,
        runs: 3,
        last_run_ms: Some(4),
        last_outcome: Some(outcome(sk::WorkflowStatus::Failed)),
    }
}

#[test]
fn workflow_types_round_trip() {
    for d in sk::Weekday::ALL {
        rt::<_, f::Weekday>(d);
    }
    assert_covers(sk::Weekday::ALL.iter().map(weekday_variant), 7, "Weekday");
    rt::<_, f::Schedule>(schedule());
    rt::<_, f::Schedule>(sk::Schedule::default());

    let ts = triggers();
    for t in &ts {
        rt::<_, f::Trigger>(t.clone());
    }
    assert_covers(ts.iter().map(trigger_variant), 4, "Trigger");
    let cs = conditions();
    for c in &cs {
        rt::<_, f::Condition>(c.clone());
    }
    assert_covers(cs.iter().map(condition_variant), 3, "Condition");
    let ss = steps();
    for s in &ss {
        rt::<_, f::Step>(s.clone());
    }
    assert_covers(ss.iter().map(step_variant), 6, "Step");
    let fs = [sk::OnFailure::Stop, sk::OnFailure::Continue];
    for o in fs {
        rt::<_, f::OnFailure>(o);
    }
    assert_covers(fs.iter().map(on_failure_variant), 2, "OnFailure");
    for s in STATUSES {
        rt::<_, f::WorkflowStatus>(s);
        rt::<_, f::WorkflowOutcome>(outcome(s));
    }
    assert_covers(STATUSES.iter().map(status_variant), 7, "WorkflowStatus");
    rt::<_, f::WorkflowOutcome>(sk::WorkflowOutcome::default());
    for (t, o) in ts.into_iter().zip(fs.into_iter().cycle()) {
        rt::<_, f::Workflow>(workflow(t, o));
    }
    rt::<_, f::Workflow>(sk::Workflow::default());
    rt::<_, f::RunLimits>(sk::RunLimits::default());
    rt::<_, f::RunLimits>(sk::RunLimits {
        max_steps: 1,
        max_total_ms: 2,
        max_wait_ms: 3,
    });
}

fn event_variant(v: &sk::WorkflowEvent) -> usize {
    match v {
        sk::WorkflowEvent::Start => 0,
        sk::WorkflowEvent::SessionFinished { .. } => 1,
        sk::WorkflowEvent::OwnerAnswered { .. } => 2,
        sk::WorkflowEvent::Notified => 3,
        sk::WorkflowEvent::WaitElapsed => 4,
        sk::WorkflowEvent::Cancel => 5,
    }
}

fn effect_variant(v: &sk::WorkflowEffect) -> usize {
    match v {
        sk::WorkflowEffect::StartSession { .. } => 0,
        sk::WorkflowEffect::AskOwner { .. } => 1,
        sk::WorkflowEffect::Notify { .. } => 2,
        sk::WorkflowEffect::Wait { .. } => 3,
        sk::WorkflowEffect::Finished { .. } => 4,
    }
}

fn awaiting_variant(v: &sk::Awaiting) -> usize {
    match v {
        sk::Awaiting::Start => 0,
        sk::Awaiting::Session => 1,
        sk::Awaiting::Answer => 2,
        sk::Awaiting::Notified => 3,
        sk::Awaiting::WaitElapsed => 4,
        sk::Awaiting::Nothing => 5,
    }
}

fn resolution_variant(v: &sk::Resolution) -> usize {
    match v {
        sk::Resolution::RunWorkflow { .. } => 0,
        sk::Resolution::RunSkill(_) => 1,
        sk::Resolution::OwnerChoice => 2,
        sk::Resolution::NoMatch => 3,
    }
}

fn skill_match() -> sk::SkillMatch {
    sk::SkillMatch {
        skill_id: "text-someone".into(),
        score: 1000,
        args: args(),
        matched_trigger: "text {contact}".into(),
        missing: vec!["message".into()],
        exact: true,
    }
}

#[test]
fn run_and_resolution_types_round_trip() {
    let mut events = vec![
        sk::WorkflowEvent::Start,
        sk::WorkflowEvent::OwnerAnswered {
            yes: true,
            text: "yes please".into(),
        },
        sk::WorkflowEvent::Notified,
        sk::WorkflowEvent::WaitElapsed,
        sk::WorkflowEvent::Cancel,
    ];
    for status in OUTCOMES {
        events.push(sk::WorkflowEvent::SessionFinished {
            status,
            summary: "ok".into(),
        });
    }
    for e in &events {
        rt::<_, f::WorkflowEvent>(e.clone());
    }
    assert_covers(events.iter().map(event_variant), 6, "WorkflowEvent");

    let effects = vec![
        sk::WorkflowEffect::StartSession {
            task: "Text Mom".into(),
            skill: Some(p::SkillContext {
                id: "text-someone".into(),
                name: "Text someone".into(),
                instructions: "Use the thread.".into(),
                apps: vec!["com.apple.MobileSMS".into()],
                confine_to_apps: true,
                tools: vec!["contacts.lookup".into()],
                args: args(),
            }),
        },
        sk::WorkflowEffect::StartSession {
            task: "free".into(),
            skill: None,
        },
        sk::WorkflowEffect::AskOwner {
            question: "?".into(),
        },
        sk::WorkflowEffect::Notify { text: "n".into() },
        sk::WorkflowEffect::Wait { until_ms: 99 },
        sk::WorkflowEffect::Finished {
            outcome: outcome(sk::WorkflowStatus::Stopped),
        },
    ];
    for e in &effects {
        rt::<_, f::WorkflowEffect>(e.clone());
    }
    assert_covers(effects.iter().map(effect_variant), 5, "WorkflowEffect");

    let awaiting = [
        sk::Awaiting::Start,
        sk::Awaiting::Session,
        sk::Awaiting::Answer,
        sk::Awaiting::Notified,
        sk::Awaiting::WaitElapsed,
        sk::Awaiting::Nothing,
    ];
    for a in awaiting {
        rt::<_, f::Awaiting>(a);
    }
    assert_covers(awaiting.iter().map(awaiting_variant), 6, "Awaiting");

    let resolutions = vec![
        sk::Resolution::RunWorkflow {
            workflow_id: "evening".into(),
        },
        sk::Resolution::RunSkill(skill_match()),
        sk::Resolution::OwnerChoice,
        sk::Resolution::NoMatch,
    ];
    for res in &resolutions {
        rt::<_, f::Resolution>(res.clone());
        let m = sk::UtteranceMatch {
            workflows: vec!["evening".into()],
            skills: vec![skill_match()],
            resolution: res.clone(),
        };
        let ffi: f::UtteranceMatch = m.clone().into();
        assert_eq!(ffi.needs_owner_choice, *res == sk::Resolution::OwnerChoice);
        rt::<_, f::UtteranceMatch>(m);
    }
    assert_covers(resolutions.iter().map(resolution_variant), 4, "Resolution");
}

// ---------------------------------------------------------------------------------------
// Remote control.
// ---------------------------------------------------------------------------------------

fn dev(i: u8) -> r::DeviceId {
    r::DeviceId([i; 16])
}

fn perms() -> r::RemotePermissions {
    r::RemotePermissions {
        start_tasks: true,
        see_progress: false,
        see_screen_text: true,
        approve: false,
    }
}

fn info() -> r::DeviceInfo {
    r::DeviceInfo {
        id: dev(3),
        name: "Work Mac".into(),
        permissions: perms(),
        paired_at_ms: 77,
    }
}

fn catalog() -> Vec<r::CatalogEntry> {
    vec![r::CatalogEntry {
        id: "wifi-off".into(),
        name: "Wi-Fi off".into(),
        summary: "Turns Wi-Fi off".into(),
    }]
}

fn close_variant(v: &r::CloseReason) -> usize {
    match v {
        r::CloseReason::AuthFailed => 0,
        r::CloseReason::Replayed => 1,
        r::CloseReason::LockedOut => 2,
        r::CloseReason::HandshakeTimeout => 3,
        r::CloseReason::IdleTimeout => 4,
        r::CloseReason::TooManyConnections => 5,
        r::CloseReason::DuplicateConnection => 6,
        r::CloseReason::TooLarge => 7,
        r::CloseReason::Revoked => 8,
        r::CloseReason::TooManyDevices => 9,
        r::CloseReason::BadEntropy => 10,
        r::CloseReason::PeerClosed => 11,
        r::CloseReason::Internal => 12,
    }
}

const CLOSES: [r::CloseReason; 13] = [
    r::CloseReason::AuthFailed,
    r::CloseReason::Replayed,
    r::CloseReason::LockedOut,
    r::CloseReason::HandshakeTimeout,
    r::CloseReason::IdleTimeout,
    r::CloseReason::TooManyConnections,
    r::CloseReason::DuplicateConnection,
    r::CloseReason::TooLarge,
    r::CloseReason::Revoked,
    r::CloseReason::TooManyDevices,
    r::CloseReason::BadEntropy,
    r::CloseReason::PeerClosed,
    r::CloseReason::Internal,
];

fn deny_variant(v: &r::DenyReason) -> usize {
    match v {
        r::DenyReason::NotPermitted => 0,
        r::DenyReason::Busy => 1,
        r::DenyReason::NotRunning => 2,
        r::DenyReason::StaleRequest => 3,
        r::DenyReason::NotRemotelyApprovable => 4,
        r::DenyReason::UnknownSkill => 5,
    }
}

const DENIES: [r::DenyReason; 6] = [
    r::DenyReason::NotPermitted,
    r::DenyReason::Busy,
    r::DenyReason::NotRunning,
    r::DenyReason::StaleRequest,
    r::DenyReason::NotRemotelyApprovable,
    r::DenyReason::UnknownSkill,
];

fn code_variant(v: &r::ErrorCode) -> usize {
    match v {
        r::ErrorCode::Malformed => 0,
        r::ErrorCode::UnknownMessage => 1,
        r::ErrorCode::RateLimited => 2,
        r::ErrorCode::InvalidRequest => 3,
        r::ErrorCode::TooLarge => 4,
    }
}

const CODES: [r::ErrorCode; 5] = [
    r::ErrorCode::Malformed,
    r::ErrorCode::UnknownMessage,
    r::ErrorCode::RateLimited,
    r::ErrorCode::InvalidRequest,
    r::ErrorCode::TooLarge,
];

fn audit_variant(v: &r::AuditEvent) -> usize {
    match v {
        r::AuditEvent::PairingExpired => 0,
        r::AuditEvent::Paired { .. } => 1,
        r::AuditEvent::PairingAbandoned => 2,
        r::AuditEvent::Connected => 3,
        r::AuditEvent::Closed { .. } => 4,
        r::AuditEvent::ConnectionRefused { .. } => 5,
        r::AuditEvent::HandshakeFailed { .. } => 6,
        r::AuditEvent::PreAuthSuppressed { .. } => 7,
        r::AuditEvent::LockedOut { .. } => 8,
        r::AuditEvent::Accepted { .. } => 9,
        r::AuditEvent::Denied { .. } => 10,
        r::AuditEvent::Rejected { .. } => 11,
        r::AuditEvent::Revoked => 12,
        r::AuditEvent::PermissionsChanged { .. } => 13,
        r::AuditEvent::DeviceConfirmed => 14,
        r::AuditEvent::PendingExpired => 15,
    }
}

fn audit_events() -> Vec<r::AuditEvent> {
    vec![
        r::AuditEvent::PairingExpired,
        r::AuditEvent::Paired { name: "Mac".into() },
        r::AuditEvent::PairingAbandoned,
        r::AuditEvent::Connected,
        r::AuditEvent::Closed {
            reason: r::CloseReason::IdleTimeout,
        },
        r::AuditEvent::ConnectionRefused {
            reason: r::CloseReason::BadEntropy,
        },
        r::AuditEvent::HandshakeFailed { failures: 4 },
        r::AuditEvent::PreAuthSuppressed {
            count: 9,
            since_ms: 1,
        },
        r::AuditEvent::LockedOut { until_ms: 5_000 },
        r::AuditEvent::Accepted {
            request: "start_task".into(),
            detail: Some("Text Sam".into()),
        },
        r::AuditEvent::Accepted {
            request: "cancel".into(),
            detail: None,
        },
        r::AuditEvent::Denied {
            request: "decision".into(),
            reason: r::DenyReason::NotRemotelyApprovable,
        },
        r::AuditEvent::Rejected {
            request: None,
            code: r::ErrorCode::Malformed,
        },
        r::AuditEvent::Revoked,
        r::AuditEvent::PermissionsChanged {
            permissions: perms(),
        },
        r::AuditEvent::DeviceConfirmed,
        r::AuditEvent::PendingExpired,
    ]
}

fn effect_r_variant(v: &r::HubEffect) -> usize {
    match v {
        r::HubEffect::Send { .. } => 0,
        r::HubEffect::Close { .. } => 1,
        r::HubEffect::StoreChanged => 2,
        r::HubEffect::StartTask { .. } => 3,
        r::HubEffect::RunSkill { .. } => 4,
        r::HubEffect::RunWorkflow { .. } => 5,
        r::HubEffect::CancelTask { .. } => 6,
        r::HubEffect::Decide { .. } => 7,
        r::HubEffect::DevicePaired { .. } => 8,
        r::HubEffect::Audit { .. } => 9,
        r::HubEffect::DeviceConfirmed { .. } => 10,
    }
}

fn hub_effects() -> Vec<r::HubEffect> {
    vec![
        r::HubEffect::Send {
            conn_id: 4,
            bytes: vec![0, 1, 255],
        },
        r::HubEffect::Close {
            conn_id: 4,
            reason: r::CloseReason::Revoked,
        },
        r::HubEffect::StoreChanged,
        r::HubEffect::StartTask {
            task_id: 1,
            device_id: dev(1),
            text: "t".into(),
        },
        r::HubEffect::RunSkill {
            task_id: 2,
            device_id: dev(2),
            id: "wifi-off".into(),
            args: args(),
        },
        r::HubEffect::RunWorkflow {
            task_id: 3,
            device_id: dev(3),
            id: "evening".into(),
        },
        r::HubEffect::CancelTask {
            device_id: dev(4),
            task_id: None,
        },
        r::HubEffect::CancelTask {
            device_id: dev(4),
            task_id: Some(9),
        },
        r::HubEffect::Decide {
            request_id: 5,
            approved: true,
            category: p::RiskCategory::SendMessage,
            device_id: dev(5),
        },
        r::HubEffect::DevicePaired { device: info() },
        r::HubEffect::DeviceConfirmed { device: info() },
        r::HubEffect::Audit {
            entry: r::AuditEntry {
                at_ms: 1,
                conn_id: Some(2),
                device_id: Some(dev(6)),
                event: r::AuditEvent::Connected,
            },
        },
    ]
}

#[test]
fn remote_records_and_enums_round_trip() {
    for e in [r::EntropyError::TooShort, r::EntropyError::NotRandom] {
        rt::<_, f::EntropyProblem>(e);
    }
    rt::<_, f::RemotePermissions>(perms());
    rt::<_, f::RemotePermissions>(r::RemotePermissions::NONE);
    rt::<_, f::RemotePermissions>(r::RemotePermissions::NEW_DEVICE);
    rt_try::<_, f::DeviceInfo>(info());
    for c in catalog() {
        rt::<_, f::CatalogEntry>(c);
    }
    rt::<_, f::PairingOffer>(r::PairingOffer {
        code: zeroize::Zeroizing::new("ABCD-EFGH".into()),
        qr_payload: zeroize::Zeroizing::new("jarvis-remote:2:ABCDEFGH".into()),
        expires_at_ms: 3,
    });
    for c in CLOSES {
        rt::<_, f::CloseReason>(c);
    }
    assert_covers(CLOSES.iter().map(close_variant), 13, "CloseReason");
    for d in DENIES {
        rt::<_, f::DenyReason>(d);
    }
    assert_covers(DENIES.iter().map(deny_variant), 6, "DenyReason");
    for c in CODES {
        rt::<_, f::RemoteErrorCode>(c);
    }
    assert_covers(CODES.iter().map(code_variant), 5, "ErrorCode");
    let audits = audit_events();
    for (i, e) in audits.iter().enumerate() {
        rt::<_, f::AuditEvent>(e.clone());
        rt_try::<_, f::AuditEntry>(r::AuditEntry {
            at_ms: 10,
            conn_id: (i % 2 == 0).then_some(1),
            device_id: (i % 3 == 0).then(|| dev(9)),
            event: e.clone(),
        });
    }
    assert_covers(audits.iter().map(audit_variant), 16, "AuditEvent");
    let fx = hub_effects();
    for e in &fx {
        rt_try::<_, f::HubEffect>(e.clone());
    }
    assert_covers(fx.iter().map(effect_r_variant), 11, "HubEffect");
    // A device id from Swift must be 32 hex characters.
    let bad = f::HubEffect::CancelTask {
        device_id: "not-hex".into(),
        task_id: None,
    };
    assert!(matches!(
        r::HubEffect::try_from(bad).unwrap_err(),
        f::JarvisError::RemoteUnknownDevice { .. }
    ));
}

fn client_msg_variant(v: &r::ClientMessage) -> usize {
    match v {
        r::ClientMessage::Hello => 0,
        r::ClientMessage::StartTask { .. } => 1,
        r::ClientMessage::RunSkill { .. } => 2,
        r::ClientMessage::RunWorkflow { .. } => 3,
        r::ClientMessage::Cancel => 4,
        r::ClientMessage::Decision { .. } => 5,
        r::ClientMessage::Subscribe => 6,
        r::ClientMessage::Unsubscribe => 7,
        r::ClientMessage::ListSkills => 8,
        r::ClientMessage::Ping => 9,
    }
}

fn server_msg_variant(v: &r::ServerMessage) -> usize {
    match v {
        r::ServerMessage::Welcome { .. } => 0,
        r::ServerMessage::TaskStarted => 1,
        r::ServerMessage::Progress { .. } => 2,
        r::ServerMessage::ConfirmPending { .. } => 3,
        r::ServerMessage::ConfirmResolved { .. } => 4,
        r::ServerMessage::Finished { .. } => 5,
        r::ServerMessage::Skills { .. } => 6,
        r::ServerMessage::PermissionsChanged { .. } => 7,
        r::ServerMessage::Denied { .. } => 8,
        r::ServerMessage::Error { .. } => 9,
        r::ServerMessage::Pong => 10,
        r::ServerMessage::AwaitingConfirmation => 11,
        r::ServerMessage::Removed => 12,
    }
}

fn client_event_variant(v: &r::ClientEvent) -> usize {
    match v {
        r::ClientEvent::Send { .. } => 0,
        r::ClientEvent::Paired { .. } => 1,
        r::ClientEvent::Connected => 2,
        r::ClientEvent::Message(_) => 3,
        r::ClientEvent::Undecodable(_) => 4,
    }
}

#[test]
fn protocol_messages_and_client_events_round_trip() {
    let cm = vec![
        r::ClientMessage::Hello,
        r::ClientMessage::StartTask { text: "t".into() },
        r::ClientMessage::RunSkill {
            id: "wifi-off".into(),
            args: args(),
        },
        r::ClientMessage::RunWorkflow { id: "w".into() },
        r::ClientMessage::Cancel,
        r::ClientMessage::Decision {
            request_id: 3,
            approved: false,
        },
        r::ClientMessage::Subscribe,
        r::ClientMessage::Unsubscribe,
        r::ClientMessage::ListSkills,
        r::ClientMessage::Ping,
    ];
    for m in &cm {
        rt::<_, f::ClientMessage>(m.clone());
    }
    assert_covers(cm.iter().map(client_msg_variant), 10, "ClientMessage");

    let mut sm = vec![
        r::ServerMessage::Welcome {
            protocol_version: 2,
            permissions: perms(),
            phone_name: Some("Phone".into()),
            busy: Some(true),
        },
        r::ServerMessage::Welcome {
            protocol_version: 2,
            permissions: r::RemotePermissions::NONE,
            phone_name: None,
            busy: None,
        },
        r::ServerMessage::TaskStarted,
        r::ServerMessage::ConfirmPending {
            request_id: 1,
            category: p::RiskCategory::Call,
            summary: "Call Sam".into(),
            can_approve: true,
        },
        r::ServerMessage::ConfirmResolved { request_id: 1 },
        r::ServerMessage::Skills {
            skills: catalog(),
            workflows: vec![],
        },
        r::ServerMessage::PermissionsChanged {
            permissions: perms(),
        },
        r::ServerMessage::Pong,
        r::ServerMessage::AwaitingConfirmation,
        r::ServerMessage::Removed,
    ];
    for phase in PHASES {
        sm.push(r::ServerMessage::Progress {
            step: 2,
            phase,
            app: Some("com.apple.Preferences".into()),
            last_action: Some("tap".into()),
            screen_text: Some("Wi-Fi".into()),
            waiting_for_unlock: phase == p::Phase::WaitingForUnlock,
        });
    }
    for status in OUTCOMES {
        sm.push(r::ServerMessage::Finished {
            status,
            summary: "s".into(),
        });
    }
    sm.extend(DENIES.map(|reason| r::ServerMessage::Denied { reason }));
    sm.extend(CODES.map(|code| r::ServerMessage::Error { code }));
    for m in &sm {
        rt::<_, f::ServerMessage>(m.clone());
    }
    assert_covers(sm.iter().map(server_msg_variant), 13, "ServerMessage");

    let credentials = r::ClientCredentials {
        device_id: dev(7),
        device_key: r::Secret32::from_bytes([0x5A; 32]),
    };
    let ce = vec![
        r::ClientEvent::Send { bytes: vec![1, 2] },
        r::ClientEvent::Paired {
            credentials: credentials.clone(),
        },
        r::ClientEvent::Connected,
        r::ClientEvent::Message(r::ServerMessage::Pong),
    ];
    for e in &ce {
        rt_try::<_, f::ClientEvent>(e.clone());
    }
    // `undecodable` carries the reason as text: ffi → Rust → ffi is the identity.
    let und = f::ClientEvent::Undecodable {
        reason: "malformed message: x".into(),
    };
    let back: r::ClientEvent = und.clone().try_into().unwrap();
    assert_eq!(client_event_variant(&back), 4);
    let again: f::ClientEvent =
        r::ClientEvent::Undecodable(r::MessageError::UnknownType("telepathy".into())).into();
    assert!(
        matches!(again, f::ClientEvent::Undecodable { reason } if reason.contains("telepathy"))
    );
    let mut seen: Vec<usize> = ce.iter().map(client_event_variant).collect();
    seen.push(client_event_variant(&back));
    assert_covers(seen, 5, "ClientEvent");

    // The secret records redact themselves in Debug.
    let key_hex = "5a".repeat(32);
    let paired: f::ClientEvent = r::ClientEvent::Paired { credentials }.into();
    let f::ClientEvent::Paired {
        credentials_json, ..
    } = &paired
    else {
        panic!()
    };
    assert!(credentials_json.contains(&key_hex));
    assert!(!format!("{paired:?}").contains(&key_hex));
    let offer = f::PairingOffer {
        code: "QRST-UVWX".into(),
        qr_payload: "jarvis-remote:2:QRSTUVWX".into(),
        expires_at_ms: 1,
    };
    let d = format!("{offer:?}");
    assert!(!d.contains("QRST") && d.contains("redacted"), "{d}");
}

fn local_variant(v: &r::LocalEvent) -> usize {
    match v {
        r::LocalEvent::SessionStarted { .. } => 0,
        r::LocalEvent::Progress { .. } => 1,
        r::LocalEvent::ConfirmRequested { .. } => 2,
        r::LocalEvent::ConfirmResolved { .. } => 3,
        r::LocalEvent::SessionFinished { .. } => 4,
        r::LocalEvent::DeviceRevoked { .. } => 5,
        r::LocalEvent::PermissionsChanged { .. } => 6,
        r::LocalEvent::CatalogChanged { .. } => 7,
    }
}

#[test]
fn every_local_event_converts_and_progress_text_is_built_from_the_redacted_screen() {
    let screen = p::fixtures::settings_screen();
    let progress = p::Progress::default();
    let events = vec![
        f::LocalEvent::SessionStarted { task_id: Some(1) },
        f::LocalEvent::SessionStarted { task_id: None },
        f::LocalEvent::Progress {
            progress: progress.clone().into(),
            screen: Some(screen.clone().into()),
        },
        f::LocalEvent::Progress {
            progress: progress.clone().into(),
            screen: None,
        },
        f::LocalEvent::ConfirmRequested {
            request: f::ConfirmRequest {
                category: f::RiskCategory::Delete,
                summary: "Delete".into(),
                app: "a".into(),
                phone_only: false,
                rows: Vec::new(),
                stage: f::ConfirmStage::Whole,
                verb: f::ConfirmVerb::Other,
                target: None,
                flow_only: false,
            },
        },
        f::LocalEvent::ConfirmResolved {
            request_id: 2,
            approved: true,
        },
        f::LocalEvent::SessionFinished {
            task_id: Some(1),
            outcome: f::Outcome {
                status: f::OutcomeStatus::Cancelled,
                summary: "c".into(),
                steps: 1,
                duration_ms: 2,
            },
        },
        f::LocalEvent::DeviceRevoked {
            device_id: dev(1).to_hex(),
        },
        f::LocalEvent::PermissionsChanged {
            device_id: dev(2).to_hex(),
            permissions: perms().into(),
        },
        f::LocalEvent::CatalogChanged {
            skills: catalog().into_iter().map(Into::into).collect(),
            workflows: vec![],
        },
    ];
    let mut seen = Vec::new();
    for e in events {
        let back: r::LocalEvent = e.try_into().unwrap();
        seen.push(local_variant(&back));
        match back {
            r::LocalEvent::Progress {
                progress: got,
                screen_text,
            } => {
                assert_eq!(got, progress);
                if let Some(t) = screen_text {
                    assert!(!t.is_empty() && !t.contains('\n'), "{t:?}");
                }
            }
            r::LocalEvent::DeviceRevoked { device_id } => assert_eq!(device_id, dev(1)),
            r::LocalEvent::PermissionsChanged {
                device_id,
                permissions,
            } => assert_eq!((device_id, permissions), (dev(2), perms())),
            _ => {}
        }
    }
    assert_covers(seen, 8, "LocalEvent");
}
