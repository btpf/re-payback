//! Level select (`FUN_01025544`), level complete (`FUN_0102395c`), game
//! over (`FUN_01021434`) and game complete (`FUN_01022348`).

use super::menus::cup_model;
use super::progress::{Profile, TARGETS};
use super::ui::*;

/// Carousel scroll offset with city `c` centred.
const CENTRED: [i32; 3] = [0x6b, 0, 0xd6];
/// Thumbnail offsets on the 320-unit ring.
const RING: [i32; 3] = [0x35, 0xa0, 0x10b];

pub struct LevelSelect {
    city: usize,
    level: usize,
    scroll: i32,
    /// Scroll direction while moving to the selected city.
    dir: i32,
    /// Confirm zoom, 0..200.
    zoom: Option<i32>,
}

impl LevelSelect {
    /// Starts on the first level without a cup.
    pub fn new(profile: &Profile) -> LevelSelect {
        let i = profile.first_open();
        LevelSelect {
            city: i / 2,
            level: i % 2,
            scroll: CENTRED[i / 2],
            dir: 0,
            zoom: None,
        }
    }
}

impl Screen for LevelSelect {
    fn tick(&mut self, cx: &mut Ctx, input: &Input) -> Step {
        // Scroll 1 unit per step, up to 8 per frame.
        for _ in 0..8 {
            if self.scroll == CENTRED[self.city] {
                break;
            }
            self.scroll = (self.scroll + self.dir).rem_euclid(320);
        }
        if let Some(z) = &mut self.zoom {
            *z += 8;
            if *z >= 200 {
                return Step::Pop(Outcome::Level(self.city as u8, self.level as u8));
            }
            return Step::Stay;
        }
        if input.left || input.right {
            let d: i32 = if input.right { 1 } else { -1 };
            self.city = (self.city as i32 + d).rem_euclid(3) as usize;
            self.dir = -d;
            cx.sound(Sfx::Move);
        } else if input.up || input.down {
            self.level ^= 1;
            cx.sound(Sfx::Move);
        } else if input.back {
            cx.sound(Sfx::Select);
            return Step::Pop(Outcome::Cancel);
        } else if input.fire {
            if super::progress::level_open(cx.profile, cx.settings, self.city * 2 + self.level) {
                cx.sound(Sfx::Select);
                self.zoom = Some(0);
            } else {
                cx.sound(Sfx::Refuse);
            }
        }
        Step::Stay
    }

    fn draw(&self, ui: &mut Ui, cx: &Ctx) {
        ui.picture(0, true);
        let k = self.zoom.unwrap_or(0);
        // Thumbnails: radius max(20, 100 - |x - 160|), corners at 45°, so
        // the side is r·√2; smallest first.
        let mut thumbs: Vec<(i32, usize, i32, i32)> = (0..3)
            .map(|c| {
                let x = (self.scroll + RING[c]).rem_euclid(320);
                let r = (100 - (x - 160).abs()).max(20);
                let (x, r, lift) = if c == self.city && self.zoom.is_some() {
                    (x, r + k, (25 * k / 40).min(25))
                } else if x < 160 {
                    (x - 4 * k, r, 0)
                } else {
                    (x + 3 * k, r, 0)
                };
                (r, c, x, lift)
            })
            .collect();
        thumbs.sort();
        for (r, c, x, lift) in thumbs {
            let fe = ui.fe;
            let (sx, sy) = (ui.vx(x), ui.cy() - 25 + lift);
            ui.square(&fe.thumbs[c], sx, sy, r * 181 / 128);
        }
        if self.zoom.is_some() {
            return;
        }
        ui.title(&cx.g(2087));
        ui.centred(&cx.g(2170 + self.city as u32), 0xb7, 1);
        let fe = ui.fe;
        for l in 0..2 {
            let y = [199, 221][l];
            let i = self.city * 2 + l;
            let c = if l == self.level { ui.flash() } else { 2 };
            if l == self.level {
                ui.icon(&fe.right_arrow, ui.vx(6), y);
            }
            ui.text(&cx.g(2180 + i as u32), ui.vx(0x14), y, c);
            let [gold, silver] = TARGETS[self.city][l];
            let cups = &fe.cups;
            ui.icon(&cups[3], ui.vx(0x96), y - 5);
            ui.text(&hmm(gold), ui.vx(0xa6), y, 1);
            ui.icon(&cups[2], ui.vx(0xcd), y - 5);
            ui.text(&hmm(silver), ui.vx(0xdd), y, 1);
            ui.icon(&cups[cx.profile.cups[i].clamp(0, 3) as usize], ui.vx(0x104), y - 5);
            let best = cx.profile.best[i];
            let t = if best < 0 { "-:--".to_string() } else { hmm(best) };
            ui.text(&t, ui.vx(0x114), y, 1);
        }
    }

    fn brightness(&self) -> f32 {
        let k = self.zoom.unwrap_or(0);
        (1.0 - (k - 100).max(0) as f32 / 100.0).max(0.0)
    }
}

