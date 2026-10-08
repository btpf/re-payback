//! Builds the vendored pl_mpeg decoder (`vendor/pl_mpeg.h`).

fn main() {
    println!("cargo:rerun-if-changed=vendor/pl_mpeg.h");
    println!("cargo:rerun-if-changed=vendor/pl_mpeg.c");
    cc::Build::new()
        .file("vendor/pl_mpeg.c")
        // Decoding 640x480 at 25 fps wants optimised code even in dev builds.
        .opt_level(2)
        .warnings(false)
        .compile("pl_mpeg");
}
