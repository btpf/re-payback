//! `payback-extract`: unpacks your copy of the original game next to
//! `payback.exe`, so the port runs without any other tool.
//!
//! Put the original files into `Payback Game + Updates/` next to the
//! executable (the build creates the empty folders):
//!
//! - `Amiga/`: the CD, as `.iso` (the data track), `.cue` with its `.bin`
//!   file(s) (one for the disc or one per track, as in the Redump set), or
//!   a `.zip` / `.rar` holding either;
//! - `Amiga/Updates/`: `PaybackUpdate7.lha` (`paybackupdateOS4Native.lha`
//!   and `Warp3D-42.lha` are listed but not used: the port does not need
//!   them);
//! - `MacOS/`: `PaybackMac.toast`.
//!
//! The tool lists what it found, then writes the files the port reads
//! (`crates/payback/data_files.txt`) into `data/amiga` (the CD's `Payback`
//! folder with Update 7 laid over it, as `tools/extract/extract.py` does)
//! and `data/mac` (the Mac CD's `Payback` folder), and the CD's audio
//! tracks 2-16 into `Music/NN.flac` (from a `.cue` image; a plain `.iso`
//! has no audio). Each output folder is replaced as a whole.
//!
//! Options: `--list` (only list), `--no-music`, `--root <dir>` (default:
//! the executable's folder), `--no-pause`.

mod cd;
mod hfs;

use payback_formats::data_files::Rules;
use std::collections::HashMap;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

const SOURCE: &str = "Payback Game + Updates";
const RULES: &str = include_str!("../../payback/data_files.txt");

const UPDATE7: &str = "PaybackUpdate7.lha";
const OS4: &str = "paybackupdateOS4Native.lha";
const WARP3D: &str = "Warp3D-42.lha";
const TOAST: &str = "PaybackMac.toast";

/// Characters Windows does not allow in names (the Python extractors'
/// replacement, so the names match theirs).
fn safe(name: &str) -> String {
    let s: String = name.chars().map(|c| if "<>:\"\\|?*".contains(c) || (c as u32) < 32 { '_' } else { c }).collect();
    let s = s.trim_end_matches([' ', '.']);
    if s.is_empty() { "_".into() } else { s.into() }
}

/// Code page 437, 0x80..=0xFF. The Amiga names (Latin-1, e.g. `Español`)
/// are decoded as 437 (`Espa±ol`), as 7-Zip did for `tools/extract/`; the
/// port expects that and maps them back (`frontend::misc::language_name`).
const CP437: &str = "ÇüéâäàåçêëèïîìÄÅÉæÆôöòûùÿÖÜ¢£¥₧ƒáíóúñÑªº¿⌐¬½¼¡«»░▒▓│┤╡╢╖╕╣║╗╝╜╛┐└┴┬├─┼╞╟╚╔╩╦╠═╬╧╨╤╥╙╘╒╓╫╪┘┌█▄▌▐▀αßΓπΣσµτΦΘΩδ∞φε∩≡±≥≤⌠⌡÷≈°∙·√ⁿ²■\u{a0}";

pub fn cp437(b: &[u8]) -> String {
    b.iter().map(|&c| if c < 0x80 { c as char } else { CP437.chars().nth(c as usize - 0x80).unwrap_or('_') }).collect()
}

/// A source of the Amiga CD.
#[derive(Clone, Debug)]
enum Cd {
    /// A data-track image (`.iso`).
    Iso(PathBuf),
    /// A cue sheet with its `.bin` files.
    Cue(PathBuf),
    /// A `.zip` / `.rar` holding one of those (`.cue` preferred); the
    /// member's name.
    Archive(PathBuf, String),
}

impl Cd {
    fn has_audio(&self) -> bool {
        match self {
            Cd::Iso(_) => false,
            Cd::Cue(p) => cd::Disc::parse(p).is_ok_and(|d| d.audio_tracks().next().is_some()),
            Cd::Archive(_, m) => m.to_lowercase().ends_with(".cue"),
        }
    }

