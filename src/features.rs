//! Perilaku fitur di notch: aksi tombol, pengingat per menit, dan data harian.
use crate::{
    Action, Alert, App, Hud, clock, input, location,
    media::Msg,
    notify, ocr,
    pages::Page,
    prayer, settings, shelf, store, system,
    todo::{self, Task},
};
use std::time::Instant;
use windows::Win32::{
    Foundation::{LPARAM, RECT, WPARAM},
    System::Diagnostics::Debug::MessageBeep,
    UI::WindowsAndMessaging::{GetWindowRect, MB_ICONASTERISK, PostMessageW},
};

pub const INPUT_TODO: usize = 1;
pub const INPUT_TODO_EDIT: usize = 2;
pub const INPUT_TIMER: usize = 3;
pub const INPUT_CAL: usize = 4;
pub const INPUT_APP_NAME: usize = 5;
pub const SNOOZE: [u32; 3] = [5, 10, 15];

impl App {
    /// Baca setting fitur.
    pub fn reload_features(&mut self) {
        self.sound = settings::flag("Sound", true);
        self.week_monday = settings::dword("Calendar.WeekStart", 0) == 0;
        self.lyrics_on = settings::flag("ShowLyrics", true);
        self.countdown = settings::string("Countdown");
        self.rates_main = (settings::dword("Rates.Main", 0) as usize).min(crate::currency::MAIN.len() - 1);
        self.snooze = SNOOZE[(settings::dword("Todo.Snooze", 1) as usize).min(2)];
        self.notify_preview = settings::flag("Notify.Preview", false);
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
        self.alert = Some(Alert { icon, text, task, open: None });
        self.beep();
        self.show_hud_for(Hud::Alert, 30_000);
    }

    /// Popup singkat tanpa bunyi (Wi-Fi, perangkat audio, notifikasi, baterai).
    pub fn info(&mut self, icon: &'static str, text: String, ms: u32) {
        if self.hud == Some(Hud::Alert) {
            return;
        }
        self.alert = Some(Alert { icon, text, task: None, open: None });
        self.show_hud_for(Hud::Info, ms);
    }

    pub fn on_clipboard(&mut self) {
        if !settings::flag("Clip.On", true) {
            return;
        }
        if let Some(clip) = shelf::read(self.hwnd) {
            let limit = [5, 10, 20, 50][(settings::dword("Clip.Limit", 1) as usize).min(3)];
            shelf::remember(&mut self.clips, clip, limit);
            shelf::save_clips(&self.clips);
        }
    }

    /// state: 1 = file sedang diseret di atas notch, 0 = keluar, 2 = dijatuhkan
    pub fn on_drag(&mut self, state: usize) {
        // file yang dijatuhkan di halaman Aplikasi menjadi pintasan; di halaman lain masuk ke Shelf
        let to_apps = self.view == crate::state::View::Expanded && self.current_page() == Page::Apps;
        self.dragging = state == 1 && !to_apps;
        if state == 1 && !to_apps {
            self.page = self.pages.iter().position(|p| *p == Page::Shelf).unwrap_or(self.page);
        }
        if state == 2 {
            let dropped: Vec<String> = std::mem::take(&mut *shelf::DROPPED.lock().unwrap());
            if to_apps {
                return self.add_apps(dropped);
            }
            for f in dropped {
                if !self.files.contains(&f) {
                    self.files.insert(0, f);
                }
            }
            shelf::save_files(&self.files);
        }
    }

    /// Dipanggil tiap detik. Setelah notch menganggur 20 detik, minta Windows melepas halaman memori
    /// yang tidak sedang dipakai; halaman itu dimuat lagi sendiri saat dibutuhkan.
    pub fn trim_when_idle(&mut self) {
        use windows::Win32::System::Threading::{GetCurrentProcess, SetProcessWorkingSetSize};
        let idle = matches!(self.view, crate::state::View::Idle | crate::state::View::Hidden) && !(self.media.session && self.media.playing);
        self.idle_secs = if idle { self.idle_secs + 1 } else { 0 };
        if self.idle_secs == 20 {
            let _ = unsafe { SetProcessWorkingSetSize(GetCurrentProcess(), usize::MAX, usize::MAX) };
        }
    }

    pub fn add_apps(&mut self, paths: Vec<String>) {
        for path in paths {
            if self.apps.len() < crate::apps::MAX && !self.apps.iter().any(|s| s.path == path) {
                self.apps.push(crate::apps::make(&path));
            }
        }
        crate::apps::save(&self.apps);
    }

