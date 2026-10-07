//! Main menu (`FUN_0102fc44`), game menu (`FUN_0102ca08`) and the in-game
//! pause menu (`FUN_0101a7dc`).

use super::levels::LevelSelect;
use super::misc::{CdSetup, ControlSetup, Credits, LanguageChooser, LoadGame, ReplayList, SaveGame, TextEntry};
use super::progress::Profile;
use super::ui::*;

/// Option models by main-menu item (`DAT_0117b6f8[0..9]`).
const MAIN_MODELS: [&str; 9] = [
    "options/joystick.obj",
    "options/disk.obj",
    "options/moviereel.obj",
    "options/cup_silver.obj",
    "options/cd.obj",
    "options/controlsetup.obj",
    "options/world.obj",
    "options/moviereel.obj",
    "options/amiga.obj",
];

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

    fn phrase(i: usize) -> u32 {
        match i {
            5 => 2454,
            6 => 2132,
            7 => 2109,
            8 => 2130,
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
            self.sel = (self.sel as i32 + d).rem_euclid(9) as usize;
            self.obj.change(MAIN_MODELS[self.sel], d);
            cx.sound(Sfx::Move);
            return Step::Stay;
        }
        if !input.fire {
            return Step::Stay;
        }
        if self.sel == 7 {
            // The OS4 build only refuses; the 68k build plays the intro
            // (`FUN_00010364`).
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
            3 => Step::Push(Box::new(Credits::new(2200, 2, true, 0x2e, 100))),
            4 => Step::Push(Box::new(CdSetup::new())),
            5 => Step::Push(Box::new(ControlSetup::new())),
            6 => Step::Push(Box::new(LanguageChooser::new(cx))),
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
        ui.centred(&cx.g(Self::phrase(self.sel)), y, 1);
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

pub struct GameMenu {
    sel: usize,
    obj: OptionObj,
    confirm: Option<Confirm>,
}

impl GameMenu {
    pub fn new() -> GameMenu {
        GameMenu {
            sel: 0,
            obj: OptionObj::new(MAIN_MODELS[0], 600, 6000),
            confirm: None,
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
}

impl Screen for GameMenu {
    fn tick(&mut self, cx: &mut Ctx, input: &Input) -> Step {
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
                // Replay items need a game played since start/load.
                if cx.profile.played || !(4..=5).contains(&self.sel) {
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
            3 => {
                cx.sound(Sfx::Select);
                Step::Push(Box::new(SaveGame::new()))
            }
            6 => {
                cx.sound(Sfx::Select);
                self.confirm = Some(Confirm::default());
                Step::Stay
            }
            // Multiplayer, rampage and replays are not implemented.
            _ => {
                cx.sound(Sfx::Refuse);
                Step::Stay
            }
        }
    }

    fn resume(&mut self, _cx: &mut Ctx, outcome: Outcome) -> Step {
        match outcome {
            Outcome::Level(c, l) => Step::Pop(Outcome::Level(c, l)),
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
    /// Graphics options sub-item.
    sub: usize,
    /// Rotation of the person model (`[3] += 0x20`).
    person: i32,
}

impl PauseMenu {
    pub fn new() -> PauseMenu {
        PauseMenu {
            sel: 0,
            obj: OptionObj::new(PAUSE_MODELS[0], 600, 0x1964).slow(),
            confirm: None,
            sub: 0,
            person: 0,
        }
    }

    fn label(&self, cx: &Ctx) -> String {
        let s = cx.settings.clone();
        let on_off = |b: bool| cx.g(if b { 2081 } else { 2080 });
        match self.sel {
            2 => format!("{}{}", cx.g(2052), cx.g(2060 + s.fx as u32)),
            3 => format!("{}{}", cx.g(2053), on_off(s.motion_blur)),
            5 => cx.g(1600),
            i => cx.g(2050 + i as u32),
        }
    }
}

impl Screen for PauseMenu {
    fn tick(&mut self, cx: &mut Ctx, input: &Input) -> Step {
        self.obj.tick();
        self.person = (self.person + 0x20).rem_euclid(TURN);
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
        if input.back {
            cx.sound(Sfx::Select);
            cx.settings.save();
            cx.requests.push(Request::Resume);
            return Step::Stay;
        }
        if input.left || input.right {
            let d = if input.right { 1 } else { -1 };
            self.sel = (self.sel as i32 + d).rem_euclid(6) as usize;
            self.sub = 0;
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
                self.sub ^= 1;
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
            3 => {
                s.motion_blur = !s.motion_blur;
                cx.sound(Sfx::Select);
            }
            5 => {
                if self.sub == 0 {
                    s.gouraud = !s.gouraud;
                } else {
                    s.rotating_view = !s.rotating_view;
                }
                cx.sound(Sfx::Select);
            }
            _ => {}
        }
        Step::Stay
    }

    fn draw(&self, ui: &mut Ui, cx: &Ctx) {
        // FadeDarken3 over the box 50..269 × 26..229 (virtual).
        let (x0, x1) = (ui.vx(50), ui.vx(269));
        ui.back.darken(x0, 26, x1, 229);
        let y = ui.hy(0x50);
        ui.centred(&self.label(cx), y, 1);
        let y1 = ui.hy(0x6e);
        let y2 = ui.hy(0x79);
        let on_off = |b: bool| cx.g(if b { 2081 } else { 2080 });
        match self.sel {
            0 => {
                let goal = if cx.goal <= 0 {
                    cx.g(2056)
                } else {
                    format!("{}{}{}", cx.g(2058), cx.goal, cx.g(2057))
                };
                ui.centred(&goal, y1, 2);
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
                let lines = [
                    format!("{}{}", cx.g(1601), on_off(cx.settings.gouraud)),
                    format!("{}{}", cx.g(1602), on_off(cx.settings.rotating_view)),
                ];
                for (i, (l, yy)) in lines.iter().zip([y1, y2]).enumerate() {
                    let c = if i == self.sub { ui.flash() } else { 1 };
                    ui.centred(l, yy, c);
                }
            }
            _ => {}
        }
        match &self.confirm {
            Some(c) => c.draw(ui, cx, y1, false),
            None => ui.arrows(y),
        }
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
