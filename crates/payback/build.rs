//! Embeds the game's icon as the Windows executable icon and hands the same
//! image to the game for its window. The Mac release's application icon
//! (`icns` 128 in `Payback (OS X)`'s resource fork, 128x128 with alpha) is
//! preferred, the later and more definitive release; otherwise the Amiga
//! Workbench icon (`Payback.info`). Both come from the extracted game data at
//! build time (`PAYBACK_MAC_DATA` / `PAYBACK_DATA`, or `extracted/`), so no
//! game art is stored in the repository; without the data there is no icon.
//!
//! Also stages the game data next to the executable, so the profile folder
//! (`target/<profile>/`) runs as it is: the files chosen by `data_files.txt`
//! from the Amiga data into `data/amiga` and from the Mac `Payback` folder
//! into `data/mac` (see [`stage_data`]), and creates the empty folders
//! `payback-extract` reads the original files from (see [`source_folders`]).

use payback_formats::data_files::Rules;
use payback_formats::icon::{self, Rgba};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

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
    if !web {
        stage_data(&manifest, &out);
        source_folders(&out);
    }
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

/// The selected files under `root`, relative to it.
fn select(rules: &Rules, root: &Path, rel: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(root.join(rel)) else { return };
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let Ok(ty) = e.file_type() else { continue };
        if ty.is_dir() {
            if !rules.skip_dir(&name) {
                select(rules, root, &rel.join(&name), out);
            }
        } else if !rules.skip_file(&name) {
            out.push(rel.join(&name));
        }
    }
}

#[derive(Default)]
struct SyncStats {
    files: usize,
    bytes: u64,
    updated: usize,
    removed: usize,
}

/// Mirrors the selected files of `src` into `dst`: a file whose size and
/// modification time match is left alone, a new or changed one is
/// hard-linked (same volume; else copied with the source's time), a file
/// no longer selected is removed. Touches nothing outside `dst`.
fn sync(src: &Path, dst: &Path, rules: &Rules, copy: bool) -> std::io::Result<SyncStats> {
    let mut files = Vec::new();
    select(rules, src, Path::new(""), &mut files);
    let mut st = SyncStats::default();
    let mut wanted = HashSet::new();
    for rel in &files {
        wanted.insert(rel.to_string_lossy().to_lowercase());
        let (from, to) = (src.join(rel), dst.join(rel));
        let meta = std::fs::metadata(&from)?;
        st.files += 1;
        st.bytes += meta.len();
        if let Ok(have) = std::fs::metadata(&to) {
            if have.len() == meta.len() && have.modified().ok() == meta.modified().ok() {
                continue;
            }
            std::fs::remove_file(&to)?;
        }
        std::fs::create_dir_all(to.parent().unwrap())?;
        if copy || std::fs::hard_link(&from, &to).is_err() {
            std::fs::copy(&from, &to)?;
            if let Ok(t) = meta.modified() {
                std::fs::File::options().write(true).open(&to)?.set_modified(t)?;
            }
        }
        st.updated += 1;
    }
    remove_stale(dst, Path::new(""), &wanted, &mut st.removed)?;
    Ok(st)
}

/// Removes the files under `dst` not in `wanted` (and folders left empty).
fn remove_stale(dst: &Path, rel: &Path, wanted: &HashSet<String>, removed: &mut usize) -> std::io::Result<()> {
    let Ok(entries) = std::fs::read_dir(dst.join(rel)) else { return Ok(()) };
    for e in entries.flatten() {
        let r = rel.join(e.file_name());
        if e.file_type()?.is_dir() {
            remove_stale(dst, &r, wanted, removed)?;
            if std::fs::read_dir(dst.join(&r))?.next().is_none() {
                std::fs::remove_dir(dst.join(&r))?;
            }
        } else if !wanted.contains(&r.to_string_lossy().to_lowercase()) {
            std::fs::remove_file(dst.join(&r))?;
            *removed += 1;
        }
    }
    Ok(())
}

