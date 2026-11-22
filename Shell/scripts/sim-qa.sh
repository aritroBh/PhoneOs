#!/usr/bin/env bash
# One command for the iPhone Simulator QA, used by CI (.github/workflows/ios.yml) and on a Mac.
#
#   scripts/sim-qa.sh [--only-testing ID]... [--skip-bootstrap] [--device NAME] [--out DIR]
#
#   --only-testing ID   run only this test (repeatable), e.g. JarvisUITests/SomeTests/testFoo
#   --skip-bootstrap    do not rebuild the Rust core / xcframework (reuse Frameworks/)
#   --device NAME       simulator device name to use instead of the automatic pick
#   --out DIR           artifact directory (default: Shell/build/qa)
#
# Steps: tool check, bootstrap, xcodegen, pick + boot a simulator, build-for-testing,
# test-without-building, then ALWAYS export attachments, test summary and logs into the
# artifact directory. The exit code is the test run's (non-zero on failures) even though the
# export still happens.
#
# Written for bash 3.2 (the macOS system bash): no mapfile, no associative arrays, and
# arrays that may be empty are expanded with the ${a[@]+"${a[@]}"} idiom.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
shell="$(dirname "$here")"
qa="$shell/build/qa"
derived="$shell/build/DerivedData"
skip_bootstrap=0
device_name=""
only_testing=()

die() { echo "sim-qa: $*" >&2; exit 2; }
log() { echo "sim-qa: $*" >&2; }
group() { if [ -n "${GITHUB_ACTIONS:-}" ]; then echo "::group::$*"; else echo "== $* =="; fi; }
endgroup() { if [ -n "${GITHUB_ACTIONS:-}" ]; then echo "::endgroup::"; fi; }

while [ "$#" -gt 0 ]; do
  case "$1" in
    --only-testing) [ "$#" -ge 2 ] || die "--only-testing needs a test identifier"
                    only_testing+=("-only-testing:$2"); shift 2 ;;
    --skip-bootstrap) skip_bootstrap=1; shift ;;
    --device) [ "$#" -ge 2 ] || die "--device needs a name"; device_name="$2"; shift 2 ;;
    --out) [ "$#" -ge 2 ] || die "--out needs a directory"; qa="$2"; shift 2 ;;
    -h|--help) sed -n '2,15p' "${BASH_SOURCE[0]}"; exit 0 ;;
    *) die "unknown argument: $1 (see --help)" ;;
  esac
done

# ---------------------------------------------------------------- 1. tools
[ "$(uname -s)" = "Darwin" ] || die "macOS only (needs Xcode and the iOS Simulator)"
need() { command -v "$1" >/dev/null 2>&1 || die "$1 not found. $2"; }
need xcodebuild "Install Xcode 26 or newer and run: sudo xcode-select -s /Applications/Xcode.app/Contents/Developer"
need xcrun "Install the Xcode command line tools: xcode-select --install"
need xcodegen "Install it with: brew install xcodegen"
need python3 "Install the command line tools (xcode-select --install) or: brew install python"
if [ "$skip_bootstrap" -eq 0 ]; then
  need rustup "Install Rust from https://rustup.rs"
  need cargo "Install Rust from https://rustup.rs"
  # Inside core/: its rust-toolchain.toml decides which toolchain the targets belong to.
  installed_targets="$(cd "$shell/../core" && rustup target list --installed)"
  for t in aarch64-apple-ios aarch64-apple-ios-sim aarch64-apple-darwin; do
    grep -qx "$t" <<<"$installed_targets" || die "Rust target $t missing. Run: rustup target add $t"
  done
fi
xcode_major="$(xcodebuild -version | sed -n 's/^Xcode \([0-9]*\).*/\1/p' | head -n 1)"
if [ -z "$xcode_major" ] || [ "$xcode_major" -lt 26 ]; then
  die "Xcode 26 or newer is required (Package.swift declares iOS 26); found: $(xcodebuild -version | head -n 1). Select one with xcode-select -s or DEVELOPER_DIR."
fi

rm -rf "$qa"
mkdir -p "$qa"
group "environment"
sw_vers
xcodebuild -version
xcrun --show-sdk-version --sdk iphonesimulator || true
xcrun simctl list runtimes
endgroup

# ---------------------------------------------------------------- 2. bootstrap + project
if [ "$skip_bootstrap" -eq 0 ]; then
  group "bootstrap (Rust core, bindings, xcframework)"
  "$here/bootstrap.sh"
  endgroup
fi
[ -d "$shell/Frameworks/JarvisCoreFFI.xcframework" ] \
  || die "Shell/Frameworks/JarvisCoreFFI.xcframework is missing; run without --skip-bootstrap"
