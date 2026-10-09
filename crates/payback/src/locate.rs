//! Finding the game data on disk (native builds; the web build reads its
//! pack's fixed folders, `data.rs` / `vfs.rs`).
//!
//! The executable needs no setup: it looks for the Amiga data (the folder
//! holding `Payback.exe`, from `tools/extract/extract.py`) and the optional
//! Mac data (the Mac CD's `Payback` folder, from
//! `tools/extract/extract_mac.py`) in this order, first match wins:
//!
//! 1. `PAYBACK_DATA` / `PAYBACK_MAC_DATA`, then `--data <dir>` /
//!    `--mac-data <dir>` (a set but unusable value is logged and skipped);
//! 2. next to the executable: `data/amiga` and `data/mac`. The build puts
//!    them there (`build.rs` stages the files of `data_files.txt` into
//!    `target/<profile>/data/`), so a built profile folder, or a copy of
//!    it, runs as it is;
//! 3. the remembered folders in `payback-data.cfg` next to the executable
//!    (`game = <dir>` / `mac = <dir>` lines; stale entries are skipped);
//! 4. `extracted/game` / `extracted/mac/Payback` in the executable's folder
//!    or any folder above it, so `target/*/payback.exe` finds the
//!    repository's data from any working directory;
//! 5. the same in the working directory and the folders above it;
//! 6. Amiga data only: a folder picker. The choice is checked (it must hold
//!    `Payback.exe`, or be a folder with `extracted/game`) and remembered in
//!    `payback-data.cfg`. The Mac data is never asked for.
//!
//! Any of these folders may also be given one level up: a folder containing
//! `extracted/game` counts as the Amiga data, one containing the Mac
//! `Payback` folder as the Mac data.
//!
//! If nothing is found the game shows a message box and exits
//! ([`report_error`]). `PAYBACK_NO_DIALOG=1` skips the picker and message
//! boxes (headless runs, tests): the message goes to stderr only.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// The remembered data folders, next to the executable.
pub const CONFIG: &str = "payback-data.cfg";

const PICKER_TITLE: &str = "Select your Payback data folder (the folder containing Payback.exe)";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    /// The Amiga install (CD + Update 7), identified by `Payback.exe`.
    Game,
    /// The Mac release's `Payback` folder.
    Mac,
}

impl Kind {
    fn key(self) -> &'static str {
        match self {
            Kind::Game => "game",
            Kind::Mac => "mac",
        }
    }

    fn env(self) -> &'static str {
        match self {
            Kind::Game => "PAYBACK_DATA",
            Kind::Mac => "PAYBACK_MAC_DATA",
        }
    }

    fn arg(self) -> &'static str {
        match self {
            Kind::Game => "--data",
            Kind::Mac => "--mac-data",
        }
    }

    fn name(self) -> &'static str {
        match self {
            Kind::Game => "Payback data",
            Kind::Mac => "Mac data",
        }
    }

    /// The staged data next to the executable (`build.rs`).
    fn beside_exe(self) -> &'static str {
        match self {
            Kind::Game => "data/amiga",
            Kind::Mac => "data/mac",
        }
    }

    /// Where the extraction scripts put the data, relative to the
    /// repository root.
    fn extracted(self) -> &'static str {
        match self {
            Kind::Game => "extracted/game",
            Kind::Mac => "extracted/mac",
        }
    }

    /// `dir` holds this kind of data directly.
    fn is_root(self, dir: &Path) -> bool {
        match self {
            Kind::Game => dir.join("Payback.exe").is_file(),
            Kind::Mac => dir.join("CubeTextures").is_dir() || dir.join("Payback (OS X)").is_file(),
        }
    }
}

/// `dir` joined with the `/`-separated `rel`, with the platform's
/// separators (for readable log lines).
fn sub(dir: &Path, rel: &str) -> PathBuf {
    rel.split('/').fold(dir.to_path_buf(), |p, c| p.join(c))
}

/// The data root at or just inside `dir`: `dir` itself, the Mac CD's
/// `Payback` folder in it, or its `extracted/game` / `extracted/mac/Payback`.
pub fn resolve(kind: Kind, dir: &Path) -> Option<PathBuf> {
    let candidates = match kind {
        // The Mac CD root also holds `Payback (OS X)`: prefer its folder.
        Kind::Mac => vec![dir.join("Payback"), dir.to_path_buf(), sub(dir, "extracted/mac/Payback")],
        Kind::Game => vec![dir.to_path_buf(), sub(dir, "extracted/game")],
    };
    candidates.into_iter().find(|d| kind.is_root(d))
}

