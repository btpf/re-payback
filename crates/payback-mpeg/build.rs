//! Builds the vendored pl_mpeg decoder (`vendor/pl_mpeg.h`).
//!
//! For `wasm32-unknown-unknown` (the web build, docs/web.md) the C compiler
//! is clang with its wasm32 target (`CC_wasm32_unknown_unknown`, set by
//! `tools/build_web.sh`); that target has no C library, so the few libc
//! calls pl_mpeg makes come from `wasm-libc/` and `src/wasm_libc.rs`.

fn main() {
    println!("cargo:rerun-if-changed=vendor/pl_mpeg.h");
    println!("cargo:rerun-if-changed=vendor/pl_mpeg.c");
    println!("cargo:rerun-if-changed=wasm-libc");
    let mut build = cc::Build::new();
    build
        .file("vendor/pl_mpeg.c")
        // Decoding 640x480 at 25 fps wants optimised code even in dev builds.
        .opt_level(2)
        .warnings(false);
    if std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() == Ok("wasm32") {
        build.include("wasm-libc").flag("-ffreestanding");
    }
    build.compile("pl_mpeg");
}
