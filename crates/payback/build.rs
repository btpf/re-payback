//! Embeds the game's Workbench icon (`Payback.info`, the image the Amiga
//! launcher showed) as the Windows executable icon. The icon comes from the
//! game data at build time (`PAYBACK_DATA`, or `extracted/game`), so no
//! game art is stored in the repository; without the data the exe simply
//! has no icon.

use std::path::PathBuf;

/// Icon sizes written to the .ico; the 66x30 art is scaled by whole factors
/// where it fits so the pixels stay sharp.
const SIZES: [u32; 6] = [16, 24, 32, 48, 128, 256];

fn main() {
    println!("cargo:rerun-if-env-changed=PAYBACK_DATA");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let data = std::env::var_os("PAYBACK_DATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| manifest.join("../../extracted/game"));
    let info = data.join("Payback.info");
    println!("cargo:rerun-if-changed={}", info.display());
    let Ok(bytes) = std::fs::read(&info) else {
        println!("cargo:warning=no {} - building without an exe icon", info.display());
        return;
    };
    let image = match payback_formats::icon::workbench_image(&bytes) {
        Ok(i) => i,
        Err(e) => {
            println!("cargo:warning=Payback.info: {e} - building without an exe icon");
            return;
        }
    };
    let mut dir = ico::IconDir::new(ico::ResourceType::Icon);
    for size in SIZES {
        let sq = image.square(size);
        let img = ico::IconImage::from_rgba_data(sq.width, sq.height, sq.pixels);
        dir.add_entry(ico::IconDirEntry::encode(&img).expect("encode icon"));
    }
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let ico_path = out.join("payback.ico");
    dir.write(std::fs::File::create(&ico_path).expect("create icon")).expect("write icon");
    let rc = out.join("payback.rc");
    let escaped = ico_path.display().to_string().replace('\\', "\\\\");
    std::fs::write(&rc, format!("1 ICON \"{escaped}\"\n")).expect("write rc");
    if let Err(e) = embed_resource::compile(&rc, embed_resource::NONE).manifest_optional() {
        println!("cargo:warning=could not embed the exe icon: {e}");
    }
}
