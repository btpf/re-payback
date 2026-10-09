//! Replay determinism (docs/port_engineering.md "Determinism"): record a
//! scripted run of the game, play the recording back, and require the world
//! hashes logged every 50 ticks (`replay.rs`, `debug_hash`) to match.
//!
//! The cases are those of `tools/checks/replay_check.py --suite`: a story level
//! in two cities, Rampage with random missions, a three-player match and a
//! run with cheats (replay events). This compares the port with itself;
//! nothing here can check it against the original game (its state is not
//! observable in this form, and its physics aren't reproduced bit for bit).
//!
//! Ignored by default: it opens game windows and needs the game data
//! (`PAYBACK_DATA`, or `extracted/game` in the repository). Run with
//! `cargo test -p payback --test replay_determinism -- --ignored`;
//! `REPLAY_CASES=story,cheats` picks cases, `REPLAY_FAST=4` runs faster.

use std::path::{Path, PathBuf};
use std::process::Command;

/// (name, ticks, record args); seeds fixed so failures reproduce.
const CASES: &[(&str, u32, &[&str])] = &[
    ("story", 1000, &["--map", "0", "--level", "0", "--seed", "1234"]),
    ("story2", 600, &["--map", "1", "--level", "1", "--seed", "4321"]),
    ("rampage", 800, &["--map", "1", "--rampage", "3", "--seed", "77"]),
    ("multiplayer", 600, &["--map", "2", "--multiplayer", "3", "--kills", "5", "--seed", "5"]),
    (
        "cheats",
        600,
        &["--map", "0", "--seed", "99", "--cheat-script", "100:god,150:vgod,200:weapons,300:vehicle=5,450:nogod,500:novgod"],
    ),
];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn data_present(root: &Path) -> bool {
    std::env::var_os("PAYBACK_DATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("extracted/game"))
        .join("Payback.exe")
        .is_file()
}

/// Run the game and return its `replay hash` lines.
fn hashes(root: &Path, args: &[&str]) -> Vec<String> {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_payback"));
    cmd.current_dir(root).args(args);
    if let Ok(n) = std::env::var("REPLAY_FAST") {
        cmd.args(["--fast", &n]);
    }
    let out = cmd.output().expect("run payback");
    let text = String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "payback {args:?} failed: {}", text.lines().rev().take(8).collect::<Vec<_>>().join("\n"));
    text.lines()
        .filter_map(|l| l.split_once("replay hash: ").map(|(_, h)| h.trim().to_string()))
        .collect()
}

#[test]
#[ignore = "opens game windows; needs the game data"]
fn replays_are_deterministic() {
    let root = root();
    if !data_present(&root) {
        eprintln!("game data not found; skipped");
        return;
    }
    let pick = std::env::var("REPLAY_CASES").ok();
    let out = root.join("target/replay_check");
    std::fs::create_dir_all(&out).unwrap();
    let mut failed = Vec::new();
    for &(name, ticks, extra) in CASES {
        if pick.as_ref().is_some_and(|p| !p.split(',').any(|c| c.trim() == name)) {
            continue;
        }
        let file = out.join(format!("test_{name}"));
        let file = file.to_str().unwrap();
        let ticks = ticks.to_string();
        let mut rec_args = vec!["--record", file, "--script-input", "--ticks", &ticks];
        rec_args.extend_from_slice(extra);
        let rec = hashes(&root, &rec_args);
        let play = hashes(&root, &["--play", file, "--ticks", &ticks]);
        assert!(!rec.is_empty(), "{name}: no hashes logged");
        if rec == play {
            eprintln!("{name}: MATCH ({} checkpoints, last: {})", rec.len(), rec.last().unwrap());
        } else {
            let at = rec.iter().zip(&play).position(|(a, b)| a != b);
            eprintln!("{name}: DIFFER at checkpoint {at:?} (rec {} / play {})", rec.len(), play.len());
            failed.push(name);
        }
    }
    assert!(failed.is_empty(), "replays diverged: {failed:?}");
}
