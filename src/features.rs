//! Perilaku fitur di notch: aksi tombol, pengingat per menit, dan data harian.
use crate::{
    Action, Alert, App, Hud, clock, input, location,
    media::Msg,
    pages::Page,
    pomodoro::{Durations, Phase},
    prayer, settings, store,
    todo::{self, Task},
    water,
};
use std::time::Instant;
use windows::Win32::{
    Foundation::RECT,
    System::Diagnostics::Debug::MessageBeep,
    UI::WindowsAndMessaging::{GetWindowRect, MB_ICONASTERISK},
};

pub const INPUT_TODO: usize = 1;

#[derive(Clone, Copy, Default)]
pub struct WaterCfg {
    pub on: bool,
    pub goal: u32,
    pub glass: u32,
    pub from: u32,
    pub until: u32,
}

const FOCUS: [u64; 7] = [15, 20, 25, 30, 45, 50, 60];
const SHORT: [u64; 3] = [3, 5, 10];
const LONG: [u64; 4] = [10, 15, 20, 30];
pub const SNOOZE: [u32; 3] = [5, 10, 15];
const GOAL: [u32; 5] = [1500, 2000, 2500, 3000, 3500];
const GLASS: [u32; 4] = [200, 250, 330, 500];

fn pick<T: Copy>(list: &[T], key: &str, default: u32) -> T {
    list[(settings::dword(key, default) as usize).min(list.len() - 1)]
}

impl App {
    /// Baca setting fitur.
    pub fn reload_features(&mut self) {
        self.durations = Durations { focus: pick(&FOCUS, "Pomo.Focus", 2), short: pick(&SHORT, "Pomo.Short", 1), long: pick(&LONG, "Pomo.Long", 1) };
        self.water_cfg = WaterCfg {
            on: settings::flag("Water.On", true),
            goal: pick(&GOAL, "Water.Goal", 1),
            glass: pick(&GLASS, "Water.Glass", 1),
            from: 5 + settings::dword("Water.From", 3).min(7),
            until: 17 + settings::dword("Water.Until", 4).min(7),
        };
        self.sound = settings::flag("Sound", true);
        self.widgets = (settings::dword("Widget.Left", 0) as usize, settings::dword("Widget.Right", 1) as usize);
        self.cities = (settings::dword("Clock.City1", 5) as usize, settings::dword("Clock.City2", 10) as usize);
        self.compute_prayer();
    }

    pub fn compute_prayer(&mut self) {
        let Some((lat, lon)) = location::current() else {
            self.prayer = None;
            return;
        };
        let now = clock::now();
        let (y, m, d) = clock::civil_from_days(now.day);
        let method = &prayer::METHODS[(settings::dword("Prayer.Method", 0) as usize).min(prayer::METHODS.len() - 1)];
        let t = prayer::times(y, m, d, lat, lon, now.offset as f64 / 60.0, method, settings::flag("Prayer.Hanafi", false));
        self.prayer = Some(t.map(prayer::minutes));
        self.prayer_info = format!("{} \u{B7} {lat:.2}, {lon:.2}", method.name);
    }

    fn beep(&self) {
        if self.sound {
            let _ = unsafe { MessageBeep(MB_ICONASTERISK) };
        }
    }

    pub fn alert(&mut self, icon: &'static str, text: String, task: Option<String>) {
        self.alert = Some(Alert { icon, text, task });
        self.beep();
        self.show_hud_for(Hud::Alert, 30_000);
    }

    fn save_tasks(&self) {
        store::write("tasks.txt", &todo::save(&self.tasks));
    }

    pub fn load_data(&mut self) {
        self.today = clock::now().day;
        self.tasks = todo::load(&store::read("tasks.txt"));
        todo::cleanup(&mut self.tasks, self.today);
        self.water = store::daily_get("water.txt", self.today);
    }

