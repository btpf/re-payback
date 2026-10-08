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

use bevy::prelude::*;
use payback_formats::{CubeTextureList, Palette, palette};
use std::path::{Path, PathBuf};

#[derive(Resource, Clone)]
pub struct GameData {
    pub root: PathBuf,
    /// The Mac release's `Payback` folder, if present.
    pub mac: Option<PathBuf>,
    pub palette: Palette,
    pub cube_textures: CubeTextureList,
}

impl GameData {
    pub fn locate() -> Result<PathBuf, String> {
        let from_args = std::env::args()
            .skip_while(|a| a != "--data")
            .nth(1)
            .map(PathBuf::from);
        let candidates = std::env::var_os("PAYBACK_DATA")
            .map(PathBuf::from)
            .into_iter()
            .chain(from_args)
            .chain([PathBuf::from("extracted/game")]);
        for dir in candidates {
            if dir.join("Payback.exe").is_file() {
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
        let from_args = std::env::args()
            .skip_while(|a| a != "--mac-data")
            .nth(1)
            .map(PathBuf::from);
        std::env::var_os("PAYBACK_MAC_DATA")
            .map(PathBuf::from)
            .into_iter()
            .chain(from_args)
            .chain([PathBuf::from("extracted/mac/Payback")])
            .find(|d| d.join("Payback (OS X)").is_file() || d.join("CubeTextures").is_dir())
    }

    pub fn load(root: &Path) -> Result<GameData, String> {
        let read = |rel: &str| {
            std::fs::read(root.join(rel)).map_err(|e| format!("{}: {e}", root.join(rel).display()))
        };
        let palette =
            palette::find_in_executable(&read("Payback.exe")?).map_err(|e| e.to_string())?;
        let cube_textures = CubeTextureList::parse(&read("CubeTextures/.CubeTextureList")?)
            .map_err(|e| e.to_string())?;
        Ok(GameData {
            root: root.to_path_buf(),
            mac: Self::locate_mac(),
            palette,
            cube_textures,
        })
    }

    pub fn read(&self, rel: impl AsRef<Path>) -> std::io::Result<Vec<u8>> {
        std::fs::read(self.root.join(rel))
    }

    /// The Mac copy of `rel`, if the Mac data is present and has the file.
    pub fn mac_path(&self, rel: impl AsRef<Path>) -> Option<PathBuf> {
        let p = self.mac.as_ref()?.join(rel);
        p.is_file().then_some(p)
    }

    /// `rel` from the Mac data when `prefer_mac` and the Mac has it, else
    /// from the Amiga data.
    pub fn read_variant(&self, rel: impl AsRef<Path>, prefer_mac: bool) -> std::io::Result<Vec<u8>> {
        let rel = rel.as_ref();
        match self.mac_path(rel).filter(|_| prefer_mac) {
            Some(p) => std::fs::read(p),
            None => self.read(rel),
        }
    }
}
