//! File access for the game data and the saves.
//!
//! Natively these are thin wrappers over `std::fs`. In the web build
//! (`wasm32`, docs/web.md) there is no file system:
//! - the game data is a read-only in-memory pack that `web/index.html`
//!   fetches (`data/payback.pak` + `data/payback.json`, written by
//!   `tools/pack_web_data.py`) and hands over with [`vfs_load`] before the
//!   game starts; its files live under `data/game/` (Amiga) and
//!   `data/mac/` (Mac);
//! - everything under [`SAVES`] (`frontend::progress::saves_dir`) is kept in
//!   the browser's localStorage, one key per file (`payback/<path>`), the
//!   bytes base64-encoded, in the same formats as the native files.

use std::io;
use std::path::{Path, PathBuf};

/// The saves directory in the web build.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub const SAVES: &str = "saves";

/// A directory entry from [`read_dir`].
pub struct Entry {
    name: String,
    path: PathBuf,
    file: bool,
}

impl Entry {
    pub fn file_name(&self) -> std::ffi::OsString {
        self.name.clone().into()
    }

    pub fn path(&self) -> PathBuf {
        self.path.clone()
    }

    pub fn is_file(&self) -> bool {
        self.file
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod imp {
    use super::*;

    pub fn read(p: &Path) -> io::Result<Vec<u8>> {
        std::fs::read(p)
    }

    pub fn write(p: &Path, bytes: &[u8]) -> io::Result<()> {
        std::fs::write(p, bytes)
    }

    pub fn rename(from: &Path, to: &Path) -> io::Result<()> {
        std::fs::rename(from, to)
    }

    pub fn remove_file(p: &Path) -> io::Result<()> {
        std::fs::remove_file(p)
    }

    pub fn create_dir_all(p: &Path) -> io::Result<()> {
        std::fs::create_dir_all(p)
    }

    pub fn is_file(p: &Path) -> bool {
        p.is_file()
    }

    pub fn is_dir(p: &Path) -> bool {
        p.is_dir()
    }

    pub fn canonicalize(p: &Path) -> io::Result<PathBuf> {
        p.canonicalize()
    }

    pub fn read_dir(p: &Path) -> io::Result<Vec<Entry>> {
        Ok(std::fs::read_dir(p)?
            .flatten()
            .map(|e| {
                let path = e.path();
                Entry { name: e.file_name().to_string_lossy().into_owned(), file: path.is_file(), path }
            })
            .collect())
    }
}

#[cfg(target_arch = "wasm32")]
mod imp {
    use super::*;
    use std::collections::BTreeMap;
    use std::sync::RwLock;
    use wasm_bindgen::prelude::*;

    /// The data pack: one byte blob and, per normalised path, its range.
    struct Pack {
        blob: Vec<u8>,
        files: BTreeMap<String, (usize, usize)>,
    }

    static PACK: RwLock<Option<Pack>> = RwLock::new(None);

    /// Installs the data pack fetched by the page: `manifest` is
    /// `payback.json` (`{"files": [[path, offset, length], ...]}`), `blob`
    /// is `payback.pak`.
    #[wasm_bindgen]
    pub fn vfs_load(manifest: &str, blob: Vec<u8>) -> Result<usize, JsValue> {
        let files = parse_manifest(manifest).ok_or_else(|| JsValue::from_str("bad payback.json"))?;
        if files.values().any(|&(o, l)| o.checked_add(l).is_none_or(|e| e > blob.len())) {
            return Err(JsValue::from_str("payback.json does not match payback.pak"));
        }
        let n = files.len();
        *PACK.write().unwrap() = Some(Pack { blob, files });
        Ok(n)
    }

    /// `[["a/b", 0, 12], ...]` inside `{"files": ...}`; paths are plain
    /// (the packer escapes only `"` and `\`).
    fn parse_manifest(s: &str) -> Option<BTreeMap<String, (usize, usize)>> {
        let start = s.find("\"files\"")?;
        let mut rest = &s[start + 7..];
        rest = &rest[rest.find('[')? + 1..];
        let mut out = BTreeMap::new();
        loop {
            rest = rest.trim_start_matches(|c: char| c.is_whitespace() || c == ',');
            if rest.starts_with(']') || rest.is_empty() {
                return Some(out);
            }
            rest = rest.strip_prefix('[')?.trim_start().strip_prefix('"')?;
            let mut path = String::new();
            let mut chars = rest.char_indices();
            let end = loop {
                let (i, c) = chars.next()?;
                match c {
                    '\\' => path.push(chars.next()?.1),
                    '"' => break i,
                    c => path.push(c),
                }
            };
            rest = &rest[end + 1..];
            let close = rest.find(']')?;
            let mut nums = rest[..close].split(',').map(str::trim).filter(|t| !t.is_empty());
            let off = nums.next()?.parse().ok()?;
            let len = nums.next()?.parse().ok()?;
            out.insert(path, (off, len));
            rest = &rest[close + 1..];
        }
    }

    /// `a/./b\c` -> `a/b/c`, without a leading `./` or trailing `/`.
    fn norm(p: &Path) -> String {
        let s = p.to_string_lossy().replace('\\', "/");
        let mut parts: Vec<&str> = Vec::new();
        for part in s.split('/') {
            match part {
                "" | "." => {}
                ".." => {
                    parts.pop();
                }
                p => parts.push(p),
            }
        }
        parts.join("/")
    }

    fn is_save(key: &str) -> bool {
        key == SAVES || key.starts_with(&format!("{SAVES}/"))
    }

    fn storage() -> io::Result<web_sys::Storage> {
        web_sys::window()
            .and_then(|w| w.local_storage().ok().flatten())
            .ok_or_else(|| io::Error::other("localStorage is not available"))
    }

    const PREFIX: &str = "payback/";

    fn not_found() -> io::Error {
        io::Error::from(io::ErrorKind::NotFound)
    }

    /// All localStorage keys of saves, as paths (`saves/...`).
    fn save_keys() -> Vec<String> {
        let Ok(st) = storage() else { return Vec::new() };
        let n = st.length().unwrap_or(0);
        (0..n)
            .filter_map(|i| st.key(i).ok().flatten())
            .filter_map(|k| k.strip_prefix(PREFIX).map(String::from))
            .collect()
    }

    pub fn read(p: &Path) -> io::Result<Vec<u8>> {
        let key = norm(p);
        if is_save(&key) {
            let v = storage()?.get_item(&format!("{PREFIX}{key}")).map_err(|_| not_found())?;
            return v.and_then(|s| base64_decode(&s)).ok_or_else(not_found);
        }
        let pack = PACK.read().unwrap();
        let pack = pack.as_ref().ok_or_else(not_found)?;
        let &(o, l) = pack.files.get(&key).ok_or_else(not_found)?;
        Ok(pack.blob[o..o + l].to_vec())
    }

    pub fn write(p: &Path, bytes: &[u8]) -> io::Result<()> {
        let key = norm(p);
        if !is_save(&key) {
            return Err(io::Error::from(io::ErrorKind::PermissionDenied));
        }
        storage()?
            .set_item(&format!("{PREFIX}{key}"), &base64_encode(bytes))
            .map_err(|e| io::Error::other(format!("localStorage: {e:?}")))
    }

    #[allow(dead_code)]
    pub fn rename(from: &Path, to: &Path) -> io::Result<()> {
        let b = read(from)?;
        write(to, &b)?;
        remove_file(from)
    }

    pub fn remove_file(p: &Path) -> io::Result<()> {
        let key = norm(p);
        if !is_save(&key) {
            return Err(io::Error::from(io::ErrorKind::PermissionDenied));
        }
        storage()?.remove_item(&format!("{PREFIX}{key}")).map_err(|_| not_found())
    }

    /// Directories are implied by the paths in the pack and localStorage.
    pub fn create_dir_all(_: &Path) -> io::Result<()> {
        Ok(())
    }

    pub fn is_file(p: &Path) -> bool {
        let key = norm(p);
        if is_save(&key) {
            return storage().is_ok_and(|s| s.get_item(&format!("{PREFIX}{key}")).ok().flatten().is_some());
        }
        PACK.read().unwrap().as_ref().is_some_and(|pk| pk.files.contains_key(&key))
    }

    pub fn is_dir(p: &Path) -> bool {
        let key = norm(p);
        if key == SAVES {
            return true;
        }
        let pre = format!("{key}/");
        if is_save(&key) {
            return save_keys().iter().any(|k| k.starts_with(&pre));
        }
        PACK.read()
            .unwrap()
            .as_ref()
            .is_some_and(|pk| pk.files.range(pre.clone()..).next().is_some_and(|(k, _)| k.starts_with(&pre)))
    }

    pub fn canonicalize(p: &Path) -> io::Result<PathBuf> {
        if is_file(p) || is_dir(p) { Ok(PathBuf::from(norm(p))) } else { Err(not_found()) }
    }

    pub fn read_dir(p: &Path) -> io::Result<Vec<Entry>> {
        let key = norm(p);
        let pre = if key.is_empty() { String::new() } else { format!("{key}/") };
        let keys: Vec<String> = if is_save(&key) {
            save_keys()
        } else {
            let pack = PACK.read().unwrap();
            let pack = pack.as_ref().ok_or_else(not_found)?;
            pack.files.range(pre.clone()..).take_while(|(k, _)| k.starts_with(&pre)).map(|(k, _)| k.clone()).collect()
        };
        let mut out: Vec<Entry> = Vec::new();
        for k in keys {
            let Some(rest) = k.strip_prefix(&pre) else { continue };
            let (name, file) = match rest.split_once('/') {
                Some((d, _)) => (d, false),
                None => (rest, true),
            };
            if !out.iter().any(|e| e.name == name) {
                out.push(Entry { name: name.to_string(), path: Path::new(&pre).join(name), file });
            }
        }
        if out.is_empty() && !is_dir(p) {
            return Err(not_found());
        }
        Ok(out)
    }

    const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    pub fn base64_encode(b: &[u8]) -> String {
        let mut s = String::with_capacity(b.len().div_ceil(3) * 4);
        for c in b.chunks(3) {
            let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
            for i in 0..4 {
                if i <= c.len() {
                    s.push(B64[(n >> (18 - 6 * i) & 63) as usize] as char);
                } else {
                    s.push('=');
                }
            }
        }
        s
    }

    pub fn base64_decode(s: &str) -> Option<Vec<u8>> {
        let mut out = Vec::with_capacity(s.len() / 4 * 3);
        let (mut acc, mut bits) = (0u32, 0);
        for c in s.bytes().filter(|&c| c != b'=' && !c.is_ascii_whitespace()) {
            acc = acc << 6 | B64.iter().position(|&x| x == c)? as u32;
            bits += 6;
            if bits >= 8 {
                bits -= 8;
                out.push((acc >> bits) as u8);
            }
        }
        Some(out)
    }
}

pub fn read(p: impl AsRef<Path>) -> io::Result<Vec<u8>> {
    imp::read(p.as_ref())
}

pub fn write(p: impl AsRef<Path>, bytes: impl AsRef<[u8]>) -> io::Result<()> {
    imp::write(p.as_ref(), bytes.as_ref())
}

#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub fn rename(from: impl AsRef<Path>, to: impl AsRef<Path>) -> io::Result<()> {
    imp::rename(from.as_ref(), to.as_ref())
}

#[allow(dead_code)]
pub fn remove_file(p: impl AsRef<Path>) -> io::Result<()> {
    imp::remove_file(p.as_ref())
}

pub fn create_dir_all(p: impl AsRef<Path>) -> io::Result<()> {
    imp::create_dir_all(p.as_ref())
}

pub fn is_file(p: impl AsRef<Path>) -> bool {
    imp::is_file(p.as_ref())
}

pub fn is_dir(p: impl AsRef<Path>) -> bool {
    imp::is_dir(p.as_ref())
}

pub fn exists(p: impl AsRef<Path>) -> bool {
    is_file(&p) || is_dir(&p)
}

pub fn canonicalize(p: impl AsRef<Path>) -> io::Result<PathBuf> {
    imp::canonicalize(p.as_ref())
}

/// The entries of directory `p` (files and sub-directories), shaped like
/// `std::fs::read_dir` (`.flatten()` works).
pub fn read_dir(p: impl AsRef<Path>) -> io::Result<impl Iterator<Item = io::Result<Entry>>> {
    Ok(imp::read_dir(p.as_ref())?.into_iter().map(Ok))
}

#[cfg(all(test, target_arch = "wasm32"))]
mod tests {
    #[test]
    fn base64_roundtrip() {
        for n in 0..20 {
            let b: Vec<u8> = (0..n).map(|i| (i * 37) as u8).collect();
            assert_eq!(super::imp::base64_decode(&super::imp::base64_encode(&b)).unwrap(), b);
        }
    }
}
