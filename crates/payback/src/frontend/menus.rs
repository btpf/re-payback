//! Main menu (`FUN_0102fc44`), game menu (`FUN_0102ca08`) and the in-game
//! pause menu (`FUN_0101a7dc`).

use super::levels::LevelSelect;
use super::multiplayer::MultiplayerSetup;
use super::misc::{CdSetup, ControlSetup, Credits, LanguageChooser, LoadGame, Message, SaveGame, TextEntry};
use super::rampage::RampageSetup;
use super::replays::{ReplayInfo, ReplayList};
use super::display::{self, GraphicsSetup, ITEMS};
use super::progress::Profile;
use super::ui::*;

/// Option models by main-menu item (`DAT_0117b6f8[0..9]`), plus two
/// port-added items before "Quit game": 8 "Graphics options" (not in the
/// original, whose PaybackSetup launcher chose the screen mode and
/// renderer) with the pause menu's model, and 9 "Unlock all levels"
/// ([`UNLOCK_ITEM`]) with the gold cup.
const MAIN_MODELS: [&str; 11] = [
    "options/joystick.obj",
    "options/disk.obj",
    "options/moviereel.obj",
    "options/cup_silver.obj",
    "options/cd.obj",
    "options/controlsetup.obj",
    "options/world.obj",
    "options/moviereel.obj",
    "options/questionmarks.obj",
    "options/cup_gold.obj",
    "options/amiga.obj",
];

/// Main-menu item "Unlock all levels: On / Off" (port-added, no phrase;
/// `Settings::unlock_all`, docs/re/frontend_menus.md §5): fire toggles it
/// in place and saves the settings, so every story level and built-in
/// city can be chosen without starting a game first.
const UNLOCK_ITEM: usize = 9;

/// Save the settings when the game quits, from the menu or by closing the
/// window (Mac: every loop polls `MacHasQuit` @ 1004f560, then
/// `CPaybackApp::Run` calls `amiga_cleanup`, which runs `SaveConfig` @
/// 10052830). Only written when they differ from the file.
pub fn save_on_exit(
    mut exits: bevy::prelude::MessageReader<bevy::app::AppExit>,
    settings: bevy::prelude::Res<super::progress::Settings>,
) {
    if exits.read().count() > 0 && *settings != super::progress::Settings::load() {
        settings.save();
    }
}

/// Cup model by cup/rating 1..3 (`DAT_0117bbf8`).
pub fn cup_model(cup: i32) -> &'static str {
    match cup {
        3 => "options/cup_gold.obj",
        2 => "options/cup_silver.obj",
        _ => "options/cup_bronze.obj",
    }
}

/// "Are you sure?" with Yes/No, shared by the menus.
#[derive(Clone, Copy, Debug, Default)]
struct Confirm {
    yes: bool,
}

impl Confirm {
    /// Some(answer) once fire/Esc is pressed.
    fn tick(&mut self, cx: &mut Ctx, input: &Input) -> Option<bool> {
        if input.left || input.right {
            cx.sound(Sfx::Move);
            self.yes = !self.yes;
        }
        if input.fire {
            cx.sound(Sfx::Select);
            return Some(self.yes);
        }
        if input.back {
            cx.sound(Sfx::Select);
            return Some(false);
        }
        None
    }

