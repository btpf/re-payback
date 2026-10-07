//! In-game HUD (docs/re/frontend_audio.md, HUD layout from OS4
//! `FUN_01019bd8` and friends).
//!
//! Like the original, it is drawn into a low-resolution palette canvas,
//! 256 lines tall and as wide as the window's aspect ratio (the original
//! lays everything out against the view width), then composited over the
//! 3D view with nearest-neighbour scaling. The original's "lighten" blend
//! (`src + dst·(255 − max(src))/255`) is premultiplied-alpha compositing
//! with alpha = max(src)/255, so it is reproduced exactly by the overlay.

pub mod icons;
pub mod paint;

use crate::audio::{Sound, SoundPos, id};
use crate::camera::CameraTarget;
use crate::data::GameData;
use crate::level::LevelSpec;
use crate::sim::Obj;
use crate::sim::people::PlayerSession;
use bevy::{prelude::*, window::PrimaryWindow};
use icons::{HudIcons, Icon};
pub use paint::{Blend, CANVAS_H, Painter, blank, canvas_width};
use payback_formats::Phrases;
use std::collections::VecDeque;


/// Values the HUD displays, written by the game systems each tick.
#[derive(Resource, Default, Clone)]
pub struct HudData {
    pub score: i32,
    pub multiplier: i32,
    pub lives: i32,
    pub health: i32,
    pub wanted: i32,
    /// Selected HUD weapon slot (0 fist .. 7 bomb).
    pub weapon: usize,
    /// Ammo by HUD slot; 0 = not carried. Slot 0 (fist) ignored.
    pub ammo: [i32; 8],
    /// Countdown timer in seconds, if a script runs one.
    pub timer: Option<i32>,
    pub start_lights: Option<usize>,
    pub lap: Option<(i32, i32)>,
    /// Player and target positions for the minimap (original units).
    pub player_pos: Option<IVec2>,
    pub target_pos: Option<IVec2>,
    pub multiplayer: bool,
    /// Draw the flashing checkpoint marker at the target.
    pub checkpoint: bool,
}

/// Message-box speaker icons (`Dialog_Begin(n)`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Speaker {
    Phone = 0,
    Lips = 1,
    Mobile = 2,
    Pager = 3,
    Info = 4,
}

/// Font of a dialogue line: the player (red "You") or the other party
/// (blue "Them").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Voice {
    You = 1,
    Them = 2,
}

#[derive(Clone, Debug)]
enum Entry {
    Open(Speaker),
    Text(String, Voice),
    Close,
}

/// The message box: a queue of open/text/close entries, typed out word by
/// word (OS4 `FUN_01016d7c`, `FUN_01016424`).
#[derive(Resource, Default)]
pub struct Dialog {
    queue: VecDeque<Entry>,
    open: Option<Speaker>,
    /// Box animation 0..15 ticks (opening/closing).
    anim: i32,
    closing: bool,
    /// Lines shown, newest last.
    lines: Vec<(String, Voice)>,
    /// Words of the entry being typed.
    pending: VecDeque<String>,
    voice: Voice,
    wait: i32,
    scroll: i32,
    typing_sound: Option<Entity>,
    pub last: Vec<Entry2>,
}

/// Copy of the last message, for "repeat last message".
#[derive(Clone, Debug)]
pub struct Entry2 {
    pub text: String,
    pub voice: Voice,
}

impl Default for Voice {
    fn default() -> Self {
        Voice::Them
    }
}

impl Dialog {
    pub fn begin(&mut self, speaker: Speaker) {
        self.queue.push_back(Entry::Open(speaker));
    }

    pub fn say(&mut self, text: impl Into<String>, voice: Voice) {
        self.queue.push_back(Entry::Text(text.into(), voice));
    }

    pub fn end(&mut self) {
        self.queue.push_back(Entry::Close);
    }

    pub fn busy(&self) -> bool {
        !self.queue.is_empty() || !self.pending.is_empty() || self.open.is_some()
    }
}

/// The big bouncing title ("Mission complete!", "Wasted!" ...), OS4
/// `FUN_01016a70`.
#[derive(Resource, Default)]
pub struct BigTitle {
    text: String,
    ticks: i32,
    y: i32,
    vy: i32,
    pitch: i32,
}