    pub fn remove_app(&mut self, i: usize) {
        if i < self.apps.len() {
            self.apps.remove(i);
            crate::apps::save(&self.apps);
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
            format!("{} \u{B7} new message", last.app)
        };
        self.info("\u{EA8F}", text, 6000);
        // info() tidak menimpa pengingat yang sedang tampil, jadi tautan app hanya dipasang pada popup notifikasi;
        // kalau notch sedang dibuka user, popupnya tidak tampil dan halaman tidak diganggu
        if self.hud == Some(Hud::Info) && self.view != crate::state::View::Expanded && !last.aumid.is_empty() {
            self.alert.as_mut().map(|a| a.open = Some(last.aumid));
        }
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
                1 => self.alert("\u{EBA0}", format!("Battery low ({pct}%) \u{B7} plug in the charger"), None),
                2 => self.alert("\u{EBA0}", format!("Battery critical ({pct}%) \u{B7} plug in now"), None),
                3 => self.info("\u{EBB5}", "Battery full \u{B7} you can unplug the charger".into(), 4000),
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
        let text = if now.is_empty() { "No internet connection".into() } else if now == "Ethernet" { "Connected via Ethernet".into() } else { format!("Connected to Wi-Fi {now}") };
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
        self.files = shelf::load_files();
        self.apps = crate::apps::load();
        self.clips = shelf::load_clips();
    }

    /// Dijalankan tiap ganti menit: pergantian hari, pengingat tugas, dan waktu salat.
    pub fn minute_jobs(&mut self) {
        let now = clock::now();
        if now.day != self.today {
            self.today = now.day;
            todo::cleanup(&mut self.tasks, now.day);
            self.save_tasks();
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
                    let text = if before == 0 { format!("Time for {} prayer", prayer::NAMES[i]) } else { format!("{} in {before} min ({})", prayer::NAMES[i], clock::hhmm(times[i])) };
                    return self.alert("\u{E706}", text, None);
                }
            }
        }
    }

    /// Dipanggil tiap 250 ms saat timer berjalan; true tepat saat waktu habis.
    pub fn timer_tick(&mut self) -> bool {
        if !self.timer.tick(Instant::now()) {
            return false;
        }
        self.beep();
        if let Some(i) = self.pages.iter().position(|p| *p == Page::Timer) {
            self.page = i;
        }
        true
    }

    pub fn act(&mut self, action: Action) {
        let now = clock::now();
        match action {
            Action::Media(msg) => drop(self.tx.send(msg())),
            Action::Page(i) => self.go(i, if i > self.page { 1.0 } else { -1.0 }),
            // ditangani di App::click karena butuh posisi klik
            Action::Seek { .. } => {}
            Action::Mute => drop(self.tx.send(Msg::Mute)),
            Action::LyricsToggle => {
                self.lyrics_on = !self.lyrics_on;
                settings::set_dword("ShowLyrics", self.lyrics_on as u32);
                let _ = self.tx.send(Msg::Refresh);
            }
            Action::OpenSettings => {
                let _ = unsafe { PostMessageW(Some(self.hwnd), crate::WM_APP_OPEN_SETTINGS, WPARAM(0), LPARAM(0)) };
            }
            Action::ShelfTab(tab) => self.shelf_tab = tab,
            Action::PrevPage => self.turn(-1),
            Action::NextPage => self.turn(1),
            Action::TimerToggle => self.timer.toggle(Instant::now()),
            Action::TimerReset => self.timer.reset(),
            Action::AppOpen(i) => {
                if let Some(s) = self.apps.get(i) {
                    shelf::open(&s.path);
                }
            }
            // dialog pilih file dibuka dari wndproc, di luar peminjaman state app
            Action::AppAdd => {
                let _ = unsafe { PostMessageW(Some(self.hwnd), crate::WM_APP_PICK, WPARAM(0), LPARAM(0)) };
            }
            Action::CalMove(by) => self.cal_offset = (self.cal_offset + by).clamp(-1200, 1200),
            Action::CalToday => (self.cal_offset, self.cal_day) = (0, None),
            Action::CalDay(day) => self.cal_day = Some(day),
            Action::TimerSet(minutes) => {
                self.timer.set(minutes);
                settings::set_dword("Timer.Minutes", minutes as u32);
            }
            Action::TodoAdd => self.open_input(INPUT_TODO, "", "New task, e.g. Meeting tomorrow 14:30"),
            Action::TodoEdit(i) => {
                if let Some(text) = self.tasks.get(i).map(|t| todo::to_input(t, now.day)) {
                    self.editing = i;
                    self.open_input(INPUT_TODO_EDIT, &text, "Edit task");
                }
            }
            Action::CalAdd => self.open_input(INPUT_CAL, "", "Event on the selected day, e.g. Meeting 14:30"),
            Action::TimerCustom => self.open_input(INPUT_TIMER, "", "Duration in minutes, or hours:minutes (e.g. 90 or 1:30)"),
            Action::TimerMode(stopwatch) => self.stopwatch_mode = stopwatch,
            Action::StopwatchToggle => self.stopwatch.toggle(Instant::now()),
            Action::StopwatchReset => self.stopwatch.reset(),
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
            Action::Snooze => {
                let title = self.alert.take().and_then(|a| a.task);
                let snooze = self.snooze;
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
                if let Some(clip) = self.clips.get(i) {
                    shelf::write(self.hwnd, clip);
                    self.info("\u{E8C8}", "Copied to clipboard".into(), 1500);
                }
            }
        }
    }

    pub fn on_ocr(&mut self) {
        let Some(result) = ocr::RESULT.lock().unwrap().take() else { return };
        match result {
            Ok(text) => {
                shelf::write_text(self.hwnd, &text);
                self.info("\u{E8C8}", "Screen text copied".into(), 4000);
            }
            Err(e) => self.info("\u{E7BA}", e, 4000),
        }
    }

}

