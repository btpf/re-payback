//! Locating and loading the original game data.
//!
//! The reimplementation ships no original assets. It reads them from an
//! installed copy (CD contents + Update 7), found via, in order:
//! the `PAYBACK_DATA` environment variable, `--data <dir>`, or
//! `extracted/game` relative to the working directory.

use bevy::prelude::*;
use payback_formats::{CubeTextureList, Palette, palette};
use std::path::{Path, PathBuf};

#[derive(Resource, Clone)]
pub struct GameData {
    pub root: PathBuf,
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
            palette,
            cube_textures,
        })
    }

    pub fn read(&self, rel: impl AsRef<Path>) -> std::io::Result<Vec<u8>> {
        std::fs::read(self.root.join(rel))
    }
}
