# Runner evaluation: LoopCtrl, iCTRL, PhoneAgent

Date: 2026-11-22. Nothing here has been run on a device.

Evidence tags used on every claim:

- **[source]**: seen in the code of the repository, at the path given.
- **[readme]**: claimed by a README or SKILL.md and not confirmed in code.
- **[unverified]**: not seen anywhere, or the file could not be read in full.

How this was read: files were fetched from raw.githubusercontent.com and github.com through a page-to-text tool that summarizes long files. Quotes below are therefore short and some function bodies were never visible. Where a file was truncated or a body was not shown, this document says so. Large files (`LoopCtrlUITests/ActionHandler.swift` is about 509 KB, `iCTRLUITests/ActionHandler.swift` about 453 KB) were read only in part: LoopCtrl's at offsets 0 to 300000, iCTRL's at 0 to 100000. The Apple private-API surface and the `launch_app` bodies were not seen in either.

## 1. Comparison

| Item | LoopCtrl | iCTRL | PhoneAgent |
|---|---|---|---|
| Repo | Souitou-iop/LoopCtrl | OpenMinis/iCTRL | rounak/PhoneAgent |
| License | Apache-2.0 [readme] | Apache-2.0 from v1.32.0, AGPL-3.0 before [readme] | MIT [readme] |
| Minimum iOS | iOS 16, "not verified on a device" [readme]. iOS 27 pairing flow documented [readme]. | iOS 16 and later [readme] | Not stated [readme] |
| Transport | JSON-RPC 2.0 over HTTP, `POST /rpc` [readme]. `JSONRPCRequest`, `JSONRPCResponse` in `LoopCtrlKit/RPCTypes.swift` [source]. Server is `JSONRPCServer(port:localOnly:)` in `LoopCtrlUITests/LoopCtrlUITests.swift` [source]. | JSON-RPC 2.0 over HTTP, `POST /rpc` [readme] | Newline-delimited JSON over raw TCP (`NWListener`), request `{"id","method","params"}`, response `{"id","result"}` or `{"id","error":{"message"}}` [source: `PhoneAgentUITests/SimulatorRPCServer.swift`] |
| Port | Default 8300, settable 1024 to 65533 [readme]. Persisted via `ServerPort.stored` [source]. | 8200 [readme] | 45678, fixed (`defaultPort`) [source] |
| Bind | `127.0.0.1` when `localOnly` ("Server started on 127.0.0.1:port (local only)") [source]. LAN only if Remote Access is on [readme]. | `127.0.0.1` default, LAN with Remote Access, USB, optional AWDL [readme] | Listener has no interface restriction; peers are filtered to IPv4 and IPv6 loopback, `fc00::/7`, and `localhost`, others cancelled [source] |
| Auth | Encrypted envelope: fields `v, tok, ts, nonce, ct, tag`; PRF-CTR stream cipher from HMAC-SHA256, encrypt-then-MAC; `|ts - now| <= 120` s [source: `LoopCtrlKit/SecureEnvelope.swift`]. X25519 pairing, user taps Allow on phone [readme]. Unauthenticated: `/skill /schema /whoami /docs /pair` [readme]. | X25519 pairing then HMAC-SHA256 sealed requests, approved on phone, key dropped after 30 days idle [readme]. Envelope code not read. | None. Only the peer-address check [source] |
| Method count | About 55 in `LoopCtrlKit/RPCMethod.swift` [source] | About 35 [readme]; per-method params partly from SKILL.md [readme] | 12 in `handleRPC` [source: `PhoneAgentUITests/PhoneAgent.swift`] |
| Tree format | Indented text lines, not JSON, inside result key `source` [source: `ForegroundApp.m`, `ActionHandler.swift`] | Same line format [source: `iCTRLUITests/ForegroundApp.m`] | One string `tree` from `XCUIApplication.debugDescription`, lightly compressed [source] |
| Screenshot | Default JPEG, quality 0.6, scale 2.0, base64 under `data`, plus pixel and point sizes [source: `ActionHandler.swift`] | Same defaults [source: `iCTRLUITests/ActionHandler.swift`] | PNG, base64 under `screenshot_base64`, `metadata.width/height` in pixels [source] |
| App launch | `launch_app{bundle_id or name}`; resumes the app, no relaunch [readme]. Body not seen. `activate_app`, `terminate_app` exist [source: `RPCMethod.swift`] | Same method names [readme]. Body not seen. | `XCUIApplication(bundleIdentifier:).activate()`, waits up to 8 s for `.runningForeground` [source] |
| Server start | In-app **Start Server** button; test bundle runs inside the host app process [readme]. `testStartServer()` then a `while keepRunning` run loop on `distantFuture` [source] | Tap **iCTRLUITests-Runner** icon or **Start Server** in the iCTRL app [readme] | Run the scheme from Xcode on a Mac; `testMain` or `testRPCBridge` waits up to 6 h on an expectation [source] |
| Keep alive | Run loop plus `SilentAudioKeepAlive` [source]. Runs with screen off [readme]. Reboot unmounts the DDI, remount with one tap [readme]. | "Never stops"; runs with screen off; reboot needs DDI remount [readme] | 6 hour timeout, then the test ends [source]. No on-device keepalive. |
| Developer Disk Image | In-app **Mount Disk Image**; iOS 17.4 to 26.3 personalized, 26.4+ Cryptex [readme]. Needs a loopback VPN (LocalDevVPN, Shadowrocket, Surge, Loon) routing `10.7.0.1` [readme]. | Mount with StikDebug, the iCTRL app, or a Mac; LocalDevVPN for on-device [readme] | Not applicable, Xcode handles it |
| Free Apple ID | 3 slots, LoopCtrl uses one, SideStore renews [readme] | 3 slots; room for only one of StikDebug or the iCTRL app beside the runner [readme] | Not discussed |
| Private APIs | `XCUIApplication(bundleIdentifier: "com.apple.springboard")`, private `XCUIScreen` screenshot selectors, `XCTImage` via selectors, KVC on `shouldHaltWhenReceivesControl`, `testmanagerd` daemon screenshot, `QuiescenceDisabler`, `XCUIDevice.accessibilityInterface` `requestSnapshotForElement:` [source: `LoopCtrlUITests.swift`, `ForegroundApp.m`]. Also a CoreDevice engine over `remoteAXService`, Indigo HID, `pasteboardservice` [source: `CoreDeviceAX.swift`; readme: `docs/coredevice-ax-no-overlay.md`]. | Private XCTest and `LSApplicationWorkspace` [readme]; `XCUIDevice.accessibilityInterface`, `XCAXClient_iOS` [source: `iCTRLUITests/ForegroundApp.m`] | None seen. Public XCUITest only (`XCUIScreen.main.screenshot()`, `debugDescription`, `typeText`) [source] |
| Notable limits | "Automation Running" overlay; passcode prompt on Start; `unlock` cannot enter passcode; `list_apps` degraded on newer iOS [readme] | `unlock` cannot enter passcode; screen stream unencrypted; `list_apps` may be empty on newer iOS [readme] | Tree "noisy/stale during animations"; keyboard input varies [readme]. No auth. Device path needs a Mac. |

