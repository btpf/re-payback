#!/bin/sh
# Plays the six story levels with the autoplay bot and prints each result:
# the bot's RESULT line, any panic, and the mission-complete / failed counts.
# usage: tools/checks/autoplay_all.sh [exe] [levels...]
#   exe     the game (default $PAYBACK_EXE, else target/debug/payback(.exe))
#   levels  "map level" pairs as m.l (default: 0.0 0.1 1.0 1.1 2.0 2.1)
#   e.g. tools/checks/autoplay_all.sh target/lead/debug/payback.exe 0.0
# env: AUTOPLAY_FAST (default 20), AUTOPLAY_SEED (1234), AUTOPLAY_TIMEOUT (1500 s)
# Logs: out/autoplay/l<map><level>.log. Exit status 1 if a level panicked or
# reported no RESULT.
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
exe=${1:-${PAYBACK_EXE:-}}
[ $# -gt 0 ] && shift
if [ -z "$exe" ]; then
    exe=$ROOT/target/debug/payback.exe
    [ -f "$exe" ] || exe=$ROOT/target/debug/payback
fi
case $exe in /*|?:*) ;; *) exe=$PWD/$exe ;; esac
[ -f "$exe" ] || { echo "no game at $exe (build it first)" >&2; exit 2; }
levels=${*:-0.0 0.1 1.0 1.1 2.0 2.1}
out=$ROOT/out/autoplay
mkdir -p "$out"
cd "$ROOT" || exit 2
status=0
for ml in $levels; do
    m=${ml%.*}; l=${ml#*.}
    log=$out/l$m$l.log
    timeout "${AUTOPLAY_TIMEOUT:-1500}" "$exe" --map "$m" --level "$l" --autoplay \
        --fast "${AUTOPLAY_FAST:-20}" --seed "${AUTOPLAY_SEED:-1234}"         --window-mode windowed --resolution 1280x800 > "$log" 2>&1
    echo "level $m.$l:"
    grep -E "RESULT|panicked" "$log" | sed 's/.*autoplay: //' | cut -c1-200
    echo "   missions: $(grep -cE 'message 2022' "$log") complete, $(grep -cE 'message 2028' "$log") failed"
    if grep -q panicked "$log" || ! grep -q RESULT "$log"; then status=1; fi
done
exit $status
