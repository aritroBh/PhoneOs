# S2: Where can inference run while another app is in front?

Question: while Jarvis is in the background, which of the four options in PLAN.md section 4.2 gives an acceptable time per step? Models: Gemma 4 E2B and Qwen3-VL-4B.

This spike needs a harness app that does not exist yet. This guide says what the harness must do and what you do with it. Do not start until the harness is delivered. The harness is separate from Jarvis and from the runner.

## The options being tested

| Option | What runs | Where |
|---|---|---|
| A | LiteRT-LM or llama.cpp on CPU | Harness app in the background |
| B | Apple Foundation Models (system model) | Harness app in the background, model runs out of process |
| C | Model on GPU, harness brought to front for the step, then the target app restored | Harness app in the foreground for each step |
| D | Model loaded inside the runner process | Runner (only if Metal works there) |

## What the harness app must do

1. Build and install on the free Apple ID like any app (see S6). One target for the harness.
2. Load a model selected from a list (Gemma 4 E2B, Qwen3-VL-4B, a name and file path). Models are downloaded by the owner and copied in through Files, or downloaded in-app; the harness shows size on disk.
3. Provide a fixed benchmark prompt set, bundled in the app:
   - 5 text steps: a compacted accessibility tree of about 800, 1500, 2500 tokens, plus a task text, expecting a one-line JSON action.
   - 3 vision steps: one screenshot with numbered marks plus a short task.
   - The same prompts every run, so numbers are comparable.
4. For each run record and display: prompt token count, prefill ms, decode tokens, decode tokens per second, total wall ms, peak memory (`os_proc_available_memory` before and after), thermal state, app state (foreground or background) at start and end, and whether the process was killed.
5. Run modes: (a) foreground baseline on GPU, (b) background on CPU, (c) background on Foundation Models, (d) foreground flip: a "Run step then return" button that launches the runner's `activate_app` for a chosen bundle id after the step finishes, and times the full round trip including the app switch.
6. A "Run in background" button that starts the benchmark, then the owner presses Home. The harness keeps going on a background audio session (silent audio) so it is not suspended, same as the plan. It writes results to a file and a screen.
7. Export all results as one JSON file (share sheet) and a copyable text table.
8. Log any Metal error or crash on backgrounding.
9. Option D needs a separate tiny test inside the runner target that tries to create a Metal device and run one kernel. Result: works or error text.

## Steps once the harness exists

1. Install the harness (S6 steps). Ensure Auto-Lock Never and the phone is on the charger, brightness low.
2. Load Gemma 4 E2B. Run foreground GPU baseline (mode a). Record numbers.
3. Run background CPU (mode b): tap Run in background, press Home, open Safari, wait for the completion notification, open the harness, read results.
4. Run Foundation Models (mode c) the same way. Note whether it refuses to run in background.
5. Run the foreground flip (mode d) with the runner already started (S1). Record the whole round trip.
6. Repeat 2 to 5 with Qwen3-VL-4B.
7. Run option D's Metal test in the runner.
8. Between runs, let the phone cool for 5 minutes. Record battery percent at start and end.

## What to observe

- Whether a background Metal job crashes the app (PLAN.md says it does on iOS 26).
- Tokens per second for prefill and decode per option.
- Whether Foundation Models answers while the harness is in the background.
- Thermal state climbing during repeated runs.

## Results

One row per model and option.

| Model | Option | Prefill tok/s | Decode tok/s | Wall ms for 1500 token tree step | Vision step ms | Peak MB | Thermal at end | Crashed or refused (text) |
|---|---|---|---|---|---|---|---|---|
| Gemma 4 E2B | A CPU bg | | | | | | | |
| Gemma 4 E2B | B Apple model | | | | | | | |
| Gemma 4 E2B | C flip | | | | | | | |
| Gemma 4 E2B | D in runner | | | | | | | |
| Qwen3-VL-4B | A CPU bg | | | | | | | |
| Qwen3-VL-4B | B Apple model | | | | | | | |
| Qwen3-VL-4B | C flip | | | | | | | |
| Qwen3-VL-4B | D in runner | | | | | | | |

Option B has no model name choice; fill it once and note it applies to both rows.

## Pass / fail (PLAN.md S2)

Pass: measured tok/s and time per step for options A to D with both models, and at least one option under about 10 seconds per text step.

## If it fails

- If no option is under 10 s per text step, record which was closest and what limits it (prefill or decode). Options: use a smaller model, shrink the prompt, accept the foreground flip for every step, or reopen the provider decision (PLAN.md 4.4: home machine or cloud backend).
- If a model does not load, that is S3's question; go there.
- If Foundation Models is unavailable, record the exact availability reason string the framework gives.