    /// 2070 at `y`, Yes/No 60 virtual px either side at `y + 12`, the
    /// chosen one flashing; arrows at `y + 6`.
    fn draw(&self, ui: &mut Ui, cx: &Ctx, y: i32, arrows: bool) {
        ui.centred(&cx.g(2070), y, 1);
        let d = ui.vx(0x3c);
        for (yes, x) in [(true, ui.cx() - d), (false, ui.cx() + d)] {
            let s = cx.g(if yes { 2071 } else { 2072 });
            let c = if yes == self.yes { ui.flash() } else { 1 };
            ui.text(&s, x - 7 * crate::hud::paint::text_len(&s) / 2, y + 12, c);
        }
        if arrows {
            ui.arrows(y + 6);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Waiting {
    None,
    Name,
    Load,
    Game,
}

pub struct MainMenu {
    sel: usize,
    obj: OptionObj,
    confirm: Option<Confirm>,
    slide: Option<Slide>,
    leave: Option<Request>,
    waiting: Waiting,
}

impl MainMenu {
    /// Entered with the title slide-in.
    pub fn new() -> MainMenu {
        MainMenu {
            slide: Some(Slide::slide_in()),
            ..MainMenu::running()
        }
    }

    /// Already on screen (returning from a level's game menu).
    pub fn running() -> MainMenu {
        MainMenu {
            sel: 0,
            obj: OptionObj::new(MAIN_MODELS[0], 600, 6000),
            confirm: None,
            slide: None,
            leave: None,
            waiting: Waiting::Game,
        }
    }

    /// Debug (`--frontend-screen main --menu-item <n>`): on item `n`.
    pub fn at(i: usize) -> MainMenu {
        let sel = i.min(MAIN_MODELS.len() - 1);
        MainMenu {
            sel,
            obj: OptionObj::new(MAIN_MODELS[sel], 600, 6000),
            ..MainMenu::running()
        }
    }

    fn phrase(i: usize) -> u32 {
        match i {
            5 => 2454,
            6 => 2132,
            7 => 2109,
            8 => 1600,
            10 => 2130,
            i => 2090 + i as u32,
        }
    }

    fn leave(&mut self, r: Request) {
        self.slide = Some(Slide::slide_out());
        self.leave = Some(r);
    }
}

impl Screen for MainMenu {
    fn tick(&mut self, cx: &mut Ctx, input: &Input) -> Step {
        if let Some(slide) = &mut self.slide {
            if slide.tick() {
                if slide.out {
                    if let Some(r) = self.leave.take() {
                        cx.requests.push(r);
                    }
                    slide.i = 25;
                } else {
                    self.slide = None;
                }
            }
            return Step::Stay;
        }
        self.obj.tick();
        if let Some(c) = &mut self.confirm {
            if let Some(yes) = c.tick(cx, input) {
                self.confirm = None;
                if yes {
                    self.leave(Request::Exit);
                }
            }
            return Step::Stay;
        }
        if input.left || input.right {
            let d = if input.right { 1 } else { -1 };
            self.sel = (self.sel as i32 + d).rem_euclid(MAIN_MODELS.len() as i32) as usize;
            self.obj.change(MAIN_MODELS[self.sel], d);
            cx.sound(Sfx::Move);
            return Step::Stay;
        }
        if !input.fire {
            return Step::Stay;
        }
        if self.sel == UNLOCK_ITEM {
            cx.sound(Sfx::Select);
            cx.settings.unlock_all = !cx.settings.unlock_all;
            cx.settings.save();
            return Step::Stay;
        }
        if self.sel == 7 {
            // The OS4 build only refuses; the 68k build plays the intro
            // (`FUN_00010364`), and so does the Mac (`DoTitleScreen` @
            // 100a2e70: `OpenAnimation("intro")`, the music stopped first
            // unless it is Disabled, see `crate::music`), refusing when
            // the film is missing.
            let film = cx.data.mac_path("Videos/payback_high_mpeg1.mpg").is_some()
                || crate::vfs::is_file(cx.data.root.join("Videos/intro.video"));
            if !film {
                cx.sound(Sfx::Refuse);
                return Step::Stay;
            }
            cx.sound(Sfx::Select);
            cx.requests.push(Request::PlayIntro);
            return Step::Stay;
        }
        cx.sound(Sfx::Select);
        self.waiting = Waiting::None;
        match self.sel {
            0 => {
                self.waiting = Waiting::Name;
                Step::Push(Box::new(TextEntry::new(cx.g(2088), "Travis", 25)))
            }
            1 => {
                self.waiting = Waiting::Load;
                Step::Push(Box::new(LoadGame::new()))
            }
            2 => Step::Push(Box::new(ReplayList::new(cx))),
            3 => Step::Push(Box::new(Credits::new(2200, 2, true, 0x2e, 100).with_about())),
            4 => Step::Push(Box::new(CdSetup::new())),
            5 => Step::Push(Box::new(ControlSetup::new(cx))),
            6 => Step::Push(Box::new(LanguageChooser::new(cx))),
            8 => Step::Push(Box::new(GraphicsSetup::new(cx.settings))),
            _ => {
                self.confirm = Some(Confirm::default());
                Step::Stay
            }
        }
    }

    fn resume(&mut self, cx: &mut Ctx, outcome: Outcome) -> Step {
        let waiting = std::mem::replace(&mut self.waiting, Waiting::None);
        match (waiting, outcome) {
            (Waiting::Name, Outcome::Text(name)) => {
                // New game: best times -1, cups 0.
                *cx.profile = Profile::new_game(name);
                self.waiting = Waiting::Game;
                Step::Push(Box::new(GameMenu::new()))
            }
            (Waiting::Load, Outcome::Done) => {
                self.waiting = Waiting::Game;
                Step::Push(Box::new(GameMenu::new()))
            }
            (Waiting::Game, Outcome::Level(city, level)) => {
                self.leave(Request::StartLevel { city, level });
                Step::Stay
            }
            (Waiting::Game, Outcome::Rampage(city, police)) => {
                self.leave(Request::StartRampage { city, police });
                Step::Stay
            }
            (Waiting::Game, Outcome::Multiplayer(city)) => {
                self.leave(Request::StartMultiplayer { city });
                Step::Stay
            }
            // "View replay" in the game menu (-2 from `FUN_0102ca08`) or
            // "View saved replay" here.
            (w @ (Waiting::Game | Waiting::None), Outcome::Replay(replay)) => {
                self.leave(Request::PlayReplay {
                    replay,
                    back_to_game: w == Waiting::Game,
                });
                Step::Stay
            }
            _ => Step::Stay,
        }
    }

    fn draw(&self, ui: &mut Ui, cx: &Ctx) {
        if let Some(s) = &self.slide {
            ui.picture(s.dy(), false);
            return;
        }
        ui.picture(0, false);
        let y = ui.hy(0x46);
        let label = if self.sel == UNLOCK_ITEM {
            format!("Unlock all levels: {}", cx.g(if cx.settings.unlock_all { 2081 } else { 2080 }))
        } else {
            cx.g(Self::phrase(self.sel))
        };
        ui.centred(&label, y, 1);
        match &self.confirm {
            Some(c) => c.draw(ui, cx, ui.cy(), true),
            None => ui.arrows(y),
        }
        ui.title(&cx.g(2085));
    }

    fn model(&self) -> Option<ModelView> {
        self.slide.is_none().then_some(self.obj.view)
    }

    fn brightness(&self) -> f32 {
        self.slide.map_or(1.0, |s| s.brightness())
    }
}

/// What the game menu is waiting for from a pushed screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GameWait {
    None,
    /// High-score name for (city, police, score).
    ScoreName(usize, u8, i32),
    ReplayName,
}

pub struct GameMenu {
    sel: usize,
    obj: OptionObj,
    confirm: Option<Confirm>,
    /// Rampage just ended: (city, police, score) to check against the
    /// table before anything else.
    high_score: Option<(usize, u8, i32)>,
    wait: GameWait,
}

impl GameMenu {
    pub fn new() -> GameMenu {
        GameMenu {
            sel: 0,
            obj: OptionObj::new(MAIN_MODELS[0], 600, 6000),
            confirm: None,
            high_score: None,
            wait: GameWait::None,
        }
    }

    /// Back from a Rampage: the high-score entry runs first
    /// (`FUN_0102ca08` with `DAT_0114c7c4` set: the score `DAT_0114c420`
    /// beats the 5th entry of the aggressiveness's table).
    pub fn after_rampage(city: usize, police: u8, score: i32) -> GameMenu {
        GameMenu {
            high_score: Some((city, police, score)),
            ..GameMenu::new()
        }
    }

    /// `DAT_0117b6f8[9..16]`; item 0 is a cup once there is a rating.
    fn model_of(i: usize, profile: &Profile) -> &'static str {
        match i {
            0 if profile.rating() > 0 => cup_model(profile.rating()),
            0 | 2 => "options/joystick.obj",
            1 => "options/joystick_multi.obj",
            3 => "options/disk.obj",
            4 | 5 => "options/moviereel.obj",
            _ => "options/amiga.obj",
        }
    }

    fn phrase(i: usize) -> u32 {
        match i {
            0 => 2134,
            1 => 2133,
            2 => 2197,
            i => 2093 + i as u32,
        }
    }

    /// The replay items need a game played since start/load (p3).
    fn has_replay(cx: &Ctx) -> bool {
        cx.profile.played && cx.last_replay.is_some()
    }
}

/// "ENTER FILENAME" prompt for saving the last game (default "<name> - ").
pub(crate) fn replay_name_entry(cx: &Ctx) -> Box<TextEntry> {
    let default = format!("{} - ", cx.profile.name);
    Box::new(TextEntry::new(cx.g(2084), &default, 30))
}

/// Save the last game as `name` (`FUN_010bc3b0`), then "REPLAY SAVED!".
pub(crate) fn save_last_replay(cx: &mut Ctx, name: &str) -> Step {
    let (Some(file), Some(r)) = (replay_file_name(name), cx.last_replay) else {
        cx.sound(Sfx::Refuse);
        return Step::Stay;
    };
    let dir = crate::replay::saves_dir();
    let res = super::progress::write_file(&dir.join(&file), &r.write());
    match res {
        Ok(()) => Step::Push(Box::new(Message::new(cx.g(2117)))),
        Err(e) => {
            bevy::log::warn!("saving replay {file}: {e}");
            cx.sound(Sfx::Refuse);
            Step::Stay
        }
    }
}

/// A replay file name from the text entry: no path separators or `:`, no
/// trailing dots or spaces (not allowed on Windows).
fn replay_file_name(name: &str) -> Option<String> {
    let s: String = name
        .chars()
        .map(|c| if matches!(c, ':' | '/' | '\\') { '_' } else { c })
        .collect();
    let s = s.trim_end_matches(['.', ' ']).trim_start().to_string();
    (!s.is_empty()).then_some(s)
}

impl Screen for GameMenu {
    fn tick(&mut self, cx: &mut Ctx, input: &Input) -> Step {
        if let Some((city, police, score)) = self.high_score.take() {
            if super::usermaps::load_scores(cx, city).rank(police as usize, score).is_some() {
                self.wait = GameWait::ScoreName(city, police, score);
                let name = cx.profile.name.clone();
                return Step::Push(Box::new(TextEntry::new(cx.g(1611), &name, 25)));
            }
        }
        if self.sel == 0 && self.obj.view.path != Self::model_of(0, cx.profile) && self.obj.view.pos[0] == 0 {
            self.obj.view.path = Self::model_of(0, cx.profile);
        }
        self.obj.tick();
        if let Some(c) = &mut self.confirm {
            if let Some(yes) = c.tick(cx, input) {
                self.confirm = None;
                if yes {
                    return Step::Pop(Outcome::Cancel);
                }
            }
            return Step::Stay;
        }
        if input.left || input.right {
            let d: i32 = if input.right { 1 } else { -1 };
            loop {
                self.sel = (self.sel as i32 + d).rem_euclid(7) as usize;
                if Self::has_replay(cx) || !(4..=5).contains(&self.sel) {
                    break;
                }
            }
            self.obj.change(Self::model_of(self.sel, cx.profile), d);
            cx.sound(Sfx::Move);
            return Step::Stay;
        }
        if input.back {
            cx.sound(Sfx::Select);
            self.confirm = Some(Confirm::default());
            return Step::Stay;
        }
        if !input.fire {
            return Step::Stay;
        }
        match self.sel {
            0 => {
                cx.sound(Sfx::Select);
                Step::Push(Box::new(LevelSelect::new(cx.profile)))
            }
            1 => {
                cx.sound(Sfx::Select);
                Step::Push(Box::new(MultiplayerSetup::new(cx)))
            }
            2 => {
                cx.sound(Sfx::Select);
                Step::Push(Box::new(RampageSetup::new(cx)))
            }
            3 => {
                cx.sound(Sfx::Select);
                Step::Push(Box::new(SaveGame::new()))
            }
            4 if Self::has_replay(cx) => {
                cx.sound(Sfx::Select);
                let r = cx.last_replay.cloned().expect("checked by has_replay");
                Step::Push(Box::new(ReplayInfo::new(r, None)))
            }
            5 if Self::has_replay(cx) => {
                // "ENTER FILENAME", default "<name> - ".
                cx.sound(Sfx::Select);
                self.wait = GameWait::ReplayName;
                Step::Push(replay_name_entry(cx))
            }
            6 => {
                cx.sound(Sfx::Select);
                self.confirm = Some(Confirm::default());
                Step::Stay
            }
            _ => {
                cx.sound(Sfx::Refuse);
                Step::Stay
            }
        }
    }

