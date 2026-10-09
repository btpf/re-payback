#!/bin/sh
# Regenerates every Ghidra export in analysis/ (git-ignored) from your own
# extracted game files. One command for all builds; see docs/decompiling.md.
#
# usage: tools/ghidra/decompile_all.sh [--out DIR] [--jobs N] [TARGET ...]
#   TARGET   68k os4 macx mac9 mapedx scripts (default: all of them)
#   --out    output root instead of analysis/ (e.g. to diff a re-run against
#            the current exports); it must be inside the repository
#   --jobs   how many Ghidra runs at once (default 2; each wants ~4 GB RAM)
#
# Needs: Docker with the workbench image (`docker build -t payback-re .` from
# the repository root; Ghidra 12.1.4), python 3, and the extracted data:
#   extracted/game/Payback.exe, extracted/game/Maps/NN.script   tools/extract/extract.py
#   extracted/os4/PaybackUpdateOS4/install_files/PaybackOS4      tools/extract/extract.py
#   extracted/mac/Payback/{Payback (OS X),Payback (OS 9),MapEditor (OS X)}
#                                                               tools/extract/extract_mac.py
#
# Writes, per build, <out>/ghidra_<build>/{decompiled.c,functions.tsv,strings.tsv}
# (+ jumptables.tsv for the Mac builds) and <out>/ghidra_<build>.log:
#   68k     Update 7 Payback.exe, flattened at 0x10000 by tools/extract/hunk.py
#           (<out>/flat/Payback68k.bin + .map), functions seeded by SeedFlat68k.java
#   scripts the six Maps/NN.script mission scripts, flattened the same way
#           (ghidra_script00 .. ghidra_script21)
#   os4     the AmigaOS 4 ELF (best Amiga decompiler output), plus
#           long_timeout.c (FUN_0105f594 with a long timeout; see docs/decompiling.md)
#   macx    Mac OS X PEF; original C++ names. Language forced to 32-bit PowerPC
#           (the PEF loader guesses VLE); MacJumpTables.java recovers the
#           CodeWarrior switch tables before the export (docs/re/mac_version.md)
#   mac9    Mac OS 9 PEF, same recipe
#   mapedx  the Mac OS X map editor, same recipe
# Each build takes about 5-10 minutes. Re-running overwrites the old export.
set -e
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
cd "$ROOT"

out=analysis
jobs=2
targets=""
while [ $# -gt 0 ]; do
    case $1 in
        --out) out=$2; shift ;;
        --jobs) jobs=$2; shift ;;
        68k|os4|macx|mac9|mapedx|scripts) targets="$targets $1" ;;
        -h|--help) sed -n '2,30p' "$0"; exit 0 ;;
        *) echo "unknown argument $1 (see --help)" >&2; exit 2 ;;
    esac
    shift
done
[ -n "$targets" ] || targets="68k scripts os4 macx mac9 mapedx"
out=${out%/}
case $out in /*|?:*) echo "--out must be a path relative to the repository root" >&2; exit 2 ;; esac

IMAGE=payback-re
docker image inspect $IMAGE > /dev/null 2>&1 || {
    echo "Docker image $IMAGE not found: run 'docker build -t $IMAGE .' in $ROOT" >&2; exit 1; }

# Docker on Windows (Git Bash) wants a Windows host path, and MSYS must not
# rewrite the container paths.
HOST_ROOT=$ROOT
command -v cygpath > /dev/null 2>&1 && HOST_ROOT=$(cygpath -m "$ROOT")
export MSYS_NO_PATHCONV=1

need() { [ -f "$1" ] || { echo "missing input $1 (run the extract tools first)" >&2; exit 1; }; }

# ghidra NAME INPUT [analyzeHeadless options...]: import INPUT, run the scripts,
# export to $out/ghidra_NAME. Runs in the background; `wait` collects it.
running=0
ghidra() {
    name=$1; input=$2; shift 2
    if [ $running -ge $jobs ]; then wait; running=0; fi
    mkdir -p "$out/ghidra_$name"
    echo "ghidra_$name: analysing $input (log: $out/ghidra_$name.log)"
    docker run --rm -v "$HOST_ROOT:/work/impl" $IMAGE sh -c "mkdir -p /tmp/proj && \
        analyzeHeadless /tmp/proj p_$name -import '/work/impl/$input' $* \
        -scriptPath /work/impl/tools/ghidra \
        -postScript ExportAll.java /work/impl/$out/ghidra_$name" \
        > "$out/ghidra_$name.log" 2>&1 &
    running=$((running + 1))
}

FLAT68K="-loader BinaryLoader -loader-baseAddr 0x10000 -processor 68000:BE:32:MC68020"
MAC="-processor PowerPC:BE:32:default"

for t in $targets; do
    case $t in
    68k)
        need extracted/game/Payback.exe
        mkdir -p "$out/flat"
        python tools/extract/hunk.py --flat extracted/game/Payback.exe "$out/flat/Payback68k.bin" > /dev/null
        # 0x117000 = the first DATA hunk (end of code) in the flat map.
        ghidra 68k "$out/flat/Payback68k.bin" $FLAT68K -preScript SeedFlat68k.java 10000 117000
        ;;
    scripts)
        mkdir -p "$out/flat"
        # code end of each flattened script (last CODE hunk start + size, from its .map)
        for s in 00:1d810 01:1a88c 10:1ad10 11:19d80 20:174a8 21:1a45c; do
            n=${s%%:*}; end=${s#*:}
            need "extracted/game/Maps/$n.script"
            python tools/extract/hunk.py --flat "extracted/game/Maps/$n.script" "$out/flat/script$n.bin" > /dev/null
            ghidra "script$n" "$out/flat/script$n.bin" $FLAT68K -preScript SeedFlat68k.java 10000 $end
        done
        ;;
    os4)
        need extracted/os4/PaybackUpdateOS4/install_files/PaybackOS4
        # DecompOne: the object main update FUN_0105f594 can fail at ExportAll's
        # 300 s timeout ("decompile failed" in the 2026-10-07 export); it gets
        # 30 min here, with FUN_01049374 (DrawMap) for comparison.
        ghidra os4 extracted/os4/PaybackUpdateOS4/install_files/PaybackOS4 -processor PowerPC:BE:32:e500 \
            -postScript DecompOne.java "/work/impl/$out/ghidra_os4/long_timeout.c" 0105f594 01049374
        ;;
    macx|mac9|mapedx)
        case $t in
            macx) f="Payback (OS X)" ;;
            mac9) f="Payback (OS 9)" ;;
            mapedx) f="MapEditor (OS X)" ;;
        esac
        need "extracted/mac/Payback/$f"
        ghidra $t "extracted/mac/Payback/$f" $MAC \
            -postScript MacJumpTables.java "tsv=/work/impl/$out/ghidra_$t/jumptables.tsv"
        ;;
    esac
done
wait

status=0
for t in $targets; do
    case $t in scripts) names="script00 script01 script10 script11 script20 script21" ;; *) names=$t ;; esac
    for n in $names; do
        if [ -s "$out/ghidra_$n/functions.tsv" ] && grep -q "REPORT: Save succeeded\|Import succeeded" "$out/ghidra_$n.log"; then
            echo "ghidra_$n: $(wc -l < "$out/ghidra_$n/functions.tsv") functions"
        else
            echo "ghidra_$n: FAILED, see $out/ghidra_$n.log" >&2; status=1
        fi
    done
done
exit $status
