//! In-game HUD (docs/re/frontend_audio.md, HUD layout from OS4
//! `FUN_01019bd8` and friends).
//!
//! Like the original, it is drawn into a palette canvas and composited
//! over the 3D view with nearest-neighbour scaling. By default the canvas
//! has the hi-res layout whenever the window is at least 640×400, as on
//! the Mac, which only offers 640×480 and up and so always shows it
//! (`analysis/mac/frontend.md` §5.1): window-sized (divided by a whole
//! factor, see [`hud_canvas`]), with the hi-res offsets and doubled icons.
//! `Settings::hires_hud` off, or a smaller window, gives the lo-res
//! layout: 256 lines tall and as wide as the window's aspect ratio (the
//! original lays everything out against the view width). The original's "lighten" blend
//! (`src + dst·(255 − max(src))/255`) is premultiplied-alpha compositing
//! with alpha = max(src)/255, so it is reproduced exactly by the overlay.

pub mod icons;
pub mod labels;
pub mod loading;
pub mod paint;

use crate::audio::{Sound, SoundPos, id};
use crate::camera::CameraTarget;
use crate::data::GameData;
use crate::level::{GameMode, LevelSpec};
use crate::sim::Obj;
use crate::sim::combat::Arsenal;
use crate::sim::people::PlayerSession;
use bevy::{
    asset::embedded_asset,
    prelude::*,
    render::render_resource::{AsBindGroup, RenderPipelineDescriptor},
    shader::ShaderRef,
    ui_render::prelude::{MaterialNode, UiMaterial, UiMaterialKey, UiMaterialPlugin},
    window::PrimaryWindow,
};
use icons::{Font, HudIcons, Icon};
pub use paint::{Blend, CANVAS_H, Painter, blank, blank_sized, canvas_width};
use paint::{for_each_glyph, text_len};
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
    /// Where the marker is drawn this frame (render space), set by
    /// `arrow.rs` while it is on and there is a target.
    pub checkpoint_at: Option<Vec3>,
    /// Split screen: each player's view, by player number.
    pub views: Vec<ViewHud>,
}

/// What one split-screen view shows (`FUN_0105bd98`): weapons and ammo,
/// hearts and the kill count.
#[derive(Clone, Default, Debug)]
pub struct ViewHud {
    pub health: i32,
    pub weapon: usize,
    pub ammo: [i32; 8],
    pub kills: i32,
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
    /// The last message's entries, for the repeat-message key
    /// (`DAT_0117c280`, count `DAT_011746a8`): every enqueue also appends
    /// here, and an Open starts the list afresh.
    last: Vec<Entry>,
}

/// Entries in the message queue and in the last-message list
/// (`DAT_0117bc40` / `DAT_0117c280`, 100 each).
const QUEUE_MAX: usize = 100;

impl Default for Voice {
    fn default() -> Self {
        Voice::Them
    }
}

impl Dialog {
    /// Enqueue an entry and copy it to the last-message list
    /// ("LastMessageEnqueue": OS4 `FUN_01015d7c` Open, which restarts the
    /// list, `FUN_01015e18` text, `FUN_01015f68` Close, `FUN_01015b2c` any).
    /// Both lists hold 100 entries: a full queue drops the entry ("Enqueue():
    /// Too many entries in list..."), a full last-message list overwrites its
    /// final entry.
    fn enqueue(&mut self, entry: Entry) {
        if self.queue.len() >= QUEUE_MAX {
            warn!("Enqueue(): Too many entries in list...");
            return;
        }
        if matches!(entry, Entry::Open(_)) {
            self.last.clear();
        } else if self.last.len() >= QUEUE_MAX {
            warn!("LastMessageEnqueue(): Too many entries in list...");
            self.last.pop();
        }
        self.last.push(entry.clone());
        self.queue.push_back(entry);
    }

    pub fn begin(&mut self, speaker: Speaker) {
        self.enqueue(Entry::Open(speaker));
    }

    pub fn say(&mut self, text: impl Into<String>, voice: Voice) {
        self.enqueue(Entry::Text(text.into(), voice));
    }

    pub fn end(&mut self) {
        self.enqueue(Entry::Close);
    }

    /// The repeat-message key (control 10, phrase 2467): queue the last
    /// message's entries again (OS4 `FUN_01015a10`, called by the keyboard
    /// reader `FUN_0103f3d0` when the bound key is released, raw code
    /// `| 0x80`, and by the pad reader `FUN_0103da68` when the bound button
    /// goes down, `FUN_0103d9a0(.., 10, ..)`). The list itself is unchanged;
    /// entries that do not fit in the queue are dropped.
    pub fn repeat(&mut self) {
        for entry in &self.last {
            if self.queue.len() >= QUEUE_MAX {
                warn!("Enqueue(): Too many entries in list...");
                continue;
            }
            self.queue.push_back(entry.clone());
        }
    }