    fn resume(&mut self, cx: &mut Ctx, outcome: Outcome) -> Step {
        let wait = std::mem::replace(&mut self.wait, GameWait::None);
        match (wait, outcome) {
            (GameWait::ScoreName(city, police, score), o) => {
                // The original keeps whatever is in the buffer, even on Esc.
                let name = match o {
                    Outcome::Text(n) => n,
                    _ => cx.profile.name.clone(),
                };
                let mut table = super::usermaps::load_scores(cx, city);
                if table.insert(police as usize, &name, score).is_some() {
                    super::usermaps::save_scores(cx, city, &table);
                }
                Step::Stay
            }
            (GameWait::ReplayName, Outcome::Text(name)) => save_last_replay(cx, &name),
            (_, Outcome::Level(c, l)) => Step::Pop(Outcome::Level(c, l)),
            (_, Outcome::Rampage(c, p)) => Step::Pop(Outcome::Rampage(c, p)),
            (_, Outcome::Multiplayer(c)) => Step::Pop(Outcome::Multiplayer(c)),
            (_, Outcome::Replay(r)) => Step::Pop(Outcome::Replay(r)),
            _ => Step::Stay,
        }
    }

    fn draw(&self, ui: &mut Ui, cx: &Ctx) {
        ui.picture(0, false);
        let y = ui.hy(0x46);
        ui.centred(&cx.g(Self::phrase(self.sel)), y, 1);
        let rating = cx.profile.rating();
        if self.sel == 0 && rating > 0 {
            ui.centred(&cx.g(2047), ui.hy(0x8c), 2);
            ui.centred(&cx.g(2072 + rating as u32), ui.hy(0x8c) + 12, 2);
        }
        match &self.confirm {
            Some(c) => c.draw(ui, cx, ui.cy(), true),
            None => ui.arrows(y),
        }
        ui.title(&cx.g(2086));
    }

