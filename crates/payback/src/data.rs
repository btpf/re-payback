//! Locating and loading the original game data.
//!
//! The reimplementation ships no original assets. It reads them from an
//! installed copy: the Amiga release (CD contents + Update 7), the Mac
//! release (`docs/re/mac_version.md`), or both. Natively each is searched
//! for (`PAYBACK_DATA` / `PAYBACK_MAC_DATA`, `--data` / `--mac-data <dir>`,
//! a remembered folder, next to the executable, `extracted/game` /
//! `extracted/mac/Payback` above the executable or the working directory,
//! else a folder picker that takes either: `locate.rs`); the web build
//! reads its pack's `data/game` and `data/mac`.
//!
//! Either release alone is enough. With both, the Amiga install is the
//! main root and the Mac folder a second one; where their content differs,
//! the player picks per item in the settings
//! (`frontend::progress::Variants`); `read_variant` serves either. With the
//! Mac data alone (or `--mac-only`, which ignores the Amiga data) the Mac
//! folder is the main root and every Mac/Amiga choice is the Mac one
//! ([`GameData::mac_variant`]). The Amiga-only content is then missing:
//! the FLC intro (the Mac film plays), `Blood.dark` (the Mac `Blood` serves
//! at night too), the eleven languages beyond English, Deutsch and
//! Français, the two shipped replays and the `Payback.info` icon.
//! A changed variant takes effect when that content is next loaded: the
//! cube textures at the next level load, the title screen at the next
//! start, the blood slot at once.
//!
//! Mac data quirks handled here (`docs/re/mac/assets.md` §14):
//! - the texture lists are `CubeTextureList.txt` / `CubeTextureRemapList.txt`
//!   (Mac OS X hides dot-files; the Mac game opens those names, strings at
//!   `1018a4f9` / `10193015`), byte-identical to the Amiga dot-files;
//! - names differ in case (`ControlSetup.obj`): lookups fall back to a
//!   case-insensitive match, as on AmigaOS and HFS;
//! - Finder and Sherlock junk (`.DS_Store`, `.FBCIndex`, `.FBCLockFolder`)
//!   and the extractor's `*.rsrc` resource forks are skipped in directory
//!   scans;
//! - short cube textures are zero-padded (`TentRoof.chunky`);
//! - the palette: from the Amiga `Payback.exe` when present, else from
//!   `Payback (OS X)` (or `Payback (OS 9)`), whose pattern-initialized PEF
//!   data section holds the same LoadRGB32 table, byte-identical to Update
//!   7's (`palette::find_in_executable`, `docs/re/mac/assets.md` §15).

use bevy::prelude::*;
use payback_formats::{CubeTextureList, CustomTextures, IndexedImage, Palette, palette};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Where the web build finds the data: the pack's folders (`vfs`,
/// docs/web.md).
#[cfg(target_arch = "wasm32")]
const DEFAULT_DATA: &str = "data/game";
#[cfg(target_arch = "wasm32")]
const DEFAULT_MAC_DATA: &str = "data/mac";

/// Logs which data roots the search used and how it found them
/// (`locate.rs`; queued until Bevy's logger is up).
pub fn log_data_roots() {
    #[cfg(not(target_arch = "wasm32"))]
    for (warn, text) in crate::locate::take_log() {
        if warn {
            warn!("{text}");
        } else {
            info!("{text}");
        }
    }
}

/// The Mac applications that hold the palette, in the order tried.
const MAC_EXES: [&str; 2] = ["Payback (OS X)", "Payback (OS 9)"];

/// The data roots the search found: the Amiga install and the Mac
/// `Payback` folder, at least one of them.
#[derive(Clone, Debug, Default)]
pub struct Roots {
    pub amiga: Option<PathBuf>,
    pub mac: Option<PathBuf>,
}

/// `--mac-only`: ignore the Amiga data and run from the Mac data alone
/// (to try a Mac-only install).
pub fn mac_only() -> bool {
    crate::args().any(|a| a == "--mac-only")
}

