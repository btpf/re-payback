//! Embeds the game's icon as the Windows executable icon and hands the same
//! image to the game for its window. The Mac release's application icon
//! (`icns` 128 in `Payback (OS X)`'s resource fork, 128x128 with alpha) is
//! preferred, the later and more definitive release; otherwise the Amiga
//! Workbench icon (`Payback.info`). Both come from the extracted game data at
//! build time (`PAYBACK_MAC_DATA` / `PAYBACK_DATA`, or `extracted/`), so no
//! game art is stored in the repository; without the data there is no icon.

use payback_formats::icon::{self, Rgba};
use std::path::{Path, PathBuf};

/// Icon sizes written to the .ico.
const SIZES: [u32; 6] = [16, 24, 32, 48, 128, 256];

/// The Mac icon family, largest first, or the Amiga Workbench image.
fn source_images(manifest: &Path) -> Option<Vec<Rgba>> {
    let mac = std::env::var_os("PAYBACK_MAC_DATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| manifest.join("../../extracted/mac/Payback"));
    let fork = mac.join("Payback (OS X).rsrc");
    println!("cargo:rerun-if-changed={}", fork.display());
    if let Ok(bytes) = std::fs::read(&fork) {
        match icon::mac_app_icon(&bytes) {
            Ok(imgs) if !imgs.is_empty() => return Some(imgs),
            Ok(_) => println!("cargo:warning=no icon images in {}", fork.display()),
            Err(e) => println!("cargo:warning={}: {e}", fork.display()),
        }
    }
    let data = std::env::var_os("PAYBACK_DATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| manifest.join("../../extracted/game"));
    let info = data.join("Payback.info");
    println!("cargo:rerun-if-changed={}", info.display());
    let bytes = std::fs::read(&info).ok()?;
    match icon::workbench_image(&bytes) {
        Ok(i) => Some(vec![i]),
        Err(e) => {
            println!("cargo:warning=Payback.info: {e}");
            None
        }
    }
}

/// The image for a square icon of `size`: an exact-size image from the
/// family if there is one, otherwise the largest one fitted to the size.
fn at_size(imgs: &[Rgba], size: u32) -> Rgba {
    imgs.iter()
        .find(|i| i.width == size && i.height == size)
        .cloned()
        .unwrap_or_else(|| imgs[0].square(size))
}

fn main() {
    println!("cargo:rerun-if-env-changed=PAYBACK_DATA");
    println!("cargo:rerun-if-env-changed=PAYBACK_MAC_DATA");
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    // The web build has no window icon (the page has none either), and
    // keeps game art out of the .wasm.
    let web = std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() == Ok("wasm32");
    let imgs = if web { None } else { source_images(&manifest) };
    // Window icon for the game: width, height (u32 LE) then RGBA; empty
    // when there is no icon.
    let window = out.join("window_icon.rgba");
    let mut blob = Vec::new();
    if let Some(imgs) = &imgs {
        let w = at_size(imgs, imgs[0].width.max(imgs[0].height).max(128));
        blob.extend_from_slice(&w.width.to_le_bytes());
        blob.extend_from_slice(&w.height.to_le_bytes());
        blob.extend_from_slice(&w.pixels);
    }
    std::fs::write(&window, blob).expect("write window icon");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let Some(imgs) = imgs else {
        println!("cargo:warning=no game data icon - building without an exe icon");
        return;
    };
    let mut dir = ico::IconDir::new(ico::ResourceType::Icon);
    for size in SIZES {
        let sq = at_size(&imgs, size);
        let img = ico::IconImage::from_rgba_data(sq.width, sq.height, sq.pixels);
        dir.add_entry(ico::IconDirEntry::encode(&img).expect("encode icon"));
    }
    let ico_path = out.join("payback.ico");
    dir.write(std::fs::File::create(&ico_path).expect("create icon")).expect("write icon");
    let rc = out.join("payback.rc");
    let escaped = ico_path.display().to_string().replace('\\', "\\\\");
    std::fs::write(&rc, format!("1 ICON \"{escaped}\"\n")).expect("write rc");
    if let Err(e) = embed_resource::compile(&rc, embed_resource::NONE).manifest_optional() {
        println!("cargo:warning=could not embed the exe icon: {e}");
    }
}
