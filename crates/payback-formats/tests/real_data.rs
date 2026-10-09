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

/// The map editor's next-hop algorithm (`Route::calculate_routing`)
/// reproduces the shipped table bit for bit.
#[test]
fn routes_recalculate_like_the_editor() {
    let Some(dir) = data_dir() else { return };
    let stored = payback_formats::Route::parse(&std::fs::read(dir.join("Maps/0.route")).unwrap()).unwrap();
    let mut r = stored.clone();
    r.calculate_routing();
    assert_eq!(r, stored);
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

#[test]
fn shipped_replays_parse() {
    let Some(dir) = data_dir() else { return };
    for (name, city, records, ticks) in [("Race for the prize", 1, 1211, 5337), ("Tanks a lot", 0, 1650, 7586)] {
        let Ok(bytes) = std::fs::read(dir.join("Replays").join(name)) else { continue };
        let r = payback_formats::replay::Replay::parse(&bytes).unwrap();
        assert_eq!(r.header.version, 3);
        assert_eq!(r.city(), Some(city));
        assert_eq!(r.records.len(), records);
        let players = r.header.players.max(1) as usize;
        let sum: i32 = r.records.iter().step_by(players).map(|x| x.input.ticks as i32).sum();
        assert_eq!(sum, ticks);
        assert_eq!(r.header.duration, ticks);
    }
}

#[test]
fn map_vehicles_parse() {
    let Some(dir) = data_dir() else { return };
    for n in 0..3 {
        let Ok(b) = std::fs::read(dir.join(format!("Maps/{n}.vehicles"))) else { continue };
        let v = payback_formats::vehicles::MapVehicles::parse(&b).unwrap();
        // Each shipped city allows only its own home-city traffic.
        assert_eq!(v.cities, std::array::from_fn(|i| i == n));
        assert!(!v.parked.is_empty() && v.parked.iter().all(|p| (0..28).contains(&p.model)));
    }
}

#[test]
fn launcher_icon_decodes() {
    let Some(dir) = data_dir() else { return };
    let Ok(b) = std::fs::read(dir.join("Payback.info")) else { return };
    let colour = payback_formats::icon::colour_images(&b).unwrap();
    assert_eq!(colour.len(), 2);
    assert_eq!((colour[0].width, colour[0].height), (66, 30));
    let classic = payback_formats::icon::classic_images(&b).unwrap();
    assert_eq!((classic[0].width, classic[0].height), (62, 27));
    // Workbench shows the colour image.
    assert_eq!(payback_formats::icon::workbench_image(&b).unwrap(), colour[0]);
}

#[test]
fn mac_app_icon_decodes() {
    let Some(dir) = data_dir() else { return };
    let Ok(fork) = std::fs::read(dir.join("../mac/Payback/Payback (OS X).rsrc")) else { return };
    let imgs = payback_formats::icon::mac_app_icon(&fork).unwrap();
    let sizes: Vec<u32> = imgs.iter().map(|i| i.width).collect();
    assert_eq!(sizes, vec![128, 48, 32, 16]);
    // Corners transparent, centre opaque.
    let big = &imgs[0];
    assert_eq!(big.pixels[3], 0);
    let c = ((64 * 128 + 64) * 4 + 3) as usize;
    assert!(big.pixels[c] > 0);
}

/// The Mac release's `Payback` folder (`tools/extract_mac.py`), if present.
fn mac_dir() -> Option<PathBuf> {
    let dir = std::env::var_os("PAYBACK_MAC_DATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../extracted/mac/Payback"));
    dir.is_dir().then_some(dir)
}