/// Shared shape of the result screens: title slide in, spinning model,
/// "Continue" on fire, slide out.
struct Result {
    obj: OptionObj,
    slide: Option<Slide>,
}

impl Result {
    fn new(model: &'static str, z: i32) -> Result {
        Result {
            obj: OptionObj::new(model, 0, z),
            slide: Some(Slide::slide_in()),
        }
    }

    /// True once the slide-out has finished.
    fn tick(&mut self, cx: &mut Ctx, input: &Input) -> bool {
        if let Some(s) = &mut self.slide {
            if s.tick() {
                if s.out {
                    return true;
                }
                self.slide = None;
            }
            return false;
        }
        self.obj.tick();
        if input.fire || input.back {
            cx.sound(Sfx::Select);
            self.slide = Some(Slide::slide_out());
        }
        false
    }

    /// Picture; false while sliding (nothing else drawn).
    fn draw(&self, ui: &mut Ui) -> bool {
        match &self.slide {
            Some(s) => {
                ui.picture(s.dy(), false);
                false
            }
            None => {
                ui.picture(0, false);
                true
            }
        }
    }

    fn model(&self) -> Option<ModelView> {
        self.slide.is_none().then_some(self.obj.view)
    }

    fn brightness(&self) -> f32 {
        self.slide.map_or(1.0, |s| s.brightness())
    }
}

pub struct LevelComplete {
    minutes: i32,
    cup: i32,
    res: Result,
}

impl LevelComplete {
    /// Awards the cup and stores the best time.
    pub fn new(profile: &mut Profile, city: usize, level: usize, minutes: i32) -> LevelComplete {
        let cup = profile.award(city, level, minutes);
        LevelComplete {
            minutes,
            cup,
            res: Result::new(cup_model(cup), 4000),
        }
    }
}

impl Screen for LevelComplete {
    /// Level complete: track 8 (Mac `CD_CheckForTrackEnd(8)`).
    fn cd_track(&self) -> i32 {
        8
    }

    fn tick(&mut self, cx: &mut Ctx, input: &Input) -> Step {
        if !self.res.tick(cx, input) {
            return Step::Stay;
        }
        if cx.profile.cups.iter().all(|&c| c > 0) {
            Step::Replace(Box::new(GameComplete::new()))
        } else {
            Step::Pop(Outcome::Done)
        }
    }

    fn draw(&self, ui: &mut Ui, cx: &Ctx) {
        if !self.res.draw(ui) {
            return;
        }
        ui.heading(&cx.g(2044), 0x4e);
        ui.centred(&format!("{}{}", cx.g(2040), hmm(self.minutes)), 100, 1);
        ui.centred(&cx.g(2040 + self.cup as u32), 0x7a, 1);
        ui.centred(&cx.g(2110), 0xa0, ui.flash());
    }

    fn model(&self) -> Option<ModelView> {
        self.res.model()
    }

    fn brightness(&self) -> f32 {
        self.res.brightness()
    }
}

pub struct GameOver {
    minutes: i32,
    res: Result,
}

impl GameOver {
    pub fn new(minutes: i32) -> GameOver {
        GameOver {
            minutes,
            res: Result::new("options/cross.obj", 0x898),
        }
    }
}

impl Screen for GameOver {
    /// Game Over: track 16 (Mac `CD_CheckForTrackEnd(0x10)`).
    fn cd_track(&self) -> i32 {
        16
    }

    fn tick(&mut self, cx: &mut Ctx, input: &Input) -> Step {
        if self.res.tick(cx, input) {
            Step::Pop(Outcome::Done)
        } else {
            Step::Stay
        }
    }

    fn draw(&self, ui: &mut Ui, cx: &Ctx) {
        if !self.res.draw(ui) {
            return;
        }
        ui.heading(&cx.g(2045), 0x54);
        ui.centred(&format!("{}{}", cx.g(2046), hmm(self.minutes)), 0x69, 1);
        ui.centred(&cx.g(2110), 0x96, ui.flash());
    }

    fn model(&self) -> Option<ModelView> {
        self.res.model()
    }

    fn brightness(&self) -> f32 {
        self.res.brightness()
    }
}

/// "GAME COMPLETE!", "Congratulations!", the overall rating (2072 + r)
/// and the hint for the next rating (2075 + r), scrolled up into place.
pub struct GameComplete {
    scroll: i32,
    slide: Option<Slide>,
}

impl GameComplete {
    pub fn new() -> GameComplete {
        GameComplete {
            scroll: H,
            slide: None,
        }
    }
}

impl Screen for GameComplete {
    /// Game complete: track 8.
    fn cd_track(&self) -> i32 {
        8
    }

    fn tick(&mut self, cx: &mut Ctx, input: &Input) -> Step {
        if let Some(s) = &mut self.slide {
            return if s.tick() { Step::Pop(Outcome::Done) } else { Step::Stay };
        }
        if self.scroll > 0 {
            self.scroll = (self.scroll - 2).max(0);
            if input.fire {
                self.scroll = 0;
            }
        } else if input.fire || input.back {
            cx.sound(Sfx::Select);
            self.slide = Some(Slide::slide_out());
        }
        Step::Stay
    }

