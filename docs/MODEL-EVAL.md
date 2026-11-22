# Local-model evaluation on the simulated phone

Date: 2026-11-22. Core snapshot: `56ae9b2` (before first-use approval, skills, lock handling and the tree fixes). Raw logs are not in the repository.

This measures how small local models do on the 16 scripted tasks of `jarvis-sim` when they replace the scripted model. It is not a phone benchmark.

## Read this first

- One run per task per model at temperature 0. A difference of one or two tasks is noise.
- CPU only, 4 x86 cores shared with other builds (load average 11 to 19). Timings are pessimistic and say nothing about an iPhone.
- A 35-minute cap per model and mode cut two models short. Unfinished tasks are not counted as failures.
- Constrained decoding was never in effect: the server used (llama-cpp-python 0.3.37) rejects the `json_schema` response format the simulator sends.
- The bundled scripted model passes all 16 tasks, so the tasks are solvable.

## Setup

- Server: `python -m llama_cpp.server --n_ctx 4096 --n_threads 4`, one model at a time, with a logging proxy in front.
- Client: `jarvis-sim eval <task.json> --model-url … --mode structured|runner-text --context-tokens 4096`, default policy, approvals auto-approved, 160 output tokens.
- Models (Hugging Face, GGUF Q4_K_M from `unsloth`, Apache-2.0 tag): `Qwen3-4B-Instruct-2507` (2.50 GB), `gemma-4-E2B-it` (3.11 GB), `Qwen3.5-4B` (2.74 GB).

## Results

| Model | Mode | Passed / finished | Not finished (cap) | Avg prompt tokens | Avg s per step | Replies cut at 160 tokens |
|---|---|---|---|---|---|---|
| Qwen3-4B-Instruct-2507 | structured | 6 / 16 | 0 | 609 | 10.3 | 0 |
| Qwen3-4B-Instruct-2507 | runner-text | 5 / 16 | 0 | 605 | 16.4 | 0 |
| Gemma-4-E2B-it | structured | 0 / 7 | 9 | 716 | 21.4 | 12 |
| Gemma-4-E2B-it | runner-text | 0 / 10 | 6 | 700 | 17.2 | 6 |
| Qwen3.5-4B | structured | 6 / 7 | 9 | 623 | 70.6 | 0 |
| Qwen3.5-4B | runner-text | 8 / 10 | 6 | 611 | 59.5 | 0 |

Generation speed on that loaded CPU: about 3.4 tokens/s for Qwen3-4B and Gemma-E2B, 1.8 for Qwen3.5-4B. Qwen3.5 re-evaluated the whole prompt every step (the runtime could not reuse its cache), which is most of its 60 to 70 s per step.

Largest prompt seen: 864 tokens of 4096. The simulated screens are small; the token budget code was never exercised.

A prompt-only tweak (shorter thoughts, "do not repeat a step marked ok", scroll hint) on Qwen3-4B structured: 5 / 16 against 6 / 16. No gain.

## Why tasks failed

1. **Loops on actions that change the screen.** "Add to Cart" tapped 8 to 10 times; a message typed again and again into the same field. The step history only said "ok", and the stuck detector fires only on an unchanged screen.
2. **Giving up or declaring done too early.** "Note not found" without scrolling (nothing on screen says the list scrolls); "done" straight after the owner declined; one false "signed in successfully" after a wrong-password alert.
3. **Gemma-E2B:** replies cut off at 160 tokens, and switches shown as `value="0"/"1"` misread (it turned a switch back on).
4. **Element numbers reused across screens:** the model kept tapping `[3]` after the screen changed.
5. **Injection task:** no model ever tapped "Place Order". The failures there were the cart loop.

## Problems this found in our own code