/// How a data root was found (for the log).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Found {
    Env,
    Arg,
    Remembered,
    BesideExe,
    AboveExe,
    WorkingDir,
    Picked,
}

impl Found {
    fn describe(self, kind: Kind) -> String {
        match self {
            Found::Env => format!("from {}", kind.env()),
            Found::Arg => format!("from {}", kind.arg()),
            Found::Remembered => format!("remembered in {CONFIG}"),
            Found::BesideExe => format!("{} next to the executable", kind.beside_exe()),
            Found::AboveExe => format!("{} at or above the executable's folder", kind.extracted()),
            Found::WorkingDir => format!("{} at or above the working directory", kind.extracted()),
            Found::Picked => "chosen in the folder picker".into(),
        }
    }
}

/// The inputs of the search, gathered from the process ([`Search::current`])
/// or set up by the tests.
#[derive(Default)]
pub struct Search {
    pub env: Option<PathBuf>,
    pub arg: Option<PathBuf>,
    pub exe_dir: Option<PathBuf>,
    pub cwd: Option<PathBuf>,
}

impl Search {
    pub fn current(kind: Kind) -> Search {
        Search {
            env: std::env::var_os(kind.env()).filter(|v| !v.is_empty()).map(PathBuf::from),
            arg: crate::arg(kind.arg()).map(PathBuf::from),
            exe_dir: exe_dir(),
            cwd: std::env::current_dir().ok(),
        }
    }

    /// Steps 1-5 of the search (module doc). Notes about skipped settings
    /// go to `notes`.
    pub fn find(&self, kind: Kind, notes: &mut Vec<String>) -> Option<(PathBuf, Found)> {
        let given = [(&self.env, Found::Env, kind.env()), (&self.arg, Found::Arg, kind.arg())];
        for (dir, how, what) in given {
            let Some(dir) = dir else { continue };
            match resolve(kind, dir) {
                Some(root) => return Some((root, how)),
                None => notes.push(format!(
                    "{what} = {} is not a {} folder; ignored",
                    dir.display(),
                    kind.name()
                )),
            }
        }
        if let Some(exe_dir) = &self.exe_dir {
            if let Some(root) = resolve(kind, &sub(exe_dir, kind.beside_exe())) {
                return Some((root, Found::BesideExe));
            }
            if let Some(dir) = remembered(exe_dir, kind) {
                match resolve(kind, &dir) {
                    Some(root) => return Some((root, Found::Remembered)),
                    None => notes.push(format!(
                        "{CONFIG}: {} = {} is no longer a {} folder; ignored",
                        kind.key(),
                        dir.display(),
                        kind.name()
                    )),
                }
            }
            if let Some(root) = in_ancestors(kind, exe_dir) {
                return Some((root, Found::AboveExe));
            }
        }
        if let Some(cwd) = &self.cwd {
            if let Some(root) = in_ancestors(kind, cwd) {
                return Some((root, Found::WorkingDir));
            }
        }
        None
    }
}

/// `extracted/...` in `dir` or the nearest folder above it that has one.
fn in_ancestors(kind: Kind, dir: &Path) -> Option<PathBuf> {
    dir.ancestors().find_map(|a| resolve(kind, &sub(a, kind.extracted())))
}

fn exe_dir() -> Option<PathBuf> {
    std::env::current_exe().ok()?.parent().map(Path::to_path_buf)
}

/// The `key = value` lines of `payback-data.cfg` in `exe_dir`.
fn read_config(exe_dir: &Path) -> Vec<(String, String)> {
    let Ok(text) = std::fs::read_to_string(exe_dir.join(CONFIG)) else {
        return Vec::new();
    };
    text.lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .filter(|(_, v)| !v.is_empty())
        .collect()
}

fn remembered(exe_dir: &Path, kind: Kind) -> Option<PathBuf> {
    read_config(exe_dir).into_iter().find(|(k, _)| k == kind.key()).map(|(_, v)| PathBuf::from(v))
}

/// Stores `dir` as the remembered `kind` folder in `exe_dir`'s
/// `payback-data.cfg`, keeping its other entries.
pub fn remember(exe_dir: &Path, kind: Kind, dir: &Path) -> std::io::Result<()> {
    let mut text = String::from(
        "# Data folders remembered by payback.exe (see README.md). Delete this file to search again.\n",
    );
    for (k, v) in read_config(exe_dir).iter().filter(|(k, _)| k != kind.key()) {
        text.push_str(&format!("{k} = {v}\n"));
    }
    text.push_str(&format!("{} = {}\n", kind.key(), dir.display()));
    std::fs::write(exe_dir.join(CONFIG), text)
}