#[derive(Resource, Clone)]
pub struct GameData {
    /// The main root: the Amiga install, or the Mac folder without it.
    pub root: PathBuf,
    /// The Amiga data is present (`root` holds `Payback.exe`); false when
    /// running from the Mac data alone (`root` = `mac`).
    pub amiga: bool,
    /// The Mac release's `Payback` folder, if present.
    pub mac: Option<PathBuf>,
    pub palette: Palette,
    pub cube_textures: CubeTextureList,
    /// Cube textures from the Mac set (`Variants::textures`; false = the
    /// "Amiga look"). Set at level load; only used with Mac data present.
    pub mac_cube_textures: bool,
    /// The loaded user map's `<map>.textures` (texture numbers >= 2000);
    /// empty for the built-in cities. Set at level load.
    pub custom_textures: Arc<CustomTextures>,
}

impl GameData {
    /// The Amiga and Mac data roots (`locate.rs`; shows a folder picker
    /// when neither is found).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn locate() -> Result<Roots, String> {
        crate::locate::locate()
    }

    /// The web build's data: the pack's folders (or `PAYBACK_DATA`,
    /// `--data`, ... as natively), either may be missing.
    #[cfg(target_arch = "wasm32")]
    pub fn locate() -> Result<Roots, String> {
        let given = |env: &str, arg: &str, default: &str| {
            std::env::var_os(env)
                .map(PathBuf::from)
                .into_iter()
                .chain(crate::arg(arg).map(PathBuf::from))
                .chain([PathBuf::from(default)])
                .collect::<Vec<_>>()
        };
        let amiga = given("PAYBACK_DATA", "--data", DEFAULT_DATA)
            .into_iter()
            .find(|d| crate::vfs::is_file(d.join("Payback.exe")))
            .filter(|_| !mac_only());
        let mac = given("PAYBACK_MAC_DATA", "--mac-data", DEFAULT_MAC_DATA)
            .into_iter()
            .find(|d| is_mac_root(d));
        if amiga.is_none() && mac.is_none() {
            return Err("Payback data not found: the data pack holds neither the Amiga release \
                        (Payback.exe) nor the Mac release's Payback folder (docs/web.md)."
                .into());
        }
        Ok(Roots { amiga, mac })
    }

    pub fn load(roots: &Roots) -> Result<GameData, String> {
        let (root, amiga) = match (&roots.amiga, &roots.mac) {
            (Some(a), _) => (a.clone(), true),
            (None, Some(m)) => (m.clone(), false),
            (None, None) => return Err("Payback data not found.".into()),
        };
        // Both releases hold the same palette (`palette.rs`); the Amiga
        // data, when present, stays the source as before.
        let palette = if amiga {
            let exe = crate::vfs::read(root.join("Payback.exe"))
                .map_err(|e| format!("{}: {e}", root.join("Payback.exe").display()))?;
            palette::find_in_executable(&exe).map_err(|e| e.to_string())?
        } else {
            mac_palette(&root)?
        };
        let mut data = GameData {
            root: root.clone(),
            amiga,
            mac: roots.mac.clone(),
            palette,
            cube_textures: CubeTextureList { entries: Vec::new() },
            mac_cube_textures: true,
            custom_textures: Arc::default(),
        };
        let list = data.read_texture_list("CubeTextureList").map_err(|e| {
            format!("{}: {e}", root.join("CubeTextures/.CubeTextureList").display())
        })?;
        data.cube_textures = CubeTextureList::parse(&list).map_err(|e| e.to_string())?;
        Ok(data)
    }

    /// The effective value of a Mac/Amiga choice (`Variants`): the
    /// setting, or always the Mac one without the Amiga data.
    pub fn mac_variant(&self, setting: bool) -> bool {
        setting || !self.amiga
    }

    /// `rel` from the Amiga data (names matched case-insensitively when the
    /// exact one is missing), else from the Mac data, for files only the
    /// Mac ships (e.g. `ObjectTextures/tanne`). Absolute paths are read as
    /// they are.
    pub fn read(&self, rel: impl AsRef<Path>) -> std::io::Result<Vec<u8>> {
        let rel = rel.as_ref();
        match crate::vfs::read(self.root.join(rel)) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                match find_in(&self.root, rel).or_else(|| self.mac_path(rel)) {
                    Some(p) => crate::vfs::read(p),
                    None => Err(e),
                }
            }
            r => r,
        }
    }

    /// The Mac copy of `rel`, if the Mac data is present and has the file
    /// (names matched case-insensitively).
    pub fn mac_path(&self, rel: impl AsRef<Path>) -> Option<PathBuf> {
        find_in(self.mac.as_ref()?, rel.as_ref())
    }

    /// `rel` from the Mac data when `prefer_mac` and the Mac has it, else
    /// from the Amiga data.
    pub fn read_variant(&self, rel: impl AsRef<Path>, prefer_mac: bool) -> std::io::Result<Vec<u8>> {
        let rel = rel.as_ref();
        match self.mac_path(rel).filter(|_| prefer_mac) {
            Some(p) => crate::vfs::read(p),
            None => self.read(rel),
        }
    }

    /// `CubeTextures/.<name>` (Amiga) or `CubeTextures/<name>.txt` (Mac),
    /// for `CubeTextureList` and `CubeTextureRemapList`.
    pub fn read_texture_list(&self, name: &str) -> std::io::Result<Vec<u8>> {
        self.read(format!("CubeTextures/.{name}")).or_else(|e| {
            let txt = format!("CubeTextures/{name}.txt");
            match find_in(&self.root, Path::new(&txt)).or_else(|| self.mac_path(&txt)) {
                Some(p) => crate::vfs::read(p),
                None => Err(e),
            }
        })
    }

    /// Cube texture `file` of `.CubeTextureList`: from the Mac set when
    /// [`Self::mac_cube_textures`] (the whole set, so the Amiga look is
    /// never mixed in while the Mac has the file), else the Amiga one.
    /// Short files are zero-padded ([`IndexedImage::cube_texture_padded`]).
    pub fn cube_texture(&self, file: &str) -> Option<IndexedImage> {
        let raw = self
            .read_variant(format!("CubeTextures/{file}"), self.mac_cube_textures)
            .ok()?;
        (!raw.is_empty()).then(|| IndexedImage::cube_texture_padded(&raw))
    }

    /// Surface flags (the `.CubeTextureList` first column: footstep sound,
    /// grip) of texture number `t`, including a user map's custom textures.
    pub fn texture_flags(&self, t: u16) -> Option<u32> {
        match self.custom_textures.get(t) {
            Some(c) => Some(c.flags),
            None => self.cube_textures.entries.get(t as usize).map(|e| e.flags),
        }
    }

    /// The files of `dir` in the Amiga data and, with `mac`, the Mac data,
    /// as (name, path). A name in both (compared case-insensitively) is
    /// listed once, from the Mac when `prefer_mac`. Skips sub-directories
    /// and junk ([`is_junk`]).
    pub fn list_dir(&self, dir: &str, mac: bool, prefer_mac: bool) -> Vec<(String, PathBuf)> {
        let mut roots = vec![self.root.clone()];
        if let Some(m) = self.mac.as_ref().filter(|_| mac) {
            if prefer_mac {
                roots.insert(0, m.clone());
            } else {
                roots.push(m.clone());
            }
        }
        let mut out: Vec<(String, PathBuf)> = Vec::new();
        for root in roots {
            let Some(d) = find_dir(&root, Path::new(dir)) else { continue };
            let Ok(entries) = crate::vfs::read_dir(&d) else { continue };
            for e in entries.flatten() {
                let name = e.file_name().to_string_lossy().into_owned();
                if is_junk(&name) || !e.is_file() {
                    continue;
                }
                if !out.iter().any(|(n, _)| n.eq_ignore_ascii_case(&name)) {
                    out.push((name, e.path()));
                }
            }
        }
        out
    }
}

