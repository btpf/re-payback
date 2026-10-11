//! CD images: `.cue` sheets with their `.bin` files (one for the disc or
//! one per track, as in the Redump set), plain `.iso` data tracks, and the
//! ISO 9660 file system on the data track.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// Bytes in a raw CD sector.
pub const RAW: u64 = 2352;

/// A track of a cue sheet.
#[derive(Debug, Clone)]
pub struct Track {
    pub num: u32,
    /// `AUDIO`, `MODE1/2352`, `MODE1/2048`, `MODE2/2352`, ...
    pub mode: String,
    pub file: PathBuf,
    /// INDEX 00 and 01, in sectors from the start of `file`.
    pub idx0: Option<u64>,
    pub idx1: u64,
}

impl Track {
    pub fn is_audio(&self) -> bool {
        self.mode == "AUDIO"
    }
}

/// A parsed cue sheet.
#[derive(Debug, Clone)]
pub struct Disc {
    pub tracks: Vec<Track>,
}

fn msf(s: &str) -> Option<u64> {
    let mut it = s.split(':').map(|p| p.parse::<u64>().ok());
    let (m, sec, f) = (it.next()??, it.next()??, it.next()??);
    Some((m * 60 + sec) * 75 + f)
}

impl Disc {
    /// Reads `cue`; the `.bin` names are taken relative to its folder.
    pub fn parse(cue: &Path) -> Result<Disc, String> {
        let bytes = std::fs::read(cue).map_err(|e| format!("{}: {e}", cue.display()))?;
        let text: String = bytes.iter().map(|&b| b as char).collect();
        let dir = cue.parent().unwrap_or(Path::new("."));
        let mut tracks: Vec<Track> = Vec::new();
        let mut file: Option<PathBuf> = None;
        for line in text.lines().map(str::trim) {
            let upper = line.to_ascii_uppercase();
            if upper.starts_with("FILE ") {
                let (name, kind) = match (line.find('"'), line.rfind('"')) {
                    (Some(a), Some(b)) if b > a => (&line[a + 1..b], line[b + 1..].trim()),
                    _ => {
                        let mut w = line[5..].rsplitn(2, ' ');
                        let kind = w.next().unwrap_or("");
                        (w.next().unwrap_or("").trim(), kind)
                    }
                };
                if !kind.eq_ignore_ascii_case("BINARY") {
                    return Err(format!("{}: unsupported FILE type {kind}", cue.display()));
                }
                file = Some(dir.join(name));
            } else if upper.starts_with("TRACK ") {
                let mut w = line.split_whitespace().skip(1);
                let num = w.next().and_then(|n| n.parse().ok()).ok_or(format!("{}: bad TRACK line", cue.display()))?;
                let mode = w.next().unwrap_or("").to_ascii_uppercase();
                let file = file.clone().ok_or(format!("{}: TRACK before FILE", cue.display()))?;
                tracks.push(Track { num, mode, file, idx0: None, idx1: 0 });
            } else if upper.starts_with("INDEX ") {
                let mut w = line.split_whitespace().skip(1);
                let (n, at) = (w.next().and_then(|n| n.parse::<u32>().ok()), w.next().and_then(msf));
                let (Some(n), Some(at), Some(t)) = (n, at, tracks.last_mut()) else {
                    return Err(format!("{}: bad INDEX line", cue.display()));
                };
                match n {
                    0 => t.idx0 = Some(at),
                    1 => t.idx1 = at,
                    _ => {}
                }
            }
        }
        if tracks.is_empty() {
            return Err(format!("{}: no tracks", cue.display()));
        }
        Ok(Disc { tracks })
    }

    /// Track `i`'s sectors in its file: from INDEX 01 to the next track's
    /// INDEX 00 (or 01) in the same file, else to the end of the file. The
    /// gaps between tracks are left out.
    pub fn range(&self, i: usize) -> io::Result<(u64, u64)> {
        let t = &self.tracks[i];
        let end = match self.tracks.get(i + 1) {
            Some(n) if n.file == t.file => n.idx0.unwrap_or(n.idx1),
            _ => std::fs::metadata(&t.file)?.len() / RAW,
        };
        Ok((t.idx1, end))
    }

    pub fn audio_tracks(&self) -> impl Iterator<Item = (usize, &Track)> {
        self.tracks.iter().enumerate().filter(|(_, t)| t.is_audio())
    }