    fn model(&self) -> Option<ModelView> {
        Some(self.obj.view)
    }
}

/// Pause-menu option models (`DAT_0117c944`, `FUN_0101a55c`); item 4 has
/// none.
const PAUSE_MODELS: [&str; 6] = [
    "options/joystick.obj",
    "options/amiga.obj",
    "options/torch.obj",
    "people/malenormal.obj",
    "",
    "options/questionmarks.obj",
];

pub struct PauseMenu {
    sel: usize,
    obj: OptionObj,
    confirm: Option<Confirm>,
    /// Graphics options sub-item (index into [`display::ITEMS`]: the
    /// original's Gouraud shading and Rotating view, `local_5c`; or
    /// `ITEMS.len()`, the added [`display::MORE`] line).
    sub: usize,
    /// Rotation of the person model (`[3] += 0x20`).
    person: i32,
    /// Background `options.obj` angle (`[3] += 0x40` per frame).
    back: i32,
}

impl PauseMenu {
    pub fn new() -> PauseMenu {
        PauseMenu {
            sel: 0,
            obj: OptionObj::new(PAUSE_MODELS[0], 600, 0x1964).slow(),
            confirm: None,
            sub: 0,
            person: 0,
            back: 0,
        }
    }

    /// Opened on item `sel` (debug `--pause-item`).
    pub fn at(sel: usize, settings: &super::progress::Settings) -> PauseMenu {
        let sel = sel.min(PAUSE_MODELS.len() - 1);
        PauseMenu {
            sel,
            obj: OptionObj::new(PAUSE_MODELS[sel], 600, 0x1964).slow(),
            sub: display::first_item(settings),
            ..PauseMenu::new()
        }
    }