## 2. What each runner returns, in detail

### 2.1 LoopCtrl

- Methods [source: `LoopCtrlKit/RPCMethod.swift`, parameters as summarized by the fetch tool, last 4 KB of the file unread]:
  - Read: `page_source{bundle_id, filter, max_depth, max_elements, max_children, enhanced, visible_only}`, `screen_text`, `screenshot{scale, bundle_id, format, quality}`, `collect`, `read_value`, `find_element{element_id*, match, element_type, bundle_id}`, `ax_probe`.
  - Touch: `tap{ref | x,y, fast, settle...}`, `double_tap`, `long_press{ref | x,y, duration}`, `swipe{x, y, direction, distance, velocity, fast}`, `page{direction, distance}`, `scroll_to_element`, `scroll` (deprecated), `drag{from_ref, to_ref, from_x, from_y, to_x, to_y, duration}`, `touch{phase, x, y}`, `gesture{fingers}`, `tap_element`, `find_and_tap`.
  - Text: `type{text*, clear, submit, x, y, fast}`, `clear_text`.
  - Apps: `list_apps`, `resolve_app{text*}`, `open_url{url*}`, `launch_app{bundle_id | name}`, `activate_app{bundle_id*}`, `terminate_app{bundle_id*}`.
  - Device: `device_info`, `home`, `app_switcher`, `notification_center`, `control_center`, `lock`, `unlock`, `is_locked`, `power`, `action_button`, `volume{direction*}`, `button{button*, phase, duration}`, `rotate`, `clipboard{action, text}`, `keep_awake{seconds}`, `system_screenshot`, `wait{text, until, source, timeout, duration}`, `wait_for{present, absent, foreground, timeout}`, `run_steps{steps*, observe}`, `coredevice`, `system_broadcast`.
  - Server: `config`, `logs`, `reset`, `stream_ticket`, `control`, `stream_ack`, `page_update`.
