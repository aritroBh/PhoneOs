# Phase 0 spikes

These six spikes decide whether Jarvis is buildable on your phone. Do them in the order below. Each one ends with a filled-in results table that you send back. No product code is written until S1 and S2 pass (PLAN.md section 7).

Setup assumed throughout: iPhone 17 Pro Max, iOS 27.x, free Apple ID, a Mac with Xcode. Runner background is in `docs/RUNNER.md`.

## Order and time

| Order | Spike | Question | Time | Needs |
|---|---|---|---|---|
| 1 | [S6 build](S6-build.md) | Can we build and install the apps at all? | 1 to 2 h | Mac, phone, cable |
| 2 | [S1 runner](S1-runner.md) | Does a runner install, start, and drive other apps? | 2 to 4 h | Output of S6 |
| 3 | [S5 system UI](S5-system-ui.md) | What does the runner see on system screens? | 1 h | S1 running |
| 4 | [S2 inference](S2-inference.md) | Where can the model run while another app is in front? | Half a day, plus the harness app (written later) | Harness app |
| 5 | [S3 memory](S3-memory.md) | How big a model survives without the paid entitlement? | 2 h, plus harness | Harness app, S2 numbers |
| 6 | [S4 wake word](S4-wake-word.md) | Does "Hey Jarvis" keep working for 8 hours? | 1 h setup, 8 h wait, 15 min readout | Harness app |

S6 is first because S1 depends on a working install path. S2 to S4 need a harness app that does not exist yet. Do S6, S1 and S5 now, then wait for the harness.

Total hands-on time: about 8 hours, plus the 8 hour soak in S4.

## What to send back

For each spike, send the filled results table from that guide, plus:

- The exact iOS version (Settings, General, About, iOS Version) and the model name.
- Screenshots or short screen recordings where the guide asks.
- For anything that failed: the exact error text, copied, and what you pressed just before it.
- Anything that was different from the guide, even small. The guides cannot be checked against a device yet.

Put results in `docs/spikes/results/<spike>.md` or paste them into the chat. Do not paste Apple ID passwords, pairing files, or keys.

## Rules

- Do not turn on automatic iOS updates and do not update iOS during Phase 0. Results are tied to one version.
- Write down the version of every third-party tool you install (SideStore, LocalDevVPN, LoopCtrl or iCTRL, with the date).
- If a step in a third-party README no longer matches what you see, follow the tool's README and tell us what changed.
- A failure is a result. Record it and move on.

## Links

- LoopCtrl: https://github.com/Souitou-iop/LoopCtrl
- iCTRL: https://github.com/OpenMinis/iCTRL
- PhoneAgent: https://github.com/rounak/PhoneAgent
- SideStore: https://github.com/SideStore/SideStore (docs linked from its README)
- LocalDevVPN: check the LoopCtrl README Requirements section for the current link
