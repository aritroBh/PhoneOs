# S1: Runner on the phone

Question: does a LoopCtrl or iCTRL runner install and run on iPhone 17 Pro Max, iOS 27.x, free Apple ID, and can it drive other apps?

Do this after S6 (the runner is installed). Try LoopCtrl first; use iCTRL only if LoopCtrl fails. Details of what each runner exposes are in `docs/RUNNER.md`.

## Prerequisites

- S6 complete: runner app installed, SideStore, loopback VPN app.
- Developer Mode on.
- A laptop that can reach the runner. By default the runner only listens on the phone itself (127.0.0.1). The LoopCtrl and iCTRL READMEs describe LAN or USB access (a "Remote Access" setting, off by default). Use the README's current steps: https://github.com/Souitou-iop/LoopCtrl and https://github.com/OpenMinis/iCTRL. Turn Remote Access off again when done.
- A third-party app you have installed (any app with a login screen or a list, not a game).
- The client script the repo points to: LoopCtrl names `Scripts/loopctrl_rpc.py` as its protocol client. Check the README for how to run it and how to pair.

## Steps

1. Start the runner. For LoopCtrl: open the app, tap Mount Disk Image if needed, then Start Server. On first start iOS may ask for your passcode, Notifications, and Local Network permission; accept them. Check the README if the button names differ.
2. Pair a client. Follow the README pairing steps. The phone asks you to Allow; tap Allow.
3. Call `device_info`. Record the result.
4. Open Safari by hand, then call `page_source{visible_only:true, max_children:50}` through the client. Save the raw output to a file. Count lines.
5. Run each action below with the client. After each one, look at the phone and write down yes or no. For taps, use the frame center from the tree.

   | Action | Method to call |
   |---|---|
   | Launch Settings | `launch_app` with `bundle_id` `com.apple.Preferences` |
   | Launch Safari | `launch_app` with `com.apple.mobilesafari` |
   | Tap an element | `tap{x,y}` |
   | Type text into the Safari address bar | `type{text}` |
   | Swipe or scroll | `drag` or `swipe` or `page` |
   | Screenshot | `screenshot` |
   | Read tree | `page_source` |
   | Go home | `home` |
   | Open URL | `open_url` |

6. Repeat steps 4 and 5 in Settings and in your third-party app.
7. Survive an app switch: start the server, press Home, open another app, call `page_source`, then reopen the runner. Does the server still answer?
8. Turn the screen off with the side button. Wait 60 seconds. Call `device_info` from the laptop. Does it answer? Call `page_source`. (Read-only should work; the READMEs claim this.)
9. Parse check: find a screen with a button whose label contains an apostrophe or a comma (many apps have one). Save its raw `page_source` line. Does the line show the label intact?
10. Time 10 calls each of `page_source`, `screenshot`, `tap`. Record the median in milliseconds.
11. Reboot the phone. Write down the exact steps needed to get the runner working again. This becomes our restart procedure.

## What to observe

- Whether a passcode prompt appears at start.
- Whether an "Automation Running" overlay shows, and whether it appears in screenshots.
- Whether the runner keeps its place after you use Home.
- Exact text of any error returned by the RPC.
- In the `page_source` result: the keys present (`source`, `keyboard`, `coordinate_space`, ...), and what is inside `keyboard`.

## Results

| Check | Safari | Settings | Third-party app (name) |
|---|---|---|---|
| Launch by bundle id | | | |
| Tap | | | |
| Type | | | |
| Swipe or scroll | | | |
| Screenshot (size in KB) | | | |
| Tree has the visible UI (yes/partial/no) | | | |
| Tree line count | | | |

| Item | Result |
|---|---|
| Runner used (LoopCtrl or iCTRL), version | |
| iOS version | |
| Passcode asked at start | |
| Survives app switch (yes/no) | |
| Answers with screen off (yes/no) | |
| `page_source` keys present | |
| Contents of `keyboard` key | |
| Label with apostrophe or comma shown correctly (paste line) | |
| Median ms: page_source / screenshot / tap | |
| `device_info` points size and scale | |
| Frontmost app id available where | |
| Restart steps after reboot (numbered list) | |
| Overlay visible in screenshot (yes/no) | |

## Pass / fail (PLAN.md S1)

Pass: tap, type, swipe, screenshot, and tree all work in Safari, Settings, and one third-party app. The server survives an app switch. Restart steps after a reboot are written down.

## If it fails

1. Try the other runner (iCTRL) with its README steps.
2. If both fail on iOS 27, try PhoneAgent from Xcode on the Mac (https://github.com/rounak/PhoneAgent, run the `PhoneAgent` scheme, test `testRPCBridge`; check the README for the physical device forwarding script). This tells us whether plain XCUITest still works.
3. If all fail, send the logs (LoopCtrl has a `logs` method and the runner app has a log view; check the README). PLAN.md says the fallback is the ESP32 actuator or stopping to reassess.
