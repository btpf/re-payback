#!/bin/sh
# Replay determinism check: record a scripted run, play it back, compare the
# world hashes logged every 50 ticks (`replay.rs`). Compares the port with
# itself only; see tools/checks/replay_check.py (same check, cross-platform, with
# the standard --suite of story, rampage, multiplayer and cheat cases).
#
# usage: tools/checks/replay_check.sh <name> <ticks> [record args...]
#   e.g. tools/checks/replay_check.sh story 1000 --map 0 --seed 1234
# env:   PAYBACK_EXE   the game (default target/debug/payback[.exe])
#        REPLAY_OUT    output directory (default target/replay_check)
# Exit status: 0 MATCH, 1 DIFFER, 2 a run failed.
cd "$(dirname "$0")/../.." || exit 2
if [ -z "$PAYBACK_EXE" ]; then
    PAYBACK_EXE=target/debug/payback.exe
    [ -f "$PAYBACK_EXE" ] || PAYBACK_EXE=target/debug/payback
fi
D=${REPLAY_OUT:-target/replay_check}
[ $# -ge 2 ] || { echo "usage: $0 <name> <ticks> [record args...]"; exit 2; }
[ -f "$PAYBACK_EXE" ] || { echo "$PAYBACK_EXE: not found"; exit 2; }
mkdir -p "$D"
name=$1; ticks=$2; shift 2
"$PAYBACK_EXE" --record "$D/$name" --script-input --ticks "$ticks" "$@" 2>&1 \
    | grep "replay hash" | sed 's/.*hash: //' > "$D/$name.rec.log"
"$PAYBACK_EXE" --play "$D/$name" --ticks "$ticks" 2>&1 \
    | grep "replay hash" | sed 's/.*hash: //' > "$D/$name.play.log"
if [ ! -s "$D/$name.rec.log" ]; then
    echo "$name: FAILED (no hashes logged)"
    exit 2
fi
if cmp -s "$D/$name.rec.log" "$D/$name.play.log"; then
    echo "$name: MATCH ($(wc -l < "$D/$name.rec.log") checkpoints, last: $(tail -1 "$D/$name.rec.log"))"
else
    echo "$name: DIFFER"
    diff "$D/$name.rec.log" "$D/$name.play.log" | head -6
    exit 1
fi
