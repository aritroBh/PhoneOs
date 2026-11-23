//! First-use approval, StepByStep and per-app trust as the session applies them, for model
//! actions, replayed flows, tool calls and `open_app`; plus tool URL validation.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use common::*;
use jarvis_memory::MemoryStore;
use jarvis_proto::{
    AppRule, ConfirmRequest, Key, OutcomeStatus, ParamKind, PolicyMode, RiskCategory,
    RunnerCommand, Screen, Tier, ToolParam, ToolSpec, TrustLevel,
};

const SHOP: &str = "com.example.shop";
const FIRST_USE_SHOP: &str = "Let Jarvis act in Shop (com.example.shop)?";

/// The shop after saving to the wishlist: harmless controls only.
fn wishlist_screen() -> Screen {
    use jarvis_proto::fixtures::{node, rect, screen, text};
    use jarvis_proto::{Role, UiNode};
    let size = jarvis_proto::fixtures::IPHONE_SIZE.to_rect();
    let root = UiNode::new(Role::Application, size)
        .with_label("Shop")
        .with_children(vec![UiNode::new(Role::Window, size).with_children(vec![
            text("Saved to your wishlist", rect(16.0, 300.0, 361.0, 30.0)),
            node(
                Role::Button,
                "View wishlist",
                rect(16.0, 600.0, 361.0, 50.0),
            ),
        ])]);
    screen(SHOP, "Shop", root, false)
}

fn shop_phone() -> Phone {
    Phone::new(product_screen(), |cmd, s: &Screen| {
        if taps(cmd, s, "Buy now") {
            Transition::Go(bought_screen())
        } else if taps(cmd, s, "Add to wishlist") {
            Transition::Go(wishlist_screen())
        } else if taps(cmd, s, "View wishlist") {
            Transition::Go(product_screen())
        } else if matches!(cmd, RunnerCommand::PressKey { key: Key::Home }) {
            Transition::Go(home_screen())
        } else {
            Transition::Stay
        }
    })
}

fn tap_label(screen: &Screen, label: &str) -> String {
    format!(
        r#"{{"action":"tap","element":{}}}"#,
        index_of(screen, label)
    )
}

fn done() -> &'static str {
    r#"{"action":"done","summary":"ok"}"#
}

fn categories(confirms: &[ConfirmRequest]) -> Vec<RiskCategory> {
    confirms.iter().map(|c| c.category).collect()
}

#[test]
fn first_action_in_a_new_app_asks_once_and_is_remembered() {
    let wish = tap_label(&product_screen(), "Add to wishlist");
    let cont = tap_label(&wishlist_screen(), "View wishlist");
    let r = run(
        first_use_config(),
        "Save the headphones",
        MemoryStore::new(),
        shop_phone(),
        Script::model(&[&wish, &cont, done()]).decisions(&[true]),
    );
    assert_eq!(
        r.outcome.status,
        OutcomeStatus::Success,
        "{}",
        r.outcome.summary
    );
    assert_eq!(r.confirms.len(), 1, "asked once per app per session");
    let c = &r.confirms[0];
    assert_eq!(c.category, RiskCategory::FirstUse);
    assert_eq!(c.summary, FIRST_USE_SHOP);
    assert_eq!(c.app, SHOP);
    // The confirmation comes before the first act, and both taps ran.
    let names: Vec<&str> = r.effects.iter().map(|e| e.name()).collect();
    assert!(
        names.iter().position(|n| *n == "confirm").unwrap()
            < names.iter().position(|n| *n == "act").unwrap()
    );
    assert_eq!(r.acts.len(), 2);
    // The approval is written into the returned memory...
    assert!(r.memory.is_app_approved(SHOP));
    // ...so the next session does not ask again.
    let again = run_with(&r.memory, &[&wish, done()], &[]);
    assert_eq!(again.outcome.status, OutcomeStatus::Success);
    assert!(again.confirms.is_empty(), "{:?}", again.confirms);
}

fn run_with(memory: &MemoryStore, model: &[&str], decisions: &[bool]) -> Run {
    run(
        first_use_config(),
        "Save the headphones",
        memory.clone(),
        shop_phone(),
        Script::model(model).decisions(decisions),
    )
}

