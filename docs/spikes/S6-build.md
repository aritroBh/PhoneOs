# S6: Build pipeline

Question: can an app be built and installed on the phone with a free Apple ID, and can the runner be installed alongside it? Or is a Mac required?

You have a Mac with Xcode, so the baseline is Xcode install. The second half checks the no-Mac route (CI-built unsigned `.ipa` installed through SideStore).

## Prerequisites

- Mac with Xcode (note the version) and a USB cable.
- iPhone with Developer Mode on: Settings, Privacy & Security, Developer Mode, restart, confirm. If the entry is missing, connect the phone to Xcode once and it appears.
- Your Apple ID signed into Xcode: Xcode, Settings, Accounts, plus.
- Free Apple ID limits to keep in mind: 7 day certificates, 3 sideloaded apps, 10 new App IDs per week.

## Part A: Xcode install (baseline)

1. In Xcode: File, New, Project, iOS, App. Name it `S6Hello`, interface SwiftUI. Choose your team (Personal Team).
2. Plug in the phone, unlock it, tap Trust This Computer.
3. Select the phone as the run destination at the top of the Xcode window.
4. Press Run (the play button). If signing fails, open the target, Signing & Capabilities, and pick a unique bundle id such as `com.yourname.s6hello`.
5. On the phone: Settings, General, VPN & Device Management, tap your Apple ID under Developer App, Trust. Run again.
6. Note whether it launches, and the date. Delete the app afterwards so you keep your 3 slots free.

## Part B: Runner install

7. Follow the install steps in the README of the runner you chose for S1: https://github.com/Souitou-iop/LoopCtrl (section Install and Requirements). Do not improvise. The README currently describes signing `LoopCtrl.ipa` with SideStore, or building from source.
8. Install SideStore by following its own docs from https://github.com/SideStore/SideStore. Check the tool's README for the current computer step.
9. Install the loopback VPN app the LoopCtrl README names in its Requirements table. Check the README for the current name and source.
10. Record how many of the 3 app slots are used after installing: SideStore, the runner app, and anything else.

## Part C: No-Mac route (optional now, required before Phase 1 ends)

11. This needs a CI workflow that does not exist yet. For now only record the answer to: does the runner project have a documented unsigned `.ipa` build (`Scripts/` and `Makefile` in the LoopCtrl repo; check the README)? When the workflow exists, we will send a one-page procedure.

## What to observe

- Every error dialog, verbatim.
- Whether the runner app crashes at open. The LoopCtrl README says the test bundle must be signed with the host app's bundle id or the app crashes as soon as it opens.
- How long each install took.
- What happens to a SideStore-installed app after you reboot the phone.

## Results

| Item | Result |
|---|---|
| Xcode version | |
| iOS version | |
| Part A: hello app launched from Xcode (yes/no) | |
| Part A: Developer Mode steps that differed from this guide | |
| SideStore version, install worked (yes/no) | |
| Loopback VPN app name and version, install worked | |
| Runner app installed (yes/no), version or commit | |
| Slots used of 3 | |
| Part C: unsigned build documented in repo (yes/no, where) | |
| Date of install (certificates expire after 7 days) | |

## Pass / fail (PLAN.md S6)

Pass: an unsigned `.ipa` from CI installs through SideStore, runner included. Or you confirm a Mac is needed. Because you have a Mac, a pass today means Part A and Part B work from Xcode and you record whether Part C is feasible.

## If it fails

- Signing error "maximum number of apps": delete old test apps and wait. Free accounts also have a weekly App ID limit; if hit, you must wait up to 7 days or buy the $99 account.
- Runner crashes at open: re-check that its bundle id matches the host app per the README.
- SideStore step fails: record the error, then try the Mac route in the runner README (`Scripts/sign-ipa.sh` is described for paid accounts; check the README for free-account instructions).
- Send the exact error text. Do not retry more than twice in a row.