    /// `Dialog_Flush` (68k `FUN_00035f76`, OS4 `FUN_01015ce4`): drop the
    /// queued entries from the first queued Open on; what is already shown
    /// and the last-message list stay.
    pub fn flush(&mut self) {
        if let Some(i) = self.queue.iter().position(|e| matches!(e, Entry::Open(_))) {
            self.queue.truncate(i);
        }
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
    /// Split screen: a different text per view ("You win!" / "You lose!").
    each: Vec<String>,
    /// Split screen: only this player's view shows it.
    view: Option<u8>,
}

impl BigTitle {
    pub fn show(&mut self, text: impl Into<String>) {
        *self = BigTitle {
            text: text.into(),
            ticks: 300,
            y: -0xF00,
            vy: 0x100,
            pitch: 120,
            each: Vec::new(),
            view: None,
        };
    }

    /// Shown in `player`'s view only (split screen; e.g. "Wasted!").
    pub fn show_for(&mut self, player: Option<u8>, text: impl Into<String>) {
        self.show(text);
        self.view = player;
    }

    /// One text per split-screen view.
    pub fn show_each(&mut self, texts: Vec<String>) {
        self.show(texts.first().cloned().unwrap_or_default());
        self.each = texts;
    }
}

/// The vehicle-name banner (OS4 `FUN_01016224`).
#[derive(Resource, Default)]
pub struct Banner {
    text: String,
    t: i32,
    /// Split screen: the player whose view shows it (None: every view).
    view: Option<u8>,
}

impl Banner {
    pub fn show(&mut self, text: impl Into<String>) {
        self.text = text.into();
        self.t = 0;
        self.view = None;
    }

    /// Shown in `player`'s view only (split screen).
    pub fn show_for(&mut self, player: Option<u8>, text: impl Into<String>) {
        self.show(text);
        self.view = player;
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
    /// The subtract plane under the HUD (software panels, [`Pen::fade`]).
    sub: Handle<Image>,
    /// Its pixels as drawn this frame, uploaded by [`upload_plane`].
    sub_px: Vec<u8>,
    w: usize,
    h: usize,
    frame: u32,
    minimap: Option<Icon>,
}

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "subtract.wgsl");
        app.init_resource::<HudData>()
            .init_resource::<Dialog>()
            .init_resource::<BigTitle>()
            .init_resource::<Banner>()
            .init_resource::<Language>()
            .init_resource::<Texts>()
            .add_plugins((loading::plugin, labels::plugin, UiMaterialPlugin::<SubtractPlane>::default()))
            .add_systems(Startup, setup)
            .add_systems(PreUpdate, load_global_texts)
            .add_systems(OnEnter(crate::level::AppState::Playing), load_level_texts.in_set(crate::level::LevelLoaded))
            .add_systems(OnExit(crate::level::AppState::Playing), reset_hud)
            .add_systems(FixedUpdate, (collect, tick_dialog, tick_title, tick_banner))
            .add_systems(PostUpdate, (draw, upload_plane).chain());
    }
}

fn setup(
    mut commands: Commands,
    data: Res<GameData>,
    mut images: ResMut<Assets<Image>>,
    mut planes: ResMut<Assets<SubtractPlane>>,
) {
    let icons = HudIcons::load(&data);
    commands.insert_resource(HudIcons2x(icons.doubled(&icons::Average::load(&data))));
    commands.insert_resource(icons);
    let w = 320;
    let image = images.add(blank(w));
    let sub = images.add(blank(w));
    commands.insert_resource(Canvas {
        image: image.clone(),
        sub: sub.clone(),
        sub_px: Vec::new(),
        w,
        h: CANVAS_H,
        frame: 0,
        minimap: None,
    });
    // Under the HUD canvas.
    commands.spawn((
        MaterialNode(planes.add(SubtractPlane { plane: sub })),
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            top: Val::Px(0.0),
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            ..default()
        },
        GlobalZIndex(-1),
        Visibility::Hidden,
        HudNode,
    ));
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

/// The HUD overlay nodes (hidden outside levels): the canvas and its
/// subtract plane.
#[derive(Component)]
pub struct HudNode;

/// The software renderer's subtract panels (`Draw2DEffect(FadeDarken…,
/// …, 0x10000)`: Alpha.256 table 1, `max(d − s, 0)`): a full-window plane
/// whose texels are subtracted from the view under the HUD canvas. The
/// subtraction is on the linear frame (the final pass then snaps to the
/// palette), where the original subtracts palette values (rendering.md
/// "Palette output", approximate like the effects' table 1).
#[derive(Asset, TypePath, AsBindGroup, Clone)]
pub struct SubtractPlane {
    #[texture(0)]
    #[sampler(1)]
    plane: Handle<Image>,
}

impl UiMaterial for SubtractPlane {
    fn fragment_shader() -> ShaderRef {
        "embedded://payback/hud/subtract.wgsl".into()
    }