#[test]
fn a_first_pay_tap_in_a_new_app_asks_twice() {
    let buy = tap_label(&product_screen(), "Buy now");
    let r = run(
        first_use_config(),
        "Buy the headphones",
        MemoryStore::new(),
        shop_phone(),
        Script::model(&[&buy, done()]).decisions(&[true, true]),
    );
    assert_eq!(
        r.outcome.status,
        OutcomeStatus::Success,
        "{}",
        r.outcome.summary
    );
    assert_eq!(r.confirms.len(), 2);
    assert_eq!(r.confirms[0].category, RiskCategory::FirstUse);
    assert!(
        matches!(
            r.confirms[1].category,
            RiskCategory::Purchase | RiskCategory::Payment
        ),
        "{:?}",
        r.confirms[1]
    );
    assert!(r.confirms[1].summary.contains("Buy now"));
    assert_eq!(r.acts.len(), 1);

    // Approving the app but declining the purchase: the app stays approved, nothing is
    // bought, and the decline is remembered for the category.
    let r = run_with(&MemoryStore::new(), &[&buy, done()], &[true, false]);
    assert_eq!(categories(&r.confirms)[0], RiskCategory::FirstUse);
    assert_eq!(r.confirms.len(), 2);
    assert!(r.acts.is_empty());
    assert!(r.memory.is_app_approved(SHOP));
    assert!(r.requests[1]
        .user
        .contains("-> not done: the owner declined"));
}

