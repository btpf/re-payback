#!/bin/sh
# Limits how many cargo builds run at once when several agents (or shells)
# build in parallel, to keep RAM in check. Waits for a free slot, runs the
# command, frees the slot (also when the command fails or is interrupted).
# usage: tools/dev/buildgate.sh <command...>
#   e.g. CARGO_TARGET_DIR=target/lead tools/dev/buildgate.sh cargo build -p payback -j 8
# env: BUILDGATE_SLOTS  concurrent builds (default 3)
#      BUILDGATE_DIR    slot directory (default target/buildgate)
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
SLOTS=${BUILDGATE_SLOTS:-3}
DIR=${BUILDGATE_DIR:-$ROOT/target/buildgate}
mkdir -p "$DIR"
slot=""
while [ -z "$slot" ]; do
    i=0
    while [ $i -lt $SLOTS ]; do
        if mkdir "$DIR/slot$i" 2>/dev/null; then
            echo $$ > "$DIR/slot$i/pid"
            slot=$i
            break
        fi
        # Reclaim a slot whose owner died.
        p=$(cat "$DIR/slot$i/pid" 2>/dev/null)
        if [ -n "$p" ] && ! kill -0 "$p" 2>/dev/null; then
            rm -rf "$DIR/slot$i"
        fi
        i=$((i + 1))
    done
    [ -z "$slot" ] && sleep 5
done
trap 'rm -rf "$DIR/slot$slot"' EXIT INT TERM
"$@"
