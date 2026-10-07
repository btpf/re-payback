//! Parse every file of the original game. Skipped when the data is absent;
//! point `PAYBACK_DATA` at an install, or run `python tools/extract.py`.

use payback_formats::{CubeTextureList, IndexedImage, Map, Obj, Phrases, palette};
use std::path::{Path, PathBuf};

fn data_dir() -> Option<PathBuf> {
    let dir = std::env::var_os("PAYBACK_DATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../extracted/game"));
    if dir.is_dir() {
        Some(dir)
    } else {
        eprintln!("skipping: no game data at {}", dir.display());
        None
    }
}

fn files(dir: &Path, filter: &dyn Fn(&Path) -> bool) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                stack.push(p);
            } else if filter(&p) {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

fn ext_is(p: &Path, ext: &str) -> bool {
    p.extension().is_some_and(|e| e.eq_ignore_ascii_case(ext))
}

#[test]
fn all_objects_parse() {
    let Some(dir) = data_dir() else { return };
    let objs = files(&dir.join("Objects"), &|p| ext_is(p, "obj"));
    assert!(objs.len() > 100);
    for p in objs {
        let o = Obj::parse(&std::fs::read(&p).unwrap())
            .unwrap_or_else(|e| panic!("{}: {e}", p.display()));
        for f in &o.faces {
            let t = &f.active_slot().texture;
            assert!(
                t.is_empty() || dir.join("ObjectTextures").join(t).exists()
                    // Names are case-insensitive on AmigaOS.
                    || std::fs::read_dir(dir.join("ObjectTextures")).unwrap().any(|e| {
                        e.unwrap().file_name().to_string_lossy().eq_ignore_ascii_case(t)
                    }),
                "{}: missing texture {t:?}",
                p.display()
            );
        }
    }
}

#[test]
fn texture_list_resolves() {
    let Some(dir) = data_dir() else { return };
    let tex = dir.join("CubeTextures");
    let list =
        CubeTextureList::parse(&std::fs::read(tex.join(".CubeTextureList")).unwrap()).unwrap();
    assert!(list.entries.len() > 800);
    for e in &list.entries {
        let data = std::fs::read(tex.join(&e.file)).unwrap_or_else(|_| panic!("{}", e.file));
        IndexedImage::cube_texture(&data).unwrap();
    }
}

#[test]
fn all_phrases_parse() {
    let Some(dir) = data_dir() else { return };
    let ph = files(&dir.join("Phrases"), &|p| ext_is(p, "phrases"));
    assert!(ph.len() > 50);
    for p in ph {
        let parsed = Phrases::parse(&std::fs::read(&p).unwrap())
            .unwrap_or_else(|e| panic!("{}: {e}", p.display()));
        assert!(!parsed.by_number.is_empty(), "{}", p.display());
    }
}

#[test]
fn all_maps_parse() {
    let Some(dir) = data_dir() else { return };
    let maps: Vec<_> = ["Maps", "UserMaps"]
        .iter()
        .flat_map(|d| files(&dir.join(d), &|p| ext_is(p, "map")))
        .collect();
    assert!(maps.len() >= 4);
    let list =
        CubeTextureList::parse(&std::fs::read(dir.join("CubeTextures/.CubeTextureList")).unwrap())
            .unwrap();
    for p in maps {
        let m = Map::parse(&std::fs::read(&p).unwrap())
            .unwrap_or_else(|e| panic!("{}: {e}", p.display()));
        for sq in &m.squares {
            for c in &sq.cubes {
                for f in c.faces.iter().filter(|f| f.visible()) {
                    assert!(
                        (f.texture as usize) < list.entries.len(),
                        "{}: texture {}",
                        p.display(),
                        f.texture
                    );
                }
            }
        }
    }
}

#[test]
fn palette_from_executable() {
    let Some(dir) = data_dir() else { return };
    let pal =
        palette::find_in_executable(&std::fs::read(dir.join("Payback.exe")).unwrap()).unwrap();
    // Grass ramp (0x7a-0x7d) is green.
    for i in 0x7a..=0x7d {
        let (r, g, b) = (pal[i * 3], pal[i * 3 + 1], pal[i * 3 + 2]);
        assert!(g > r && g > b, "colour {i:#x} = {r},{g},{b}");
    }
}

#[test]
fn routes_parse_and_connect() {
    let Some(dir) = data_dir() else { return };
    for n in 0..3 {
        let r = payback_formats::Route::parse(
            &std::fs::read(dir.join(format!("Maps/{n}.route"))).unwrap(),
        )
        .unwrap();
        let used: Vec<usize> = (0..r.nodes.len()).filter(|&i| r.nodes[i].used).collect();
        assert!(used.len() > 300, "map {n}: {} nodes", used.len());
        // Following next hops from the first node reaches the last one.
        let (mut at, dest) = (used[0], *used.last().unwrap());
        for _ in 0..1024 {
            if at == dest {
                break;
            }
            at = r.next(at, dest).unwrap();
        }
        assert_eq!(at, dest, "map {n}");
    }
}

#[test]
fn intro_video_decodes_and_loops() {
    let Some(dir) = data_dir() else { return };
    let Ok(bytes) = std::fs::read(dir.join("Videos/intro.video")) else { return };
    let mut flc = payback_formats::flc::Flc::parse(bytes).unwrap();
    assert_eq!((flc.frames, flc.width, flc.height), (450, 320, 200));
    assert!(flc.next_frame(false).unwrap());
    let first = flc.pixels.clone();
    while flc.next_frame(true).unwrap() {}
    // The ring frame after the last one reproduces frame 0 exactly.
    assert_eq!(flc.position(), 451);
    assert_eq!(flc.pixels, first);
}