    fn label(&self, cx: &Ctx) -> String {
        let s = cx.settings.clone();
        let on_off = |b: bool| cx.g(if b { 2081 } else { 2080 });
        match self.sel {
            2 => format!("{}{}", cx.g(2052), cx.g(2060 + s.fx as u32)),
            // Hardware: always the base phrase 0x820 "Off" (drawn dimmed).
            3 if cx.hardware => format!("{}{}", cx.g(2053), cx.g(2080)),
            3 => format!("{}{}", cx.g(2053), on_off(s.motion_blur)),
            5 => cx.g(1600),
            i => cx.g(2050 + i as u32),
        }
    }
}

impl Screen for PauseMenu {
    /// Keep the music going: `CD_CheckForTrackEnd(0)` (Mac `ShowOptions`
    /// @ 10093240, OS4 `FUN_0101a7dc`).
    fn cd_track(&self) -> i32 {
        0
    }

    fn tick(&mut self, cx: &mut Ctx, input: &Input) -> Step {
        self.obj.tick();
        self.person = (self.person + 0x20).rem_euclid(TURN);
        self.back = (self.back + 0x40).rem_euclid(TURN);
        if let Some(c) = &mut self.confirm {
            if let Some(yes) = c.tick(cx, input) {
                self.confirm = None;
                if yes {
                    cx.settings.save();
                    cx.requests.push(Request::QuitToTitle);
                }
            }
            return Step::Stay;
        }
        // Esc, or a gamepad's B or Start (port addition: Start toggles).
        if input.back || input.start {
            cx.sound(Sfx::Select);
            cx.settings.save();
            cx.requests.push(Request::Resume);
            return Step::Stay;
        }
        if input.left || input.right {
            let d = if input.right { 1 } else { -1 };
            self.sel = (self.sel as i32 + d).rem_euclid(6) as usize;
            self.sub = display::first_item(cx.settings);
            self.obj.change(PAUSE_MODELS[self.sel], d);
            cx.sound(Sfx::Move);
            return Step::Stay;
        }
        let s = &mut *cx.settings;
        match self.sel {
            4 if input.up || input.down || input.fire => {
                // Volume 0..100 in steps of 10 (`FUN_01006558`/`64`).
                let v = s.volume as i32 + if input.down { -10 } else { 10 };
                s.volume = if input.fire && v > 100 { 0 } else { v.clamp(0, 100) as u8 };
                cx.sound(Sfx::Move);
            }
            5 if input.up || input.down => {
                self.sub = display::step_item(self.sub, if input.down { 1 } else { -1 }, s);
                cx.sound(Sfx::Move);
            }
            _ if !input.fire => {}
            0 => {
                cx.sound(Sfx::Select);
                cx.settings.save();
                cx.requests.push(Request::Resume);
            }
            1 => {
                cx.sound(Sfx::Select);
                self.confirm = Some(Confirm::default());
            }
            2 => {
                s.fx = (s.fx + 1) % 3;
                cx.sound(Sfx::Select);
            }
            // Refused under hardware rendering (case 3: `DAT_0114c7d8`).
            3 if s.hardware || cx.hardware => cx.sound(Sfx::Refuse),
            3 => {
                s.motion_blur = !s.motion_blur;
                cx.sound(Sfx::Select);
            }
            5 if self.sub >= ITEMS.len() => {
                // Every graphics setting, as on the front end's screen.
                let screen = GraphicsSetup::in_game(s);
                cx.sound(Sfx::Select);
                return Step::Push(Box::new(screen));
            }
            5 => {
                if !ITEMS[self.sub].selectable(s) {
                    self.sub = display::first_item(s);
                }
                ITEMS[self.sub].activate(cx, 1);
            }
            _ => {}
        }
        Step::Stay
    }

