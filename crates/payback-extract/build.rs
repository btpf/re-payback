//! unrar's C++ library uses the Windows registry and process-token calls
//! (advapi32), which `unrar_sys` does not link itself.
fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rustc-link-lib=advapi32");
    }
}
