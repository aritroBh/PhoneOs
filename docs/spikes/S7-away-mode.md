# S7: Away mode (dark but not locked)

Question: can the phone look off, stay unlocked, and still be driven by the runner and woken by voice?

Do this after S1 (runner works) and S4 (wake word keeps running).

## Prerequisites

- S1 passed, runner started, client script working.
- Settings → Display & Brightness → Auto-Lock → Never. Restore it after.
- A small test build of Jarvis (or any app you can edit) with three switches: black full-screen view, `UIScreen.main.brightness = 0`, `UIDevice.current.isProximityMonitoringEnabled = true`, and `UIApplication.shared.isIdleTimerDisabled = true`.
- Phone on a charger.

## Steps

1. **Brightness persists.** In the test app set brightness to 0. Go to the Home Screen and open Settings. Is the screen still at minimum? Lock and unlock: does brightness return to the old value?
2. **Black screen.** Show the black view at brightness 0 in a dark room. Is any glow visible? Note it.
3. **Proximity blanking, idle.** Enable proximity monitoring, show the black view, place the phone face down. Does the display turn fully off? Lift it: does it come back without Face ID? Leave it face down 30 minutes: is it still unlocked when lifted?
4. **Runner while blanked.** Face down, display off by proximity. From the client call `page_source`, then `launch_app` Settings, then `page_source` and `tap`. Record which work, and whether the display turns on when another app comes to the front.
5. **Runner at brightness 0.** Face up, brightness 0, Jarvis in front. Run a three-step task through the runner (open Settings, tap Wi-Fi, go back). Does it complete? Does brightness stay at 0 in the other app?
6. **Return to dark.** After step 5, `launch_app` the test app. Does the black view and proximity blanking resume?
7. **Idle timer from synthesized touches.** Set Auto-Lock to 30 seconds. Have the runner tap once every 20 seconds for 3 minutes with another app in front. Does the phone stay awake? Then stop tapping: does it lock at 30 seconds? Set Auto-Lock back to Never.
8. **Wake word.** In Away mode (face down, blanked), say the wake phrase from 2 m. Detected? Repeat after 1 hour.
9. **Darker than minimum.** Settings → Accessibility → Zoom → Zoom Filter → Low Light, and Accessibility Shortcut → Zoom. Triple-click: how dark is brightness 0 now? Does the runner still drive (zoom can shift coordinates: check one tap lands)?
10. **Eight hours.** Leave it in Away mode on the charger overnight. In the morning: still unlocked, runner still answering, wake word still detected, battery temperature (feel, and Settings → Battery).

## What to observe

| Check | Result |
|---|---|
| Brightness 0 persists across apps | |
| Proximity blanking keeps device unlocked | |
| Runner reads tree while blanked | |
| Runner taps while blanked | |
| Display behaviour when another app comes to front while face down | |
| Synthesized touches reset the idle timer | |
| Wake word detected while blanked, after 1 h | |
| Still alive after 8 h | |

## Decision

- Steps 3 to 6 pass → Away mode = black screen + proximity blanking, tasks run dark.
- Blanking breaks the runner → Away mode = black screen at brightness 0, face up, no blanking.
- Phone locks anyway → Away mode is not possible; "closed" means "screen on, dimmed, on a stand".
