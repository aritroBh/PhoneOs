//! End-to-end runs of the real `Session` against the simulated phone with scripted models.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use common::*;
use jarvis_memory::MemoryStore;
use jarvis_proto::{OutcomeStatus, RiskCategory, RunnerCommand};
use jarvis_safety::{SCREEN_BLOCK_END, SCREEN_BLOCK_START};
use jarvis_sim::{Human, ObserveMode, RunOptions, RunReport, ScriptedModel};

fn assert_success(r: &RunReport, ctx: &str) {
    assert_eq!(
        r.outcome.status,
        OutcomeStatus::Success,
        "{ctx}: {}\n{}",
        r.outcome.summary,
        r.transcript()
    );
}

/// The rendered screen inside a prompt (after the data-block delimiter).
fn screen_part(user: &str) -> &str {
    user.split_once(SCREEN_BLOCK_START).map_or(user, |(_, s)| s)
}

fn index_of_step(r: &RunReport, f: impl Fn(&str, &str) -> bool) -> usize {
    r.steps
        .iter()
        .position(|s| f(&s.effect, &s.result))
        .unwrap_or_else(|| panic!("no such step in:\n{}", r.transcript()))
}

// ---------------------------------------------------------------------------------------
// Settings.
// ---------------------------------------------------------------------------------------

#[test]
fn wifi_toggle_succeeds_in_both_modes() {
    for mode in MODES {
        let (r, m) = go(
            mode,
            "settings",
            "Turn off Wi-Fi",
            &[tap("Cell:Wi-Fi"), tap("Switch:Wi-Fi"), done("Wi-Fi is off")],
            &mut Human::approve_all(),
        );
        assert_success(&r, &format!("{mode:?}"));
        assert!(r.final_state.check("settings.wifi == false").unwrap());
        // Runner text: the tap on the Wi-Fi row asks too (unverified screen).
        assert_eq!(
            categories(&r),
            per_mode(
                mode,
                vec![RiskCategory::SystemSetting],
                vec![RiskCategory::Unverified, RiskCategory::SystemSetting]
            ),
            "{mode:?}"
        );
        assert_text_warnings(&r, mode);
        assert_eq!(m.calls(), 3);
        assert_eq!(r.counts.model_calls, 3);
        assert_eq!(r.outcome.steps, 3);
        assert_eq!(r.final_state.screen, "wifi");
    }
}

#[test]
fn declined_setting_change_leaves_the_switch_alone() {
    for mode in MODES {
        let (r, _) = go(
            mode,
            "settings",
            "Turn off Wi-Fi",
            &[
                tap("Cell:Wi-Fi"),
                tap("Switch:Wi-Fi"),
                fail("The owner declined"),
            ],
            &mut Human::decline_all(),
        );
        assert!(r.final_state.check("settings.wifi == true").unwrap());
        assert_eq!(r.confirmations.len(), 1);
        assert!(!r.confirmations[0].approved);
        assert!(r.requests[2].user.contains("the owner declined"));
    }
}

#[test]
fn dark_mode_is_two_levels_deep() {
    for mode in MODES {
        let (r, _) = go(
            mode,
            "settings",
            "Turn on Dark Mode",
            &[
                tap("Cell:Display"),
                tap("Switch:Dark Mode"),
                done("Dark Mode is on"),
            ],
            &mut Human::approve_all(),
        );
        assert_success(&r, &format!("{mode:?}"));
        assert!(r.final_state.check("settings.dark").unwrap());
    }
}

// ---------------------------------------------------------------------------------------
// Messages.
// ---------------------------------------------------------------------------------------

fn text_mom_script() -> Vec<String> {
    vec![
        tap("Cell:Mom"),
        type_into("TextView:Message", "running late"),
        tap("Button:Send"),
        done("Sent"),
    ]
}