/// `dir` is the Mac release's `Payback` folder: it has the Mac names of
/// the texture lists (`CubeTextures/CubeTextureList.txt`; the Amiga data
/// has `.CubeTextureList`), or one of the Mac applications.
pub fn is_mac_root(dir: &Path) -> bool {
    crate::vfs::is_file(dir.join("CubeTextures").join("CubeTextureList.txt"))
        || MAC_EXES.iter().any(|e| crate::vfs::is_file(dir.join(e)) && crate::vfs::is_dir(dir.join("CubeTextures")))
}

/// The palette from a Mac application in `mac`: the LoadRGB32 table in its
/// PEF data section, identical to Update 7's (`MAC_EXES`).
fn mac_palette(mac: &Path) -> Result<Palette, String> {
    let mut errors = String::new();
    for name in MAC_EXES {
        let Some(path) = find_in(mac, Path::new(name)) else { continue };
        let found = crate::vfs::read(&path)
            .map_err(|e| e.to_string())
            .and_then(|b| palette::find_in_executable(&b).map_err(|e| e.to_string()));
        match found {
            Ok(p) => return Ok(p),
            Err(e) => errors.push_str(&format!("\n{}: {e}", path.display())),
        }
    }
    Err(format!(
        "No palette: the Amiga data (Payback.exe) is missing and the Mac data at {} has no \
         usable `Payback (OS X)` or `Payback (OS 9)` application to read it from.{errors}",
        mac.display()
    ))
}