    fn draw(&self, ui: &mut Ui, cx: &Ctx) {
        if let Some(s) = &self.slide {
            ui.picture(s.dy(), true);
            return;
        }
        ui.picture(0, true);
        let r = cx.profile.rating().max(1) as u32;
        let y = 50 + self.scroll;
        ui.heading(&cx.g(2047), y);
        ui.centred(&cx.g(2048), y + 30, 1);
        ui.centred(&cx.g(2072 + r), y + 55, 2);
        ui.wrapped(&cx.g(2075 + r), y + 75, 1);
        if self.scroll == 0 {
            ui.centred(&cx.g(2110), 200, ui.flash());
        }
    }

    fn brightness(&self) -> f32 {
        self.slide.map_or(1.0, |s| s.brightness())
    }
}

#[cfg(test)]
mod tests {
    use super::super::misc::{LoadGame, SaveGame};
    use super::super::progress::{self, testing};
    use super::*;

    fn key(f: impl FnOnce(&mut Input)) -> Input {
        let mut i = Input::default();
        f(&mut i);
        i
    }

    /// Complete a level, save from the game menu's Save screen, start
    /// again, load the slot and check the level select (`FUN_0102395c`,
    /// `FUN_0101f738`, `FUN_0101f1a4`, `FUN_01025544`).
    #[test]
    fn complete_save_restart_load() {
        let dir = testing::scratch("flow");
        let (down, fire, right) = (key(|i| i.down = true), key(|i| i.fire = true), key(|i| i.right = true));
        progress::with_saves_dir(&dir, || {
            let mut env = testing::Env::new();
            env.profile = Profile::new_game("Tester".into());
            let _ = LevelComplete::new(&mut env.profile, 0, 0, 7);
            assert_eq!((env.profile.cups[0], env.profile.best[0]), (3, 7));
            let desc = env.profile.slot_description();
            assert_eq!(desc, "Tester - C1, L2");

            // Save into slot 3 (two down, fire, enter the description).
            let mut save = SaveGame::new();
            let mut cx = env.cx();
            save.tick(&mut cx, &down);
            save.tick(&mut cx, &down);
            assert!(matches!(save.tick(&mut cx, &fire), Step::Push(_)), "text entry");
            assert!(matches!(save.resume(&mut cx, Outcome::Text(desc.clone())), Step::Push(_)), "GAME SAVED!");
            assert!(matches!(save.tick(&mut cx, &Input::default()), Step::Pop(Outcome::Done)));
            assert_eq!(progress::slot_names()[2].as_deref(), Some(desc.as_str()));

            // Restart: a fresh profile; the empty slot 1 is refused.
            let mut env = testing::Env::new();
            let mut cx = env.cx();
            let mut load = LoadGame::new();
            assert!(matches!(load.tick(&mut cx, &fire), Step::Push(_)), "SLOT EMPTY!");
            assert_eq!(cx.sfx.last(), Some(&Sfx::Refuse));
            assert_eq!(*cx.profile, Profile::default());
            load.tick(&mut cx, &down);
            load.tick(&mut cx, &down);
            assert!(matches!(load.tick(&mut cx, &fire), Step::Push(_)), "GAME LOADED!");
            assert!(matches!(load.tick(&mut cx, &Input::default()), Step::Pop(Outcome::Done)));
            assert_eq!(cx.profile.name, "Tester");
            assert_eq!((cx.profile.cups, cx.profile.best), ([3, 0, 0, 0, 0, 0], [7, -1, -1, -1, -1, -1]));

            // Level select starts on city 0 level 1, which may be played.
            let mut sel = LevelSelect::new(cx.profile);
            assert_eq!((sel.city, sel.level), (0, 1));
            sel.tick(&mut cx, &fire);
            assert!(sel.zoom.is_some());

            // City 1 is still locked, until "Unlock all" is on.
            let mut sel = LevelSelect::new(cx.profile);
            sel.tick(&mut cx, &right);
            sel.tick(&mut cx, &fire);
            assert_eq!((sel.city, sel.zoom, cx.sfx.last()), (1, None, Some(&Sfx::Refuse)));
            assert_eq!(super::super::usermaps::step_city(&cx, 0, 1), 0, "Rampage: only city 0");
            cx.settings.unlock_all = true;
            sel.tick(&mut cx, &fire);
            assert!(sel.zoom.is_some());
            assert_eq!(super::super::usermaps::step_city(&cx, 0, 1), 1);
            assert_eq!(super::super::usermaps::step_city(&cx, 1, 1), 2);
            // The profile is untouched; off again restores the gating.
            assert_eq!(cx.profile.cups, [3, 0, 0, 0, 0, 0]);
            cx.settings.unlock_all = false;
            let mut sel = LevelSelect::new(cx.profile);
            sel.tick(&mut cx, &right);
            sel.tick(&mut cx, &fire);
            assert!(sel.zoom.is_none());
        });
        let _ = std::fs::remove_dir_all(&dir);
    }
}