    fn specialize(descriptor: &mut RenderPipelineDescriptor, _key: UiMaterialKey<Self>) {
        use bevy::render::render_resource::{BlendComponent, BlendFactor, BlendOperation, BlendState};
        if let Some(target) = descriptor.fragment.as_mut().and_then(|f| f.targets[0].as_mut()) {
            target.blend = Some(BlendState {
                color: BlendComponent {
                    src_factor: BlendFactor::One,
                    dst_factor: BlendFactor::One,
                    operation: BlendOperation::ReverseSubtract,
                },
                alpha: BlendComponent {
                    src_factor: BlendFactor::Zero,
                    dst_factor: BlendFactor::One,
                    operation: BlendOperation::Add,
                },
            });
        }
    }
}

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

/// `Phrases/<name>.<language>.phrases`, falling back to English. The Mac
/// files come first: they are the baseline for English, Deutsch and
/// Francais (`Français`; global, level and credits texts: "Difficulty:",
/// "Music setup", the control-neutral hints, the Mac credits;
/// `analysis/mac/frontend.md` §6). The other 11 languages exist only in
/// the Amiga data.
pub fn load_phrases(data: &GameData, name: &str, language: &str) -> Phrases {
    let prefix = format!("{name}.");
    let files = data.list_dir("Phrases", true, true);
    [language, "English"]
        .iter()
        .find_map(|l| {
            let (_, path) = files.iter().find(|(n, _)| {
                n.strip_prefix(&prefix)
                    .and_then(|n| n.strip_suffix(".phrases"))
                    .is_some_and(|mid| crate::data::same_language(mid, l))
            })?;
            std::fs::read(path).ok().and_then(|b| Phrases::parse(&b).ok())
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
    // Level phrases ("%d%d", e.g. "00") exist for the built-in cities only.
    texts.level = match spec.city() {
        Some(c) => load_phrases(&data, &format!("{c}{}", spec.level), &language.0),
        None => Default::default(),
    };
    canvas.minimap = load_minimap(&data, &spec.path("mini"));
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
fn load_minimap(data: &GameData, path: &std::path::Path) -> Option<Icon> {
    data.read(path).ok().map(|b| {
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

/// Copy the followed player's state into the HUD values (every player's
/// in split screen).
fn collect(
    player: Query<(&Obj, Option<&Arsenal>), With<CameraTarget>>,
    session: Res<PlayerSession>,
    mode: Res<GameMode>,
    game: Option<Res<crate::multiplayer::Match>>,
    mut hud: ResMut<HudData>,
) {
    hud.lives = session.lives;
    hud.multiplier = session.multiplier;
    let weapons = |ars: Option<&Arsenal>| ars.map_or((0, [0; 8]), |a| (a.current as usize, a.ammo));
    if mode.is_multiplayer() {
        hud.multiplayer = true;
        hud.views.resize(mode.players(), ViewHud::default());
        for (i, v) in hud.views.iter_mut().enumerate() {
            v.kills = game.as_ref().map_or(0, |g| g.kills[i]);
            v.health = 0;
        }
        for (obj, ars) in &player {
            if let Some(v) = obj.player.and_then(|p| hud.views.get_mut(p as usize)) {
                v.health = obj.health;
                (v.weapon, v.ammo) = weapons(ars);
            }
        }
        return;
    }
    if let Ok((obj, ars)) = player.single() {
        hud.health = obj.health;
        hud.wanted = obj.wanted;
        hud.score = obj.score;
        hud.player_pos = Some(obj.pos.truncate());
        (hud.weapon, hud.ammo) = weapons(ars);
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

/// The software hi-res icon set ([`HudIcons::doubled`]).
#[derive(Resource)]
pub struct HudIcons2x(pub HudIcons);

/// The HUD canvas for a window (physical pixels): the hi-res layout when
/// enabled and the window is at least 640×400 (`InitGraphics` @ 1008d110,
/// Amiga `FUN_010138dc`: W > 639 && H > 399 sets `AreIconsScaled` @
/// 10085eb0), on a canvas of window / s pixels, s the largest whole factor
/// that keeps it at least 640×400 (s = 1 for the Mac's 640×480, 800×600
/// and 1024×768); otherwise the lo-res 256-line canvas as wide as the
/// window's aspect. Returns (w, h, hi-res).
pub fn hud_canvas(physical: UVec2, hires: bool) -> (usize, usize, bool) {
    let (w, h) = (physical.x as usize, physical.y.max(1) as usize);
    if hires && w > 639 && h > 399 {
        let s = (w / 640).min(h / 400).max(1);
        (w / s, h / s, true)
    } else {
        (canvas_width(w as f32 / h as f32), CANVAS_H, false)
    }
}

/// Draws icons and text at the HUD's scale. Hi-res doubles every offset
/// and pitch that the original doubles (each call site below cites its
/// own constants; several are not exactly 2×) and uses doubled icons:
/// pre-doubled through Alpha table 4 in software ([`HudIcons2x`]),
/// bilinear 2× quads in hardware (`DrawIcon__FP4Iconii` @ 10085300).
struct Pen<'p, 'a> {
    p: &'p mut Painter<'a>,
    /// The subtract plane (RGBA like the canvas), see [`SubtractPlane`].
    sub: &'p mut [u8],
    /// The icons as loaded for this mode (lo-res or software-doubled).
    icons: &'p HudIcons,
    hi: bool,
    /// Hardware hi-res: lo-res icons drawn ×2 bilinear.
    bilinear: bool,
    /// Hardware (Warp3D) renderer.
    hardware: bool,
}

impl Pen<'_, '_> {
    /// Offset scale: 2 in hi-res.
    fn k(&self) -> i32 {
        if self.hi { 2 } else { 1 }
    }

    /// `Draw2DEffect(tex, x0, y0, x1, y1, 0x10000)` (Mac @ 100a8210 ->
    /// `drawGroundAlpha`; OS4 `FUN_0103500c`): the 64×64 texture
    /// stretched over (x0, y0)–(x1, y1), u += 0x3FFFFF/(w + 1) per pixel
    /// (16.16), v += 0xFC0/(y1 − y0) per row (6 fraction bits), through
    /// table 1: the texel's palette colour is subtracted from the view
    /// (the subtract plane). Warp3D blends `(7, 8)` instead, with the
    /// texture as loaded: `FadeDarken` inverted with alpha max/2
    /// (`API_3D_Set_Invert`), `FadeDarken2` black with alpha max
    /// (`API_3D_Set_Shadow`; `InitGraphics` @ 1008d110).
    fn fade(&mut self, which: usize, x0: i32, y0: i32, x1: i32, y1: i32) {
        let tex = &self.icons.fade[which];
        if tex.px.is_empty() {
            return;
        }
        let (w, h) = (self.p.w as i32, self.p.h() as i32);
        let du = 0x3FFFFF / (x1 - x0 + 1).max(1);
        let dv = 0xFC0 / (y1 - y0).max(1);
        let pal = self.p.pal;
        let mut v = 0;
        for y in y0..=y1 {
            let row = ((v >> 6) as usize).min(tex.h - 1);
            v += dv;
            if !(0..h).contains(&y) {
                continue;
            }
            let mut u = 0;
            for x in x0..=x1 {
                let col = ((u >> 16) as usize).min(tex.w - 1);
                u += du;
                let c = tex.px[row * tex.w + col] as usize;
                if !(0..w).contains(&x) || c == 0 {
                    continue;
                }
                let rgb = [pal[c * 3], pal[c * 3 + 1], pal[c * 3 + 2]];
                let m = rgb[0].max(rgb[1]).max(rgb[2]);
                let i = (y as usize * self.p.w + x as usize) * 4;
                if !self.hardware {
                    self.sub[i..i + 4].copy_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
                } else if which == 0 {
                    self.p.px[i..i + 4].copy_from_slice(&[255 - rgb[0], 255 - rgb[1], 255 - rgb[2], m / 2]);
                } else {
                    self.p.px[i..i + 4].copy_from_slice(&[0, 0, 0, m]);
                }
            }
        }
    }

    fn icon(&mut self, icon: &Icon, x: i32, y: i32, blend: Blend) {
        if self.bilinear {
            self.p.icon_bilinear2(icon, x, y, blend);
        } else {
            self.p.icon(icon, x, y, blend);
        }
    }

    /// `DisplayText*` (@ 1008ecd0, 1008bcd0): the pitch doubles in hi-res.
    fn text(&mut self, font: &Font, s: &str, x: i32, y: i32, pitch: i32, blend: Blend) {
        let pitch = pitch * self.k();
        let bilinear = self.bilinear;
        let p = &mut *self.p;
        for_each_glyph(font, s, x, pitch, |g, gx| {
            if bilinear {
                p.icon_bilinear2(g, gx, y, blend);
            } else {
                p.icon(g, gx, y, blend);
            }
        });
    }
}

/// `ys` of `ReadScreenParams` @ 10051fc0 (`FUN_0104107c`): ((H+8)·16)>>8.
fn ys(h: i32) -> i32 {
    ((h + 8) * 16) >> 8
}

#[allow(clippy::too_many_arguments)]
fn draw(
    mut canvas: ResMut<Canvas>,
    mut images: ResMut<Assets<Image>>,
    window: Query<&Window, With<PrimaryWindow>>,
    icons: Option<Res<HudIcons>>,
    icons2x: Option<Res<HudIcons2x>>,
    settings: Option<Res<crate::frontend::progress::Settings>>,
    data: Res<GameData>,
    hud: Res<HudData>,
    dialog: Res<Dialog>,
    title: Res<BigTitle>,
    banner: Res<Banner>,
    texts: Res<Texts>,
    labels: Res<labels::Labels>,
    view: Query<(&Camera, &GlobalTransform), With<crate::camera::GameCamera>>,
) {
    let Some(lo) = icons else { return };
    let physical = window.single().map_or(UVec2::new(1280, 800), |w| w.physical_size());
    let (w, h, hi) = hud_canvas(physical, settings.as_ref().is_none_or(|s| s.hires_hud));
    let bilinear = hi && crate::render_mode::is_hardware();
    let set: &HudIcons = match &icons2x {
        Some(d) if hi && !bilinear => &d.0,
        _ => &lo,
    };
    canvas.frame = canvas.frame.wrapping_add(1);
    let frame = canvas.frame;
    let minimap = canvas.minimap.clone();
    if (canvas.w, canvas.h) != (w, h) {
        canvas.w = w;
        canvas.h = h;
        for handle in [&canvas.image, &canvas.sub] {
            if let Some(mut img) = images.get_mut(handle) {
                *img = blank_sized(w, h);
            }
        }
    }
    let mut sub = std::mem::take(&mut canvas.sub_px);
    sub.clear();
    sub.resize(w * h * 4, 0);
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
    let hardware = crate::render_mode::is_hardware();
    let mut pen = Pen { p: &mut p, sub: &mut sub, icons: set, hi, bilinear, hardware };
    if hud.multiplayer {
        let label = texts.global(2189);
        let views = Views { title: &title, banner: &banner, label: &label, frame };
        views.draw(&mut pen, &hud.views, physical);
        canvas.sub_px = sub;
        return;
    }
    let ic = pen.icons;
    let k = pen.k();
    let xmax = w as i32 - 1;
    let ymax = h as i32 - 1;
    let cx = (xmax >> 1) + 1;

    // Floating labels (`DoLabels__Fsiiii` @ 10094f20), drawn in the view
    // before the HUD: font 2 at (sx − 7·len/2, sy + 3) of the projected
    // point, the pitch doubled in hi-res but not the centring offset.
    if let (Ok((cam, eye)), Ok(win)) = (view.single(), window.single()) {
        let size = win.size();
        let origin = cam.logical_viewport_rect().map_or(Vec2::ZERO, |r| r.min);
        for l in &labels.0 {
            let at = crate::world::object_to_render(l.pos.as_vec3());
            let Ok(p) = cam.world_to_viewport(eye, at) else { continue };
            let p = (p + origin) / size * Vec2::new(w as f32, h as f32);
            let x = p.x.floor() as i32 - ((7 * text_len(&l.text)) >> 1);
            pen.text(&ic.them, &l.text, x, p.y.floor() as i32 + 3, 7, Blend::Lighten);
        }
        // Checkpoint marker (`DrawCheckpoint__Fiiiiii` @ 10095240, after the
        // labels): `Checkpoint.red/blue` by the frame counter
        // (`_DAT_1017c898 >> 1`, 0..3) through `DrawIconAlpha` centred on
        // the projected target, −27 (−54 hi-res, icon doubled).
        if let Some(at) = hud.checkpoint_at
            && let Ok(p) = cam.world_to_viewport(eye, at)
        {
            let p = (p + origin) / size * Vec2::new(w as f32, h as f32);
            let icon = &ic.checkpoint[((frame >> 1) & 1) as usize];
            pen.icon(icon, p.x.floor() as i32 - 27 * k, p.y.floor() as i32 - 27 * k, Blend::Lighten);
        }
    }

    // `DrawGraphics__FiP6Objectisi` @ 100929e0 (OS4 `FUN_01019bd8`).
    let blink = frame & 2 != 0;
    draw_weapons(&mut pen, hud.weapon, &hud.ammo, blink);

    // Score (`DisplayScore__Fiii` @ 10090f70, called with xmax − 75):
    // 7 digits from x (hi-res x − 70), 8 when larger from x − 10 (x − 80),
    // 10 px apart (20), y 5 (10).
    let score = hud.score.max(0);
    let digits = if score > 9_999_999 {
        format!("{score:08}")
    } else {
        format!("{score:07}")
    };
    let x0 = xmax
        - 75
        - match (digits.len() > 7, hi) {
            (false, false) => 0,
            (true, false) => 10,
            (false, true) => 70,
            (true, true) => 80,
        };
    for (i, d) in digits.bytes().enumerate() {
        pen.icon(&ic.small_digits[(d - b'0') as usize], x0 + 10 * k * i as i32, 5 * k, Blend::Lighten);
    }
    // Bonus multiplier (green, `DisplayBonusMultiplier__Fiii` @ 10091190)
    // at (xmax − 75, 19), hi-res (xmax − 150, 38); lives
    // (`DisplayLives__Fiii` @ 100910b0) at (xmax − 35, 19), (xmax − 70, 38).
    pen.icon(&ic.small_x, xmax - 75 * k, 19 * k, Blend::Green);
    pen.icon(
        &ic.small_digits[hud.multiplier.clamp(0, 9) as usize],
        xmax - 75 * k + 10 * k,
        19 * k,
        Blend::Green,
    );
    pen.icon(&ic.small_person, xmax - 35 * k, 19 * k, Blend::Plain);
    pen.icon(&ic.small_x, xmax - 25 * k, 19 * k, Blend::Lighten);
    pen.icon(
        &ic.small_digits[hud.lives.clamp(0, 9) as usize],
        xmax - 15 * k,
        19 * k,
        Blend::Lighten,
    );

    // Minimap (`DisplayMinimap__Fiiiiii` @ 10091630, called with
    // (xmax − 71, 33), hi-res (xmax − 142, 66)), image at (X+1, Y+1) with
    // two blobs (`PlotPoint__Fiii` @ 100915d0, 7×7 centred, 1 px = 2
    // squares): the followed object, Blob.0 green / Blob.1 orange, then the
    // seek target when its x ≥ 0, Blob.2 blue / Blob.3 red, both switching
    // on the same 4-frame counter (`_DAT_1017d2e4 > 1`). Nothing else is
    // plotted: no police, pedestrians or pickups (the radar pickup has no
    // effect). The map itself is not doubled in software hi-res; hardware
    // draws it ×2 and plots at >> 11.
    if let Some(mm) = &minimap {
        let (mx, my) = (xmax - 71 * k, 33 * k);
        pen.icon(mm, mx + 1, my + 1, Blend::Plain);
        let shift = if bilinear { 11 } else { 12 };
        if let Some(pp) = hud.player_pos {
            let b = if blink { &ic.blobs[1] } else { &ic.blobs[0] };
            pen.icon(b, (pp.x >> shift) + mx - 3, (pp.y >> shift) + my - 3, Blend::Plain);
        }
        if let Some(t) = hud.target_pos.filter(|t| t.x >= 0) {
            let b = if blink { &ic.blobs[3] } else { &ic.blobs[2] };
            pen.icon(b, (t.x >> shift) + mx - 3, (t.y >> shift) + my - 3, Blend::Plain);
        }
    }

    // Hearts (`DisplayHealth__Fiii` @ 10091390): x = cx − 25 + 10i, y 31;
    // hi-res cx − 50 + 20i, y 62.
    for i in 0..5 {
        let big = hud.health >= 1 && hud.health / 20 >= i;
        let icon = if big { &ic.heart_big } else { &ic.heart_small };
        pen.icon(icon, cx - 25 * k + 10 * k * i, 31 * k, Blend::Lighten);
    }

    // Wanted stars (`DisplayWanted__Fiii` @ 10091270): a ripple of
    // spinning stars, centred, y 16 (32). Pitch = icon width + 2, doubled
    // again in hardware hi-res (20 in software hi-res, 22 in hardware).
    let n = hud.wanted.clamp(0, 6);
    if n > 0 {
        let s = (ic.siren[0].w as i32 + 2) * if bilinear { 2 } else { 1 };
        let x0 = cx - n * s / 2;
        let mut ph = (frame % 30) as i32;
        for i in 0..n {
            let f = if ph <= 9 { ph as usize } else { 0 };
            pen.icon(&ic.siren[f], x0 + i * s, 16 * k, Blend::Lighten);
            ph += 2;
            if ph > 29 {
                ph = 0;
            }
        }
    }

    // Countdown pager (`DisplayTimer__Fii` @ 100914d0) at (5, 45), digits
    // at x + 8 + 8i, y + 15; hi-res (10, 90), x + 16 + 16i, y + 30. Or the
    // lap counter (`DisplayLap__Fii` @ 10090ea0) at (5, 45), (10, 90).
    if let Some(t) = hud.timer.filter(|t| (0..1000).contains(t)) {
        pen.icon(&ic.pager, 5 * k, 45 * k, Blend::Plain);
        for (i, d) in format!("{t:03}").bytes().enumerate() {
            pen.icon(
                &ic.pager_digits[(d - b'0') as usize],
                5 * k + 8 * k + 8 * k * i as i32,
                60 * k,
                Blend::Plain,
            );
        }
    } else if let Some((lap, total)) = hud.lap {
        pen.text(&ic.you, &format!("Lap {lap} of {total}"), 5 * k, 45 * k, 7, Blend::Plain);
    }
    // Start lights (`DisplayLights__Fii` @ 10090db0) at (cx − 19, H·50/200),
    // hi-res x cx − 38.
    if let Some(l) = hud.start_lights {
        if let Some(icon) = ic.lights.get(l) {
            pen.icon(icon, cx - 19 * k, h as i32 * 50 / 200, Blend::Plain);
        }
    }

    draw_banner(&mut pen, &banner, cx);

    // Message box (`DisplayMessage__Fv` @ 1008f840): opening/closing over
    // 40 lines (80 hi-res) in 15 ticks, icon at (2, ymax − 39) (ymax − 78),
    // text at x 29 (58), lines ymax − 34 + 11·min(n − 1, 2) + scroll − 11k
    // (hi-res −68, 22 apart; the scroll is not doubled).
    if let Some(speaker) = dialog.open {
        let top = ymax - 1 - 40 * k * dialog.anim / 15;
        pen.fade(0, 0, top, xmax, ymax);
        if dialog.anim >= 15 {
            pen.icon(&ic.speakers[speaker as usize], 2, ymax - 39 * k, Blend::Plain);
            let n = dialog.lines.len() as i32;
            for (i, (line, voice)) in dialog.lines.iter().rev().enumerate() {
                let y = ymax - 34 * k + 11 * k * (n - 1).min(2) + dialog.scroll - 11 * k * i as i32;
                if y < top {
                    continue;
                }
                let font = if *voice == Voice::You { &ic.you } else { &ic.them };
                pen.text(font, line, 29 * k, y, 7, Blend::Plain);
            }
        }
    }

    // Big title (`DrawAnnouncement__Fi` @ 1008fe10): x = cx −
    // (k·pitch·realstrlen >> 1), y = (ys·y >> 12) − 11, the pitch doubled
    // in hi-res.
    if title.ticks > 0 {
        let n = text_len(&title.text);
        let x = cx - ((k * title.pitch * n) >> 1);
        let y = ((ys(h as i32) * title.y) >> 12) - 11;
        pen.text(&ic.big, &title.text.to_uppercase(), x, y, title.pitch, Blend::Lighten);
    }
    canvas.sub_px = sub;
}

/// Copy the subtract plane drawn by [`draw`] into its image.
fn upload_plane(canvas: Res<Canvas>, mut images: ResMut<Assets<Image>>) {
    if let Some(mut img) = images.get_mut(&canvas.sub)
        && let Some(px) = img.data.as_mut()
        && px.len() == canvas.sub_px.len()
    {
        px.copy_from_slice(&canvas.sub_px);
    }
}

/// Weapons (`DrawWeapons__FP6Object` @ 10092480): fist at (5,5), then
/// slots 1..7 every 10 px (hi-res 20); ammo of the selected one
/// (`DisplayAmmo__Fiii` @ 10090490) at (5, 26), hi-res (5, 47), digits
/// 10 (20) apart.
fn draw_weapons(pen: &mut Pen, weapon: usize, ammo: &[i32; 8], blink: bool) {
    let ic = pen.icons;
    let k = pen.k();
    for slot in 0..8 {
        if slot != 0 && ammo[slot] <= 0 {
            continue;
        }
        let x = 5 + 10 * k * slot as i32;
        let (normal, bright) = &ic.weapons[slot];
        let icon = if slot == weapon && blink { bright } else { normal };
        pen.icon(icon, x, 5, Blend::Plain);
    }
    if weapon != 0 {
        let ammo = ammo[weapon].clamp(0, 99999);
        let y = if pen.hi { 47 } else { 26 };
        for (i, d) in format!("{ammo:05}").bytes().enumerate() {
            pen.icon(&ic.small_digits[(d - b'0') as usize], 5 + 10 * k * i as i32, y, Blend::Lighten);
        }
    }
}

/// Vehicle-name banner (`DisplayVehicleName__Fi` @ 1008f5f0): slides in,
/// holds, slides out. Lo-res: band (cx − 80, 0)–(cx + 80, h), h = 16t/20,
/// text y = h − 14, holding at h 16, y 2. Hi-res: ±160, h = 16t/10, text y
/// = h − 28, holding at h 32, y 2. Text x = cx − (7k·realstrlen >> 1)
/// (`SetVehicleName__FiPc` @ 1008fc80; `realstrlen` skips `^`).
fn draw_banner(pen: &mut Pen, banner: &Banner, cx: i32) {
    if banner.text.is_empty() || banner.t > 149 {
        return;
    }
    let k = pen.k();
    let d = 20 / k;
    let t = banner.t;
    let (h, ty) = if t < 20 {
        (16 * t / d, 16 * t / d - 14 * k)
    } else if t < 130 {
        (16 * k, 2)
    } else {
        let r = 149 - t;
        (16 * r / d, 16 * r / d - 14 * k)
    };
    pen.fade(1, cx - 80 * k, 0, cx + 80 * k, h);
    let tx = cx - ((7 * k * text_len(&banner.text)) >> 1);
    let ic = pen.icons;
    pen.text(&ic.you, &banner.text, tx, ty, 7, Blend::Lighten);
}

/// The split-screen HUD (`FUN_0105bd98`, Mac `DrawView__Fv` @ 1006eb80):
/// per view the vehicle banner, weapons and ammo at the usual place,
/// hearts centred at x = xmax' − 30 (hi-res − 60), y = 10, and the kills
/// box (`DrawKills__Fiii` @ 10092860) at (5, ymax' − 13) (− 26); no
/// minimap, score, stars, lives, timer or message box.
struct Views<'a> {
    title: &'a BigTitle,
    banner: &'a Banner,
    /// "Kill(s):" (2189).
    label: &'a str,
    frame: u32,
}

impl Views<'_> {
    /// Draw every view into its rectangle of the canvas; `physical` is the
    /// window size the view rectangles are laid out in.
    fn draw(&self, pen: &mut Pen, views: &[ViewHud], physical: UVec2) {
        let (w, h) = (pen.p.w, pen.p.h());
        let (pw, ph) = (physical.x.max(2), physical.y.max(2));
        let rects = crate::camera::view_rects(views.len(), pw, ph);
        for (i, v) in views.iter().enumerate() {
            let Some(&(x, y, rw, rh)) = rects.get(i) else { continue };
            let sx = |a: u32| (a as u64 * w as u64 / pw as u64) as usize;
            let sy = |a: u32| (a as u64 * h as u64 / ph as u64) as usize;
            let (x0, y0) = (sx(x), sy(y));
            let (vw, vh) = (sx(rw).clamp(1, w - x0.min(w - 1)), sy(rh).clamp(1, h - y0.min(h - 1)));
            let mut buf = vec![0u8; vw * vh * 4];
            let mut plane = vec![0u8; vw * vh * 4];
            let mut sub = Painter { px: &mut buf, w: vw, pal: pen.p.pal };
            let mut sp = Pen {
                p: &mut sub,
                sub: &mut plane,
                icons: pen.icons,
                hi: pen.hi,
                bilinear: pen.bilinear,
                hardware: pen.hardware,
            };
            self.view(&mut sp, i, v, vw as i32, vh as i32, h as i32);
            for row in 0..vh {
                let at = ((y0 + row) * w + x0) * 4;
                let span = row * vw * 4..(row + 1) * vw * 4;
                pen.p.px[at..at + vw * 4].copy_from_slice(&buf[span.clone()]);
                pen.sub[at..at + vw * 4].copy_from_slice(&plane[span]);
            }
        }
    }

    /// One view, `vw`×`vh` of a canvas `h` lines tall.
    fn view(&self, pen: &mut Pen, i: usize, v: &ViewHud, vw: i32, vh: i32, h: i32) {
        let ic = pen.icons;
        let k = pen.k();
        let (xmax, ymax) = (vw - 1, vh - 1);
        let cx = (xmax >> 1) + 1;
        draw_weapons(pen, v.weapon, &v.ammo, self.frame & 2 != 0);
        // Hearts centred at xmax − 30 (60), y 10 in both.
        for n in 0..5 {
            let big = v.health >= 1 && v.health / 20 >= n;
            let icon = if big { &ic.heart_big } else { &ic.heart_small };
            pen.icon(icon, xmax - 30 * k - 25 * k + 10 * k * n, 10, Blend::Lighten);
        }
        // Kills box: FadeDarken (0, y − 5)–(5 + (len + 3)·7k + 5, y + 13k).
        let y = ymax - 13 * k;
        let len = text_len(self.label);
        pen.fade(0, 0, y - 5, 5 + (len + 3) * 7 * k + 5, y + 13 * k);
        let s = format!("{} {}", self.label, v.kills);
        pen.text(&ic.them, &s, 5, y, 7, Blend::Lighten);
        if self.banner.view.is_none_or(|b| b as usize == i) {
            draw_banner(pen, self.banner, cx);
        }
        // Big title, smaller in a split view (pitch - 8), scaled to the
        // view height.
        let t = self.title;
        if t.ticks > 0 && t.view.is_none_or(|p| p as usize == i) {
            let text = t.each.get(i).unwrap_or(&t.text).to_uppercase();
            let pitch = (t.pitch - 8).max(8);
            let n = text_len(&text);
            let x = cx - k * pitch * n / 2;
            let y = ((t.y * ys(h)) >> 12) * vh / h - 11;
            pen.text(&ic.big, &text, x, y, pitch, Blend::Lighten);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(d: &Dialog) -> Vec<String> {
        d.queue
            .iter()
            .map(|e| match e {
                Entry::Open(s) => format!("open {s:?}"),
                Entry::Text(t, _) => t.clone(),
                Entry::Close => "close".into(),
            })
            .collect()
    }

    /// The repeat key re-queues the last message from its Open on
    /// (`FUN_01015a10`); a new Open restarts the list.
    #[test]
    fn repeat_last_message() {
        let mut d = Dialog::default();
        d.begin(Speaker::Phone);
        d.say("one", Voice::Them);
        d.end();
        d.begin(Speaker::Mobile);
        d.say("two", Voice::You);
        d.say("three", Voice::Them);
        d.end();
        d.queue.clear();
        d.repeat();
        assert_eq!(texts(&d), ["open Mobile", "two", "three", "close"]);
        d.repeat();
        assert_eq!(texts(&d).len(), 8);
        // Text without an Open joins the current list.
        d.queue.clear();
        d.say("four", Voice::Them);
        d.queue.clear();
        d.repeat();
        assert_eq!(texts(&d), ["open Mobile", "two", "three", "close", "four"]);
    }

    /// `Dialog_Flush` drops the queue from the first queued Open on.
    #[test]
    fn flush_from_first_open() {
        let mut d = Dialog::default();
        d.say("left", Voice::Them);
        d.end();
        d.begin(Speaker::Phone);
        d.say("dropped", Voice::Them);
        d.flush();
        assert_eq!(texts(&d), ["left", "close"]);
        assert_eq!(d.last.len(), 2);
    }
}