| # | Problem | Status |
|---|---|---|
| 1 | Tapping a denied app's icon on the home screen launched it; the deny check ran only on the next observation. | Fixed: home-screen taps, long presses and tap_points are resolved to a known app by label and get `open_app`'s checks before anything is sent; unknown icons ask first. `small_models::tapping_a_denied_icon_on_the_home_screen_never_sends_the_tap` (+ 4 more in `tests/small_models.rs`), sim task `icon_tap_denied`. |
| 2 | Tapping any cell with a price asked for a Purchase confirmation. | Fixed (partly already: rows over three words were already read as content). A `Cell` naming an item next to its price is not a purchase; price-only rows, buttons/links with a price, buy/pay/price-verb rows and checkout screens still ask. `classify_context::priced_list_rows_navigate_and_are_not_purchases`, `priced_controls_and_command_rows_still_ask`; `order_headphones` still confirms at Place Order. |
| 3 | Step results carry no state change, so loops on changing screens are invisible to the model and to stuck detection. | Fixed: step lines say what changed (`-> ok; changed: Button "Cart (0)" -> "Cart (1)"`, `field now "…"`, `now in Settings > Wi-Fi`), and the same action on the same control on a changing screen is warned at 3, warned again at 5 and stopped at the 6th. `small_models::step_results_say_what_changed`, `a_loop_on_a_changing_screen_is_warned_twice_then_stopped`, `typing_the_same_text_into_the_same_field_again_is_a_loop`, `legitimate_repetition_is_not_a_loop`, `scrolling_and_filling_a_form_are_not_loops`, `changes::tests::*`; sim task `cart_add_once`. |
| 4 | Return in a message field does not send; the prompt suggested it does. | Fixed in the prompt ("in a message field Return does not send: tap the Send button"). `prompt::tests::system_prompt_lists_every_action_and_the_delimiters`, snapshot `wifi_prompt.txt`. |
| 5 | Switch values render as 0/1. | Fixed: `(on)`/`(off)`; the raw value stays in `Element::value`. `ios26::switches_render_as_on_or_off_and_keep_the_raw_value`; sim task `switch_already_on`. |
| 6 | Nothing marks a list as scrollable. | Fixed: `… more below; scroll down` (and `… more above; scroll up`) from iOS's own scroll indicator; the home screen's `Page 1 of 2` is kept as one line. `ios26::scroll_cues_follow_the_scroll_indicator_of_real_trees`, `home_screen_keeps_its_page_indicator_as_one_line`; sim task `notes_two_scrolls`. |
| 7 | Prompt examples ("No contact named Alex") leak into answers. | Fixed: every free-text example field is a `<placeholder>`. `prompt::tests::system_prompt_lists_every_action_and_the_delimiters`. |
| 8 | `done` right after a declined confirmation is accepted as success. | Fixed: it finishes `Blocked` (and `done` right after a failed step, with nothing working since, finishes `Failed`). `small_models::done_right_after_a_declined_confirmation_is_blocked`, `done_right_after_a_failed_step_is_failed`; sim task `done_after_decline`. |
| 9 | 160 output tokens is too few for a chatty model. | Fixed: 256 (the longest complete reply seen was 91 tokens), the thought is capped at 12 words in the prompt, and a cut-off reply gets one retry asking for a shorter one. `session_flow::a_cut_off_reply_asks_once_for_a_shorter_one`. |
| 10 | The simulator's `json_schema` request is rejected by llama-cpp-python. | Fixed in the simulator: on a 4xx/5xx naming the response format it retries with `json_object` (schema attached), then with none, and reports the mode used. `http::tests::a_rejected_json_schema_falls_back_to_json_object_then_none`. |
| 11 | Eval tasks compare sent messages by exact text and expect Failed where Blocked is correct. | Fixed: `has(text, part)` (normalised contains) in the message tasks; `expect_status` takes a list, `["failed", "blocked"]` after a decline. `expr::tests::has_is_a_tolerant_contains_for_sent_text`; `eval --json` now streams one object per task with its transcript (`cli::eval_json_is_machine_readable_and_strict_controls_the_exit_code`). |
| 12 | Runner-text observations lose focused/disabled (the runner's format does not carry them). | Not changed: a limit of that format; the typed tree keeps them. Nothing is invented, and the fixed prompt does not mention either flag (asserted in `prompt::tests::system_prompt_lists_every_action_and_the_delimiters`). |

Positives: the lenient parser accepted a nested action form; a password aimed at a secure field became an owner hand-off every time.

## What it means for model choice

- Qwen3.5-4B was clearly the most capable (14 of 17 finished tasks across both modes, scrolled on its own, used the hand-off). It needs a runtime that reuses the prompt cache and a short-thought prompt to be usable.
- Qwen3-4B-Instruct was 6 to 7 times faster per step here and passed about a third.
- Gemma-4-E2B with this prompt passed nothing it finished.
- Not tested: any model of 2B or less, Gemma-4-E4B, constrained decoding, repeats for variance, the same models after the fixes above.
