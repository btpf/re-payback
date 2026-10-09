#!/bin/sh
# Builds the web (WebAssembly) version into web/dist/ (docs/web.md):
#   web/dist/index.html, payback.js, payback_bg.wasm  - the game
#   web/dist/data/payback.{pak,json}                  - YOUR extracted game data
#
# usage: tools/build_web.sh [--debug] [--no-data] [--no-film]
#   --debug    unoptimised build (faster to compile, much slower to run)
#   --no-data  skip packing the game data (keeps an existing web/dist/data)
#   --no-film  leave the Mac intro film out of the pack (the FLC plays)
# Environment: CARGO_TARGET_DIR, CARGO_FLAGS (e.g. "-j 4"), PAYBACK_DATA,
# PAYBACK_MAC_DATA, LLVM_BIN (a clang with the wasm32 target, for pl_mpeg).
#
# Needs: rustup target add wasm32-unknown-unknown; wasm-bindgen-cli of the
# same version as the wasm-bindgen crate in Cargo.lock; clang/llvm-ar with
# wasm32 support; python 3. wasm-opt (binaryen) is used when present.
set -e
cd "$(dirname "$0")/.."

profile=web
data=1
pack_args=""
for a in "$@"; do
    case $a in
        --debug) profile=dev ;;
        --no-data) data=0 ;;
        --no-film) pack_args="$pack_args --no-film" ;;
        *) echo "unknown option $a" >&2; exit 2 ;;
    esac
done

target_dir=${CARGO_TARGET_DIR:-target}
out=web/dist

# The C compiler for the vendored pl_mpeg (crates/payback-mpeg/build.rs).
if [ -z "$LLVM_BIN" ]; then
    if [ -x "/c/Program Files/LLVM/bin/clang.exe" ]; then
        LLVM_BIN="/c/Program Files/LLVM/bin"
    else
        LLVM_BIN=$(dirname "$(command -v clang)")
    fi
fi
ext=""
[ -e "$LLVM_BIN/clang.exe" ] && ext=".exe"
export CC_wasm32_unknown_unknown="$LLVM_BIN/clang$ext"
export AR_wasm32_unknown_unknown="$LLVM_BIN/llvm-ar$ext"

want=$(grep -A1 '^name = "wasm-bindgen"$' Cargo.lock | sed -n 's/version = "\(.*\)"/\1/p')
have=$(wasm-bindgen --version 2>/dev/null | awk '{print $2}')
if [ "$want" != "$have" ]; then
    echo "wasm-bindgen-cli $want is needed (found '${have:-none}'):" >&2
    echo "  cargo install -f wasm-bindgen-cli --version $want" >&2
    exit 1
fi

cargo build -p payback --profile $profile --target wasm32-unknown-unknown $CARGO_FLAGS
dir=$profile
[ $profile = dev ] && dir=debug

mkdir -p $out
wasm-bindgen --target web --no-typescript --out-dir $out --out-name payback \
    "$target_dir/wasm32-unknown-unknown/$dir/payback.wasm"
if command -v wasm-opt > /dev/null && [ $profile = web ]; then
    wasm-opt -O2 --enable-bulk-memory --enable-nontrapping-float-to-int --enable-sign-ext \
        --enable-mutable-globals --enable-reference-types --enable-multivalue -o $out/payback_bg.opt.wasm $out/payback_bg.wasm
    mv $out/payback_bg.opt.wasm $out/payback_bg.wasm
fi

if [ $data = 1 ]; then
    python tools/pack_web_data.py --out $out/data $pack_args \
        ${PAYBACK_DATA:+--game "$PAYBACK_DATA"} ${PAYBACK_MAC_DATA:+--mac "$PAYBACK_MAC_DATA"}
fi

# Cache-busting: version the module, the .wasm and the data by content hash
# so a browser never runs a cached payback.js against a newer .wasm (the
# wasm-bindgen glue names change on every build).
build=$(cat $out/payback.js $out/payback_bg.wasm | sha1sum | cut -c1-12)
datav=$( (cat $out/data/payback.json 2>/dev/null || echo none) | sha1sum | cut -c1-12)
sed -e "s/__BUILD__/$build/g" -e "s/__DATA__/$datav/g" web/index.html > $out/index.html

ls -l $out/payback_bg.wasm
echo "Built $out. Serve it with: python -m http.server -d $out 8080  (then open http://localhost:8080/)"