    /// Back from the graphics screen: the renderer may have changed, so
    /// Gouraud may no longer be selectable.
    fn resume(&mut self, cx: &mut Ctx, _outcome: Outcome) -> Step {
        if ITEMS.get(self.sub).is_some_and(|it| !it.selectable(cx.settings)) {
            self.sub = display::first_item(cx.settings);
        }
        Step::Stay
    }

    fn draw(&self, ui: &mut Ui, cx: &Ctx) {
        // FadeDarken3 over the box 50..269 × 26..229 (virtual), as a
        // translucent panel over the frozen game picture. Also under
        // hardware rendering, where the original (`FUN_0101a7dc`) clears
        // the screen every frame instead: a departure at the user's
        // request (docs/re/frontend_menus.md).
        let (x0, x1) = (ui.vx(50), ui.vx(269));
        ui.back.darken(x0, 26, x1, 229);
        let y = ui.hy(0x50);
        if self.sel == 3 && cx.hardware {
            // `FUN_0100cc78` under hardware.
            ui.centred_dim(&self.label(cx), y, 1);
        } else {
            ui.centred(&self.label(cx), y, 1);
        }
        let y1 = ui.hy(0x6e);
        let y2 = ui.hy(0x79);
        match self.sel {
            0 => {
                // Not in rampage or multiplayer (`DAT_0114c7c6`, `DAT_0114c7c8`).
                if cx.story {
                    let goal = if cx.goal <= 0 {
                        cx.g(2056)
                    } else {
                        format!("{}{}{}", cx.g(2058), cx.goal, cx.g(2057))
                    };
                    ui.centred(&goal, y1, 2);
                }
                let minutes = (cx.level_ticks / (crate::mission::LevelClock::TICKS_PER_SECOND * 60)) as i32;
                ui.centred(&format!("{}{}", cx.g(2059), hmm(minutes)), y2, 2);
            }
            4 => {
                // Ten LoadingPiece segments, filled up to the volume.
                let n = cx.settings.volume as i32 / 10;
                let x0 = ui.cx() - 40;
                for i in 0..10 {
                    let fe = ui.fe;
                    ui.icon(if i < n { &fe.piece } else { &fe.piece_empty }, x0 + 8 * i, y1);
                }
            }
            5 => {
                // 1601 at 0x6e/200 and 1602 at 0x79/200 as in the
                // original, the added line 11/200 below. A dimmed item
                // uses `FUN_0100cc78`.
                for (i, item) in ITEMS.iter().enumerate() {
                    let yy = ui.hy(0x6e + 11 * i as i32);
                    let l = item.label(cx);
                    if item.dimmed(cx.settings) {
                        ui.centred_dim(&l, yy, 1);
                    } else {
                        let c = if i == self.sub { ui.flash() } else { 1 };
                        ui.centred(&l, yy, c);
                    }
                }
                let i = ITEMS.len();
                let c = if self.sub == i { ui.flash() } else { 1 };
                ui.centred(display::MORE, ui.hy(0x6e + 11 * i as i32), c);
            }
            _ => {}
        }
        match &self.confirm {
            Some(c) => c.draw(ui, cx, y1, false),
            None => ui.arrows(y),
        }
    }

