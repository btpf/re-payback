//! Locating and loading the original game data.
//!
//! The reimplementation ships no original assets. It reads them from an
//! installed copy (CD contents + Update 7), found via, in order:
//! the `PAYBACK_DATA` environment variable, `--data <dir>`, or
//! `extracted/game` relative to the working directory.
//!
//! The Mac release (`docs/re/mac_version.md`) is an optional second root:
//! `PAYBACK_MAC_DATA`, `--mac-data <dir>`, or `extracted/mac/Payback`. Where
//! the Mac and Amiga content differ, the player picks per item in the
//! settings (`frontend::progress::Variants`); `read_variant` serves either.
//! A changed variant takes effect when that content is next loaded: the
//! cube textures at the next level load, the title screen at the next
//! start, the blood slot at once.
//!
//! Mac data quirks handled here (`analysis/mac/assets.md` §14):
//! - the texture lists are `CubeTextureList.txt` / `CubeTextureRemapList.txt`
//!   (Mac OS X hides dot-files; the Mac game opens those names, strings at
//!   `1018a4f9` / `10193015`), byte-identical to the Amiga dot-files;
//! - names differ in case (`ControlSetup.obj`): lookups fall back to a
//!   case-insensitive match, as on AmigaOS and HFS;
//! - Finder and Sherlock junk (`.DS_Store`, `.FBCIndex`, `.FBCLockFolder`)
//!   and the extractor's `*.rsrc` resource forks are skipped in directory
//!   scans;
//! - short cube textures are zero-padded (`TentRoof.chunky`);
//! - the palette still comes from the Amiga `Payback.exe` (the Mac binaries
//!   hold no LoadRGB32 table; the Mac palette is the Update 7 one).

use bevy::prelude::*;
use payback_formats::{CubeTextureList, CustomTextures, IndexedImage, Palette, palette};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Where the data is found by default: the extracted CD (and Mac CD) next
/// to the working directory, or in the web build the pack's folders
/// (`vfs`, docs/web.md).
#[cfg(not(target_arch = "wasm32"))]
const DEFAULT_DATA: &str = "extracted/game";
#[cfg(not(target_arch = "wasm32"))]
const DEFAULT_MAC_DATA: &str = "extracted/mac/Payback";
#[cfg(target_arch = "wasm32")]
const DEFAULT_DATA: &str = "data/game";
#[cfg(target_arch = "wasm32")]
const DEFAULT_MAC_DATA: &str = "data/mac";

#[derive(Resource, Clone)]
pub struct GameData {
    pub root: PathBuf,
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
    pub fn locate() -> Result<PathBuf, String> {
        let from_args = crate::args()
            .skip_while(|a| a != "--data")
            .nth(1)
            .map(PathBuf::from);
        let candidates = std::env::var_os("PAYBACK_DATA")
            .map(PathBuf::from)
            .into_iter()
            .chain(from_args)
            .chain([PathBuf::from(DEFAULT_DATA)]);
        for dir in candidates {
            if crate::vfs::is_file(dir.join("Payback.exe")) {
                return Ok(dir);
            }
        }
        Err(
            "Payback data not found. Set PAYBACK_DATA or pass --data <dir> pointing at an \
             install containing Payback.exe (run `python tools/extract.py` to unpack the CD)."
                .into(),
        )
    }

    /// The Mac data folder (`Payback` in `PaybackMac.toast`, extracted by
    /// `tools/extract_mac.py`), if there is one.
    pub fn locate_mac() -> Option<PathBuf> {
        let from_args = crate::args()
            .skip_while(|a| a != "--mac-data")
            .nth(1)
            .map(PathBuf::from);
        std::env::var_os("PAYBACK_MAC_DATA")
            .map(PathBuf::from)
            .into_iter()
            .chain(from_args)
            .chain([PathBuf::from(DEFAULT_MAC_DATA)])
            .find(|d| crate::vfs::is_file(d.join("Payback (OS X)")) || crate::vfs::is_dir(d.join("CubeTextures")))
    }

    pub fn load(root: &Path) -> Result<GameData, String> {
        let exe = crate::vfs::read(root.join("Payback.exe"))
            .map_err(|e| format!("{}: {e}", root.join("Payback.exe").display()))?;
        let palette = palette::find_in_executable(&exe).map_err(|e| e.to_string())?;
        let mut data = GameData {
            root: root.to_path_buf(),
            mac: Self::locate_mac(),
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
