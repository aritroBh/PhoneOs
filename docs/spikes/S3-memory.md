# S3: Memory budget without the paid entitlement

Question: what is the largest model that loads and completes 50 steps without iOS killing the app (a jetsam kill)? Free Apple IDs probably do not get the increased memory limit entitlement. PLAN.md calls this unverified.

This uses the same harness app as S2. The harness does not exist yet; this guide says what it must add.

## What the harness app must add

1. A readout of memory: `os_proc_available_memory()` at launch, after model load, and after each step; plus resident size from `task_info`. Show and log them.
2. A model ladder: it must be able to load each of these in turn, loaded one at a time with the previous one unloaded: about 0.5B, 1B, 2B (Gemma 4 E2B), 3B, 4B (Qwen3-VL-4B) all at 4-bit, and one 8-bit 2B for comparison. The owner places the files; the harness lists what it finds.
3. A 50-step soak: run the S2 text prompt set in a loop of 50 steps, one model at a time, with a fixed context length (set to 4096 tokens and, on a second pass, 8192). After each step log memory and thermal state. Detect a kill by writing a "run in progress" marker on start and checking it on next launch ("last run was killed during step N").
4. A switch to run the loop in the foreground and, separately, after pressing Home (background).
5. A log of the entitlement state: the harness reads its own entitlements (the provisioning profile embedded in the app) and shows whether `com.apple.developer.kernel.increased-memory-limit` is present.
6. Export of the log as JSON through the share sheet.

## Steps once the harness exists

1. Install the harness through Xcode with your free Apple ID.
2. Open it and read the "available memory at launch" and the entitlement line. Record both.
3. Starting from the smallest model: load, run the 50-step loop in the foreground at context 4096. If it completes, note the peak memory, then go to the next model up.
4. Repeat each completed model in the background (press Home after starting, open Safari, return after 10 minutes).
5. For the largest model that completes at 4096, repeat at 8192.
6. After a kill, relaunch the harness: it reports the step where it died. Record it, then stop climbing the ladder.
7. Close all other apps, reboot once, and repeat the largest passing model, to get a clean-phone number.

## What to observe

- Available memory at launch, in MB.
- At what model size the first kill occurs and during load or during steps.
- Whether context size matters more than weights.
- Thermal state.

## Results

| Model (size, quant) | Context | Mode (fg/bg) | Load OK | Steps completed of 50 | Peak MB | Available MB at end | Killed at step |
|---|---|---|---|---|---|---|---|
| | | | | | | | |
| | | | | | | | |
| | | | | | | | |
| | | | | | | | |

| Item | Result |
|---|---|
| Increased-memory entitlement present (yes/no) | |
| Available MB at launch, clean phone | |
| Largest model that passes 50 steps, foreground | |
| Largest model that passes 50 steps, background | |

## Pass / fail (PLAN.md S3)

Pass: you have a named largest model that loads and completes 50 steps without a jetsam kill. It may be small; that is still a pass of the measurement. Also note if it falls below the 2B class, which makes the paid account worth buying (PLAN.md section 10).

## If it fails

- If even the 0.5B model gets killed, check that nothing else is wrong (another app using memory, a debug build under Xcode's memory limits). Run from the Home Screen, not from Xcode, and repeat.
- If the entitlement is absent and the largest model is under 2B, record it and ask whether to buy the $99 account before continuing.