impl App {
    /// Buka kotak teks di tengah panel. `id` menentukan untuk apa teksnya nanti dipakai.
    pub fn open_input(&mut self, id: usize, initial: &str, hint: &str) {
        let mut wr = RECT::default();
        let _ = unsafe { GetWindowRect(self.hwnd, &mut wr) };
        let cx = crate::WIN.0 / 2.0;
        let px = |x: f32| (x * self.scale) as i32;
        let rect = RECT { left: wr.left + px(cx - 170.0), top: wr.top + px(72.0), right: wr.left + px(cx + 170.0), bottom: wr.top + px(110.0) };
        input::open(self.hwnd, id, rect, self.scale, initial, hint);
    }

    pub fn on_input(&mut self, id: usize, result: isize) {
        let Some(text) = input::take_text() else { return };
        if result != input::SUBMIT {
            return;
        }
        let now = clock::now();
        match id {
            INPUT_TIMER => {
                if let Some(minutes) = crate::timer::parse_minutes(&text) {
                    self.timer.set(minutes);
                    settings::set_dword("Timer.Minutes", minutes as u32);
                }
            }
            INPUT_APP_NAME => {
                if let Some(s) = self.apps.get_mut(self.editing).filter(|_| !text.trim().is_empty()) {
                    s.name = text.trim().to_string();
                    crate::apps::save(&self.apps);
                }
            }
            // tugas baru, tugas yang diedit, atau acara pada hari yang dipilih di Kalender
            _ => {
                let default_day = if id == INPUT_CAL { self.cal_day.unwrap_or(now.day) } else { now.day };
                let Some((title, time, day)) = todo::parse(&text, now.day) else { return };
                let day = day.unwrap_or(default_day);
                // jam yang sudah lewat hari ini tidak langsung memicu pengingat
                let reminded = day == now.day && time.is_some_and(|m| m < now.min);
                match self.tasks.get_mut(self.editing).filter(|_| id == INPUT_TODO_EDIT) {
                    Some(t) => (t.title, t.time, t.day, t.reminded) = (title, time, day, reminded),
                    None => self.tasks.push(Task { title, time, done: false, reminded, day }),
                }
                self.save_tasks();
            }
        }
    }
}

impl App {
    /// Jalankan pilihan menu klik kanan untuk pintasan ke-`i`: 1 ganti nama, 2 kiri, 3 kanan, 4 hapus.
    pub fn app_command(&mut self, i: usize, cmd: i32) {
        if i >= self.apps.len() {
            return;
        }
        match cmd {
            1 => {
                self.editing = i;
                let name = self.apps[i].name.clone();
                self.open_input(INPUT_APP_NAME, &name, "Shortcut name");
            }
            2 if i > 0 => self.apps.swap(i, i - 1),
            3 if i + 1 < self.apps.len() => self.apps.swap(i, i + 1),
            4 => self.remove_app(i),
            _ => return,
        }
        self.hot = None;
        crate::apps::save(&self.apps);
    }
}