/// Log lines about the search, written once Bevy's logger is up
/// (`data::log_data_roots`): (is a warning, text).
static LOG: Mutex<Vec<(bool, String)>> = Mutex::new(Vec::new());

fn log(warn: bool, text: String) {
    LOG.lock().unwrap().push((warn, text));
}

pub fn take_log() -> Vec<(bool, String)> {
    std::mem::take(&mut *LOG.lock().unwrap())
}

/// `PAYBACK_NO_DIALOG` is set: no picker or message boxes.
fn no_dialog() -> bool {
    std::env::var_os("PAYBACK_NO_DIALOG").is_some_and(|v| !v.is_empty() && v != "0")
}

fn search(kind: Kind) -> Option<(PathBuf, Found)> {
    let mut notes = Vec::new();
    let found = Search::current(kind).find(kind, &mut notes);
    for n in notes {
        log(true, n);
    }
    found
}

fn found(kind: Kind, root: &Path, how: Found) {
    log(false, format!("{}: {} ({})", kind.name(), root.display(), how.describe(kind)));
}

/// The Amiga data root: the search, else the folder picker.
pub fn locate_game() -> Result<PathBuf, String> {
    if let Some((root, how)) = search(Kind::Game) {
        found(Kind::Game, &root, how);
        return Ok(root);
    }
    if let Some(root) = pick_game() {
        found(Kind::Game, &root, Found::Picked);
        return Ok(root);
    }
    // The logger never starts: put the skipped settings in the message.
    let skipped: String =
        take_log().into_iter().filter(|(warn, _)| *warn).map(|(_, t)| format!("\n\nSkipped: {t}")).collect();
    Err(format!(
        "Payback data not found.\n\n\
         The game needs the files of the original Amiga release (the folder containing \
         Payback.exe). Unpack your CD with `python tools/extract/extract.py` (it writes \
         extracted/game) and build the game, which copies what it needs into data/amiga \
         next to payback.exe; or set PAYBACK_DATA (or pass --data <dir>) to its location.{skipped}"
    ))
}

/// The Mac data root, if the search finds one (never asked for).
pub fn locate_mac() -> Option<PathBuf> {
    let found_mac = search(Kind::Mac);
    match &found_mac {
        Some((root, how)) => found(Kind::Mac, root, *how),
        None => log(false, "Mac data: not found (optional)".into()),
    }
    found_mac.map(|(root, _)| root)
}

/// Asks for the Amiga data folder until a valid one is chosen or the
/// player gives up; remembers the choice.
fn pick_game() -> Option<PathBuf> {
    if no_dialog() {
        return None;
    }
    loop {
        let dir = rfd::FileDialog::new().set_title(PICKER_TITLE).pick_folder()?;
        if let Some(root) = resolve(Kind::Game, &dir) {
            if let Some(exe_dir) = exe_dir() {
                match remember(&exe_dir, Kind::Game, &root) {
                    Ok(()) => log(false, format!("remembered in {}", exe_dir.join(CONFIG).display())),
                    Err(e) => log(true, format!("could not write {}: {e}", exe_dir.join(CONFIG).display())),
                }
            }
            return Some(root);
        }
        let again = rfd::MessageDialog::new()
            .set_level(rfd::MessageLevel::Warning)
            .set_title("Payback")
            .set_description(format!(
                "{} does not contain Payback.exe.\n\nChoose the folder of the original Amiga \
                 release that holds Payback.exe (for example extracted/game).\n\n\
                 Choose another folder?",
                dir.display()
            ))
            .set_buttons(rfd::MessageButtons::YesNo)
            .show();
        if again != rfd::MessageDialogResult::Yes {
            return None;
        }
    }
}