/// Mac Finder and Sherlock files (`.DS_Store`, `.FBCIndex`,
/// `.FBCLockFolder`), other dot-files, and the extractor's resource forks.
pub fn is_junk(name: &str) -> bool {
    name.starts_with('.') || name.ends_with(".rsrc")
}

/// `root/rel` if it is a file, matching each component case-insensitively
/// when the exact path is missing.
fn find_in(root: &Path, rel: &Path) -> Option<PathBuf> {
    let p = root.join(rel);
    if crate::vfs::is_file(&p) {
        return Some(p);
    }
    resolve_nocase(root, rel).filter(|p| crate::vfs::is_file(p))
}

/// Like [`find_in`], for a directory.
fn find_dir(root: &Path, rel: &Path) -> Option<PathBuf> {
    let p = root.join(rel);
    if crate::vfs::is_dir(&p) {
        return Some(p);
    }
    resolve_nocase(root, rel).filter(|p| crate::vfs::is_dir(p))
}

fn resolve_nocase(root: &Path, rel: &Path) -> Option<PathBuf> {
    if rel.is_absolute() {
        // A user map outside the game data (`UserMaps/` next to the
        // executable, the Mac data): only the file name is matched without
        // case, as the Amiga and HFS file systems do (`Foo.MAP` with
        // `foo.thumb`).
        let (dir, name) = (rel.parent()?, rel.file_name()?.to_string_lossy());
        return crate::vfs::read_dir(dir)
            .ok()?
            .flatten()
            .find(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(&name))
            .map(|e| e.path());
    }
    let mut cur = root.to_path_buf();
    for part in rel.components() {
        let part = part.as_os_str().to_string_lossy();
        let exact = cur.join(&*part);
        cur = if crate::vfs::exists(&exact) {
            exact
        } else {
            crate::vfs::read_dir(&cur)
                .ok()?
                .flatten()
                .find(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(&part))?
                .path()
        };
    }
    Some(cur)
}

/// Two spellings of one language name: equal ignoring ASCII case, where a
/// non-ASCII character matches any character (the Mac's ASCII `Francais`
/// against the Amiga's Latin-1 `Français`, however the file system decoded
/// the `ç`).
pub fn same_language(a: &str, b: &str) -> bool {
    a.chars().count() == b.chars().count()
        && a
            .chars()
            .zip(b.chars())
            .all(|(x, y)| x.eq_ignore_ascii_case(&y) || !x.is_ascii() || !y.is_ascii())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_spellings() {
        assert!(same_language("Francais", "Français"));
        assert!(same_language("Franτais", "francais"));
        assert!(!same_language("English", "Deutsch"));
        assert!(!same_language("Francais", "Franc"));
    }
}
