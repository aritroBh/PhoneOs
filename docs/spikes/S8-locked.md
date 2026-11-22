# S8: Locked phone

Question: what exactly happens to the runner and to Jarvis when the side button is pressed, and can anything get back in without you?

Do this after S1 and S5. Phone has a passcode and Face ID. Keep the Mac connected for logs.

## Steps

1. **Lock while idle.** Runner started, Jarvis in front. Press the side button. From the client call `is_locked`, `device_info`, `page_source`, `screenshot`, `tap`, `launch_app` Settings. Save every response and error text verbatim.
2. **Unlock and resume.** Unlock with Face ID. Without restarting anything, call `page_source` and `tap`. Do they work at once? How long after unlock?
3. **Lock mid-task.** Start a five-step task. Press the side button after step 2. What does the runner return for the next call? Unlock: does the next call succeed?
4. **Long lock.** Lock for 10 minutes, 1 hour, overnight. After each: is the runner process still answering `device_info`? Does it work again after unlock, or must it be restarted (and does restarting ask for the passcode)?
5. **What is on the lock screen.** Locked, display woken (raise or tap). Call `page_source` for SpringBoard and save it. Swipe up so the passcode pad shows; call `page_source` again. Record whether the pad's buttons or a "Passcode field" appear in the tree, with types and identifiers.
6. **Keypad experiment (optional, your decision).** Only if step 5 shows the pad in the tree: with a throwaway passcode, send one `tap` at a digit's coordinates. Does the dot fill? If yes, try the whole code. Record the result either way, then restore your passcode. This is to settle the question, not to adopt it.
7. **keep_awake.** Auto-Lock 30 seconds. Call the runner's `keep_awake` for 5 minutes with another app in front. Does the phone stay awake? Repeat with plain taps every 20 seconds instead. Set Auto-Lock back.
8. **Lock button from the runner.** Call `lock`. On iOS 27 does it lock, do nothing, or open Siri?
9. **Background intent while locked.** From the lock screen, run a Jarvis App Shortcut by voice and from a notification action. Which run without asking for unlock?
10. **Wake word while locked.** Say the wake phrase with the phone locked on a table. Detected? Is the orange microphone dot shown?
11. **After a call.** Take a phone call while Jarvis is listening in the background. After hanging up, is the wake word still detected without opening Jarvis?
12. **Reboot.** Restart the phone. Before unlocking: nothing should work. After unlocking once: what must be redone (disk image mount, runner start, passcode prompt)? Time it.
13. **Bluetooth keyboard (optional).** Pair a keyboard. Lock the phone. Press a key, type the passcode, Return. Unlocked? Does the runner then work?

## What to observe

| Check | Result |
|---|---|
| `is_locked` true after side button | |
| Runner answers while locked (which calls) | |
| Exact error from `launch_app` while locked | |
| Works again after Face ID with no restart | |
| Runner alive after 1 h / overnight locked | |
| Passcode pad visible in SpringBoard tree | |
| Synthesized tap fills a passcode dot | |
| `keep_awake` holds off auto-lock | |
| Plain taps hold off auto-lock | |
| `lock` works on iOS 27 | |
| Voice shortcut / notification action runs while locked | |
| Wake word detected while locked | |
| Listening survives a phone call | |
| Steps needed after reboot | |

## Decision

- Steps 1 to 4 as expected (dead while locked, alive after unlock) → the pause-and-resume design in PLAN 3.3 stands.
- Runner dies on lock → Jarvis must restart it after unlock; measure whether that needs the passcode each time.
- Step 6 works → note it; adopting it is a separate security decision.