group "xcodegen"
(cd "$shell" && xcodegen generate)
endgroup
[ -d "$shell/Jarvis.xcodeproj" ] || die "xcodegen did not produce Shell/Jarvis.xcodeproj"

# ---------------------------------------------------------------- 3. simulator
# Picks at runtime instead of hard-coding a name or UDID: runner images change their
# simulator set between releases.
group "simulator selection"
xcrun simctl list devices available --json > "$qa/simctl-devices.json"
xcrun simctl list devicetypes --json > "$qa/simctl-devicetypes.json"
xcrun simctl list runtimes --json > "$qa/simctl-runtimes.json"

pick="$(python3 -I - "$qa/simctl-devices.json" "$device_name" <<'PY'
import json, re, sys

devices_file, wanted = sys.argv[1], sys.argv[2].strip().lower()
data = json.load(open(devices_file))["devices"]

def version(key):
    m = re.search(r"SimRuntime\.iOS-(\d+)-(\d+)(?:-(\d+))?$", key)
    return tuple(int(x or 0) for x in m.groups()) if m else None

rows = []
for key, devs in data.items():
    v = version(key)
    if v is None:
        continue
    for d in devs:
        if d.get("isAvailable", True):
            rows.append((v, d["name"], d["udid"], d.get("state", "?")))
rows.sort(key=lambda r: (r[0], r[1]), reverse=True)
sys.stderr.write("available iOS simulator devices (newest runtime first):\n")
for v, name, udid, state in rows:
    sys.stderr.write("  iOS %-8s %-34s %s  %s\n" % (".".join(map(str, v)), name, udid, state))

def tier(name):
    if name == "iPhone 17 Pro Max":
        return 0
    if name.startswith("iPhone") and "Pro Max" in name:
        return 1
    return 2 if name.startswith("iPhone") else 9

def model_number(name):
    m = re.match(r"iPhone (\d+)", name)
    return int(m.group(1)) if m else 0

cands = []
for v, name, udid, state in rows:
    if v < (26, 0):  # Package.swift declares .iOS("26.0")
        continue
    if wanted:
        if name.lower() == wanted:
            cands.append(((0, tuple(-x for x in v), 0), v, name, udid))
    elif tier(name) < 9:
        cands.append(((tier(name), tuple(-x for x in v), -model_number(name)), v, name, udid))
if not cands:
    sys.exit(4)
cands.sort()
_, v, name, udid = cands[0]
print("%s\t%s\t%s" % (udid, name, ".".join(map(str, v))))
PY
)" || pick_rc=$?
pick_rc="${pick_rc:-0}"

if [ "$pick_rc" -ne 0 ] || [ -z "$pick" ]; then
  log "no matching iOS 26+ simulator device is installed; trying to create one"
  create="$(python3 -I - "$qa/simctl-devicetypes.json" "$qa/simctl-runtimes.json" "$device_name" <<'PY'
import json, re, sys

types = json.load(open(sys.argv[1]))["devicetypes"]
runtimes = json.load(open(sys.argv[2]))["runtimes"]
wanted = sys.argv[3].strip().lower()

def version(r):
    return tuple(int(x) for x in re.findall(r"\d+", r.get("version", "0")))

ios = [r for r in runtimes
       if r.get("isAvailable", True) and r.get("identifier", "").find("SimRuntime.iOS-") >= 0
       and version(r) >= (26, 0)]
if not ios:
    sys.exit(4)
runtime = max(ios, key=version)
iphones = [t for t in types if t["name"].startswith("iPhone")]

def rank(t):
    name = t["name"]
    if wanted:
        return (0 if name.lower() == wanted else 9, 0)
    if name == "iPhone 17 Pro Max":
        return (0, 0)
    n = re.match(r"iPhone (\d+)", name)
    return (1 if "Pro Max" in name else 2, -(int(n.group(1)) if n else 0))

best = min(iphones, key=rank, default=None)
if best is None or (wanted and rank(best)[0] != 0):
    sys.exit(4)
print("%s\t%s\t%s" % (best["identifier"], runtime["identifier"], best["name"]))
PY
)" || die "no iOS 26+ runtime / iPhone device type available. Install the iOS 26 simulator runtime (Xcode > Settings > Components); see $qa/simctl-*.json"
  type_id="$(cut -f1 <<<"$create")"
  runtime_id="$(cut -f2 <<<"$create")"
  name="$(cut -f3 <<<"$create")"
  udid="$(xcrun simctl create "Jarvis QA $name" "$type_id" "$runtime_id")"
  ios_version="$(sed -n 's/.*SimRuntime\.iOS-\([0-9-]*\)$/\1/p' <<<"$runtime_id" | tr - .)"
