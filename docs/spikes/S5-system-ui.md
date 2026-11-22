# S5: Runner against system UI

Question: what does the runner see and do on the iOS screens that matter for an agent: AutoFill bar, Face ID sheet, permission alerts, Control Center, display off?

Do this right after S1 passes, with the runner running and paired.

## Prerequisites

- S1 passed, client script working, runner started.
- A login screen in some app where Passwords AutoFill offers a saved credential.
- An app you have never granted a permission to (for the camera or location alert), or reset permissions: Settings, General, Transfer or Reset iPhone, Reset, Reset Location & Privacy. This resets all your app permissions, so only do it if you accept that.
- Auto-Lock: set Settings, Display & Brightness, Auto-Lock, Never for the display tests. Restore it after.

## Steps

For each scenario, set up the screen by hand, then call `page_source` and `screenshot` through the client and save both. Then try one tap from the client.

1. **Keyboard.** Tap a text field in Safari. Call `page_source`. Record the `keyboard` value and whether `Key` or `Keyboard` lines appear. Type with `type`. Does the text appear? Read the field back.
2. **AutoFill bar.** Open the login screen. Tap the username field. The password suggestion appears above the keyboard. Call `page_source`. Is the suggestion in the tree? Try to tap it by coordinates. Does iOS then ask for Face ID?
3. **Face ID sheet.** While the Face ID or passcode sheet is showing, call `page_source` and `screenshot`. Is anything returned? Does `tap` do anything? (Assume it cannot approve Face ID; PLAN.md says so. We only need to know what the tree says so the agent can detect it.)
4. **Permission alert.** Open the app that triggers a permission request (camera, notifications, location). When the alert shows, call `page_source`. Does it list the alert and its buttons? Does the response list the frontmost app as the app or as SpringBoard? Tap Allow by coordinates.
5. **Control Center.** Call the runner's `control_center` method if present (LoopCtrl lists it; iCTRL README does not), or swipe down from the top right by hand. Call `page_source`. Does the tree show Control Center?
6. **Notification Center and Home Screen.** Call `home`. Call `page_source`. What does the tree contain (SpringBoard icons)? Does `launch_app` still work after that?
7. **Notification banner.** Send yourself a message while the runner is on another app. Does the banner show in the tree or screenshot?
8. **Share sheet and in-app sheets.** In Safari tap Share. `page_source` lists it? Dismiss with a tap.
9. **Display off.** Press the side button once. Wait 30 seconds. Call `page_source`, `screenshot`, then `tap`. Record which work. Press the side button again; does the lock screen need Face ID or passcode? Call `unlock`; record what happens.
10. **Locked with passcode.** Leave the phone locked 2 minutes. Call `device_info` and `page_source`. Record.

## What to observe

- What the tree returns in each state: empty, only SpringBoard, or full UI.
- Which method names worked, with exact error messages.
- Whether a screenshot is black while the display is off or locked.
- Whether "Automation Running" appears and where.

## Results

| Scenario | Tree has the elements (yes/partial/no) | Screenshot useful | Tap works | Notes (frontmost app id, errors) |
|---|---|---|---|---|
| Keyboard visible | | | | |
| AutoFill suggestion | | | | |
| Face ID or passcode sheet | | | | |
| Permission alert | | | | |
| Control Center | | | | |
| Home Screen | | | | |
| Notification banner | | | | |
| Share sheet | | | | |
| Display off, unlocked state | | | | |
| Locked with passcode | | | | |

## Pass / fail

PLAN.md S5 has no numeric pass condition; it asks "what happens". Treat as pass when every row is filled with an observation. The decisions it feeds:

- If the tree shows permission alerts and the AutoFill bar, the credential flow in PLAN.md section 2.5 is feasible.
- If the Face ID sheet is invisible, Jarvis must detect it from the unexpected empty tree and pause.
- If display-off blocks reads, the "pocket mode" design (Auto-Lock Never, minimum brightness) is required, not optional.

## If it fails

- If `page_source` returns nothing on system screens, take a screenshot too and send both. The LoopCtrl README says OCR is the fallback; check the README for `screen_text`.
- If the runner dies on lock, record when, and the exact restart steps.
