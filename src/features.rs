//! Perilaku fitur di notch: aksi tombol, pengingat per menit, dan data harian.
use crate::{
    Action, Alert, App, Hud, ai, clock, currency, input, location,
    media::Msg,
    notify, ocr, online,
    pages::Page,
    pomodoro::{Durations, Phase},
    prayer, settings, shelf, store, system,
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
pub const INPUT_ASK: usize = 2;
pub const INPUT_TRANSLATE: usize = 3;
pub const INPUT_KEY: usize = 4;

#[derive(Clone, Copy, PartialEq, Default)]
pub enum Job {
    #[default]
    Ask,
    Translate,
    Verify,
}

/// Percakapan di halaman Tanya.
#[derive(Default)]
pub struct AskState {
    /// (dari user?, teks), urut dari yang lama
    pub history: Vec<(bool, String)>,
    pub thinking: bool,
    pub translate: bool,
    pub error: Option<String>,
    /// teks dari Smart OCR yang bisa ditanyakan
    pub context: Option<String>,
    pub job: Job,
}

/// Jawaban Gemini dari thread pekerja.
pub static AI_RESULT: std::sync::Mutex<Option<Result<String, String>>> = std::sync::Mutex::new(None);

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
        self.notify_preview = settings::flag("Notify.Preview", false);
        self.has_key = settings::secret("Gemini").is_some();
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

    /// Popup singkat tanpa bunyi (Wi-Fi, perangkat audio, notifikasi, baterai).
    pub fn info(&mut self, icon: &'static str, text: String, ms: u32) {
        if self.hud == Some(Hud::Alert) {
            return;
        }
        self.alert = Some(Alert { icon, text, task: None });
        self.show_hud_for(Hud::Info, ms);
    }

    pub fn on_clipboard(&mut self) {
        if !settings::flag("Clip.On", true) {
            return;
        }
        if let Some(text) = shelf::read_text(self.hwnd) {
            let limit = [5, 10, 20, 50][(settings::dword("Clip.Limit", 1) as usize).min(3)];
            shelf::remember(&mut self.clips, text, limit);
            shelf::save_clips(&self.clips);
        }
    }

    /// state: 1 = file sedang diseret di atas notch, 0 = keluar, 2 = dijatuhkan
    pub fn on_drag(&mut self, state: usize) {
        self.dragging = state == 1;
        if state == 1 {
            self.page = self.pages.iter().position(|p| *p == Page::Shelf).unwrap_or(self.page);
        }
        if state == 2 {
            let dropped: Vec<String> = std::mem::take(&mut *shelf::DROPPED.lock().unwrap());
            for f in dropped {
                if !self.files.contains(&f) {
                    self.files.insert(0, f);
                }
            }
            shelf::save_files(&self.files);
        }
    }

    pub fn on_notify(&mut self) {
        let fresh: Vec<_> = std::mem::take(&mut *notify::QUEUE.lock().unwrap());
        let Some(last) = fresh.last().cloned() else { return };
        for n in fresh {
            self.notices.push((n, Instant::now()));
        }
        let excess = self.notices.len().saturating_sub(20);
        self.notices.drain(..excess);
        let text = if self.notify_preview {
            [last.app.as_str(), last.title.as_str(), last.body.as_str()].iter().filter(|s| !s.is_empty()).copied().collect::<Vec<_>>().join(" \u{B7} ")
        } else {
            format!("{} \u{B7} pesan baru", last.app)
        };
        self.info("\u{EA8F}", text, 6000);
    }

    pub fn on_battery(&mut self, plugged: bool, pct: u8) {
        if !settings::flag("Popup.Battery", true) {
            return;
        }
        // pita: 0 = normal, 1 = lemah, 2 = kritis, 3 = penuh saat dicas
        let band = match (plugged, pct) {
            (true, 100) => 3,
            (false, p) if p <= 10 => 2,
            (false, p) if p <= 20 => 1,
            _ => 0,
        };
        if band != self.battery_band {
            self.battery_band = band;
            match band {
                1 => self.alert("\u{EBA0}", format!("Baterai lemah ({pct}%) \u{B7} colokkan charger"), None),
                2 => self.alert("\u{EBA0}", format!("Baterai kritis ({pct}%) \u{B7} colokkan sekarang"), None),
                3 => self.info("\u{EBB5}", "Baterai penuh \u{B7} charger boleh dicabut".into(), 4000),
                _ => {}
            }
        }
    }

    pub fn on_net(&mut self) {
        let Some(now) = system::NETWORK.lock().unwrap().clone() else { return };
        let before = self.net.replace(now.clone());
        if before.is_none() || before.as_ref() == Some(&now) || !settings::flag("Popup.Wifi", true) {
            return;
        }
        let text = if now.is_empty() { "Tidak ada koneksi internet".into() } else if now == "Ethernet" { "Terhubung lewat kabel".into() } else { format!("Terhubung ke Wi-Fi {now}") };
        self.info(if now.is_empty() { "\u{EB5E}" } else { "\u{E701}" }, text, 4000);
    }

    pub fn on_audio_device(&mut self) {
        let name = system::AUDIO_DEVICE.lock().unwrap().clone();
        if !name.is_empty() && settings::flag("Popup.Audio", true) {
            self.info("\u{E7F6}", format!("Audio: {name}"), 4000);
        }
    }

    /// Dipanggil tiap detik: munculkan HUD saat bahasa input berganti di jendela yang sama.
    pub fn check_layout(&mut self) {
        let (window, layout) = system::input_layout();
        let (old_window, old_layout) = self.layout;
        self.layout = (window, layout);
        if window == old_window && layout != old_layout && old_layout != 0 && settings::flag("Hud.Lang", true) {
            self.show_hud(Hud::Lang(layout));
        }
    }

    fn save_tasks(&self) {
        store::write("tasks.txt", &todo::save(&self.tasks));
    }

    pub fn load_data(&mut self) {
        self.today = clock::now().day;
        self.tasks = todo::load(&store::read("tasks.txt"));
        todo::cleanup(&mut self.tasks, self.today);
        self.water = store::daily_get("water.txt", self.today);
        // pengingat minum pertama baru datang 45 menit setelah Ponian dibuka, bukan langsung
        self.water_reminded = clock::now().min;
        self.files = shelf::load_files();
        self.clips = shelf::load_clips();
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
            Action::ShelfOpen(i) => {
                if let Some(f) = self.files.get(i) {
                    shelf::open(f);
                }
            }
            Action::ShelfRemove(i) => {
                if i < self.files.len() {
                    self.files.remove(i);
                    shelf::save_files(&self.files);
                }
            }
            Action::ClipCopy(i) => {
                if let Some(text) = self.clips.get(i).cloned() {
                    shelf::write_text(self.hwnd, &text);
                    self.info("\u{E8C8}", "Disalin ke clipboard".into(), 1500);
                }
            }
            Action::AskOpen(translate) => self.open_ask_input(if translate { INPUT_TRANSLATE } else { INPUT_ASK }),
            Action::AskKey => self.open_ask_input(INPUT_KEY),
            Action::AskCopy => {
                if let Some((false, answer)) = self.ask.history.last().cloned() {
                    shelf::write_text(self.hwnd, &answer);
                    self.info("\u{E8C8}", "Jawaban disalin".into(), 1500);
                }
            }
            Action::AskClear => self.ask = AskState::default(),
        }
    }

    /// Kotak teks di bagian bawah notch untuk halaman Tanya.
    pub fn open_ask_input(&mut self, id: usize) {
        if let Some(i) = self.pages.iter().position(|p| *p == Page::Ask) {
            self.page = i;
        }
        self.update();
        let mut wr = RECT::default();
        let _ = unsafe { GetWindowRect(self.hwnd, &mut wr) };
        let cx = crate::WIN.0 / 2.0;
        let bottom = crate::ASK_HEIGHT - 14.0;
        let px = |x: f32| (x * self.scale) as i32;
        let rect = RECT { left: wr.left + px(cx - 172.0), top: wr.top + px(bottom - 32.0), right: wr.left + px(cx + 172.0), bottom: wr.top + px(bottom) };
        let hint = match id {
            INPUT_KEY => "Tempel API key Gemini (Ctrl+V) lalu Enter",
            INPUT_TRANSLATE => "Teks yang mau diterjemahkan",
            _ if self.has_key => "Tanya apa saja \u{B7} Tab untuk terjemahkan",
            _ => "Hitung cepat, misalnya 12,5 x 4 =",
        };
        input::open(self.hwnd, id, rect, self.scale, "", hint);
    }

    /// Jawab langsung tanpa internet kalau teks diakhiri "=": kurs atau hitungan.
    fn local_answer(&self, text: &str) -> Option<String> {
        if !text.trim_end().ends_with('=') {
            return None;
        }
        if let Some((amount, from, to)) = currency::parse_query(text) {
            let main = currency::MAIN[(settings::dword("Rates.Main", 0) as usize).min(currency::MAIN.len() - 1)];
            let to = to.unwrap_or_else(|| main.to_string());
            let rates = online::RATES.lock().unwrap().clone();
            return Some(match rates.and_then(|r| currency::convert(&r, amount, &from, &to)) {
                Some(v) => format!("{} {from} = {}", ai::format_number(amount), currency::format(v, &to)),
                None => "Kurs belum tersedia. Pastikan internet tersambung dan kurs aktif di Pengaturan.".into(),
            });
        }
        ai::calc(text).map(|v| format!("= {}", ai::format_number(v)))
    }

    fn start_ai(&mut self, job: Job, history: Vec<(bool, String)>) {
        let Some(key) = settings::secret("Gemini") else { return };
        (self.ask.thinking, self.ask.error, self.ask.job, self.ask.translate) = (true, None, job, job == Job::Translate);
        let notch = self.hwnd.0 as isize;
        std::thread::spawn(move || {
            let _ = unsafe { windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_MULTITHREADED) };
            let result = match job {
                Job::Verify => ai::model(&key).map(|_| "Key tersimpan. Sekarang tanya apa saja!".to_string()),
                Job::Translate => ai::ask(&key, &history, ai::TRANSLATE_PROMPT),
                Job::Ask => ai::ask(&key, &history, ai::ASK_PROMPT),
            };
            *AI_RESULT.lock().unwrap() = Some(result);
            let _ = unsafe { windows::Win32::UI::WindowsAndMessaging::PostMessageW(Some(windows::Win32::Foundation::HWND(notch as _)), crate::WM_APP_AI, windows::Win32::Foundation::WPARAM(0), windows::Win32::Foundation::LPARAM(0)) };
        });
    }

    pub fn on_ai(&mut self) {
        let Some(result) = AI_RESULT.lock().unwrap().take() else { return };
        self.ask.thinking = false;
        match (result, self.ask.job) {
            (Ok(text), Job::Verify) => self.ask.history = vec![(false, text)],
            (Ok(text), _) => self.ask.history.push((false, text)),
            (Err(e), Job::Verify) => {
                // key yang ditolak tidak disimpan
                settings::set_secret("Gemini", "");
                self.has_key = false;
                self.ask.error = Some(e);
            }
            (Err(e), _) => self.ask.error = Some(e),
        }
    }

    pub fn on_ocr(&mut self) {
        let Some(result) = ocr::RESULT.lock().unwrap().take() else { return };
        match result {
            Ok(text) => {
                shelf::write_text(self.hwnd, &text);
                self.ask.context = Some(text);
                let tip = if self.has_key { " \u{B7} buka Tanya untuk menanyakannya" } else { "" };
                self.info("\u{E8C8}", format!("Teks dari layar disalin{tip}"), 4000);
            }
            Err(e) => self.info("\u{E7BA}", e, 4000),
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
        let text = text.trim().to_string();
        if text.is_empty() {
            return;
        }
        // Tab di kotak Tanya: terjemahkan teks yang sedang diketik
        let id = if result == input::TAB && id == INPUT_ASK { INPUT_TRANSLATE } else { id };
        if result == input::TAB {
            input::close();
        } else if result != input::SUBMIT {
            return;
        }
        match id {
            INPUT_KEY => {
                if ai::looks_like_key(&text) {
                    settings::set_secret("Gemini", &text);
                    settings::set_string("Gemini.Model", "");
                    self.has_key = true;
                    self.start_ai(Job::Verify, Vec::new());
                } else {
                    self.ask.error = Some("Itu tidak terlihat seperti API key. Salin key-nya saja, lalu tempel lagi.".into());
                }
                return;
            }
            INPUT_ASK => {
                if let Some(answer) = self.local_answer(&text) {
                    self.ask.error = None;
                    self.ask.history.extend([(true, text), (false, answer)]);
                    return;
                }
                if !self.has_key {
                    self.ask.error = Some("Untuk bertanya ke AI, tempel API key Gemini dulu.".into());
                    return;
                }
                // pertanyaan pertama tentang teks dari layar membawa teks itu
                let message = match (&self.ask.context, self.ask.history.is_empty()) {
                    (Some(ctx), true) => format!("Ini teks yang saya salin dari layar:\n{ctx}\n\n{text}"),
                    _ => text.clone(),
                };
                self.ask.history.push((true, text));
                let mut convo = self.ask.history.clone();
                if let Some(last) = convo.last_mut() {
                    last.1 = message;
                }
                // percakapan dibatasi 12 pesan terakhir supaya permintaan tetap kecil
                let skip = convo.len().saturating_sub(12);
                self.start_ai(Job::Ask, convo[skip..].to_vec());
                return;
            }
            INPUT_TRANSLATE => {
                if !self.has_key {
                    self.ask.error = Some("Terjemahan memakai Gemini; tempel API key dulu.".into());
                    return;
                }
                self.ask.history = vec![(true, text.clone())];
                self.start_ai(Job::Translate, vec![(true, text)]);
                return;
            }
            _ => {}
        }
        if id != INPUT_TODO {
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