    fn describe(&self) -> String {
        let name = |p: &Path| p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        match self {
            Cd::Iso(p) => format!("{} (data track only, no music)", name(p)),
            Cd::Cue(p) => match cd::Disc::parse(p) {
                Ok(d) => {
                    let audio = d.audio_tracks().count();
                    let missing = d.tracks.iter().filter(|t| !t.file.is_file()).count();
                    let miss = if missing > 0 { format!("; {missing} .bin file(s) MISSING") } else { String::new() };
                    format!("{} (data track + {audio} audio tracks{miss})", name(p))
                }
                Err(e) => format!("{} (unreadable: {e})", name(p)),
            },
            Cd::Archive(p, m) => format!("{} (holds {m})", name(p)),
        }
    }
}

/// What was found in `Payback Game + Updates/`.
#[derive(Default)]
struct Found {
    cds: Vec<Cd>,
    /// Files that are neither a CD image nor an archive holding one.
    ignored: Vec<String>,
    update7: Option<PathBuf>,
    os4: Option<PathBuf>,
    warp3d: Option<PathBuf>,
    toast: Option<PathBuf>,
}

fn files_in(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .collect();
    v.sort();
    v
}

fn ext(p: &Path) -> String {
    p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default()
}

fn named(dir: &Path, name: &str) -> Option<PathBuf> {
    files_in(dir).into_iter().find(|p| p.file_name().is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(name)))
}