else
  udid="$(cut -f1 <<<"$pick")"
  name="$(cut -f2 <<<"$pick")"
  ios_version="$(cut -f3 <<<"$pick")"
fi
log "picked: $name (iOS $ios_version) $udid"
echo "$name|$ios_version|$udid" > "$qa/device.txt"
endgroup

group "boot simulator"
# The UI tests type through the software keyboard; with a hardware keyboard "connected"
# the simulator hides it and typing can fail. Set before the Simulator app starts.
defaults write com.apple.iphonesimulator ConnectHardwareKeyboard -bool false || true
# bootstatus -b boots the device if needed and returns once it is usable; the watchdog
# stops a hung boot from eating the whole job timeout.
boot_timeout=420
( xcrun simctl bootstatus "$udid" -b ) &
boot_pid=$!
( sleep "$boot_timeout"; kill "$boot_pid" 2>/dev/null ) &
watchdog_pid=$!
boot_rc=0
wait "$boot_pid" || boot_rc=$?
kill "$watchdog_pid" 2>/dev/null || true
wait "$watchdog_pid" 2>/dev/null || true
[ "$boot_rc" -eq 0 ] || die "simulator $udid did not finish booting within ${boot_timeout}s (exit $boot_rc)"
xcrun simctl list devices | grep -F "$udid" || true
endgroup

# ---------------------------------------------------------------- 4. build + test
# Pipes through xcbeautify only when it is installed; the raw log is always kept.
run_xcodebuild() { # run_xcodebuild LOGFILE args...
  local logfile="$1"; shift
  local rc=0
  if command -v xcbeautify >/dev/null 2>&1; then
    local beautify=(xcbeautify)
    if [ -n "${GITHUB_ACTIONS:-}" ]; then beautify+=(--renderer github-actions); fi
    xcodebuild "$@" 2>&1 | tee "$logfile" | "${beautify[@]}" || rc=$?
  else
    xcodebuild "$@" 2>&1 | tee "$logfile" || rc=$?
  fi
  return "$rc"
}

common=(
  -project "$shell/Jarvis.xcodeproj"
  -scheme Jarvis
  -destination "platform=iOS Simulator,id=$udid"
  -derivedDataPath "$derived"
  CODE_SIGNING_ALLOWED=NO
)
result="$qa/Jarvis.xcresult"
build_result="$qa/Jarvis-build.xcresult"
rm -rf "$result" "$build_result"

group "xcodebuild build-for-testing"
build_rc=0
run_xcodebuild "$qa/xcodebuild-build.log" build-for-testing "${common[@]}" \
  -resultBundlePath "$build_result" || build_rc=$?
endgroup
write_failure_summary() {
  {
    echo "### iPhone Simulator QA: build failed"
    echo
    echo "Device: $name, iOS $ios_version"
    echo
    echo '```'
    grep -E "error:|\*\* BUILD FAILED|Command .* failed" "$qa/xcodebuild-build.log" | head -n 40 || true
    echo '```'
  } > "$qa/summary.md"
  cat "$qa/summary.md"
  if [ -n "${GITHUB_STEP_SUMMARY:-}" ]; then cat "$qa/summary.md" >> "$GITHUB_STEP_SUMMARY"; fi
}
if [ "$build_rc" -ne 0 ]; then
  log "build-for-testing failed (exit $build_rc)"
  write_failure_summary
  exit "$build_rc"
fi

group "xcodebuild test-without-building"
test_rc=0
run_xcodebuild "$qa/xcodebuild-test.log" test-without-building "${common[@]}" \
  -resultBundlePath "$result" \
  -parallel-testing-enabled NO \
  ${only_testing[@]+"${only_testing[@]}"} || test_rc=$?
endgroup
log "test-without-building exit code: $test_rc"

# ---------------------------------------------------------------- 5. export (always)
# Best-effort from here on: a failed export must not hide the test result.
xcrun simctl io "$udid" screenshot "$qa/final-screen.png" >/dev/null 2>&1 || true

if [ -d "$result" ]; then
  group "export attachments and test results"
  # Syntax per `xcresulttool` man page (Xcode 16+/26): export attachments --path --output-path.
  mkdir -p "$qa/attachments"
  if ! xcrun xcresulttool export attachments --path "$result" --output-path "$qa/attachments"; then
    log "WARNING: 'xcresulttool export attachments' failed; the .xcresult itself is still kept"
  fi
  xcrun xcresulttool get test-results summary --path "$result" --compact > "$qa/summary.json" \
    || { log "WARNING: 'xcresulttool get test-results summary' failed"; rm -f "$qa/summary.json"; }
  xcrun xcresulttool get test-results tests --path "$result" --compact > "$qa/tests.json" \
    || { log "WARNING: 'xcresulttool get test-results tests' failed"; rm -f "$qa/tests.json"; }
  # Attachment files are exported under opaque names; add a copy under readable names.
  python3 -I - "$qa/attachments" <<'PY' || log "WARNING: could not build the readable attachment index"