- Result shapes are open dictionaries (`result: [String: JSONValue]?`). Errors: `-32700, -32600, -32601, -32602, -32603`, auth `-32099`, business `-32000`; business errors return HTTP 200 [source: `RPCTypes.swift`].
- Tree line format [source: `LoopCtrlUITests/ForegroundApp.m`, `renderBoundedSnapshot` / `LCBuildLine`]:
  - Two spaces of indent per depth.
  - Type name from `LCTypeName` (`Button`, `Cell`, `Table`, `StaticText`...), unmapped types become `Type<n>`.
  - Then, only when non-empty: `, label: '...'`, `, value: '...'`, `, id: '...'`.
  - Always last: `, {{x, y}, {w, h}}`, each component truncated to an integer.
  - Example built from that format (not copied from output): `Button, label: 'OK', id: 'okBtn', {{12, 300}, {80, 44}}`.
  - Only label, identifier, value, elementType and frame are rendered. Enabled, selected, hasFocus and placeholder are requested from the snapshot but not rendered. Hittable is not emitted. Secure text fields appear only as the type name `SecureTextField`.
  - Nodes with zero-size frames and scroll indicators (type 47) are dropped when a visible rect is given.
- **Trust.** The tree line format is ambiguous and forgeable: quotes and newlines inside labels and values are not escaped, so on-screen text (a text field's value, a web page's text) can contain characters that read as format — a newline followed by a well-formed line, a fake role, a fake frame. No parser can make that unambiguous. Jarvis's parser (`jarvis_observe::parse_runner_text`) stops what it can: after a node start whose quoted value is still open, every line is text of that value until a line closes the node (never a node of its own). It cannot detect text that closes the quote itself (`…', {{x, y}, {w, h}}` at the end of a line), after which the page chooses the nodes. Therefore **acting on a text screen always asks**: the session marks such an observation `ObservedRunnerText`, every state-changing action on it is at least `Unverified`, and the owner's question starts with a warning that the screen was read through an unverified text format. The typed tree from Jarvis's own runner is the supported path for acting; the text path is for experiments. To check in spike S1: whether the third-party runner offers a JSON or otherwise typed tree (escaped strings, explicit flags; the `CoreDeviceAX` engine below produces typed `AXElement`s, and `page_source` may have a non-text `format`), or a per-point element probe (`ax_probe`, `find_element`) that could confirm the element under a planned tap before it is sent.
- `page_source` result keys seen in code: `format: "text"`, `source`, `coordinate_space: "points"`, `keyboard`, `source_channel`, `truncated`, `total_returned`, `hint`, `snapshot_path`, `timing`, `max_children_used`, `ax_coverage`, `channel`, `scroll_position` [source: `ActionHandler.swift`, offset 200000 window]. The inner shape of `keyboard` is not in the code I saw. SKILL.md says `keyboard{visible, covers_from_y}` [readme].
- `ObserveFormat.compact` can strip frames to shrink the tree (9.7 KB to 3.2 KB on a 132 node page); we need frames, so do not use it [source].
- Frontmost app: `ForegroundApp.currentBundleID` uses `XCUIDevice.accessibilityInterface.activeApplications`, skips SpringBoard unless nothing else is running [source]. It is surfaced as `foreground_app` in `home`, `wait_for` and the `device_info` system block [source: method result lists in `RPCMethod.swift`]. It is not a documented field of `page_source`.
- Hittable: `find_element` returns `hittable` [source: `RPCMethod.swift`].
- Coordinates: points of the interface, following device orientation [readme: SKILL.md]. Screenshots are pixels; point equals pixel divided by the `scale` returned with that screenshot [readme].
- Screenshot result keys: `format, encoding, data, bytes, pixel_width, pixel_height, point_width, point_height, scale, requested_scale, device_scale, device_name, width, height, hint, prefer_instead, scope` [source, from the iCTRL file; the LoopCtrl window showed the same defaults and the `data` and `encoding` keys].
- Second engine: `LoopCtrlKit/CoreDeviceAX.swift` reads the tree through `remoteAXService` over an RSD tunnel and produces `AXElement` with keys `pid, object_id, is_app, parent, depth, children, type, role, traits, label, value, identifier, placeholder, value_description, x/y/width/height, has_frame, is_element, enabled, selected, focused, visible, remote, remote_pid` [source]. Frames are points. There is no hittable and no secure flag. `docs/coredevice-ax-no-overlay.md` says this path is aimed at iOS 27, avoids the Automation banner, and was shown working "from the app process on an iPhone 17 with iOS 27.3" on 2026-11-08, with remaining work [readme]. Whether the on-phone `page_source` uses this engine today was not confirmed [unverified].
- Wire protocol client reference: `Scripts/loopctrl_rpc.py` is named as "the protocol v1 client" [readme: `docs/server.md`]. It was not read.

### 2.2 iCTRL

- Methods [readme]: `tap, double_tap, long_press, tap_element, find_and_tap, swipe, scroll, scroll_to_element, drag, page, type, clear_text, screenshot, page_source, find_element, screen_text, launch_app, activate_app, terminate_app, list_apps, resolve_app, open_url, device_info, home, lock, unlock, is_locked, wait, config, reset, stream_ticket`, plus `collect`. Parameter shapes from SKILL.md [readme]:
  - `find_element{element_id*, element_type, match:"contains", bundle_id}`
  - `tap | double_tap | long_press{ref | x,y, duration}` and "tap takes only ref or x+y, never element_id"
  - `type{text*, submit, clear}`
  - `drag{points: 2 to 64 {x,y}, duration 0.1 to 10 s}` needs capability `drag_path`
  - `page{direction, distance}`, `swipe{direction, distance, velocity, x, y}`
  - `page_source{visible_only, max_children, filter{text|label|type}}`, `max_elements` default 2000, cap 4000
  - `keyboard{visible, covers_from_y}`, `loading`, `channel.recommended`
- Source seen [source]: `iCTRLUITests/ForegroundApp.m` renders the same line format as LoopCtrl (`ICTRLBuildLine`, label, value, id, integer frame). Only label, identifier, value, elementType, frame are read (`ICTRLReadNode`). No hittable, enabled, selected, focused, secure output. `iCTRLUITests/ActionHandler.swift` screenshot is JPEG by default, quality 0.6, scale 2.0, result keys as in 2.1. `iCTRLUITests/iCTRLUITests.swift` captures JPEG at quality 0.25 for the video stream (`captureJPEGQuality`).
- The file layout (`ActionCommand.swift`, `ActionHandler.swift`, `ForegroundApp.m`, `QuiescenceDisabler.m`) matches LoopCtrl's, so I treat the two as one lineage [source: directory listings]. iCTRL's `page_source` body and `launch_app` body were not visible.
- No `button`, `volume`, `clipboard`, `run_steps` in the iCTRL README list [readme]. They may exist in code [unverified].

### 2.3 PhoneAgent

All of this is from `PhoneAgentUITests/PhoneAgent.swift` `handleRPC` [source]. Numeric params accept a number or numeric string. Unknown method throws `invalidCommand`.

| Method | Params | Result |
|---|---|---|
| `get_tree` | none | `tree` |
| `get_screen_image` | none | `screenshot_base64`, `metadata{width,height}` (pixels from PNG header) |
| `get_context` | none | `screenshot_base64`, `metadata`, `tree` |
| `tap` | `x`, `y` | `tree` |
| `tap_element` | `coordinate` (string `"{{x, y}, {w, h}}"`), `count` (default 1), `longPress` (default false) | `coordinate`, `count`, `longPress`, `tree` |
| `enter_text` | `coordinate`, `text` | `coordinate`, `tree` |
| `scroll` | `x, y, distanceX, distanceY` | `tree` |
| `swipe` | `x, y, direction` | `tree` |
| `open_app` | `bundle_identifier` | `bundle_identifier`, `tree` |
| `set_api_key` | `api_key` | `ok` |
| `submit_prompt` | `prompt` | `started` |
| `stop` | none | `{}` and shutdown |

- The tree is `XCUIApplication.debugDescription` with hex addresses removed, `", ,"` collapsed, and `Other` lines dropped unless they carry `identifier:`, `label:` or `placeholderValue:`. Keyboard presence is read from `app.keyboards.element.exists` but is not output [source].
- Typing is `app.typeText(text + "\n")` after tapping the element and waiting up to 2 s for the keyboard; the newline is always sent [source].
- Long press is a fixed 0.5 s; swipe is a fixed 100 points after a 0.1 s press [source].
- Coordinates are offsets from the screen origin; unit not named in code [source], points by XCUITest convention [unverified].
- Frontmost bundle id: not reported. The runner tracks its own current target [source].

## 3. Mapping from our types to each runner

Our types are from `docs/ARCHITECTURE.md`, jarvis-proto. "Gap" means we need it and the runner has no way to give it. "Adapter" means the Swift `RunnerClient` must compute or translate it.

### 3.1 `RunnerCommand`

| Ours | LoopCtrl | iCTRL | PhoneAgent |
|---|---|---|---|
| `Tap{x,y}` points | `tap{x,y}` [source] | `tap{x,y}` [readme] | `tap{x,y}` [source] |
| `LongPress{x,y,duration_ms}` | `long_press{x,y,duration}`; unit of `duration` unseen, likely seconds [unverified] | `long_press{x,y,duration}` [readme] | Gap. Only `tap_element{coordinate,longPress}` with fixed 0.5 s. Adapter builds a fake frame string around the point. |
| `Swipe{from,to,duration_ms}` | No `from/to` on `swipe`. Use `drag{from_x,from_y,to_x,to_y,duration}` [source] | `drag{points[2..64], duration}` [readme] | `scroll{x,y,distanceX,distanceY}` is a zero-hold drag; `swipe` only gives a fixed 100 pt [source] |
| `TypeText{text}` | `type{text, clear, submit}` [source]. Types into the focused field when no `x,y`. Focus semantic [unverified]. | `type{text, submit, clear}` [readme] | `enter_text{coordinate, text}` always taps the element first and always appends a newline. Gap: cannot type into the already focused field without a frame, and cannot avoid submit. |
| `PressKey{Return}` | `type{text:"", submit:true}` is a guess [unverified]. `button{button, phase, duration}` takes hardware buttons [source], names unseen. | Not in README | Gap |
| `PressKey{Delete}` | `clear_text` clears the field; single Delete not seen [unverified] | `clear_text` [readme] | Gap |
| `PressKey{Home}` | `home` [source] | `home` [readme] | Gap (no home method) |
| `LaunchApp{bundle_id}` | `launch_app{bundle_id}` [source for name; body unseen]. Resumes, does not relaunch [readme]. Use `terminate_app` first for a clean start. | same [readme] | `open_app{bundle_identifier}` [source] |
| `OpenUrl{url}` | `open_url{url}` [source] | `open_url{url}` [readme] | Gap |
| `Wait{ms}` | `wait{duration}`; unit unseen [unverified]. Also `wait_for{present, absent, foreground, timeout}` [source] which we could use for stability. | `wait` [readme] | Gap, wait on the client |
| `RunnerResult{ok,error}` | Success is a result dictionary with `message`; failure is a JSON-RPC `error{code,message,data}`, `-32000` for business errors [source]. Adapter maps error to `ok:false, error:message`. | same [readme] | `error.message` only [source] |

Extras we do not model yet but the runner offers: LoopCtrl `ref` taps that re-resolve a live element, `settle` options, `run_steps`, `find_and_tap`, `scroll_to_element`, `clipboard`.

### 3.2 `Screen` and `UiNode`

| Ours | LoopCtrl | iCTRL | PhoneAgent |
|---|---|---|---|
| `Screen.app_bundle_id` | `foreground_app` in `home`, `wait_for`, `device_info.system` [source: method result lists]. Not in `page_source` output. Can pass `bundle_id` into `page_source` to scope. Adapter makes a second call. | `currentBundleID` exists internally [source]; RPC surface [unverified] | Gap. Runner knows only the app it last opened. |
| `Screen.app_name` | `list_apps`, `resolve_app` return names [source]; adapter can cache | same [readme] | Gap |
| `Screen.size` | `device_info` point dimensions, `scale` [source] | same [readme] | Gap for points. Screenshot `metadata` is pixels. |
| `Screen.keyboard_visible` | `page_source.keyboard` key exists [source]; inner shape `{visible, covers_from_y}` [readme] | [readme] | Gap. The code reads it but does not return it. Adapter can look for a `Keyboard` line in the tree. |
| `Screen.orientation` | `device_info.orientation` [readme], `rotate` [source] | [unverified] | Gap |
| `Screen.root` as nested `UiNode` | Gap. Output is flat text with indentation. Adapter parses indent into children. | same | same, but debugDescription format, not the `label: '...'` format |
| `UiNode.role` | Type name string at line start; unmapped become `Type<n>` [source]. Map to our `Role`, rest to `Other(String)`. | same [source] | debugDescription type name [unverified, not seen] |
| `UiNode.label` | `, label: '...'` [source] | same [source] | `label:` [source] |
| `UiNode.value` | `, value: '...'` [source] | same | not listed as kept; only `identifier`, `label`, `placeholderValue`, frame are named [source] |
| `UiNode.identifier` | `, id: '...'` [source] | same | `identifier:` [source] |
| `UiNode.placeholder` | Requested from snapshot, not rendered [source]. Gap. | same | `placeholderValue:` [source] |
| `UiNode.frame` | `{{x, y}, {w, h}}` points, integer truncated [source] | same | `{{x, y}, {w, h}}` [source], unit unnamed |
| `UiNode.enabled` | Gap in text output. Present in the CoreDevice `AXElement` [source]. | Gap [source] | Gap |
| `UiNode.visible` | `visible_only` filter [source]; per node `visibility: in_view / partial / offscreen` [readme]. The text format has no slot for it [source]. Cull happens server side, so absence means off screen. | same | Gap |
| `UiNode.hittable` | Only via `find_element` per element [source]. Gap for a whole tree. Not in CoreDevice AX [source]. | Gap | Gap |
| `UiNode.focused` | Gap in text output. `focused` exists in CoreDevice `AXElement` [source]. | Gap | Gap |
| `UiNode.selected` | Gap in text output; in CoreDevice `AXElement` [source] | Gap | Gap |
| `UiNode.secure` | Only inferable from type `SecureTextField` [source] | same | [unverified] |
| `Observation.screenshot` | `screenshot` returns base64 JPEG in `data`, default scale 2.0 and quality 0.6 [source]. Sizes in points and pixels. | same [source] | `screenshot_base64` PNG [source] |
| Ref to element for tapping | `ref` from `find_element`, re-resolved at tap time [source: find_element returns `ref`; readme for semantics]. Not present in `page_source` lines [source]. We tap `frame.center()`. | same | none; frame string |

### 3.3 The biggest gaps

1. No runner returns a structured tree on the XCTest path. Adapter must parse indented text. Quote and comma handling inside labels is not shown in the code I read [unverified]; test with labels containing `'`, `,`, `{{`, and newlines.
2. `enabled`, `selected`, `focused`, `hittable`, `secure`, `placeholder` are not in the rendered tree on the XCTest path. Core code must not depend on them being true. Consider extending the forked `ICTRLBuildLine` / `LCBuildLine` to emit them: the snapshot already carries enabled, selected, hasFocus and placeholder [source comment]. Hittable needs a per-element call and is expensive.
3. Frontmost bundle id and keyboard visibility need extra calls or text sniffing on every runner. Neither is clearly in the `page_source` payload.
4. Frames are integer points. Fine for centers, coarse for small targets.
5. Authentication on LoopCtrl and iCTRL requires implementing their envelope. Reuse `LoopCtrlKit/SecureEnvelope.swift` and `AuthManager.swift` from the fork rather than reimplementing.

