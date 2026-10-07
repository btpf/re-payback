//! Parsers for the data files of Payback (Apex Designs, Amiga, 2001).
//!
//! Everything here is engine-agnostic: plain Rust types, no Bevy. Formats are
//! documented in each module and in `docs/formats.md`.

pub mod map;
pub mod motion;
pub mod obj;
pub mod palette;
pub mod phrases;
pub mod route;
pub mod sound;
pub mod texture;

pub use map::Map;
pub use motion::Motion;
pub use obj::Obj;
pub use palette::Palette;
pub use phrases::Phrases;
pub use route::Route;
pub use texture::{CubeTextureList, IndexedImage};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}: unexpected end of data")]
    Truncated(&'static str),
    #[error("{what}: {detail}")]
    Parse { what: &'static str, detail: String },
}

pub type Result<T> = std::result::Result<T, Error>;