#[test]
fn declining_first_use_blocks_the_app_for_the_session() {
    let buy = tap_label(&product_screen(), "Buy now");
    let wish = tap_label(&product_screen(), "Add to wishlist");
    // Declined, then the model tries another control in the same app: Blocked, untouched.
    let r = run_with(&MemoryStore::new(), &[&wish, &buy, done()], &[false]);
    assert_eq!(
        r.outcome.status,
        OutcomeStatus::Blocked,
        "{}",
        r.outcome.summary
    );
    assert_eq!(categories(&r.confirms), vec![RiskCategory::FirstUse]);
    assert!(r.acts.is_empty());
    assert!(
        r.outcome.summary.contains("did not let Jarvis act in Shop"),
        "{}",
        r.outcome.summary
    );
    assert!(!r.memory.is_app_approved(SHOP));
    assert!(r.requests[1]
        .user
        .contains("not done: the owner does not want Jarvis to act in this app"));
    // Reading it closer (look) is blocked too.
    let r = run_with(
        &MemoryStore::new(),
        &[&wish, r#"{"action":"look"}"#],
        &[false],
    );
    assert_eq!(r.outcome.status, OutcomeStatus::Blocked);
    assert!(r.observes.iter().all(|s| !s));
}

#[test]
fn a_declined_app_cannot_be_reached_by_open_app_or_a_tool() {
    let wish = tap_label(&product_screen(), "Add to wishlist");
    let mut c = first_use_config();
    c.tools = vec![ToolSpec {
        name: "shop_wishlist".into(),
        description: "Order in Shop".into(),
        params: vec![],
        tier: Tier::Shortcut,
        risk: RiskCategory::None,
        app: Some(SHOP.into()),
    }];
    // press_home leaves the app (allowed), then open_app goes back: Blocked.
    let r = run(
        c.clone(),
        "Save the headphones",
        MemoryStore::new(),
        shop_phone(),
        Script::model(&[
            &wish,
            r#"{"action":"press_home"}"#,
            r#"{"action":"open_app","app":"Shop"}"#,
            done(),
        ])
        .decisions(&[false]),
    );
    assert_eq!(
        r.outcome.status,
        OutcomeStatus::Blocked,
        "{}",
        r.outcome.summary
    );
    assert_eq!(r.acts, vec![RunnerCommand::PressKey { key: Key::Home }]);
    assert_eq!(categories(&r.confirms), vec![RiskCategory::FirstUse]);
    // A tool acting on the declined app: Blocked, never called.
    let r = run(
        c,
        "Save the headphones",
        MemoryStore::new(),
        shop_phone(),
        Script::model(&[
            &wish,
            r#"{"action":"call_tool","name":"shop_wishlist","args":{}}"#,
            done(),
        ])
        .decisions(&[false]),
    );
    assert_eq!(
        r.outcome.status,
        OutcomeStatus::Blocked,
        "{}",
        r.outcome.summary
    );
    assert!(r.tool_calls.is_empty());
}

#[test]
fn tools_acting_in_an_app_need_its_first_use_approval() {
    let mut c = first_use_config();
    c.tools = vec![ToolSpec {
        name: "shop_wishlist".into(),
        description: "Order in Shop".into(),
        params: vec![],
        tier: Tier::Shortcut,
        risk: RiskCategory::None,
        app: Some(SHOP.into()),
    }];
    let r = run(
        c,
        "Order",
        MemoryStore::new(),
        Phone::still(home_screen()),
        Script::model(&[
            r#"{"action":"call_tool","name":"shop_wishlist","args":{}}"#,
            done(),
        ])
        .decisions(&[true]),
    );
    assert_eq!(
        r.outcome.status,
        OutcomeStatus::Success,
        "{}",
        r.outcome.summary
    );
    assert_eq!(categories(&r.confirms), vec![RiskCategory::FirstUse]);
    // The name comes from known_apps when the app is not in front.
    assert_eq!(r.confirms[0].summary, FIRST_USE_SHOP);
    assert_eq!(r.tool_calls.len(), 1);
}

#[test]
fn exempt_apps_never_ask() {
    let shop = tap_label(&home_screen(), "Shop");
    // SpringBoard.
    let r = run(
        first_use_config(),
        "Open Shop",
        MemoryStore::new(),
        Phone::new(home_screen(), |_, _| Transition::Go(product_screen())),
        Script::model(&[&shop, done()]),
    );
    assert_eq!(
        r.outcome.status,
        OutcomeStatus::Success,
        "{}",
        r.outcome.summary
    );
    assert!(r.confirms.is_empty(), "{:?}", r.confirms);
    // The agent's own app.
    let mut c = first_use_config();
    c.agent_bundle_id = Some(SHOP.into());
    let wish = tap_label(&product_screen(), "Add to wishlist");
    let r = run(
        c,
        "Save",
        MemoryStore::new(),
        shop_phone(),
        Script::model(&[&wish, done()]),
    );
    assert!(r.confirms.is_empty(), "{:?}", r.confirms);
    // A Trusted app rule.
    let mut c = first_use_config();
    c.policy.app_rules = vec![AppRule::new(SHOP, TrustLevel::Trusted)];
    let r = run(
        c,
        "Save",
        MemoryStore::new(),
        shop_phone(),
        Script::model(&[&wish, done()]),
    );
    assert!(r.confirms.is_empty(), "{:?}", r.confirms);
    assert_eq!(r.acts.len(), 1);
    assert!(
        !r.memory.is_app_approved(SHOP),
        "trusted is not an approval"
    );
    // Turned off.
    let mut c = first_use_config();
    c.policy.first_use_approval = false;
    let r = run(
        c,
        "Save",
        MemoryStore::new(),
        shop_phone(),
        Script::model(&[&wish, done()]),
    );
    assert!(r.confirms.is_empty());
}

#[test]
fn denying_first_use_restricts_the_agent_to_approved_apps() {
    let wish = tap_label(&product_screen(), "Add to wishlist");
    let mut c = first_use_config();
    c.policy.deny_categories = vec![RiskCategory::FirstUse];
    let r = run(
        c.clone(),
        "Save",
        MemoryStore::new(),
        shop_phone(),
        Script::model(&[&wish, &wish, done()]),
    );
    assert_eq!(
        r.outcome.status,
        OutcomeStatus::Blocked,
        "{}",
        r.outcome.summary
    );
    assert!(r.confirms.is_empty());
    assert!(r.acts.is_empty());
    assert!(r.outcome.summary.contains("has not been approved"));
    let mut approved = MemoryStore::new();
    approved.approve_app(SHOP);
    let r = run(
        c,
        "Save",
        approved,
        shop_phone(),
        Script::model(&[&wish, done()]),
    );
    assert_eq!(r.outcome.status, OutcomeStatus::Success);
}

#[test]
fn replayed_flows_obey_first_use_approval() {
    let wish = tap_label(&product_screen(), "Add to wishlist");
    // Learn with approval.
    let first = run_with(&MemoryStore::new(), &[&wish, done()], &[true]);
    assert_eq!(first.memory.flows.len(), 1);
    // A store that has the flow but not the approval (e.g. approvals reset).
    let mut memory = first.memory.clone();
    memory.approved_apps.clear();
    let second = run_with(
        &memory,
        &[r#"{"action":"fail","reason":"not allowed"}"#],
        &[false],
    );
    assert_eq!(categories(&second.confirms), vec![RiskCategory::FirstUse]);
    assert!(second.acts.is_empty());
    // The decline hands over to the model, which is told why.
    assert_eq!(second.infers(), 1);
    assert!(second.requests[0]
        .user
        .contains("(saved) -> not done: the owner does not want Jarvis to act in this app"));
    assert_eq!(second.outcome.status, OutcomeStatus::Failed);
    // A model that insists on the saved step is blocked.
    let insists = run_with(&memory, &[&wish], &[false]);
    assert_eq!(insists.outcome.status, OutcomeStatus::Blocked);
    assert!(insists.acts.is_empty());
    // Approved: the replay continues with zero model calls.
    let third = run_with(&memory, &[], &[true]);
    assert_eq!(
        third.outcome.status,
        OutcomeStatus::Success,
        "{}",
        third.outcome.summary
    );
    assert_eq!(third.infers(), 0);
    assert_eq!(categories(&third.confirms), vec![RiskCategory::FirstUse]);
}

#[test]
fn step_by_step_confirms_each_step_and_a_decline_is_not_sticky() {
    let wish = tap_label(&product_screen(), "Add to wishlist");
    let cont = tap_label(&wishlist_screen(), "View wishlist");
    let mut c = config();
    c.policy.mode = PolicyMode::StepByStep;
    let r = run(
        c.clone(),
        "Save",
        MemoryStore::new(),
        shop_phone(),
        Script::model(&[
            r#"{"action":"scroll","direction":"down"}"#,
            &wish,
            &cont,
            &cont,
            done(),
        ])
        .decisions(&[true, false, true]),
    );
    assert_eq!(
        r.outcome.status,
        OutcomeStatus::Success,
        "{}",
        r.outcome.summary
    );
    // The plain scroll ran without asking; each tap asked (the declined one did not run).
    assert_eq!(
        categories(&r.confirms),
        vec![RiskCategory::None, RiskCategory::None, RiskCategory::None]
    );
    assert!(r.confirms[0].summary.contains("Add to wishlist"));
    assert_eq!(r.acts.len(), 3, "{:?}", r.acts);
    // An AppRule can make one app StepByStep while the rest stays Guarded.
    let mut c = config();
    c.policy.app_rules = vec![AppRule::new(SHOP, TrustLevel::StepByStep)];
    let r = run(
        c,
        "Save",
        MemoryStore::new(),
        shop_phone(),
        Script::model(&[&wish, done()]).decisions(&[true]),
    );
    assert_eq!(categories(&r.confirms), vec![RiskCategory::None]);
}

#[test]
fn tool_url_arguments_are_scheme_validated_before_the_call() {
    let mut c = config();
    c.tools = vec![ToolSpec {
        name: "open_link".into(),
        description: "Open a link".into(),
        params: vec![ToolParam {
            name: "url".into(),
            description: String::new(),
            kind: ParamKind::String,
            required: true,
        }],
        tier: Tier::Native,
        risk: RiskCategory::None,
        app: None,
    }];
    let r = run(
        c,
        // Fourteenth review (Rule A): a link in a tool argument passes the owner-gave-the-link
        // gate like `open_url`; the owner gives it here (without it the call asks).
        "Open the link https://example.com",
        MemoryStore::new(),
        Phone::still(home_screen()),
        Script::model(&[
            r#"{"action":"call_tool","name":"open_link","args":{"url":"javascript:alert(1)"}}"#,
            r#"{"action":"call_tool","name":"open_link","args":{"url":"https://example.com"}}"#,
            done(),
        ]),
    );
    assert_eq!(
        r.outcome.status,
        OutcomeStatus::Success,
        "{}",
        r.outcome.summary
    );
    assert_eq!(r.tool_calls.len(), 1);
    assert_eq!(r.tool_calls[0].args["url"], "https://example.com");
    assert!(
        r.requests[1].user.contains("not allowed: tool argument"),
        "{}",
        r.requests[1].user
    );
}

#[test]
fn a_declined_app_matches_regardless_of_bundle_id_case() {
    let wish = tap_label(&product_screen(), "Add to wishlist");
    let mut c = first_use_config();
    c.known_apps
        .push(jarvis_proto::AppInfo::new("COM.EXAMPLE.SHOP", "Shop Upper"));
    let r = run(
        c,
        "Save",
        MemoryStore::new(),
        shop_phone(),
        Script::model(&[
            &wish,
            r#"{"action":"press_home"}"#,
            r#"{"action":"open_app","app":"Shop Upper"}"#,
        ])
        .decisions(&[false]),
    );
    assert_eq!(
        r.outcome.status,
        OutcomeStatus::Blocked,
        "{}",
        r.outcome.summary
    );
    assert!(!r
        .acts
        .iter()
        .any(|a| matches!(a, RunnerCommand::LaunchApp { .. })));
}