    /// The first data track as a sector source.
    pub fn data_track(&self) -> Result<Sectors, String> {
        let t = self.tracks.iter().find(|t| !t.is_audio()).ok_or("no data track")?;
        let (size, user) = match t.mode.as_str() {
            "MODE1/2352" => (RAW, 16),
            "MODE2/2352" => (RAW, 24),
            "MODE1/2048" => (2048, 0),
            m => return Err(format!("unsupported data track mode {m}")),
        };
        let mut s = Sectors::open(&t.file, size, user).map_err(|e| format!("{}: {e}", t.file.display()))?;
        s.first = t.idx1;
        Ok(s)
    }

    /// The 16-bit stereo samples of track `i` (little-endian on the disc).
    pub fn audio(&self, i: usize) -> io::Result<Vec<i32>> {
        let (start, end) = self.range(i)?;
        let mut f = File::open(&self.tracks[i].file)?;
        f.seek(SeekFrom::Start(start * RAW))?;
        let mut buf = vec![0; ((end - start) * RAW) as usize];
        f.read_exact(&mut buf)?;
        Ok(buf.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]]) as i32).collect())
    }
}

/// The 2048-byte user data of the sectors of a data track.
pub struct Sectors {
    file: File,
    /// Raw sector size in the file, and where the user data starts in it.
    size: u64,
    user: u64,
    /// The track's first sector in the file.
    first: u64,
}

impl Sectors {
    pub fn open(path: &Path, size: u64, user: u64) -> io::Result<Sectors> {
        Ok(Sectors { file: File::open(path)?, size, user, first: 0 })
    }

    /// `len` bytes from sector `lba` on.
    pub fn read(&mut self, lba: u64, len: usize) -> io::Result<Vec<u8>> {
        let n = len.div_ceil(2048) as u64;
        let mut out = Vec::with_capacity(len);
        if self.size == 2048 {
            self.file.seek(SeekFrom::Start((self.first + lba) * 2048))?;
            out.resize(len, 0);
            self.file.read_exact(&mut out)?;
            return Ok(out);
        }
        self.file.seek(SeekFrom::Start((self.first + lba) * self.size))?;
        let mut raw = vec![0; (n * self.size) as usize];
        self.file.read_exact(&mut raw)?;
        for s in raw.chunks_exact(self.size as usize) {
            out.extend_from_slice(&s[self.user as usize..self.user as usize + 2048]);
        }
        out.truncate(len);
        Ok(out)
    }
}

/// Calls `f` with the `/`-separated path and the contents of every file of
/// the ISO 9660 file system (primary volume; the Amiga CD keeps its long
/// mixed-case names there, Latin-1, with `;1` versions, which are cut;
/// decoded as code page 437 like 7-Zip, see `crate::CP437`).
pub fn walk_iso(src: &mut Sectors, f: &mut dyn FnMut(&str, Vec<u8>) -> io::Result<()>) -> io::Result<()> {
    let pvd = src.read(16, 2048)?;
    if &pvd[1..6] != b"CD001" || pvd[0] != 1 {
        return Err(io::Error::other("no ISO 9660 primary volume descriptor"));
    }
    let root = &pvd[156..190];
    walk_dir(src, u32le(&root[2..]) as u64, u32le(&root[10..]) as usize, "", f, 0)
}

fn u32le(b: &[u8]) -> u32 {
    u32::from_le_bytes([b[0], b[1], b[2], b[3]])
}

fn walk_dir(
    src: &mut Sectors,
    lba: u64,
    len: usize,
    prefix: &str,
    f: &mut dyn FnMut(&str, Vec<u8>) -> io::Result<()>,
    depth: u32,
) -> io::Result<()> {
    if depth > 32 {
        return Err(io::Error::other("ISO directories nested too deep"));
    }
    let d = src.read(lba, len)?;
    let mut entries = Vec::new();
    let mut i = 0;
    while i < d.len() {
        let l = d[i] as usize;
        if l == 0 {
            // Records do not cross sectors: on to the next one.
            i = (i / 2048 + 1) * 2048;
            continue;
        }
        let r = &d[i..(i + l).min(d.len())];
        i += l;
        if r.len() < 34 {
            break;
        }
        let nl = r[32] as usize;
        let name = &r[33..(33 + nl).min(r.len())];
        if name == [0] || name == [1] {
            continue;
        }
        let mut name = crate::cp437(name);
        if let Some(p) = name.rfind(';') {
            name.truncate(p);
        }
        entries.push((name, u32le(&r[2..]) as u64, u32le(&r[10..]) as usize, r[25] & 2 != 0));
    }
    for (name, lba, len, dir) in entries {
        let path = if prefix.is_empty() { name } else { format!("{prefix}/{name}") };
        if dir {
            walk_dir(src, lba, len, &path, f, depth + 1)?;
        } else {
            let data = src.read(lba, len)?;
            f(&path, data)?;
        }
    }
    Ok(())
}
