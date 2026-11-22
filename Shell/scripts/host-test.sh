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

echo "::group::per-suite rerun (the whole run exited $rc)"
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
  fi
done < "$log.suites"
echo "::endgroup::"

if [ "$(uname -s)" = Darwin ]; then
  # Crash reports are written asynchronously after the process dies.
  sleep 10
  reports="$HOME/Library/Logs/DiagnosticReports"
  newest="$(ls -t "$reports"/*.ips 2>/dev/null | head -n 1)"
  if [ -n "$newest" ]; then
    echo "::group::crash report $(basename "$newest")"
    python3 -I - "$newest" <<'PY' | { xcrun swift-demangle 2>/dev/null || cat; }
import json, sys

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
            print("%3d %-28s %s + %s" % (i, image, frame.get("symbol", "0x%x" % frame.get("imageOffset", 0)),
                                         frame.get("symbolLocation", 0)))
except Exception as error:  # an unknown report layout: show the start of it as it is
    print("could not parse the report (%s); first 120 lines:" % error)
    print("\n".join(text.splitlines()[:120]))
PY
    echo "::endgroup::"
  else
    echo "no crash report found in $reports"
  fi
fi

rm -f "$log" "$log.suites"
exit "$rc"