#[test]
fn mac_texture_lists_match_amiga() {
    let (Some(dir), Some(mac)) = (data_dir(), mac_dir()) else { return };
    for name in ["CubeTextureList", "CubeTextureRemapList"] {
        let amiga = std::fs::read(dir.join(format!("CubeTextures/.{name}"))).unwrap();
        let txt = std::fs::read(mac.join(format!("CubeTextures/{name}.txt"))).unwrap();
        assert_eq!(amiga, txt, "{name}");
    }
    let list =
        CubeTextureList::parse(&std::fs::read(mac.join("CubeTextures/CubeTextureList.txt")).unwrap())
            .unwrap();
    let mut short = Vec::new();
    for e in &list.entries {
        let data = std::fs::read(mac.join("CubeTextures").join(&e.file))
            .unwrap_or_else(|_| panic!("{}", e.file));
        if data.len() < 64 * 64 {
            short.push(e.file.clone());
        }
        assert_eq!(IndexedImage::cube_texture_padded(&data).pixels.len(), 64 * 64);
    }
    assert_eq!(short, ["TentRoof.chunky"]);
}

#[test]
fn mac_user_map_textures() {
    let Some(mac) = mac_dir() else { return };
    let maps = files(&mac.join("UserMaps"), &|p| ext_is(p, "map"));
    assert_eq!(maps.len(), 32);
    let mut with_custom = 0;
    for p in maps {
        let m = Map::parse(&std::fs::read(&p).unwrap()).unwrap();
        let custom = std::fs::read(p.with_extension("textures"))
            .map(|b| {
                payback_formats::CustomTextures::parse(&b)
                    .unwrap_or_else(|e| panic!("{}: {e}", p.display()))
            })
            .unwrap_or_default();
        let mut uses = false;
        for sq in &m.squares {
            for c in &sq.cubes {
                for f in c.faces.iter().filter(|f| f.visible() && f.texture >= 2000) {
                    uses = true;
                    assert!(custom.get(f.texture).is_some(), "{}: {}", p.display(), f.texture);
                }
            }
        }
        with_custom += uses as usize;
    }
    assert_eq!(with_custom, 12);
}

/// Every bundled Mac user map with its companions: `.mini` 65×65,
/// `.thumb` 256×256, `.route` (optional; four maps ship none and LoadRouting
/// @ 100f1560 falls back to InitRouting), `.vehicles` (optional; Obj_Load
/// @ 100ad180 falls back to Obj_Init: no flags, no parked vehicles), and a
/// random start pool (special 1 squares) for the Rampage spawn.
#[test]
fn mac_user_map_companions() {
    let Some(mac) = mac_dir() else { return };
    let maps = files(&mac.join("UserMaps"), &|p| ext_is(p, "map"));
    let (mut routes, mut vehicles) = (0, 0);
    for p in maps {
        let name = p.display();
        let m = Map::parse(&std::fs::read(&p).unwrap()).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(m.variant <= 2, "{name}");
        assert_eq!(std::fs::read(p.with_extension("mini")).unwrap().len(), 65 * 65, "{name}");
        assert_eq!(std::fs::read(p.with_extension("thumb")).unwrap().len(), 256 * 256, "{name}");
        if let Ok(b) = std::fs::read(p.with_extension("route")) {
            payback_formats::Route::parse(&b).unwrap_or_else(|e| panic!("{name}: {e}"));
            routes += 1;
        }
        if let Ok(b) = std::fs::read(p.with_extension("vehicles")) {
            let v = payback_formats::vehicles::MapVehicles::parse(&b).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(v.version, 1, "{name}");
            for r in &v.parked {
                assert!((0..28).contains(&r.model) && (0..3).contains(&r.layer), "{name}: {r:?}");
                assert!((0..128 * 0x800).contains(&r.x) && (0..128 * 0x800).contains(&r.y), "{name}: {r:?}");
            }
            vehicles += 1;
        }
        let pool = m
            .squares
            .iter()
            .flat_map(|s| s.cubes.iter())
            .filter(|c| c.scenery == 0 && c.flags == 0x20 && c.special == 1)
            .count();
        assert!(pool > 0, "{name}: no start points");
        for c in m.squares.iter().flat_map(|s| s.cubes.iter()) {
            assert!(c.scenery <= 36, "{name}: scenery {}", c.scenery);
        }
    }
    assert_eq!((routes, vehicles), (28, 22));
}