## 4. Recommendation

Fork **LoopCtrl** first, iCTRL second, keep PhoneAgent as a minimal reference and an S1 smoke test.

Reasons for LoopCtrl:

- It is the only one whose documented install path needs no computer at run time (in-app DDI mount, iOS 27 on-device pairing) [readme], which matches the plan.
- Its tree source is the same as iCTRL's, and it also contains a second engine (CoreDevice AX) that already exposes `enabled`, `selected`, `focused`, `visible`, `placeholder` as JSON with point frames [source]. That is the closest thing to our `UiNode` found anywhere. Whether it is usable from the runner process or only the app process is a device question.
- It has the larger method set we will want later (`run_steps`, `wait_for`, `button`, `clipboard`, `keep_awake`, `find_element` with `hittable`) [source].
- Apache-2.0.

Risks: bigger codebase (about 35 files in `LoopCtrlKit` and 31 in `LoopCtrlUITests`), a custom crypto envelope, features we do not need (relay, Live Activity, stream, remote HTML). Plan to delete those in the fork. iCTRL is smaller and nearly identical, so S1 should try both only if LoopCtrl fails.

PhoneAgent is MIT, has no auth, no private API, and a 12-method surface that is easy to read. It needs a Mac with Xcode at every start and stops after 6 hours [source]. Use it only to learn whether plain XCUITest sees other apps on iOS 27 in S1.