/// The folders next to the executable where the player puts the original
/// files for `payback-extract` (crates/payback-extract): `Payback Game +
/// Updates/Amiga` (the CD and Update 7) and `Payback Game + Updates/MacOS`,
/// created empty; anything already in them is left alone.
fn source_folders(out: &Path) {
    let Some(profile) = out.ancestors().nth(3).filter(|p| p.join("build").is_dir()) else { return };
    let root = profile.join("Payback Game + Updates");
    for dir in [root.join("Amiga"), root.join("MacOS")] {
        if let Err(e) = std::fs::create_dir_all(&dir) {
            println!("cargo:warning=could not create {}: {e}", dir.display());
        }
    }
}

/// Stages the game data next to the executable: `target/<profile>/data/amiga`
/// (from `PAYBACK_DATA` or `extracted/game`) and `data/mac` (from
/// `PAYBACK_MAC_DATA` or `extracted/mac/Payback`), the files chosen by
/// `data_files.txt`. `PAYBACK_STAGE_DATA=0` turns it off, `=copy` copies
/// instead of hard-linking.
///
/// Cargo reruns this when the rules, the source folders (it scans them for
/// changes) or `data/.staged` change. The stamp's time is set far back so
/// writing it does not itself trigger the next build; deleting `data/`
/// removes it, so the next build stages again.
fn stage_data(manifest: &Path, out: &Path) {
    println!("cargo:rerun-if-env-changed=PAYBACK_STAGE_DATA");
    let mode = std::env::var("PAYBACK_STAGE_DATA").unwrap_or_default();
    if mode == "0" {
        return;
    }
    // OUT_DIR is <target>/<profile>/build/payback-<hash>/out.
    let Some(profile) = out.ancestors().nth(3).filter(|p| p.join("build").is_dir()) else {
        println!("cargo:warning=unexpected OUT_DIR {}; game data not staged", out.display());
        return;
    };
    let rules_path = manifest.join("data_files.txt");
    println!("cargo:rerun-if-changed={}", rules_path.display());
    let rules = Rules::parse(&std::fs::read_to_string(&rules_path).expect("read data_files.txt")).unwrap();
    let extracted = manifest.ancestors().nth(2).unwrap_or(manifest).join("extracted");
    let env_or = |var: &str, default: PathBuf| std::env::var_os(var).map(PathBuf::from).unwrap_or(default);
    let game = env_or("PAYBACK_DATA", extracted.join("game"));
    let mac = env_or("PAYBACK_MAC_DATA", extracted.join("mac").join("Payback"));
    let sources = [
        (game.join("Payback.exe").is_file(), game, "amiga"),
        (mac.join("CubeTextures").is_dir(), mac, "mac"),
    ];
    let data = profile.join("data");
    // Written to the stamp, not as cargo warnings: cargo replays a build
    // script's warnings on every build until it runs again.
    let mut summary = String::from("written by crates/payback/build.rs; delete data/ to stage it again\n");
    let mut staged = false;
    for (ok, src, sub) in sources {
        if !ok {
            if sub == "amiga" {
                println!(
                    "cargo:warning=no game data at {} (run tools/extract/extract.py or set PAYBACK_DATA); {} not staged",
                    src.display(),
                    data.join(sub).display()
                );
            }
            continue;
        }
        println!("cargo:rerun-if-changed={}", src.display());
        match sync(&src, &data.join(sub), &rules, mode == "copy") {
            Ok(st) => {
                staged = true;
                summary.push_str(&format!(
                    "{sub}: {} files, {:.1} MB from {} ({} updated, {} removed in the last run)\n",
                    st.files,
                    st.bytes as f64 / 1e6,
                    src.display(),
                    st.updated,
                    st.removed
                ));
            }
            Err(e) => println!("cargo:warning=staging {} failed: {e}", data.join(sub).display()),
        }
    }
    if staged {
        let stamp = data.join(".staged");
        let written = std::fs::write(&stamp, summary)
            .and_then(|()| std::fs::File::options().write(true).open(&stamp))
            .and_then(|f| f.set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(86_400)));
        if written.is_ok() {
            println!("cargo:rerun-if-changed={}", stamp.display());
        }
    }
}