    /// Dijalankan tiap ganti menit: pergantian hari, pengingat tugas, minum air, dan waktu salat.
    pub fn minute_jobs(&mut self) {
        let now = clock::now();
        if now.day != self.today {
            self.today = now.day;
            todo::cleanup(&mut self.tasks, now.day);
            self.save_tasks();
            self.water = store::daily_get("water.txt", now.day);
            self.water_reminded = 0;
            self.compute_prayer();
        }

        if settings::flag("Todo.Remind", true)
            && let Some(i) = todo::due(&self.tasks, now.day, now.min)
        {
            self.tasks[i].reminded = true;
            self.save_tasks();
            let title = self.tasks[i].title.clone();
            return self.alert("\u{E823}", title.clone(), Some(title));
        }

        let w = self.water_cfg;
        if w.on && water::due(now.min, w.from, w.until, w.goal, self.water, w.glass, now.min.saturating_sub(self.water_reminded)) {
            self.water_reminded = now.min;
            return self.alert("\u{EB42}", format!("Waktunya minum segelas air ({} ml)", w.glass), None);
        }

        let notify = settings::dword("Prayer.Notify", 1);
        if let Some(times) = self.prayer
            && notify != 0
        {
            let before = if notify == 2 { 10 } else { 0 };
            for i in [0, 2, 3, 4, 5] {
                let key = format!("{}-{i}", now.day);
                if times[i].saturating_sub(before) == now.min && self.prayer_done != key {
                    self.prayer_done = key;
                    if before == 0 && settings::flag("Prayer.Pause", false) {
                        let _ = self.tx.send(Msg::Pause);
                    }
                    let text = if before == 0 { format!("Waktunya salat {}", prayer::NAMES[i]) } else { format!("{} {before} menit lagi ({})", prayer::NAMES[i], clock::hhmm(times[i])) };
                    return self.alert("\u{E706}", text, None);
                }
            }
        }
    }

    /// Dipanggil tiap 250 ms saat Pomodoro berjalan.
    pub fn pomo_tick(&mut self) -> bool {
        let Some(phase) = self.pomo.tick(Instant::now()) else { return false };
        if phase == Phase::Focus {
            let day = clock::now().day;
            store::daily_set("focus.txt", day, store::daily_get("focus.txt", day) + self.durations.focus as u32);
        }
        self.beep();
        self.page = self.pages.iter().position(|p| *p == Page::Pomodoro).unwrap_or(self.page);
        true
    }

    pub fn act(&mut self, action: Action) {
        let now = clock::now();
        match action {
            Action::Media(msg) => drop(self.tx.send(msg())),
            Action::Page(i) => self.page = i,
            Action::PrevPage => self.turn(-1),
            Action::NextPage => self.turn(1),
            Action::PomoToggle => self.pomo.toggle(Instant::now(), self.durations),
            Action::PomoReset => self.pomo.reset(),
            Action::TodoAdd => self.open_todo_input(),
            Action::TodoToggle(i) => {
                if let Some(t) = self.tasks.get_mut(i) {
                    t.done = !t.done;
                    if t.done {
                        t.day = now.day;
                    }
                    self.save_tasks();
                }
            }
            Action::TodoDelete(i) => {
                if i < self.tasks.len() {
                    self.tasks.remove(i);
                    self.save_tasks();
                }
            }
            Action::WaterAdd => {
                self.water += self.water_cfg.glass;
                store::daily_set("water.txt", now.day, self.water);
            }
            Action::WaterUndo => {
                self.water = self.water.saturating_sub(self.water_cfg.glass);
                store::daily_set("water.txt", now.day, self.water);
            }
            Action::Snooze => {
                let title = self.alert.take().and_then(|a| a.task);
                let snooze = SNOOZE[(settings::dword("Todo.Snooze", 1) as usize).min(2)];
                if let Some(t) = self.tasks.iter_mut().find(|t| Some(&t.title) == title.as_ref() && !t.done) {
                    (t.time, t.day, t.reminded) = (Some((now.min + snooze).min(1439)), now.day, false);
                }
                self.save_tasks();
                self.hud = None;
            }
        }
    }

    fn open_todo_input(&mut self) {
        let mut wr = RECT::default();
        let _ = unsafe { GetWindowRect(self.hwnd, &mut wr) };
        let cx = crate::WIN.0 / 2.0;
        let px = |x: f32| (x * self.scale) as i32;
        let rect = RECT { left: wr.left + px(cx - 170.0), top: wr.top + px(74.0), right: wr.left + px(cx + 170.0), bottom: wr.top + px(108.0) };
        input::open(self.hwnd, INPUT_TODO, rect, self.scale, "", "Tugas baru, misalnya: Rapat 14:30");
    }

    pub fn on_input(&mut self, id: usize, result: isize) {
        let Some(text) = input::take_text() else { return };
        if result != input::SUBMIT || id != INPUT_TODO {
            return;
        }
        let Some((title, time)) = todo::parse(&text) else { return };
        let now = clock::now();
        // jam yang sudah lewat hari ini tidak langsung memicu pengingat
        let reminded = time.is_some_and(|m| m < now.min);
        self.tasks.push(Task { title, time, done: false, reminded, day: now.day });
        self.save_tasks();
    }
}