import json, os, re, shutil, sys

root = sys.argv[1]
manifest = os.path.join(root, "manifest.json")
if not os.path.exists(manifest):
    sys.exit(0)
for entry in json.load(open(manifest)):
    test = re.sub(r"[^A-Za-z0-9._-]+", "_", str(entry.get("testIdentifier", "unknown")))
    for att in entry.get("attachments", []):
        src = os.path.join(root, att.get("exportedFileName", ""))
        if not os.path.isfile(src):
            continue
        human = re.sub(r"[^A-Za-z0-9._-]+", "_", att.get("suggestedHumanReadableName") or att["exportedFileName"])
        dest_dir = os.path.join(root, "by-test", test)
        os.makedirs(dest_dir, exist_ok=True)
        shutil.copyfile(src, os.path.join(dest_dir, human))
PY
  endgroup
else
  log "WARNING: no result bundle at $result (the test run did not get far enough)"
fi

# ---------------------------------------------------------------- 6. summary
group "test summary"
python3 -I - "$qa" "$name" "$ios_version" "$test_rc" <<'PY' | tee "$qa/summary.md"
import json, os, sys

qa, device, ios, rc = sys.argv[1], sys.argv[2], sys.argv[3], int(sys.argv[4])

def load(name):
    try:
        return json.load(open(os.path.join(qa, name)))
    except Exception:
        return None

summary, tests = load("summary.json"), load("tests.json")
cases = []  # (identifier, result, [failure messages])

def walk(node, path):
    kind, label = node.get("nodeType", ""), node.get("name", "")
    if kind == "Test Case":
        msgs = []
        def collect(n):
            for c in n.get("children", []):
                if c.get("nodeType") == "Failure Message":
                    msgs.append(c.get("name", ""))
                collect(c)
        collect(node)
        cases.append(("/".join(path + [label]), node.get("result", "Unknown"), msgs))
        return
    nxt = path if kind in ("Test Plan", "Test Target") else path + [label]
    for c in node.get("children", []):
        walk(c, nxt)

if tests:
    for n in tests.get("testNodes", []):
        walk(n, [])

passed = sum(1 for c in cases if c[1] == "Passed")
failed = sum(1 for c in cases if c[1] == "Failed")
skipped = sum(1 for c in cases if c[1] == "Skipped")
total = len(cases)
if not cases and summary:
    passed, failed = summary.get("passedTests", 0), summary.get("failedTests", 0)
    skipped, total = summary.get("skippedTests", 0), summary.get("totalTestCount", 0)

failures = [(i, m) for i, r, m in cases if r == "Failed"]
if not failures and summary:
    failures = [(f.get("testIdentifierString") or f.get("testName", "?"), [f.get("failureText", "")])
                for f in summary.get("testFailures", [])]

have_results = bool(cases) or bool(summary)
verdict = "PASSED" if rc == 0 else "FAILED"
out = ["### iPhone Simulator QA: %s" % verdict, "",
       "- Device: %s, iOS %s" % (device, ios)]
if have_results:
    out.append("- Tests: %d total, %d passed, %d failed, %d skipped" % (total, passed, failed, skipped))
else:
    out.append("- Tests: no readable result bundle (see xcodebuild-test.log)")
if rc != 0 and not failures:
    out.append("- xcodebuild exited %d without per-test failures (build/launch problem; see xcodebuild-test.log)" % rc)
if failures:
    out += ["", "Failed tests:", ""]
    for ident, msgs in failures:
        out.append("- `%s`" % ident)
        for m in msgs:
            if m:
                out.append("  - %s" % " ".join(str(m).split()))
print("\n".join(out))

if os.environ.get("GITHUB_STEP_SUMMARY"):
    with open(os.environ["GITHUB_STEP_SUMMARY"], "a") as f:
        f.write("\n".join(out) + "\n")
if os.environ.get("GITHUB_ACTIONS"):
    for ident, msgs in failures:
        text = " | ".join(" ".join(str(m).split()) for m in msgs if m) or "failed"
        sys.stderr.write("::error title=UI test failed::%s: %s\n" % (ident, text.replace("\n", "%0A")))
PY
endgroup

log "artifacts in $qa"
ls -la "$qa" >&2 || true
exit "$test_rc"