/// The member names of a `.zip` or `.rar`.
fn archive_names(p: &Path) -> Result<Vec<String>, String> {
    if ext(p) == "zip" {
        let z = zip::ZipArchive::new(std::fs::File::open(p).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        Ok(z.file_names().map(str::to_string).collect())
    } else {
        let list = unrar::Archive::new(p).open_for_listing().map_err(|e| e.to_string())?;
        list.map(|h| h.map(|h| h.filename.to_string_lossy().replace('\\', "/")).map_err(|e| e.to_string())).collect()
    }
}

fn scan(src: &Path) -> Found {
    let mut f = Found::default();
    let amiga = src.join("Amiga");
    let mut cues = Vec::new();
    let mut bins = Vec::new();
    let mut archives = Vec::new();
    for p in files_in(&amiga) {
        let name = p.file_name().unwrap().to_string_lossy().into_owned();
        match ext(&p).as_str() {
            "iso" => f.cds.push(Cd::Iso(p)),
            "cue" => cues.push(p),
            "bin" => bins.push(p),
            "zip" | "rar" => match archive_names(&p) {
                Ok(names) => {
                    let pick = |e: &str| names.iter().find(|n| n.to_lowercase().ends_with(e)).cloned();
                    match pick(".cue").or_else(|| pick(".iso")) {
                        Some(m) => archives.push(Cd::Archive(p, m)),
                        None => f.ignored.push(format!("{name} (no .cue or .iso inside)")),
                    }
                }
                Err(e) => f.ignored.push(format!("{name} (cannot read: {e})")),
            },
            _ => f.ignored.push(name),
        }
    }
    // A .bin counts through its cue sheet.
    let used: Vec<PathBuf> = cues
        .iter()
        .filter_map(|c| cd::Disc::parse(c).ok())
        .flat_map(|d| d.tracks.into_iter().map(|t| t.file))
        .collect();
    for b in bins {
        if !used.iter().any(|u| u.file_name() == b.file_name()) {
            f.ignored.push(format!("{} (a .bin needs its .cue)", b.file_name().unwrap().to_string_lossy()));
        }
    }
    f.cds.extend(cues.into_iter().map(Cd::Cue));
    f.cds.extend(archives);
    let updates = amiga.join("Updates");
    f.update7 = named(&updates, UPDATE7);
    f.os4 = named(&updates, OS4);
    f.warp3d = named(&updates, WARP3D);
    f.toast = named(&src.join("MacOS"), TOAST)
        .or_else(|| files_in(&src.join("MacOS")).into_iter().find(|p| ext(p) == "toast"));
    f
}

/// Archives unpacked for this run, removed when done.
struct Scratch {
    dir: PathBuf,
    unpacked: HashMap<PathBuf, PathBuf>,
}

impl Scratch {
    fn new(src: &Path) -> Scratch {
        Scratch { dir: src.join(".extract-tmp"), unpacked: HashMap::new() }
    }

    /// The folder `archive` was unpacked into.
    fn unpack(&mut self, archive: &Path) -> Result<PathBuf, String> {
        if let Some(d) = self.unpacked.get(archive) {
            return Ok(d.clone());
        }
        let out = self.dir.join(self.unpacked.len().to_string());
        let _ = std::fs::remove_dir_all(&out);
        std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
        println!("  unpacking {} ...", archive.file_name().unwrap().to_string_lossy());
        if ext(archive) == "zip" {
            let mut z = zip::ZipArchive::new(std::fs::File::open(archive).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
            z.extract(&out).map_err(|e| e.to_string())?;
        } else {
            let mut a = unrar::Archive::new(archive).open_for_processing().map_err(|e| e.to_string())?;
            while let Some(h) = a.read_header().map_err(|e| e.to_string())? {
                a = if h.entry().is_file() { h.extract_with_base(&out) } else { h.skip() }.map_err(|e| e.to_string())?;
            }
        }
        self.unpacked.insert(archive.to_path_buf(), out.clone());
        Ok(out)
    }

    /// The loose image a CD source stands for.
    fn resolve(&mut self, cd: &Cd) -> Result<Cd, String> {
        match cd {
            Cd::Archive(p, member) => {
                let path = self.unpack(p)?.join(member);
                Ok(if member.to_lowercase().ends_with(".cue") { Cd::Cue(path) } else { Cd::Iso(path) })
            }
            other => Ok(other.clone()),
        }
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        if self.dir.exists() {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }
}

/// Writes the selected files into a fresh `dst`.
struct Out<'a> {
    dst: PathBuf,
    rules: &'a Rules,
    files: usize,
    bytes: u64,
}

impl<'a> Out<'a> {
    /// Empties `dst` first. The build hard-links its staged data, so files
    /// are removed, never written through.
    fn new(dst: PathBuf, rules: &'a Rules) -> io::Result<Out<'a>> {
        if dst.exists() {
            std::fs::remove_dir_all(&dst)?;
        }
        std::fs::create_dir_all(&dst)?;
        Ok(Out { dst, rules, files: 0, bytes: 0 })
    }

    /// `rel` is `/`-separated, below the data root.
    fn put(&mut self, rel: &str, data: &[u8]) -> io::Result<()> {
        if !self.rules.keep(rel) {
            return Ok(());
        }
        let path = rel.split('/').filter(|c| !c.is_empty()).fold(self.dst.clone(), |p, c| p.join(safe(c)));
        std::fs::create_dir_all(path.parent().unwrap())?;
        // `dst` started empty, so an existing file is one of ours (no
        // build hard link): overwrite it in place, which keeps the CD's
        // spelling of the name (Update 7 writes `bigpager.0` over the CD's
        // `BigPager.0`), as copying Update 7 over the CD did.
        if !path.exists() {
            self.files += 1;
        }
        std::fs::write(&path, data)?;
        self.bytes += data.len() as u64;
        Ok(())
    }

    fn summary(&self) -> String {
        format!("{} files, {:.1} MB -> {}", self.files, self.bytes as f64 / 1e6, self.dst.display())
    }
}

/// The CD's `Payback/` folder, then Update 7's `PaybackUpdate7/Files/` over
/// it, into `data/amiga`.
fn extract_amiga(cd: &Cd, update7: Option<&Path>, root: &Path, rules: &Rules) -> Result<String, String> {
    let mut sectors = match cd {
        Cd::Iso(p) => cd::Sectors::open(p, 2048, 0).map_err(|e| format!("{}: {e}", p.display()))?,
        Cd::Cue(p) => cd::Disc::parse(p)?.data_track()?,
        Cd::Archive(..) => unreachable!("resolved before"),
    };
    let mut out = Out::new(root.join("data").join("amiga"), rules).map_err(|e| e.to_string())?;
    let mut found_exe = false;
    cd::walk_iso(&mut sectors, &mut |path, data| {
        let Some(rel) = strip_prefix_ci(path, "Payback/") else { return Ok(()) };
        found_exe |= rel.eq_ignore_ascii_case("Payback.exe");
        out.put(rel, &data)
    })
    .map_err(|e| format!("reading the CD: {e}"))?;
    if !found_exe {
        return Err("the CD image has no Payback/Payback.exe: is it the Payback CD?".into());
    }
    if let Some(lha) = update7 {
        let n = lha_files(lha, &mut |path, data| match strip_prefix_ci(path, "PaybackUpdate7/Files/") {
            Some(rel) => out.put(rel, &data).map(|()| true),
            None => Ok(false),
        })
        .map_err(|e| format!("{}: {e}", lha.display()))?;
        if n == 0 {
            return Err(format!("{}: no PaybackUpdate7/Files/ inside", lha.display()));
        }
    }
    Ok(out.summary())
}

fn strip_prefix_ci<'a>(path: &'a str, prefix: &str) -> Option<&'a str> {
    let head = path.get(..prefix.len())?;
    head.eq_ignore_ascii_case(prefix).then(|| &path[prefix.len()..]).filter(|r| !r.is_empty())
}

/// Calls `f` for every file of an LHA archive; counts the calls that
/// returned true.
fn lha_files(path: &Path, f: &mut dyn FnMut(&str, Vec<u8>) -> io::Result<bool>) -> io::Result<usize> {
    let mut lha = delharc::parse_file(path)?;
    let mut n = 0;
    loop {
        let h = lha.header();
        if !h.is_directory() {
            let name = lha_name(h);
            if !lha.is_decoder_supported() {
                return Err(io::Error::other(format!("{name}: unsupported compression")));
            }
            let mut data = Vec::new();
            lha.read_to_end(&mut data)?;
            lha.crc_check().map_err(|e| io::Error::other(format!("{name}: {e}")))?;
            if f(&name, data)? {
                n += 1;
            }
        }
        if !lha.next_file().map_err(|e| io::Error::other(e.to_string()))? {
            break;
        }
    }
    Ok(n)
}

/// An LHA entry's `/`-separated path, decoded as code page 437 (see
/// [`CP437`]): the path and name extended headers, else the base header's
/// name (Amiga archives end it at a NUL, the file comment follows).
fn lha_name(h: &delharc::LhaHeader) -> String {
    let (mut dir, mut file) = (None, None);
    for x in h.iter_extra() {
        match x {
            [1, name @ ..] => file = Some(name),
            [2, path @ ..] => dir = Some(path),
            _ => {}
        }
    }
    let base = h.filename.split(|&c| c == 0).next().unwrap_or(&[]);
    let mut parts: Vec<String> = Vec::new();
    for raw in [dir.unwrap_or(&[]), file.unwrap_or(base)] {
        parts.extend(raw.split(|&c| c == 0xFF || c == b'/' || c == b'\\').filter(|p| !p.is_empty()).map(cp437));
    }
    parts.join("/")
}

/// The Mac CD's `Payback/` folder into `data/mac`.
fn extract_mac(toast: &Path, root: &Path, rules: &Rules) -> Result<String, String> {
    let img = std::fs::read(toast).map_err(|e| format!("{}: {e}", toast.display()))?;
    let vol = hfs::Volume::open(&img).map_err(|e| format!("{}: {e}", toast.display()))?;
    let mut out = Out::new(root.join("data").join("mac"), rules).map_err(|e| e.to_string())?;
    let mut any = false;
    vol.walk(&safe, &mut |path, data| {
        let Some(rel) = strip_prefix_ci(path, "Payback/") else { return Ok(()) };
        any = true;
        out.put(rel, &data)
    })
    .map_err(|e| format!("{}: {e}", toast.display()))?;
    if !any {
        return Err(format!("{}: no Payback folder on the volume", toast.display()));
    }
    Ok(out.summary())
}

/// Tracks 2-16 of the cue image as `Music/NN.flac`, a few at a time.
fn extract_music(cue: &Path, root: &Path) -> Result<String, String> {
    let disc = cd::Disc::parse(cue)?;
    let music = root.join("Music");
    std::fs::create_dir_all(&music).map_err(|e| e.to_string())?;
    let jobs: Vec<(usize, u32)> = disc.audio_tracks().map(|(i, t)| (i, t.num)).collect();
    let next = std::sync::Mutex::new(jobs.iter());
    let threads = std::thread::available_parallelism().map_or(2, |n| n.get()).clamp(1, 4);
    let errors = std::sync::Mutex::new(Vec::new());
    std::thread::scope(|s| {
        for _ in 0..threads {
            s.spawn(|| {
                loop {
                    let Some(&(i, num)) = next.lock().unwrap().next() else { break };
                    let path = music.join(format!("{num:02}.flac"));
                    let r = disc.audio(i).map_err(|e| e.to_string()).and_then(|pcm| encode_flac(&pcm, &path));
                    match r {
                        Ok(secs) => println!("  track {num:2}  {}:{:02}  {}", secs / 60, secs % 60, path.display()),
                        Err(e) => errors.lock().unwrap().push(format!("track {num}: {e}")),
                    }
                }
            });
        }
    });
    let errors = errors.into_inner().unwrap();
    if !errors.is_empty() {
        return Err(errors.join("; "));
    }
    Ok(format!("{} tracks -> {}", jobs.len(), music.display()))
}

/// 16-bit stereo 44.1 kHz samples as FLAC; returns the length in seconds.
fn encode_flac(pcm: &[i32], path: &Path) -> Result<u64, String> {
    use flacenc::component::BitRepr;
    use flacenc::error::Verify;
    // flacenc pads the last block with silence; a CD track is whole
    // sectors of 588 stereo samples, so a block of 588·d samples, d
    // dividing the sector count, ends exactly on its last sample.
    let sectors = pcm.len() / 2 / 588;
    let d = (1..=8).rev().find(|d| sectors % d == 0).unwrap_or(1);
    let mut config = flacenc::config::Encoder::default();
    config.block_size = 588 * d;
    let config = config.into_verified().map_err(|e| format!("{e:?}"))?;
    let source = flacenc::source::MemSource::from_samples(pcm, 2, 16, 44100);
    let stream =
        flacenc::encode_with_fixed_block_size(&config, source, config.block_size).map_err(|e| format!("{e:?}"))?;
    let mut sink = flacenc::bitsink::ByteSink::new();
    stream.write(&mut sink).map_err(|e| format!("{e:?}"))?;
    if path.exists() {
        std::fs::remove_file(path).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, sink.as_slice()).map_err(|e| e.to_string())?;
    Ok(pcm.len() as u64 / 2 / 44100)
}

fn mark(found: bool) -> &'static str {
    if found { "[x]" } else { "[ ]" }
}