impl BigTitle {
    pub fn show(&mut self, text: impl Into<String>) {
        *self = BigTitle {
            text: text.into(),
            ticks: 300,
            y: -0xF00,
            vy: 0x100,
            pitch: 120,
        };
    }
}

/// The vehicle-name banner (OS4 `FUN_01016224`).
#[derive(Resource, Default)]
pub struct Banner {
    text: String,
    t: i32,
}

impl Banner {
    pub fn show(&mut self, text: impl Into<String>) {
        self.text = text.into();
        self.t = 0;
    }
}

/// Phrase text for the current level and the global UI strings.
#[derive(Resource, Default)]
pub struct Texts {
    pub level: Phrases,
    pub global: Phrases,
}

impl Texts {
    pub fn level(&self, n: u32) -> String {
        self.level.get(n).unwrap_or("").to_string()
    }

    pub fn global(&self, n: u32) -> String {
        self.global.get(n).unwrap_or("").to_string()
    }
}

#[derive(Resource)]
struct Canvas {
    image: Handle<Image>,
    w: usize,
    frame: u32,
    minimap: Option<Icon>,
}

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<HudData>()
            .init_resource::<Dialog>()
            .init_resource::<BigTitle>()
            .init_resource::<Banner>()
            .init_resource::<Language>()
            .init_resource::<Texts>()
            .add_systems(Startup, setup)
            .add_systems(PreUpdate, load_global_texts)
            .add_systems(OnEnter(crate::level::AppState::Playing), load_level_texts.in_set(crate::level::LevelLoaded))
            .add_systems(OnExit(crate::level::AppState::Playing), reset_hud)
            .add_systems(FixedUpdate, (collect, tick_dialog, tick_title, tick_banner))
            .add_systems(PostUpdate, draw);
    }
}

fn setup(mut commands: Commands, data: Res<GameData>, mut images: ResMut<Assets<Image>>) {
    commands.insert_resource(HudIcons::load(&data));
    let w = 320;
    let image = images.add(blank(w));
    commands.insert_resource(Canvas {
        image: image.clone(),
        w,
        frame: 0,
        minimap: None,
    });
    commands.spawn((
        ImageNode::new(image),
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            top: Val::Px(0.0),
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            ..default()
        },
        Visibility::Hidden,
        HudNode,
    ));
}

/// The HUD overlay node (hidden outside levels).
#[derive(Component)]
pub struct HudNode;

/// UI language: the middle token of `Phrases/Global.<language>.phrases`
/// as found on disk (`DAT_012a63d4`). Changing it reloads the global
/// phrases; level phrases are read at level start.
#[derive(Resource, Clone, Debug, PartialEq, Eq)]
pub struct Language(pub String);

impl Default for Language {
    fn default() -> Self {
        Language("English".into())
    }
}

/// `Phrases/<name>.<language>.phrases`, falling back to English.
pub fn load_phrases(data: &GameData, name: &str, language: &str) -> Phrases {
    [language, "English"]
        .iter()
        .find_map(|l| {
            data.read(format!("Phrases/{name}.{l}.phrases"))
                .ok()
                .and_then(|b| Phrases::parse(&b).ok())
        })
        .unwrap_or_default()
}

/// (Re)load the global phrases when the language changes
/// (`FUN_0107234c("global")`).
fn load_global_texts(data: Res<GameData>, language: Res<Language>, mut texts: ResMut<Texts>) {
    if language.is_changed() {
        texts.global = load_phrases(&data, "Global", &language.0);
    }
}

/// Level phrases and the minimap at level start.
fn load_level_texts(
    data: Res<GameData>,
    spec: Res<LevelSpec>,
    language: Res<Language>,
    mut texts: ResMut<Texts>,
    mut canvas: ResMut<Canvas>,
    mut node: Query<&mut Visibility, With<HudNode>>,
) {
    texts.level = load_phrases(&data, &format!("{}{}", spec.map, spec.level), &language.0);
    canvas.minimap = load_minimap(&data, &spec.map);
    for mut v in &mut node {
        *v = Visibility::Inherited;
    }
}

