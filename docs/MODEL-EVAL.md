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
| 1 | Tapping a denied app's icon on the home screen launched it; the deny check ran only on the next observation. | Fix in progress |
| 2 | Tapping any cell with a price asked for a Purchase confirmation. | Fix in progress |
| 3 | Step results carry no state change, so loops on changing screens are invisible to the model and to stuck detection. | Fix in progress |
| 4 | Return in a message field does not send; the prompt suggested it does. | Fix in progress |
| 5 | Switch values render as 0/1. | Fix in progress |
| 6 | Nothing marks a list as scrollable. | Fix in progress |
| 7 | Prompt examples ("No contact named Alex") leak into answers. | Fix in progress |
| 8 | `done` right after a declined confirmation is accepted as success. | Fix in progress |
| 9 | 160 output tokens is too few for a chatty model. | Fix in progress |
| 10 | The simulator's `json_schema` request is rejected by llama-cpp-python. | Fix in progress |
| 11 | Eval tasks compare sent messages by exact text and expect Failed where Blocked is correct. | Fix in progress |
| 12 | Runner-text observations lose focused/disabled (the runner's format does not carry them). | Known limit of that format; the typed tree keeps them |

Positives: the lenient parser accepted a nested action form; a password aimed at a secure field became an owner hand-off every time.

## What it means for model choice

- Qwen3.5-4B was clearly the most capable (14 of 17 finished tasks across both modes, scrolled on its own, used the hand-off). It needs a runtime that reuses the prompt cache and a short-thought prompt to be usable.
- Qwen3-4B-Instruct was 6 to 7 times faster per step here and passed about a third.
- Gemma-4-E2B with this prompt passed nothing it finished.
- Not tested: any model of 2B or less, Gemma-4-E4B, constrained decoding, repeats for variance, the same models after the fixes above.