### What the Swift `RunnerClient` adapter must translate

1. **Transport and auth.** HTTP POST `/rpc` to `127.0.0.1:<port>`, wrap each JSON-RPC body in the LoopCtrl envelope (pairing once, store the key in Keychain). Behind a protocol so a plaintext PhoneAgent-style client can be swapped in for tests.
2. **Command mapping** per section 3.1: `Swipe` to `drag`; `TypeText` to `type`; `PressKey` to `home`, `clear_text`, `button` or `type{submit}`; `LaunchApp` to `launch_app` (consider `terminate_app` first when a fresh start is wanted); `Wait` to `wait` with unit conversion.
3. **Error mapping.** JSON-RPC `error` to `RunnerResult{ok:false}`. Treat `-32099` as "re-pair", transport failure as "runner dead".
4. **Observation.** Call `page_source{visible_only:true, max_children:50}` (the documented on-screen read [readme]), parse indented lines into `UiNode`, read `keyboard`, make a second call for frontmost bundle id and size, optionally `screenshot` (JPEG base64, point equals pixel divided by returned `scale`).
5. **Tree repair.** Map type names to `Role`, unknown to `Other`. Set missing booleans to defaults (`enabled: true`, `hittable: true`, `focused: false`, `selected: false`, `secure: role == SecureTextField`) and carry a flag so the core can tell "unknown" from "true". This requires a small proto change; raise it with the core owner.
6. **Settle.** Use the runner's `settle` params or `wait_for` before re-snapshotting, because stale snapshots during animation are a known problem [readme].
7. **Verify typing.** After `type`, re-read the field `value`.