/// Hide the HUD and clear per-level HUD state.
fn reset_hud(mut commands: Commands, mut node: Query<&mut Visibility, With<HudNode>>) {
    for mut v in &mut node {
        *v = Visibility::Hidden;
    }
    commands.insert_resource(HudData::default());
    commands.insert_resource(Dialog::default());
    commands.insert_resource(BigTitle::default());
    commands.insert_resource(Banner::default());
}

/// Minimap: 65x65 cropped to 64x64, red north arrow at the top centre.
fn load_minimap(data: &GameData, map: &str) -> Option<Icon> {
    data.read(format!("Maps/{map}.mini")).ok().map(|b| {
        let mut px = vec![0u8; 64 * 64];
        for y in 0..64 {
            for x in 0..64 {
                px[y * 64 + x] = b.get(y * 65 + x).copied().unwrap_or(0);
            }
        }
        // Arrow pixels (x 29-34, y 0-4) with a black shadow.
        for (y, row) in ["..##..", ".####.", "######", "..##..", "..##.."]
            .iter()
            .enumerate()
        {
            for (x, c) in row.chars().enumerate() {
                if c == '#' {
                    px[(y + 1) * 64 + 30 + x] = 8;
                    px[y * 64 + 29 + x] = 0xB0;
                }
            }
        }
                Icon { w: 64, h: 64, px }
    })
}

/// Copy the followed player's state into the HUD values.
fn collect(
    player: Query<&Obj, With<CameraTarget>>,
    session: Res<PlayerSession>,
    mut hud: ResMut<HudData>,
) {
    hud.lives = session.lives;
    hud.multiplier = session.multiplier;
    if let Ok(obj) = player.single() {
        hud.health = obj.health;
        hud.wanted = obj.wanted;
        hud.score = obj.score;
        hud.player_pos = Some(obj.pos.truncate());
    }
}

fn tick_dialog(mut commands: Commands, mut d: ResMut<Dialog>) {
    if d.scroll > 0 {
        d.scroll -= 1;
    }
    if d.closing {
        d.anim -= 1;
        if d.anim <= 0 {
            d.closing = false;
            d.open = None;
            d.lines.clear();
        }
        return;
    }
    if d.open.is_some() && d.anim < 15 {
        d.anim += 1;
        return;
    }
    if d.wait > 0 {
        d.wait -= 1;
        return;
    }
    if let Some(word) = d.pending.pop_front() {
        let cap = 41usize;
        let voice = d.voice;
        let new_line = match d.lines.last() {
            Some((line, v)) => *v != voice || word.len() >= cap.saturating_sub(line.len()),
            None => true,
        };
        if new_line {
            if d.lines.len() >= 3 {
                d.scroll = 10;
            }
            d.lines.push((String::new(), voice));
            if d.lines.len() > 4 {
                d.lines.remove(0);
            }
        }
        let line = &mut d.lines.last_mut().unwrap().0;
        line.push_str(&word);
        line.push(' ');
        d.wait = 2 * word.len() as i32;
        if d.pending.is_empty() {
            d.wait += 20;
            if let Some(e) = d.typing_sound.take() {
                commands.entity(e).despawn();
            }
        }
        return;
    }
    match d.queue.pop_front() {
        Some(Entry::Open(s)) => {
            d.open = Some(s);
            d.anim = 0;
            d.lines.clear();
        }
        Some(Entry::Text(text, voice)) => {
            if d.open.is_none() {
                d.open = Some(Speaker::Info);
                d.anim = 0;
            }
            d.voice = voice;
            d.last = vec![Entry2 {
                text: text.clone(),
                voice,
            }];
            d.pending = text.split_whitespace().map(str::to_string).collect();
            // SqueakyVoice loops while the text types out.
            let e = commands
                .spawn(Sound::looping(
                    id::SQUEAKY_VOICE,
                    255.0,
                    12000.0,
                    SoundPos::Global,
                ))
                .id();
            d.typing_sound = Some(e);
        }
        Some(Entry::Close) => {
            d.closing = true;
        }
        None => {}
    }
}