/// This process owns its console window (started from Explorer, not from
/// a shell), so the window would close before the output can be read.
#[cfg(windows)]
fn own_console() -> bool {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetConsoleProcessList(list: *mut u32, count: u32) -> u32;
    }
    let mut ids = [0u32; 4];
    // SAFETY: the buffer holds `ids.len()` entries.
    unsafe { GetConsoleProcessList(ids.as_mut_ptr(), ids.len() as u32) <= 1 }
}

#[cfg(not(windows))]
fn own_console() -> bool {
    false
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let has = |a: &str| args.iter().any(|x| x == a);
    if has("--help") || has("-h") {
        println!("{}", include_str!("main.rs").lines().take_while(|l| l.starts_with("//!")).map(|l| l.trim_start_matches("//!").strip_prefix(' ').unwrap_or("")).collect::<Vec<_>>().join("\n"));
        return;
    }
    let root = args
        .iter()
        .position(|a| a == "--root")
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from)
        .or_else(|| std::env::current_exe().ok()?.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."));
    let ok = run(&root, has("--list"), has("--no-music"));
    if !has("--no-pause") && own_console() {
        print!("\nPress Enter to close this window.");
        let _ = io::stdout().flush();
        let _ = io::stdin().read_line(&mut String::new());
    }
    std::process::exit(if ok { 0 } else { 1 });
}

