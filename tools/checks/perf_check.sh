#!/bin/sh
# Automatic performance check (`perf.rs`, `--perf-check`): plays a level
# with the autoplay bot for a while and fails when the frame times exceed
# the limits after a 5 s warm-up (frame p95 / max, main-app CPU time p95).
# Catches regressions such as a system rebuilding something every tick.
#
# usage: tools/checks/perf_check.sh [seconds] [game args...]
#   e.g. tools/checks/perf_check.sh 30 --map 0
#        tools/checks/perf_check.sh 30 --map 2 --level 1 --hardware
# env:   PAYBACK_EXE   the game (default target/release/payback[.exe], else
#                      target/debug/...; debug builds need looser limits)
#        PAYBACK_PERF_MAX_P95, PAYBACK_PERF_MAX_FRAME, PAYBACK_PERF_MAX_MAIN
#                      limits in ms (defaults 25, 150, 16)
# Exit status: 0 PASS, 1 FAIL, 2 the game did not run.
cd "$(dirname "$0")/../.." || exit 2
if [ -z "$PAYBACK_EXE" ]; then
    for p in target/release/payback.exe target/release/payback target/debug/payback.exe target/debug/payback; do
        [ -f "$p" ] && { PAYBACK_EXE=$p; break; }
    done
fi
[ -f "$PAYBACK_EXE" ] || { echo "no game binary (set PAYBACK_EXE)"; exit 2; }
secs=${1:-30}
[ $# -gt 0 ] && shift
[ $# -gt 0 ] || set -- --map 0
out=$("$PAYBACK_EXE" --perf-check "$secs" --autoplay --window-mode windowed --resolution 1280x800 "$@" 2>&1)
status=$?
echo "$out" | grep -E "perf-check|perf whole run" | sed 's/^.*\(perf\)/\1/'
echo "$out" | grep -q "perf-check: PASS" && exit 0
echo "$out" | grep -q "perf-check: FAIL" && exit 1
echo "the game exited ($status) without a verdict"
exit 2
