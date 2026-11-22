# S4: Wake word and keepalive

Question: does an in-app wake word engine keep detecting "Hey Jarvis" from the background for 8 hours, with the process still alive, and what does it cost in battery?

This needs a harness app that does not exist yet. This guide says what it must do.

## What the harness app must do

1. Run a keyword spotter (sherpa-onnx keyword spotting, per PLAN.md section 2.4) on live microphone audio. Use a background audio session so the app stays alive, with the microphone entitlement (Info.plist `UIBackgroundModes` audio, and the microphone permission text).
2. On detection, play a short system sound, vibrate, and append a line to a log with a timestamp, confidence score, and thermal state.
3. Log a heartbeat every 60 seconds with: time, battery level, thermal state, app state, and CPU time used so far (`task_info`).
4. On launch, detect whether the previous run ended without a clean stop (marker file). If so, log "process was killed or crashed" with the last heartbeat time.
5. Show counters on screen: detections, heartbeats, uptime.
6. A "false trigger test" mode that does not require the keyword: counts detections only, to measure false positives overnight.
7. Export the log through the share sheet.
8. Handle audio interruptions (calls, Siri, other audio) and log them; try to restart the audio session after an interruption ends and log whether that worked.

## Steps once the harness exists

1. Install via Xcode. Grant microphone permission.
2. Plug in the phone. Note battery. Then unplug (the battery test must be unplugged).
3. Start the harness. Confirm the heartbeat counter moves. Press Home. Open a few apps. Lock the phone.
4. Say "Hey Jarvis" once, at normal volume, from 1 m, then at 3 m. Wait a few seconds between. Check on the log later (or unlock quickly) whether it counted.
5. Leave the phone idle for the first 30 minutes in a quiet room. Note battery drop and count false triggers.
6. Over 8 hours, say the phrase once an hour and note the time. Do a few normal activities: a phone call (accept an interruption), play music, use a few apps.
7. At the end open the harness. Read heartbeat count (should be about 480), detections, interruptions, and whether the process survived.
8. Optionally repeat with Action Button and Vocal Shortcuts as fallbacks, per PLAN.md. The Vocal Shortcuts setup steps are in Apple's docs: search Apple Support for "Vocal Shortcuts" and follow the current page rather than this guide.

## What to observe

- The orange microphone dot is on the whole time.
- Whether heartbeats stop (process suspended or killed) and when.
- Whether detections fail only after some time.
- Battery drop per hour, with the screen off.
- Phone temperature.

## Results

| Item | Result |
|---|---|
| iOS version | |
| Wake engine and model | |
| Hours alive (first gap in heartbeats, if any) | |
| Heartbeats logged / expected | |
| Spoken phrases detected / spoken (list time) | |
| Detection at 1 m / 3 m | |
| False triggers in first 30 min and in 8 h | |
| Battery start / end, percent | |
| Battery per hour, screen off | |
| Interruptions and whether audio resumed | |
| Process killed (yes/no, time) | |
| Thermal states seen | |

## Pass / fail (PLAN.md S4)

Pass: "Hey Jarvis" detected from the background for 8 hours; battery cost measured; Jarvis process still alive at the end.

## If it fails

- If the process is suspended after some time, record the time and what was running. The silent audio session is the keepalive; check whether another app took the audio session.
- If detection works but false triggers are high, record the count and note that a custom keyword may be needed.
- If the process cannot survive, fall back to the Action Button and Vocal Shortcuts options (PLAN.md section 2.4) and mark wake word as not available.