fn run(root: &Path, list_only: bool, no_music: bool) -> bool {
    let src = root.join(SOURCE);
    for d in [src.join("Amiga").join("Updates"), src.join("MacOS")] {
        let _ = std::fs::create_dir_all(d);
    }
    println!("Payback extractor\nLooking in {}\n", src.display());
    let found = scan(&src);

    println!("Amiga/  (the CD)");
    if found.cds.is_empty() {
        println!("  {} no CD image (.iso, .cue + .bin, or a .zip / .rar of them)", mark(false));
    }
    for c in &found.cds {
        println!("  {} {}", mark(true), c.describe());
    }
    for i in &found.ignored {
        println!("      ignored: {i}");
    }
    println!("Amiga/Updates/");
    println!("  {} {UPDATE7}  (laid over the CD)", mark(found.update7.is_some()));
    println!("  {} {OS4}  (not used by the game)", mark(found.os4.is_some()));
    println!("  {} {WARP3D}  (not used by the game)", mark(found.warp3d.is_some()));
    println!("MacOS/");
    println!("  {} {TOAST}", mark(found.toast.is_some()));
    println!();

    // Plan: the data from the first CD source (an .iso needs no unpacking),
    // the music from the first one with audio tracks.
    let data_cd = found.cds.first().cloned();
    let music_cd = if no_music { None } else { found.cds.iter().find(|c| c.has_audio()).cloned() };
    if data_cd.is_none() && found.toast.is_none() {
        println!("Nothing to extract: put the Amiga CD into Amiga/ and/or {TOAST} into MacOS/.");
        return false;
    }
    if data_cd.is_some() && found.update7.is_none() {
        println!(
            "Note: without {UPDATE7} the Amiga data is the original pre-update release \
             (older palette, textures and vehicles)."
        );
    }
    if data_cd.is_none() && found.update7.is_some() {
        println!("Note: {UPDATE7} alone is not the game; it is laid over the CD. Skipping the Amiga data.");
    }
    if !no_music && music_cd.is_none() {
        println!("Note: no music: it needs a .cue + .bin image of the CD (an .iso holds only the data track).");
    }
    if list_only {
        return true;
    }

    let rules = Rules::parse(RULES).expect("data_files.txt");
    let mut scratch = Scratch::new(&src);
    let mut ok = true;
    let t0 = Instant::now();
    let mut step = |what: &str, r: Result<String, String>| match r {
        Ok(s) => println!("  {what}: {s}"),
        Err(e) => {
            println!("  {what}: FAILED: {e}");
            ok = false;
        }
    };
    if let Some(cd) = &data_cd {
        println!("Extracting the Amiga data ...");
        let r = scratch.resolve(cd).and_then(|cd| extract_amiga(&cd, found.update7.as_deref(), root, &rules));
        step("Amiga", r);
    }
    if let Some(toast) = &found.toast {
        println!("Extracting the Mac data ...");
        step("Mac", extract_mac(toast, root, &rules));
    }
    if let Some(cd) = &music_cd {
        println!("Extracting the music (FLAC) ...");
        let r = scratch.resolve(cd).and_then(|cd| match cd {
            Cd::Cue(p) => extract_music(&p, root),
            _ => Err("not a cue image".into()),
        });
        step("Music", r);
    }
    drop(scratch);
    println!(
        "\n{} in {:.0} s.",
        if ok { "Done" } else { "Finished with errors" },
        t0.elapsed().as_secs_f64()
    );
    if ok {
        println!("Start payback.exe to play.");
    }
    ok
}