/// A start-up failure: on stderr and, unless `PAYBACK_NO_DIALOG`, in a
/// message box (the game may have been started without a console).
pub fn report_error(text: &str) {
    eprintln!("{text}");
    if !no_dialog() {
        rfd::MessageDialog::new()
            .set_level(rfd::MessageLevel::Error)
            .set_title("Payback")
            .set_description(text)
            .set_buttons(rfd::MessageButtons::Ok)
            .show();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh scratch folder in the system temp directory.
    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("payback-locate-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn game_at(dir: &Path) -> PathBuf {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("Payback.exe"), b"").unwrap();
        dir.to_path_buf()
    }

    fn mac_at(dir: &Path) -> PathBuf {
        std::fs::create_dir_all(dir.join("CubeTextures")).unwrap();
        dir.to_path_buf()
    }

    fn find(kind: Kind, s: &Search) -> Option<(PathBuf, Found)> {
        s.find(kind, &mut Vec::new())
    }

    #[test]
    fn beside_exe_then_above_exe_then_working_dir() {
        let t = temp("order");
        let exe = t.join("repo/target/release");
        std::fs::create_dir_all(&exe).unwrap();
        let cwd_data = game_at(&t.join("elsewhere/extracted/game"));
        let s = Search { exe_dir: Some(exe.clone()), cwd: Some(t.join("elsewhere/sub")), ..Default::default() };
        assert_eq!(find(Kind::Game, &s), Some((cwd_data, Found::WorkingDir)));
        let repo_data = game_at(&t.join("repo/extracted/game"));
        assert_eq!(find(Kind::Game, &s), Some((repo_data, Found::AboveExe)));
        let beside = game_at(&exe.join("data/amiga"));
        assert_eq!(find(Kind::Game, &s), Some((beside, Found::BesideExe)));
        let _ = std::fs::remove_dir_all(&t);
    }

    #[test]
    fn env_and_args_win_and_bad_ones_are_skipped() {
        let t = temp("env");
        let exe = t.join("exe");
        let beside = game_at(&exe.join("data/amiga"));
        let other = game_at(&t.join("other"));
        let mut s = Search { env: Some(t.join("missing")), exe_dir: Some(exe), ..Default::default() };
        let mut notes = Vec::new();
        assert_eq!(s.find(Kind::Game, &mut notes), Some((beside, Found::BesideExe)));
        assert_eq!(notes.len(), 1);
        s.arg = Some(other.clone());
        assert_eq!(find(Kind::Game, &s), Some((other.clone(), Found::Arg)));
        // A repository root counts as its `extracted/game`.
        let repo = game_at(&t.join("repo/extracted/game"));
        s.env = Some(t.join("repo"));
        assert_eq!(find(Kind::Game, &s), Some((repo, Found::Env)));
        let _ = std::fs::remove_dir_all(&t);
    }

    #[test]
    fn remembered_path_and_stale_entry() {
        let t = temp("cfg");
        let exe = t.join("repo/exe");
        std::fs::create_dir_all(&exe).unwrap();
        let above = game_at(&t.join("repo/extracted/game"));
        let picked = game_at(&t.join("picked"));
        remember(&exe, Kind::Mac, &t.join("some mac")).unwrap();
        remember(&exe, Kind::Game, &picked).unwrap();
        assert_eq!(remembered(&exe, Kind::Mac), Some(t.join("some mac")), "other entries kept");
        let s = Search { exe_dir: Some(exe.clone()), ..Default::default() };
        assert_eq!(find(Kind::Game, &s), Some((picked.clone(), Found::Remembered)));
        // The remembered folder is gone: noted, and the search goes on.
        std::fs::remove_dir_all(&picked).unwrap();
        let mut notes = Vec::new();
        assert_eq!(s.find(Kind::Game, &mut notes), Some((above, Found::AboveExe)));
        assert!(notes[0].contains(CONFIG));
        // The staged data next to the executable comes first.
        game_at(&t.join("picked"));
        let beside = game_at(&exe.join("data/amiga"));
        assert_eq!(find(Kind::Game, &s), Some((beside, Found::BesideExe)));
        let _ = std::fs::remove_dir_all(&t);
    }

    #[test]
    fn mac_layouts() {
        let t = temp("mac");
        let exe = t.join("repo/target/debug");
        std::fs::create_dir_all(&exe).unwrap();
        let s = Search { exe_dir: Some(exe.clone()), ..Default::default() };
        assert_eq!(find(Kind::Mac, &s), None);
        // The extracted Mac CD: its `Payback` folder, not the CD root
        // (which also holds `Payback (OS X)`).
        std::fs::create_dir_all(t.join("repo/extracted/mac")).unwrap();
        std::fs::write(t.join("repo/extracted/mac/Payback (OS X)"), b"").unwrap();
        let repo = mac_at(&t.join("repo/extracted/mac/Payback"));
        assert_eq!(find(Kind::Mac, &s), Some((repo, Found::AboveExe)));
        let beside = mac_at(&exe.join("data/mac"));
        assert_eq!(find(Kind::Mac, &s), Some((beside, Found::BesideExe)));
        let _ = std::fs::remove_dir_all(&t);
    }

    #[test]
    fn nothing_found() {
        let t = temp("none");
        let s = Search { exe_dir: Some(t.join("a/b")), cwd: Some(t.join("c")), ..Default::default() };
        assert_eq!(find(Kind::Game, &s), None);
        let _ = std::fs::remove_dir_all(&t);
    }
}