## 5. Open questions that only a device test can answer

| # | Question | Why it matters | Spike |
|---|---|---|---|
| 1 | Does the runner start, stay up, and answer RPC on iPhone 17 Pro Max, iOS 27.x, free Apple ID? | Gate for everything | S1 |
| 2 | Does `page_source` text really match the line format above? Do labels with quotes, commas, braces, or newlines break parsing? | Parser correctness | S1 |
| 3 | What is inside `page_source.keyboard`? Is `foreground_app` returned anywhere cheap? | Missing `Screen` fields | S1 |
| 4 | Does `page_source` in iOS 27 use the XCTest snapshot or the CoreDevice AX engine, and does `ax_probe` work from the runner? | Decides whether we get `enabled`, `focused`, `selected` for free | S1 |
| 5 | Is `launch_app` activate or relaunch? Does it work from the background app to a third-party app? | Reliability of app switching | S1 |
| 6 | Units of `duration` in `long_press`, `wait`, `drag` | Adapter conversion | S1 |
| 7 | Does `type` go into the focused field without `x,y`? Does `submit` press Return? Does Delete exist? | `TypeText`, `PressKey` | S1 |
| 8 | Latency per call: snapshot of a dense page, screenshot, tap | Time budget per step vs plan's 10 s | S1, S2 |
| 9 | Does the runner survive Jarvis going to background, the screen turning off, and an app switch? | Core assumption of the plan | S1, S4 |
| 10 | What does the tree show for AutoFill bar, Face ID sheet, permission alert, Control Center, keyboard? | Safety and credential flow | S5 |
| 11 | Passcode prompt at server start, and what happens after a reboot | Restart procedure | S1, S6 |
| 12 | Does the "Automation Running" overlay appear and does it show in screenshots? | Vision model confusion | S1, S5 |
| 13 | Does `list_apps` return anything on iOS 27? | App catalog for the router | S1 |
| 14 | Is a Metal workload allowed inside the runner process? | Plan option D | S2 |
| 15 | Is the loopback VPN app available without a paid account, and does its VPN icon matter? | 3 app slot budget | S6 |