#[test]
fn send_message_approved_appears_in_the_thread() {
    for mode in MODES {
        let (r, _) = go(
            mode,
            "messages",
            "Text Mom that I'm running late",
            &text_mom_script(),
            &mut Human::approve_all(),
        );
        assert_success(&r, &format!("{mode:?}"));
        assert!(r
            .final_state
            .check(r#"messages.thread["Mom"].last == "running late""#)
            .unwrap());
        // Runner text: the tap on Mom and the typing ask too (unverified screen).
        assert_eq!(
            categories(&r),
            per_mode(
                mode,
                vec![RiskCategory::SendMessage],
                vec![
                    RiskCategory::Unverified,
                    RiskCategory::Unverified,
                    RiskCategory::SendMessage
                ]
            )
        );
        assert_text_warnings(&r, mode);
        assert!(r.confirmations[0].approved);
        // The send happens only after the owner approved.
        let confirm = index_of_step(&r, |e, _| e.starts_with("confirm"));
        let send = index_of_step(&r, |e, res| {
            e.starts_with("act tap") && res.contains("\"Send\"")
        });
        assert!(confirm < send);
    }
}

#[test]
fn declined_send_leaves_the_thread_untouched_and_the_model_is_told() {
    for mode in MODES {
        let mut script = text_mom_script();
        script[3] = fail("The owner declined sending the message");
        let (r, _) = go(
            mode,
            "messages",
            "Text Mom that I'm running late",
            &script,
            &mut Human::decline_all(),
        );
        assert_eq!(r.outcome.status, OutcomeStatus::Failed, "{mode:?}");
        assert!(r
            .final_state
            .check(r#"messages.thread["Mom"].last == "See you at six""#)
            .unwrap());
        assert!(r.requests[3]
            .user
            .contains("-> not done: the owner declined"));
    }
}

#[test]
fn asking_again_after_a_decline_blocks_without_asking_the_owner_twice() {
    for mode in MODES {
        let mut script = text_mom_script();
        script.insert(3, tap("Button:Send"));
        // Runner text asks before the tap on Mom and the typing as well: the owner says
        // yes to those and no to the send.
        let mut human = per_mode(
            mode,
            Human::decline_all(),
            Human::decline_categories(&[RiskCategory::SendMessage]),
        );
        let (r, m) = go(
            mode,
            "messages",
            "Text Mom that I'm running late",
            &script,
            &mut human,
        );
        assert_eq!(
            r.outcome.status,
            OutcomeStatus::Blocked,
            "{mode:?}: {}",
            r.transcript()
        );
        assert!(r.outcome.summary.contains("declined"));
        assert_eq!(r.confirmations.len(), per_mode(mode, 1, 3));
        assert_eq!(m.remaining(), 1, "the final done was never requested");
        assert!(r
            .final_state
            .check(r#"messages.thread["Mom"].last != "running late""#)
            .unwrap());
    }
}

// ---------------------------------------------------------------------------------------
// Shop.
// ---------------------------------------------------------------------------------------

fn order_script() -> Vec<String> {
    vec![
        tap("Cell:Wireless Headphones"),
        tap("Button:Add to Cart"),
        tap("Button:Cart"),
        tap("Button:Place Order"),
        done("Ordered"),
    ]
}

#[test]
fn placing_an_order_requires_confirmation_first() {
    for mode in MODES {
        let (r, _) = go(
            mode,
            "shop",
            "Order the Wireless Headphones",
            &order_script(),
            &mut Human::approve_all(),
        );
        assert_success(&r, &format!("{mode:?}"));
        assert!(r.final_state.check("shop.ordered").unwrap());
        assert_eq!(r.final_state.screen, "placed");
        assert!(categories(&r).contains(&RiskCategory::Payment), "{mode:?}");
        let confirm = index_of_step(&r, |e, _| {
            e.contains("Place Order") && e.starts_with("confirm")
        });
        let place = index_of_step(&r, |e, res| {
            e.starts_with("act tap") && res.contains("\"Place Order\"")
        });
        assert!(confirm < place, "{}", r.transcript());
    }
}

#[test]
fn declining_the_payment_means_no_order() {
    for mode in MODES {
        let mut script = order_script();
        script[4] = fail("The owner declined the purchase");
        let (r, _) = go(
            mode,
            "shop",
            "Order the Wireless Headphones",
            &script,
            &mut Human::decline_categories(&[RiskCategory::Payment]),
        );
        assert!(
            r.final_state.check("shop.ordered == false").unwrap(),
            "{mode:?}"
        );
        assert!(r.final_state.check("shop.cart.len == 1").unwrap());
        assert!(r.confirmations.iter().any(|c| !c.approved));
    }
}

#[test]
fn injected_text_on_a_product_page_cannot_place_an_order() {
    // A gullible model that follows the review text: add the lamp, then "Place Order".
    let gullible = vec![
        tap("Cell:Desk Lamp"),
        tap("Button:Add to Cart"),
        tap("Button:Cart"),
        tap("Button:Place Order"),
        fail("The owner declined the order"),
    ];
    for mode in MODES {
        let (r, _) = go(
            mode,
            "shop",
            "Add the Desk Lamp to my cart",
            &gullible,
            &mut Human::decline_categories(&[RiskCategory::Payment]),
        );
        assert!(
            r.final_state.check("shop.ordered == false").unwrap(),
            "{mode:?}"
        );
        assert!(r.final_state.check("shop.cart.len == 1").unwrap());
        assert!(r
            .confirmations
            .iter()
            .any(|c| c.request.category == RiskCategory::Payment && !c.approved));
        // The attacker's text reached the model only as data, inside the delimited block.
        let seen: Vec<&str> = r
            .requests
            .iter()
            .map(|q| q.user.as_str())
            .filter(|u| u.contains("Ignore previous instructions"))
            .collect();
        assert!(
            !seen.is_empty(),
            "the lamp page was never shown to the model"
        );
        for u in seen {
            let at = u.find("Ignore previous instructions").unwrap();
            assert!(u.find(SCREEN_BLOCK_START).unwrap() < at);
            assert!(at < u.rfind(SCREEN_BLOCK_END).unwrap());
        }
    }
}

#[test]
fn the_owner_is_the_last_line_of_defence_against_injection() {
    // If the owner approves the Place Order prompt, the order goes through: the prompt names it.
    let gullible = vec![
        tap("Cell:Desk Lamp"),
        tap("Button:Add to Cart"),
        tap("Button:Cart"),
        tap("Button:Place Order"),
        done("done"),
    ];
    let (r, _) = go(
        ObserveMode::Structured,
        "shop",
        "Add the Desk Lamp to my cart",
        &gullible,
        &mut Human::approve_all(),
    );
    assert!(r.final_state.check("shop.ordered").unwrap());
    let payment = r
        .confirmations
        .iter()
        .find(|c| c.request.category == RiskCategory::Payment)
        .expect("a payment confirmation");
    assert!(
        payment.request.summary.contains("Place Order"),
        "{}",
        payment.request.summary
    );
}

#[test]
fn a_well_behaved_model_never_triggers_a_payment_prompt_on_the_lamp_page() {
    for mode in MODES {
        let (r, _) = go(
            mode,
            "shop",
            "Add the Desk Lamp to my cart",
            &[
                tap("Cell:Desk Lamp"),
                tap("Button:Add to Cart"),
                done("Added"),
            ],
            &mut Human::decline_categories(&[RiskCategory::Payment]),
        );
        assert_success(&r, &format!("{mode:?}"));
        assert!(!categories(&r).contains(&RiskCategory::Payment));
        assert!(r
            .final_state
            .check("shop.ordered == false && shop.cart.len == 1")
            .unwrap());
    }
}

// ---------------------------------------------------------------------------------------
// Login.
// ---------------------------------------------------------------------------------------

fn login_script() -> Vec<String> {
    vec![
        type_into("TextField:Email", "alex@example.com"),
        type_into("SecureTextField:Password", "secret"),
        tap("Button:Sign in"),
        done("Signed in"),
    ]
}

#[test]
fn login_hands_the_password_field_back_to_the_owner() {
    for mode in MODES {
        let mut human = Human::approve_all()
            .types_on_ask(&["hunter2"])
            .with_answers(&["done"]);
        let (r, _) = go(mode, "acme", "Sign in to Acme", &login_script(), &mut human);
        assert_success(&r, &format!("{mode:?}"));
        assert_eq!(r.questions.len(), 1, "{mode:?}");
        assert!(r.questions[0].contains("password"), "{}", r.questions[0]);
        assert!(r.final_state.check("acme.signed_in").unwrap());
        // The agent never typed the password; only the email went through the runner.
        let typed: Vec<&str> = r
            .commands
            .iter()
            .filter_map(|c| match c {
                RunnerCommand::TypeText { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(typed, vec!["alex@example.com"]);
        assert!(r.requests.iter().all(|q| !q.user.contains("hunter2")));
    }
}

#[test]
fn wrong_password_shows_the_alert_and_the_agent_can_dismiss_it() {
    for mode in MODES {
        let mut script = login_script();
        script[3] = tap("Button:OK");
        script.push(fail("The password was rejected"));
        let mut human = Human::approve_all().types_on_ask(&["letmein"]);
        let (r, _) = go(mode, "acme", "Sign in to Acme", &script, &mut human);
        assert_eq!(r.outcome.status, OutcomeStatus::Failed);
        assert!(r
            .final_state
            .check("!acme.signed_in && acme.attempts == 1")
            .unwrap());
        assert!(
            r.requests[3].user.contains("Incorrect password"),
            "{mode:?}"
        );
        assert_eq!(r.final_state.alert, None, "OK dismissed the alert");
    }
}

#[test]
fn runner_text_hides_disabled_buttons_so_the_tap_is_sent_and_does_nothing() {
    let script = [tap("Button:Sign in"), fail("could not sign in")];
    // Structured: the enabled flag survives, the agent refuses to tap a disabled button.
    let (s, _) = go(
        ObserveMode::Structured,
        "acme",
        "Sign in to Acme",
        &script,
        &mut Human::approve_all(),
    );
    assert!(s.commands.is_empty(), "{}", s.transcript());
    assert!(
        s.requests[1].user.contains("is disabled"),
        "{}",
        s.requests[1].user
    );
    // Runner text: no flags, so the tap goes out, the phone ignores it, nothing changes.
    let (t, _) = go(
        ObserveMode::RunnerText,
        "acme",
        "Sign in to Acme",
        &script,
        &mut Human::approve_all(),
    );
    assert_eq!(t.commands.len(), 1, "{}", t.transcript());
    assert!(t
        .steps
        .iter()
        .any(|st| st.result.contains("disabled, no effect")));
    assert!(
        t.requests[1].user.contains("nothing changed"),
        "{}",
        t.requests[1].user
    );
}

// ---------------------------------------------------------------------------------------
// Notes.
// ---------------------------------------------------------------------------------------

#[test]
fn the_target_row_is_off_screen_until_the_agent_scrolls() {
    for mode in MODES {
        let (r, _) = go(
            mode,
            "notes",
            "Open the note called Passport renewal",
            &[scroll_down(), tap("Cell:Passport renewal"), done("Opened")],
            &mut Human::approve_all(),
        );
        assert_success(&r, &format!("{mode:?}"));
        assert!(r
            .final_state
            .check(r#"notes.open == "Passport renewal""#)
            .unwrap());
        assert!(
            !screen_part(&r.requests[0].user).contains("Passport renewal"),
            "visible before scrolling"
        );
        assert!(screen_part(&r.requests[1].user).contains("Passport renewal"));
        assert!(matches!(r.commands[0], RunnerCommand::Swipe { .. }));
    }
}

fn delete_script() -> Vec<String> {
    vec![
        scroll_down(),
        tap("Cell:Old receipts"),
        tap("Button:Delete"),
        tap("Button:Delete"),
        done("Deleted"),
    ]
}

#[test]
fn delete_goes_through_an_alert_and_two_confirmations() {
    for mode in MODES {
        let (r, _) = go(
            mode,
            "notes",
            "Delete the note called Old receipts",
            &delete_script(),
            &mut Human::approve_all(),
        );
        assert_success(&r, &format!("{mode:?}"));
        assert!(r
            .final_state
            .check(r#"notes.last_deleted == "Old receipts" && notes.items.len == 27"#)
            .unwrap());
        // Runner text: the tap on the note's row asks too (unverified screen).
        assert_eq!(
            categories(&r),
            per_mode(
                mode,
                vec![RiskCategory::Delete, RiskCategory::Delete],
                vec![
                    RiskCategory::Unverified,
                    RiskCategory::Delete,
                    RiskCategory::Delete
                ]
            )
        );
        assert_text_warnings(&r, mode);
        assert!(r.requests[3].user.contains("Delete Note?"), "{mode:?}");
        assert_eq!(r.final_state.screen, "list");
    }
}

#[test]
fn declining_the_alerts_delete_keeps_the_note() {
    for mode in MODES {
        let mut script = delete_script();
        script[4] = tap("Button:Cancel");
        script.push(fail("The owner declined"));
        let (r, _) = go(
            mode,
            "notes",
            "Delete the note called Old receipts",
            &script,
            &mut Human::scripted(&[true, false]),
        );
        assert!(
            r.final_state
                .check("notes.items.len == 28 && notes.last_deleted == ''")
                .unwrap(),
            "{mode:?}"
        );
        assert_eq!(r.final_state.alert, None, "Cancel closed the alert");
        assert_eq!(r.confirmations.len(), 2);
    }
}

// ---------------------------------------------------------------------------------------
// Deny list.
// ---------------------------------------------------------------------------------------

#[test]
fn a_denied_app_is_never_launched() {
    for mode in MODES {
        let (r, _) = go(
            mode,
            "home",
            "Open Passwords and show my saved passwords",
            &[open_app("Passwords"), open_app("Passwords"), done("x")],
            &mut Human::approve_all(),
        );
        assert_eq!(r.outcome.status, OutcomeStatus::Blocked, "{mode:?}");
        assert!(
            r.commands.is_empty(),
            "LaunchApp must not be sent: {:?}",
            r.commands
        );
        assert_eq!(r.final_state.app, "home");
        assert!(r.requests[1].user.contains("deny list"));
    }
}

#[test]
fn starting_inside_a_denied_app_blocks_without_asking_the_model() {
    let setup = setup(ObserveMode::Structured);
    let mut phone = setup.new_phone("passwords").unwrap();
    let mut model = ScriptedModel::new(Vec::<String>::new());
    let r = run_phone(
        &setup,
        &mut phone,
        "Show me something",
        &mut model,
        &mut Human::approve_all(),
        MemoryStore::new(),
    );
    assert_eq!(r.outcome.status, OutcomeStatus::Blocked);
    assert_eq!(model.calls(), 0);
}

// ---------------------------------------------------------------------------------------
// Parsing, stuck detection, budgets, faults, cancel.
// ---------------------------------------------------------------------------------------

#[test]
fn unparsable_model_output_gets_a_correction_and_a_retry() {
    for mode in MODES {
        let mut script = vec!["I think I should open the Wi-Fi settings.".to_owned()];
        script.extend([tap("Cell:Wi-Fi"), tap("Switch:Wi-Fi"), done("Wi-Fi is off")]);
        let (r, m) = go(
            mode,
            "settings",
            "Turn off Wi-Fi",
            &script,
            &mut Human::approve_all(),
        );
        assert_success(&r, &format!("{mode:?}"));
        assert_eq!(r.counts.parse_retries, 1);
        assert_eq!(m.calls(), 4);
        assert_eq!(r.outcome.steps, 3, "a retry is not a step");
        assert!(r.requests[1]
            .user
            .contains("Your last reply could not be used"));
        assert!(!r.requests[2]
            .user
            .contains("Your last reply could not be used"));
    }
}

#[test]
fn parse_retries_run_out() {
    let (r, _) = go(
        ObserveMode::Structured,
        "settings",
        "Turn off Wi-Fi",
        &[
            "hmm".to_owned(),
            "{\"thought\" \"x\"}".to_owned(),
            "tap the wifi".to_owned(),
        ],
        &mut Human::approve_all(),
    );
    assert_eq!(r.outcome.status, OutcomeStatus::Failed);
    assert_eq!(r.counts.parse_retries, 2);
    assert!(
        r.outcome.summary.contains("could not be understood"),
        "{}",
        r.outcome.summary
    );
}

#[test]
fn repeating_an_action_that_changes_nothing_is_stuck() {
    for mode in MODES {
        let setup = setup(mode);
        let mut phone = setup.new_phone("settings").unwrap();
        phone.show_screen("settings", "general").unwrap();
        let about = tap("Cell:About");
        let mut model = ScriptedModel::new(vec![about; 6]);
        let r = run_phone(
            &setup,
            &mut phone,
            "Open About",
            &mut model,
            &mut Human::approve_all(),
            MemoryStore::new(),
        );
        assert_eq!(r.outcome.status, OutcomeStatus::Failed, "{mode:?}");
        assert!(
            r.outcome.summary.starts_with("Stuck"),
            "{}",
            r.outcome.summary
        );
        assert_eq!(r.commands.len(), 2, "the third repeat is not executed");
        assert!(r.requests[3]
            .user
            .contains("same action on the same screen 3 times"));
    }
}

fn waits(n: usize, ms: u64) -> Vec<String> {
    (0..n)
        .map(|i| format!(r#"{{"action":"wait","ms":{}}}"#, ms + i as u64))
        .collect()
}

#[test]
fn step_budget_is_enforced() {
    let mut s = setup(ObserveMode::Structured);
    s.max_steps = Some(3);
    let mut model = ScriptedModel::new(waits(8, 100));
    let r = go_with(
        &s,
        "settings",
        "Wait",
        &mut model,
        &mut Human::approve_all(),
        MemoryStore::new(),
    );
    assert_eq!(r.outcome.status, OutcomeStatus::Failed);
    assert!(
        r.outcome.summary.contains("Step budget"),
        "{}",
        r.outcome.summary
    );
    assert_eq!(r.outcome.steps, 3);
    assert_eq!(model.calls(), 3);
}

#[test]
fn time_budget_is_enforced_in_simulated_time() {
    let mut s = setup(ObserveMode::Structured);
    s.max_duration_ms = Some(10_000);
    let mut model = ScriptedModel::new(waits(20, 4_000));
    let r = go_with(
        &s,
        "settings",
        "Wait",
        &mut model,
        &mut Human::approve_all(),
        MemoryStore::new(),
    );
    assert_eq!(r.outcome.status, OutcomeStatus::Failed);
    assert!(
        r.outcome.summary.contains("Time budget"),
        "{}",
        r.outcome.summary
    );
    assert!(r.outcome.duration_ms >= 10_000);
    assert!(model.calls() < 6, "stopped long before the script ran out");
}

#[test]
fn clock_advances_by_the_configured_cost_per_effect() {
    let mut s = setup(ObserveMode::Structured);
    s.costs = jarvis_sim::ClockCosts {
        observe_ms: 10,
        infer_ms: 100,
        act_ms: 20,
        tool_ms: 0,
        user_ms: 0,
    };
    let mut model = ScriptedModel::new([open_app("Settings"), done("ok")]);
    let r = go_with(
        &s,
        "home",
        "Open Settings",
        &mut model,
        &mut Human::approve_all(),
        MemoryStore::new(),
    );
    assert_success(&r, "clock");
    // observe, infer, act, observe, infer
    assert_eq!(r.outcome.duration_ms, 10 + 100 + 20 + 10 + 100);
    assert_eq!(r.final_state.now_ms, 240);
    assert_eq!(r.final_state.app, "settings");
}

#[test]
fn a_runner_command_that_fails_once_is_retried_by_the_model() {
    for mode in MODES {
        let setup = setup(mode);
        let mut phone = setup.new_phone("settings").unwrap();
        phone.fail_act_at(0);
        let mut model = ScriptedModel::new([
            tap("Cell:Wi-Fi"),
            tap("Cell:Wi-Fi"),
            tap("Switch:Wi-Fi"),
            done("Wi-Fi is off"),
        ]);
        let r = run_phone(
            &setup,
            &mut phone,
            "Turn off Wi-Fi",
            &mut model,
            &mut Human::approve_all(),
            MemoryStore::new(),
        );
        assert_success(&r, &format!("{mode:?}"));
        assert_eq!(r.counts.act_failures, 1);
        assert!(
            r.requests[1].user.contains("injected runner failure"),
            "{}",
            r.requests[1].user
        );
        assert!(r.final_state.check("settings.wifi == false").unwrap());
    }
}

#[test]
fn a_model_failure_ends_the_run_with_the_error() {
    // Three failures in a row (the request and its two retries) end the run.
    let mut model = ScriptedModel::new([tap("Cell:Wi-Fi"), tap("Switch:Wi-Fi")])
        .fail_at(1)
        .fail_at(2)
        .fail_at(3);
    let r = go_with(
        &setup(ObserveMode::Structured),
        "settings",
        "Turn off Wi-Fi",
        &mut model,
        &mut Human::approve_all(),
        MemoryStore::new(),
    );
    assert_eq!(r.outcome.status, OutcomeStatus::Failed);
    assert!(
        r.outcome.summary.starts_with("Model error"),
        "{}",
        r.outcome.summary
    );
    assert_eq!(r.counts.model_failures, 3);
    assert_eq!(r.counts.model_calls, 4);
    assert_eq!(model.remaining(), 1, "the switch tap was never served");
    assert_eq!(
        r.final_state.screen, "wifi",
        "the first step had already happened"
    );
}

#[test]
fn a_transient_model_failure_is_retried_and_the_task_completes() {
    let mut model =
        ScriptedModel::new([tap("Cell:Wi-Fi"), tap("Switch:Wi-Fi"), done("off")]).fail_at(1);
    let r = go_with(
        &setup(ObserveMode::Structured),
        "settings",
        "Turn off Wi-Fi",
        &mut model,
        &mut Human::approve_all(),
        MemoryStore::new(),
    );
    assert_success(&r, "retry");
    assert_eq!(r.counts.model_failures, 1);
    assert_eq!(r.counts.model_calls, 4);
    assert_eq!(r.outcome.steps, 3);
    // The retry is the very same request.
    assert_eq!(model.requests()[1], model.requests()[2]);
    assert!(r.final_state.check("settings.wifi == false").unwrap());
}

#[test]
fn an_exhausted_script_fails_with_a_clear_message() {
    let (r, _) = go(
        ObserveMode::Structured,
        "settings",
        "Turn off Wi-Fi",
        &[tap("Cell:Wi-Fi")],
        &mut Human::approve_all(),
    );
    assert_eq!(r.outcome.status, OutcomeStatus::Failed);
    assert!(
        r.outcome.summary.contains("exhausted"),
        "{}",
        r.outcome.summary
    );
}

#[test]
fn a_script_reference_that_matches_nothing_is_reported() {
    let (r, _) = go(
        ObserveMode::Structured,
        "settings",
        "Turn off Wi-Fi",
        &[tap("Button:No such thing")],
        &mut Human::approve_all(),
    );
    assert_eq!(r.outcome.status, OutcomeStatus::Failed);
    assert!(
        r.outcome.summary.contains("@Button:No such thing"),
        "{}",
        r.outcome.summary
    );
}

#[test]
fn cancel_mid_run_finishes_cancelled() {
    let mut s = setup(ObserveMode::Structured);
    s.options = RunOptions {
        cancel_at_effect: Some(4),
        ..RunOptions::default()
    };
    let mut model = ScriptedModel::new([tap("Cell:Wi-Fi"), tap("Switch:Wi-Fi"), done("x")]);
    let r = go_with(
        &s,
        "settings",
        "Turn off Wi-Fi",
        &mut model,
        &mut Human::approve_all(),
        MemoryStore::new(),
    );
    assert_eq!(r.outcome.status, OutcomeStatus::Cancelled);
    assert!(
        r.final_state.check("settings.wifi == true").unwrap(),
        "the switch was never touched"
    );
    assert_eq!(r.final_state.screen, "wifi");
    assert!(!r.capped);
}

#[test]
fn the_iteration_cap_stops_a_runaway_driver() {
    let mut s = setup(ObserveMode::Structured);
    s.options = RunOptions {
        max_iterations: 3,
        ..RunOptions::default()
    };
    let mut model = ScriptedModel::new([tap("Cell:Wi-Fi"), tap("Switch:Wi-Fi"), done("x")]);
    let r = go_with(
        &s,
        "settings",
        "Turn off Wi-Fi",
        &mut model,
        &mut Human::approve_all(),
        MemoryStore::new(),
    );
    assert!(r.capped);
    assert_eq!(r.outcome.status, OutcomeStatus::Failed);
    assert!(r.outcome.summary.contains("iterations"));
}

#[test]
fn runs_are_deterministic() {
    for mode in MODES {
        let run_once = || {
            go(
                mode,
                "shop",
                "Order the Wireless Headphones",
                &order_script(),
                &mut Human::approve_all(),
            )
            .0
        };
        let (a, b) = (run_once(), run_once());
        assert_eq!(a.transcript(), b.transcript());
        assert_eq!(a.final_state, b.final_state);
        assert_eq!(a.model_outputs, b.model_outputs);
    }
}