    /// `options.obj` at (0, -2000), z 10000 (`FUN_0101a7dc`).
    fn back_model(&self) -> Option<ModelView> {
        Some(ModelView { path: "options/options.obj", pos: [0, -2000, 10000], rot: [self.back, 0, 0], paint: None })
    }

    fn model(&self) -> Option<ModelView> {
        let mut v = self.obj.view;
        if v.path.is_empty() {
            return None;
        }
        if v.path == PAUSE_MODELS[3] {
            v.pos[1] = 700;
            v.pos[2] = 0x9c4;
            v.rot = [self.person, 200, 0];
        }
        Some(v)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn replay_names() {
        assert_eq!(super::replay_file_name("Travis - 12:30. ").as_deref(), Some("Travis - 12_30"));
        assert_eq!(super::replay_file_name(" ..").as_deref(), None);
    }

    /// "Unlock all levels" on the main menu: fire toggles the setting in
    /// place and saves it; the menu stays.
    #[test]
    fn main_menu_unlock_all() {
        use super::super::progress::{self, Settings, testing};
        use super::*;
        let dir = testing::scratch("unlock_menu");
        progress::with_saves_dir(&dir, || {
            let mut env = testing::Env::new();
            let mut cx = env.cx();
            let fire = Input { fire: true, ..Input::default() };
            let mut m = MainMenu::at(UNLOCK_ITEM);
            assert!(!cx.settings.unlock_all);
            assert!(matches!(m.tick(&mut cx, &fire), Step::Stay));
            assert!(cx.settings.unlock_all);
            assert!(Settings::load().unlock_all, "saved");
            m.tick(&mut cx, &fire);
            assert!(!cx.settings.unlock_all && !Settings::load().unlock_all);
            assert_eq!(MainMenu::phrase(UNLOCK_ITEM + 1), 2130, "Quit game stays last");
        });
        let _ = std::fs::remove_dir_all(&dir);
    }
}
