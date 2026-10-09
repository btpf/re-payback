#!/bin/bash
# User-map smoke test: every map in the Mac and Amiga UserMaps folders is
# loaded in Rampage, screenshotted, and run for N ticks (scripted input,
# recorded), from a scratch working directory (its own Saves/).
# usage: tools/checks/usermaps_smoke.sh [exe] [ticks] [jobs] [name-filter]
#   exe defaults to $PAYBACK_EXE, else target/debug/payback(.exe)
# Output: out/usermaps/{shots/*.png, logs/*.log, rec/, summary.tsv}; the
# working directory is scratch/usermaps_work. Contact sheet of the shots:
#   python tools/checks/contact_sheet.py out/usermaps/shots out/usermaps/sheet.png
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
DEFAULT_EXE=$ROOT/target/debug/payback.exe
[ -f "$DEFAULT_EXE" ] || DEFAULT_EXE=$ROOT/target/debug/payback
EXE=${1:-${PAYBACK_EXE:-$DEFAULT_EXE}}
case $EXE in /*|?:*) ;; *) EXE=$PWD/$EXE ;; esac
TICKS=${2:-2000}
JOBS=${3:-4}
FILTER=${4:-}
OUT="$ROOT/out/usermaps"
WORK="$ROOT/scratch/usermaps_work"
DATA="$ROOT/extracted/game"
MAC="$ROOT/extracted/mac/Payback"
mkdir -p "$OUT/shots" "$OUT/logs" "$OUT/rec" "$WORK"
export RUST_BACKTRACE=1

# name<TAB>map path without .map (absolute for the Mac maps, relative to
# the game data for the Amiga ones, as the user-map list hands them out).
list() {
    for f in "$DATA"/UserMaps/*.map; do
        b=$(basename "$f" .map); echo -e "amiga:$b\tUserMaps/$b"
    done
    for f in "$MAC"/UserMaps/*.map; do
        b=$(basename "$f" .map); echo -e "mac:$b\t$MAC/UserMaps/$b"
    done
}

one() {
    tag=$1; map=$2
    safe=$(echo "$tag" | tr ' :' '__')
    cd "$WORK"
    common=(--data "$DATA" --mac-data "$MAC" --map "$map" --rampage 3 --seed 1234 --resolution 640x480 --window-mode windowed)
    timeout 120 "$EXE" "${common[@]}" --screenshot "$OUT/shots/$safe.png" > "$OUT/logs/$safe.shot.log" 2>&1
    s1=$?
    timeout 600 "$EXE" "${common[@]}" --record "$OUT/rec/$safe.rep" --script-input --ticks "$TICKS" --fast 8 > "$OUT/logs/$safe.run.log" 2>&1
    s2=$?
    log="$OUT/logs/$safe.run.log"
    last=$(grep "replay hash" "$log" | tail -1 | sed 's/.*replay hash: //')
    route=$(grep -o "route: [0-9]* route nodes\|[0-9]* route nodes" "$log" | head -1)
    warn=$(cat "$OUT/logs/$safe.shot.log" "$log" | sed -E 's/\x1b\[[0-9;]*m//g' | grep -E " (WARN|ERROR) " | grep -v "wgpu\|naga\|bevy_render\|gilrs\|winit" | sed -E 's/.*(WARN|ERROR) //' | sort | uniq -c | sort -rn | head -5 | tr '\n' ';')
    panic=$(cat "$OUT/logs/$safe.shot.log" "$log" | grep -m1 "panicked" )
    echo -e "$tag\tshot=$s1\trun=$s2\t$last\t$route\t$panic\t$warn"
}
: > "$OUT/summary.tsv"
while IFS=$'	' read -r t m; do
    while [ "$(jobs -rp | wc -l)" -ge "$JOBS" ]; do wait -n; done
    one "$t" "$m" >> "$OUT/summary.tsv" &
done < <(list | grep -i -- "$FILTER")
wait
sort "$OUT/summary.tsv" -o "$OUT/summary.tsv"
cat "$OUT/summary.tsv"
