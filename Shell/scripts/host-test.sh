#!/usr/bin/env bash
# Builds and tests the Swift package on this host (Linux or macOS). When the test run fails
# or the test process crashes, it reruns every suite on its own and, on macOS, prints the
# newest crash report, so the job log names the suite and the crashing frames. A crash of the
# whole runner otherwise leaves only "exited with unexpected signal code 6".
#
#   scripts/host-test.sh
#
# JARVIS_HOST_TEST_DIAGNOSE=1 runs the diagnosis even when the tests pass (to check it works).
# Written for bash 3.2 (the macOS system bash).
set -uo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$(dirname "$here")" || exit 1

swift --version 2>&1 | head -n 2
swift build || exit 1

log="$(mktemp)"
swift test 2>&1 | tee "$log"
rc=${PIPESTATUS[0]}
if [ "$rc" = 0 ] && [ "${JARVIS_HOST_TEST_DIAGNOSE:-0}" != 1 ]; then
  rm -f "$log"
  exit 0
fi

echo
echo "::group::per-suite rerun (the whole run exited $rc)"
swift --version 2>&1 | head -n 2
swift test list --skip-build 2>/dev/null \
  | sed -n 's/^\(JarvisKitTests\.[A-Za-z0-9_]*\).*/\1/p' | sort -u > "$log.suites"
echo "suites: $(wc -l < "$log.suites" | tr -d ' ')"
while IFS= read -r suite; do
  out="$(swift test --skip-build --filter "^${suite}[./]" 2>&1)"
  status=$?
  if [ "$status" = 0 ]; then
    echo "ok      $suite"
  else
    echo "FAILED  $suite (exit $status)"
    echo "$out" | grep -E "✘|error:|freed pointer|Fatal error|signal|Time limit" | head -n 12 | sed 's/^/        /'
    echo "$suite" >> "$log.failed"
  fi
done < "$log.suites"
# Each test of a failing suite on its own: names the tests that fail or crash alone.
if [ -s "$log.failed" ]; then
  swift test list --skip-build 2>/dev/null > "$log.tests"
  while IFS= read -r suite; do
    grep -F "$suite/" "$log.tests" | while IFS= read -r test; do
      # No "$" at the end: the test's full identifier carries more after the name.
      pattern="^$(printf '%s' "$test" | sed 's/[][().*+?^$|\\]/\\&/g')"
      if swift test --skip-build --filter "$pattern" 2>&1 | grep -Eq "Test run with [1-9][0-9]* tests? .*passed"; then
        echo "  ok      $test"
      else
        echo "  FAILED  $test"
      fi
    done
  done < "$log.failed"
fi
echo "::endgroup::"

if [ "$(uname -s)" = Darwin ]; then
  # Crash reports are written asynchronously after the process dies.
  sleep 10
  reports="$HOME/Library/Logs/DiagnosticReports"
  newest="$(ls -t "$reports"/*.ips 2>/dev/null | head -n 1)"
  if [ -n "$newest" ]; then
    echo "::group::crash report $(basename "$newest")"
    bundle="$(find .build -path '*JarvisKitPackageTests.xctest/Contents/MacOS/JarvisKitPackageTests' -type f 2>/dev/null | head -n 1)"
    python3 -I - "$newest" "$bundle" <<'PY' | { xcrun swift-demangle 2>/dev/null || cat; }
import json, subprocess, sys

path = sys.argv[1]
text = open(path, errors="replace").read()
try:
    header, _, body = text.partition("\n")
    head, data = json.loads(header), json.loads(body)
    print("process:", head.get("app_name") or data.get("procName"), "| os:", head.get("os_version"))
    print("exception:", json.dumps(data.get("exception", {})))
    print("termination:", json.dumps(data.get("termination", {})))
    print("application-specific:", json.dumps(data.get("asi", {})))
    images = data.get("usedImages", [])
    for thread in data.get("threads", []):
        if not thread.get("triggered"):
            continue
        print("crashed thread:", thread.get("name") or thread.get("queue") or thread.get("id"))
        for i, frame in enumerate(thread.get("frames", [])[:60]):
            index = frame.get("imageIndex", -1)
            image = images[index].get("name", "?") if 0 <= index < len(images) else "?"
            print("%3d %-28s %s + %s  [image offset 0x%x]" % (
                i, image, frame.get("symbol", "?"), frame.get("symbolLocation", 0), frame.get("imageOffset", 0)))
        # The code before the crashing call in our own frames: which await it came back from.
        own = [f for f in thread.get("frames", [])[:12]
               if 0 <= f.get("imageIndex", -1) < len(images)
               and "JarvisKit" in images[f["imageIndex"]].get("name", "")][:2]
        for frame in own:
            # The report hides the user's path; the script passes the bundle it built.
            binary = sys.argv[2] if len(sys.argv) > 2 and sys.argv[2] else images[frame["imageIndex"]].get("path", "")
            at = frame.get("imageOffset", 0)
            print("--- code before image offset 0x%x in %s" % (at, binary))
            try:
                names = subprocess.run(["xcrun", "nm", "-n", binary], capture_output=True, text=True, timeout=120).stdout
                best = None
                for line in names.splitlines():
                    parts = line.split()
                    if len(parts) >= 3:
                        try:
                            address = int(parts[0], 16)
                        except ValueError:
                            continue
                        if address <= at:
                            best = (address, parts[-1])
                        else:
                            break
                print("containing symbol:", best[1] if best else "?", "(+%d)" % (at - best[0]) if best else "")
                start = max(best[0] if best else at - 400, at - 400)
                code = subprocess.run(["xcrun", "llvm-objdump", "-d", "--no-show-raw-insn",
                                       "--start-address=0x%x" % start, "--stop-address=0x%x" % (at + 8), binary],
                                      capture_output=True, text=True, timeout=120).stdout
                print("\n".join(code.splitlines()[-110:]))
            except Exception as error:
                print("could not disassemble:", error)
except Exception as error:  # an unknown report layout: show the start of it as it is
    print("could not parse the report (%s); first 120 lines:" % error)
    print("\n".join(text.splitlines()[:120]))
PY
    echo "::endgroup::"
  else
    echo "no crash report found in $reports"
  fi
fi

rm -f "$log" "$log.suites" "$log.failed" "$log.tests"
exit "$rc"