fn tick_title(mut t: ResMut<BigTitle>) {
    if t.ticks <= 0 {
        return;
    }
    t.ticks -= 1;
    t.pitch = (t.pitch - 2).max(16);
    if t.ticks > 150 {
        t.vy += 0x80;
        t.y += t.vy;
        if t.y > 0xA000 {
            t.y = 0xA000;
            t.vy = -t.vy / 2;
        }
    } else if t.ticks <= 100 {
        t.vy += 0x80;
        t.y += t.vy;
        if t.y > 0x10900 {
            t.ticks = 0;
        }
    }
}

fn tick_banner(mut b: ResMut<Banner>) {
    if b.t <= 149 {
        b.t += 1;
    }
}

#[allow(clippy::too_many_arguments)]
fn draw(
    mut canvas: ResMut<Canvas>,
    mut images: ResMut<Assets<Image>>,
    window: Query<&Window, With<PrimaryWindow>>,
    icons: Option<Res<HudIcons>>,
    data: Res<GameData>,
    hud: Res<HudData>,
    dialog: Res<Dialog>,
    title: Res<BigTitle>,
    banner: Res<Banner>,
) {
    let Some(icons) = icons else { return };
    let aspect = window
        .single()
        .map(|w| w.width() / w.height().max(1.0))
        .unwrap_or(1.25);
    let w = canvas_width(aspect);
    canvas.frame = canvas.frame.wrapping_add(1);
    let frame = canvas.frame;
    let minimap = canvas.minimap.clone();
    if canvas.w != w {
        canvas.w = w;
        if let Some(mut img) = images.get_mut(&canvas.image) {
            *img = blank(w);
        }
    }
    let Some(mut img) = images.get_mut(&canvas.image) else {
        return;
    };
    let Some(px) = img.data.as_mut() else { return };
    px.fill(0);
    let mut p = Painter {
        px,
        w,
        pal: &data.palette,
    };
    let xmax = w as i32 - 1;
    let ymax = CANVAS_H as i32 - 1;
    let cx = (xmax >> 1) + 1;

    // Weapons: fist at (5,5), then slots 1..7 every 10 px.
    let blink = frame & 2 != 0;
    for slot in 0..8 {
        if slot != 0 && hud.ammo[slot] <= 0 {
            continue;
        }
        let x = if slot == 0 { 5 } else { 5 + 10 * slot as i32 };
        let (normal, bright) = &icons.weapons[slot];
        let icon = if slot == hud.weapon && blink {
            bright
        } else {
            normal
        };
        p.icon(icon, x, 5, Blend::Plain);
    }
    if hud.weapon != 0 {
        let ammo = hud.ammo[hud.weapon].clamp(0, 99999);
        for (k, d) in format!("{ammo:05}").bytes().enumerate() {
            p.icon(
                &icons.small_digits[(d - b'0') as usize],
                5 + 10 * k as i32,
                26,
                Blend::Lighten,
            );
        }
    }

    if !hud.multiplayer {
        // Score: 7 digits (8 when larger).
        let score = hud.score.max(0);
        let digits = if score > 9_999_999 {
            format!("{score:08}")
        } else {
            format!("{score:07}")
        };
        let x0 = xmax - 75 - if digits.len() > 7 { 10 } else { 0 };
        for (k, d) in digits.bytes().enumerate() {
            p.icon(
                &icons.small_digits[(d - b'0') as usize],
                x0 + 10 * k as i32,
                5,
                Blend::Lighten,
            );
        }
        // Bonus multiplier (green) and lives.
        p.icon(&icons.small_x, xmax - 75, 19, Blend::Green);
        p.icon(
            &icons.small_digits[hud.multiplier.clamp(0, 9) as usize],
            xmax - 65,
            19,
            Blend::Green,
        );
        p.icon(&icons.small_person, xmax - 35, 19, Blend::Plain);
        p.icon(&icons.small_x, xmax - 25, 19, Blend::Lighten);
        p.icon(
            &icons.small_digits[hud.lives.clamp(0, 9) as usize],
            xmax - 15,
            19,
            Blend::Lighten,
        );

        // Minimap with player/target blobs (1 px = 2 squares).
        if let Some(mm) = &minimap {
            let (mx, my) = (xmax - 71, 33);
            p.icon(mm, mx + 1, my + 1, Blend::Plain);
            if let Some(t) = hud.target_pos {
                let b = if blink {
                    &icons.blobs[3]
                } else {
                    &icons.blobs[2]
                };
                p.icon(b, (t.x >> 12) + mx - 3, (t.y >> 12) + my - 3, Blend::Plain);
            }
            if let Some(pp) = hud.player_pos {
                let b = if blink {
                    &icons.blobs[1]
                } else {
                    &icons.blobs[0]
                };
                p.icon(
                    b,
                    (pp.x >> 12) + mx - 3,
                    (pp.y >> 12) + my - 3,
                    Blend::Plain,
                );
            }
        }
    }

    // Hearts.
    for i in 0..5 {
        let big = hud.health >= 1 && hud.health / 20 >= i;
        let icon = if big {
            &icons.heart_big
        } else {
            &icons.heart_small
        };
        p.icon(icon, cx - 25 + 10 * i, 31, Blend::Lighten);
    }

    // Wanted stars: a ripple of spinning stars, centred.
    let n = hud.wanted.clamp(0, 6);
    if n > 0 {
        let s = 11;
        let x0 = cx - n * s / 2;
        let mut ph = (frame % 30) as i32;
        for i in 0..n {
            let f = if ph <= 9 { ph as usize } else { 0 };
            p.icon(&icons.siren[f], x0 + i * s, 16, Blend::Lighten);
            ph += 2;
            if ph > 29 {
                ph = 0;
            }
        }
    }

    // Countdown pager or lap counter.
    if let Some(t) = hud.timer.filter(|t| (0..1000).contains(t)) {
        p.icon(&icons.pager, 5, 45, Blend::Plain);
        for (k, d) in format!("{t:03}").bytes().enumerate() {
            p.icon(
                &icons.pager_digits[(d - b'0') as usize],
                13 + 8 * k as i32,
                60,
                Blend::Plain,
            );
        }
    } else if let Some((lap, total)) = hud.lap {
        p.text(
            &icons.you,
            &format!("Lap {lap} of {total}"),
            5,
            45,
            7,
            Blend::Plain,
        );
    }
    if let Some(l) = hud.start_lights {
        if let Some(icon) = icons.lights.get(l) {
            p.icon(icon, cx - 19, CANVAS_H as i32 / 4, Blend::Plain);
        }
    }

    // Vehicle-name banner: slides in, holds, slides out.
    if !banner.text.is_empty() && banner.t <= 149 {
        let t = banner.t;
        let (h, ty) = if t < 20 {
            (16 * t / 20, 16 * t / 20 - 14)
        } else if t < 130 {
            (16, 2)
        } else {
            let k = 149 - t;
            (16 * k / 20, 16 * k / 20 - 14)
        };
        p.darken(cx - 80, 0, cx + 80, h);
        let tx = cx - 7 * banner.text.len() as i32 / 2;
        p.text(&icons.you, &banner.text, tx, ty, 7, Blend::Lighten);
    }

    // Message box.
    if let Some(speaker) = dialog.open {
        let top = ymax - 1 - 40 * dialog.anim / 15;
        p.darken(0, top, xmax, ymax);
        if dialog.anim >= 15 {
            p.icon(
                &icons.speakers[speaker as usize],
                2,
                ymax - 39,
                Blend::Plain,
            );
            let n = dialog.lines.len() as i32;
            for (k, (line, voice)) in dialog.lines.iter().rev().enumerate() {
                let y = ymax - 34 + 11 * (n - 1).min(2) + dialog.scroll - 11 * k as i32;
                if y < top {
                    continue;
                }
                let font = if *voice == Voice::You {
                    &icons.you
                } else {
                    &icons.them
                };
                p.text(font, line, 29, y, 7, Blend::Plain);
            }
        }
    }

    // Big title.
    if title.ticks > 0 {
        let n = title.text.len() as i32;
        let x = cx - title.pitch * n / 2;
        let y = (title.y * 16 >> 12) - 11;
        p.text(
            &icons.big,
            &title.text.to_uppercase(),
            x,
            y,
            title.pitch,
            Blend::Lighten,
        );
    }
}
