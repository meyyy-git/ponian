#![windows_subsystem = "windows"]
//! Ponian: notch untuk Windows.

mod input;
mod lyrics;
mod media;
mod pages;
mod power;
mod render;
mod settings;
mod state;
mod tray;
mod volume;

use media::{Media, Msg};
use pages::Page;
use settings::{Item, Section, opts};
use render::{DIM, GRAY, GREEN, Gfx, R, WHITE};
use state::{Inputs, Timer, View};
use std::{
    cell::RefCell,
    sync::{atomic::Ordering::Relaxed, mpsc::Sender},
    time::Instant,
};
use windows::{
    Win32::{
        Foundation::*,
        System::{Diagnostics::Debug::MessageBeep, LibraryLoader::GetModuleHandleW, SystemInformation::GetLocalTime, Threading::CreateMutexW},
        Graphics::Gdi::{EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO},
        UI::{
            HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForMonitor, MDT_EFFECTIVE_DPI, SetProcessDpiAwarenessContext},
            Input::KeyboardAndMouse::{TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent},
            WindowsAndMessaging::*,
        },
    },
    core::*,
};

pub const WM_APP_MEDIA: u32 = WM_APP + 1;
pub const WM_APP_VOLUME: u32 = WM_APP + 2;
pub const WM_APP_TRAY: u32 = WM_APP + 3;
const WM_APP_OPEN_SETTINGS: u32 = WM_APP + 5;
const WM_MOUSELEAVE: u32 = 0x02A3;
const WM_MOUSEHWHEEL: u32 = 0x020E;

/// Ukuran window dalam DIP. Notch digambar di dalamnya, sisanya transparan dan tembus klik.
const WIN: (f32, f32) = (440.0, 190.0);
const IDLE: (f32, f32) = (152.0, 30.0);
const EXPANDED: (f32, f32) = (405.0, 172.0);
/// Notch sedikit membesar begitu kursor masuk, sebelum jeda hover selesai.
const PEEK: (f32, f32) = (8.0, 3.0);
const EAR: f32 = 6.0;
const HUD_MS: u32 = 3000;
const HOVER_MS: u32 = 150;
const FADE_SECS: f32 = 0.22;

const T_ANIM: usize = 1;
const T_TICK: usize = 2;
const T_POLL: usize = 3;
const T_HOVER: usize = 4;
const T_HUD: usize = 5;

#[derive(Clone, Copy)]
enum Hud {
    Volume,
    Power(bool, u8),
    Track,
}

#[derive(Clone, Copy)]
pub enum Action {
    Media(fn() -> Msg),
    Timer(u64),
    StopTimer,
    Page(usize),
    PrevPage,
    NextPage,
}

pub struct App {
    hwnd: HWND,
    gfx: Gfx,
    scale: f32,
    tx: Sender<Msg>,
    media: Media,
    last_track: String,
    timer: Timer,
    hud: Option<Hud>,
    plugged: Option<bool>,
    inside: bool,
    hover: bool,
    user_hidden: bool,
    fullscreen: bool,
    ticking: bool,
    pages: Vec<Page>,
    page: usize,
    lyric_offset_ms: i64,
    hide_fullscreen: bool,
    minute: u16,
    view: View,
    cur: (f32, f32),
    from: (f32, f32),
    to: (f32, f32),
    /// isi notch di-fade hanya kalau bentuknya berganti, bukan saat ukurannya saja yang berubah
    fade: bool,
    anim_start: Option<Instant>,
    born: Instant,
    buttons: Vec<(R, Action)>,
}

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
}

impl App {
    fn ringing(&self) -> bool {
        self.timer == Timer::Ringing
    }

    fn timer_running(&self) -> bool {
        matches!(self.timer, Timer::Running(_))
    }

    /// Posisi putar untuk lirik, sudah dimajukan sesuai setting "Majukan lirik".
    fn lyric_pos(&self) -> i64 {
        self.media.pos() + self.lyric_offset_ms
    }

    fn current_page(&self) -> Page {
        self.pages[self.page.min(self.pages.len() - 1)]
    }

    /// Baca ulang setting yang dipakai notch.
    fn reload(&mut self) {
        let page = self.current_page();
        self.pages = pages::enabled();
        self.page = self.pages.iter().position(|p| *p == page).unwrap_or(0);
        self.lyric_offset_ms = [0, 300, 600, 1000][settings::dword("LyricOffset", 0).min(3) as usize];
        self.hide_fullscreen = settings::flag("HideFullscreen", true);
        self.user_hidden = settings::flag("HideNotch", false);
        if !self.hide_fullscreen {
            self.fullscreen = false;
        }
    }

    fn target(&self, view: View) -> (f32, f32) {
        let m = &self.media;
        let peek = if self.inside { PEEK } else { (0.0, 0.0) };
        match view {
            View::Hidden => (IDLE.0, 0.0),
            View::Idle => (IDLE.0 + peek.0, IDLE.1 + peek.1),
            View::Compact => {
                let w = if m.lyrics.is_some() { 360.0 } else { 280.0 } + if self.timer_running() { 54.0 } else { 0.0 };
                (w + peek.0, IDLE.1 + peek.1)
            }
            View::Hud => (300.0, 34.0),
            View::Expanded => EXPANDED,
        }
    }

    /// Hitung ulang bentuk notch dari keadaan sekarang, lalu gambar.
    fn update(&mut self) {
        let playing = self.media.session && self.media.playing;
        let view = state::view(Inputs {
            hidden: self.user_hidden || self.fullscreen,
            hover: self.hover,
            ringing: self.ringing(),
            hud: self.hud.is_some(),
            playing,
        });
        let to = self.target(view);
        unsafe {
            if view != self.view || to != self.to {
                self.fade = view != self.view;
                (self.view, self.from, self.to) = (view, self.cur, to);
                self.anim_start = Some(Instant::now());
                SetTimer(Some(self.hwnd), T_ANIM, 16, None);
            }
            let need_tick = self.timer != Timer::Off || (playing && self.media.lyrics.is_some());
            if need_tick != self.ticking {
                self.ticking = need_tick;
                if need_tick {
                    SetTimer(Some(self.hwnd), T_TICK, 250, None);
                } else {
                    let _ = KillTimer(Some(self.hwnd), T_TICK);
                }
            }
        }
        self.draw();
    }

    fn show_hud(&mut self, hud: Hud) {
        self.hud = Some(hud);
        unsafe { SetTimer(Some(self.hwnd), T_HUD, HUD_MS, None) };
        self.update();
    }

    fn draw(&mut self) {
        let mut k = 1.0;
        if let Some(start) = self.anim_start {
            let t = start.elapsed().as_secs_f32();
            let e = state::spring(t);
            self.cur = (self.from.0 + (self.to.0 - self.from.0) * e, self.from.1 + (self.to.1 - self.from.1) * e);
            if self.fade {
                k = (t / FADE_SECS).min(1.0);
            }
            if t >= state::SPRING_SECS {
                (self.anim_start, self.fade, self.cur) = (None, false, self.to);
                let _ = unsafe { KillTimer(Some(self.hwnd), T_ANIM) };
            }
        }

        let mut buttons = Vec::new();
        let (w, h) = self.cur;
        let cx = WIN.0 / 2.0;
        let g = &self.gfx;
        g.begin();
        if h > 0.1 {
            let blink = self.ringing() && (self.born.elapsed().as_millis() / 500) % 2 == 1;
            // radius sudut bawah ikut membesar bersama tinggi notch
            let radius = 11.0 + ((h - IDLE.1) / (EXPANDED.1 - IDLE.1)).clamp(0.0, 1.0) * 13.0;
            g.alpha.set(1.0);
            g.notch(cx - w / 2.0, cx + w / 2.0, h, radius, EAR, if blink { 0x8a3a00 } else { 0x000000 });
            g.clip([cx - w / 2.0, 0.0, cx + w / 2.0, h]);
            g.alpha.set(k);
            self.content(cx, &mut buttons);
            g.unclip();
        }
        g.end(self.hwnd);
        self.buttons = buttons;
    }

    /// Isi notch diletakkan menurut ukuran tujuan supaya tidak ikut bergeser selama animasi.
    fn content(&self, cx: f32, buttons: &mut Vec<(R, Action)>) {
        let (g, m) = (&self.gfx, &self.media);
        let (x0, x1, h) = (cx - self.to.0 / 2.0, cx + self.to.0 / 2.0, self.to.1);
        let now = Instant::now();
        let remaining = self.timer.remaining(now).map(state::mmss);
        let battery = power::status();
        let clock = unsafe { GetLocalTime() };

        match self.view {
            View::Idle | View::Compact => {
                if let Some((plugged, pct)) = battery {
                    battery_ring(g, x0 + 19.0, h / 2.0, 7.5, plugged, pct);
                }
                let right = if self.view == View::Compact && remaining.is_some() { 64.0 } else { 40.0 };
                let center = [x0 + 34.0, 0.0, x1 - right, h];
                if self.view == View::Compact {
                    let line = m.lyrics.as_ref().and_then(|l| Some(l[lyrics::active(l, self.lyric_pos())?].1.as_str())).filter(|s| !s.is_empty());
                    let track = if m.artist.is_empty() { m.title.clone() } else { format!("{} \u{2014} {}", m.title, m.artist) };
                    g.text(line.unwrap_or(&track), &g.f.center, center, WHITE);
                } else {
                    let text = remaining.clone().unwrap_or_else(|| format!("{:02}:{:02}", clock.wHour, clock.wMinute));
                    g.text(&text, &g.f.cbold, center, WHITE);
                }
                match (&remaining, self.view) {
                    (Some(rem), View::Compact) => g.text(rem, &g.f.csmall, [x1 - 62.0, 0.0, x1 - 12.0, h], GRAY),
                    (Some(_), _) => g.text("\u{E916}", &g.f.icon_small, [x1 - 30.0, 0.0, x1 - 14.0, h], GRAY),
                    (None, _) if m.session && m.playing => wave(g, x1 - 22.0, h / 2.0),
                    _ => {}
                }
            }
            View::Hud => match self.hud {
                Some(Hud::Volume) => {
                    let v = volume::VOLUME.load(Relaxed);
                    let (muted, pct) = (v & volume::MUTE != 0, (v & !volume::MUTE).min(100));
                    g.text(if muted { "\u{E74F}" } else { "\u{E767}" }, &g.f.icon, [x0 + 14.0, 0.0, x0 + 44.0, h], WHITE);
                    let (b0, b1) = (x0 + 50.0, x1 - 60.0);
                    g.rrect([b0, h / 2.0 - 2.0, b1, h / 2.0 + 2.0], 2.0, 0x3a3a3a);
                    if pct > 0 && !muted {
                        g.rrect([b0, h / 2.0 - 2.0, b0 + (b1 - b0) * pct as f32 / 100.0, h / 2.0 + 2.0], 2.0, WHITE);
                    }
                    g.text(&format!("{pct}%"), &g.f.csmall, [x1 - 56.0, 0.0, x1 - 14.0, h], WHITE);
                }
                Some(Hud::Power(plugged, pct)) => {
                    battery_ring(g, x0 + 22.0, h / 2.0, 8.0, plugged, pct);
                    let label = if plugged { "Mengisi daya" } else { "Memakai baterai" };
                    g.text(&format!("{label} \u{B7} {pct}%"), &g.f.text, [x0 + 40.0, 0.0, x1 - 14.0, h], WHITE);
                }
                Some(Hud::Track) => {
                    let track = if m.artist.is_empty() { m.title.clone() } else { format!("{} \u{2014} {}", m.title, m.artist) };
                    g.text(&track, &g.f.center, [x0 + 34.0, 0.0, x1 - 34.0, h], WHITE);
                    wave(g, x1 - 22.0, h / 2.0);
                }
                None => {}
            },
            View::Expanded => {
                let page = self.current_page();
                g.text(page.title(), &g.f.bold, [x0 + 24.0, 8.0, x0 + 190.0, 34.0], WHITE);
                g.text(&format!("{:02}:{:02}", clock.wHour, clock.wMinute), &g.f.rbold, [x1 - 74.0, 8.0, x1 - 22.0, 34.0], WHITE);
                let date = state::date_id(clock.wDayOfWeek, clock.wDay, clock.wMonth, clock.wYear);
                g.text(&date, &g.f.rsmall, [x1 - 190.0, 8.0, x1 - 80.0, 34.0], GRAY);
                if let Some((plugged, pct)) = battery {
                    battery_ring(g, x1 - 200.0, 21.0, 7.0, plugged, pct);
                }

                page.draw(self, g, [x0 + 32.0, 42.0, x1 - 32.0, 148.0], buttons);

                let n = self.pages.len();
                if n > 1 {
                    for (glyph, x, action) in [("\u{E76B}", x0 + 6.0, Action::PrevPage), ("\u{E76C}", x1 - 28.0, Action::NextPage)] {
                        let btn = [x, 78.0, x + 22.0, 112.0];
                        g.text(glyph, &g.f.icon_small, btn, DIM);
                        buttons.push((btn, action));
                    }
                    let widths: Vec<f32> = (0..n).map(|i| if i == self.page { 16.0 } else { 5.0 }).collect();
                    let total = widths.iter().sum::<f32>() + 5.0 * (n - 1) as f32;
                    let mut x = cx - total / 2.0;
                    for (i, w) in widths.into_iter().enumerate() {
                        g.rrect([x, 158.0, x + w, 163.0], 2.5, if i == self.page { WHITE } else { DIM });
                        buttons.push(([x - 3.0, 152.0, x + w + 3.0, 170.0], Action::Page(i)));
                        x += w + 5.0;
                    }
                }
            }
            View::Hidden => {}
        }
    }

    fn click(&mut self, x: f32, y: f32) {
        // saat timer berbunyi, klik di mana pun pada notch menghentikannya
        if self.ringing() {
            self.timer = Timer::Off;
        } else if let Some(&(_, action)) = self.buttons.iter().find(|(b, _)| x >= b[0] && x < b[2] && y >= b[1] && y < b[3]) {
            match action {
                Action::Media(msg) => drop(self.tx.send(msg())),
                Action::Timer(minutes) => self.timer = Timer::start(minutes, Instant::now()),
                Action::StopTimer => self.timer = Timer::Off,
                Action::Page(i) => self.page = i,
                Action::PrevPage => self.turn(-1),
                Action::NextPage => self.turn(1),
            }
        }
        self.update();
    }

    fn turn(&mut self, by: isize) {
        self.page = (self.page as isize + by).rem_euclid(self.pages.len() as isize) as usize;
    }

    /// Letakkan window di tengah-atas monitor pilihan dan buat ulang permukaan gambar untuk DPI dan ukuran sekarang.
    fn place(hwnd: HWND) -> Result<(Gfx, f32)> {
        unsafe {
            let monitors = monitors();
            let pick = settings::dword("Monitor", 0) as usize;
            // indeks 0 = monitor utama; monitor yang sudah dicabut kembali ke monitor utama
            let (mon, rc) = monitors.get(pick).or(monitors.first()).map_or((HMONITOR::default(), RECT::default()), |m| (m.0, m.1));
            let (mut dpi, mut _y) = (96u32, 0u32);
            let _ = GetDpiForMonitor(mon, MDT_EFFECTIVE_DPI, &mut dpi, &mut _y);
            let size = [0.9, 1.0, 1.15, 1.3][settings::dword("Size", 1).min(3) as usize];
            let dpi = dpi.max(96) as f32 * size;
            let scale = dpi / 96.0;
            let (w, h) = ((WIN.0 * scale).ceil() as i32, (WIN.1 * scale).ceil() as i32);
            let x = rc.left + (rc.right - rc.left - w) / 2;
            SetWindowPos(hwnd, Some(HWND_TOPMOST), x, rc.top, w, h, SWP_NOACTIVATE | SWP_SHOWWINDOW)?;
            Ok((Gfx::new(w, h, dpi)?, scale))
        }
    }

    fn on(&mut self, msg: u32, wp: WPARAM, lp: LPARAM) {
        unsafe {
            match msg {
                WM_MOUSEMOVE if !self.inside => {
                    self.inside = true;
                    let mut track = TRACKMOUSEEVENT { cbSize: size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE, hwndTrack: self.hwnd, dwHoverTime: 0 };
                    let _ = TrackMouseEvent(&mut track);
                    // jeda singkat supaya notch tidak terbuka saat kursor hanya lewat
                    SetTimer(Some(self.hwnd), T_HOVER, HOVER_MS, None);
                    self.update();
                }
                WM_MOUSELEAVE => {
                    (self.inside, self.hover) = (false, false);
                    let _ = KillTimer(Some(self.hwnd), T_HOVER);
                    self.update();
                }
                WM_LBUTTONUP => {
                    let dip = |v: isize| (v & 0xffff) as i16 as f32 / self.scale;
                    self.click(dip(lp.0), dip(lp.0 >> 16));
                }
                WM_MOUSEWHEEL | WM_MOUSEHWHEEL if self.view == View::Expanded => {
                    let delta = (wp.0 >> 16) as i16;
                    // roda vertikal: turun = halaman berikutnya; roda horizontal: kanan = berikutnya
                    let forward = if msg == WM_MOUSEWHEEL { delta < 0 } else { delta > 0 };
                    self.turn(if forward { 1 } else { -1 });
                    self.update();
                }
                WM_TIMER => match wp.0 {
                    T_ANIM => self.draw(),
                    T_TICK => {
                        if self.timer.tick(Instant::now()) {
                            let _ = MessageBeep(MB_ICONASTERISK);
                            self.page = self.pages.iter().position(|p| *p == Page::Timer).unwrap_or(self.page);
                            self.update();
                        } else if self.anim_start.is_none() && self.view != View::Hidden {
                            self.draw();
                        }
                    }
                    T_POLL => {
                        let fullscreen = self.hide_fullscreen && power::fullscreen();
                        let minute = GetLocalTime().wMinute;
                        if fullscreen != self.fullscreen {
                            self.fullscreen = fullscreen;
                            self.update();
                        } else if minute != self.minute && self.view != View::Hidden && self.anim_start.is_none() {
                            self.draw();
                        }
                        self.minute = minute;
                    }
                    T_HOVER => {
                        let _ = KillTimer(Some(self.hwnd), T_HOVER);
                        if self.inside {
                            self.hover = true;
                            self.update();
                        }
                    }
                    T_HUD => {
                        let _ = KillTimer(Some(self.hwnd), T_HUD);
                        self.hud = None;
                        self.update();
                    }
                    _ => {}
                },
                WM_APP_MEDIA => {
                    self.media = media::MEDIA.lock().unwrap().clone().unwrap_or_default();
                    let track = format!("{}\n{}", self.media.title, self.media.artist);
                    if self.media.playing && !self.media.title.is_empty() && track != self.last_track {
                        self.last_track = track;
                        self.show_hud(Hud::Track);
                    } else {
                        self.update();
                    }
                }
                WM_APP_VOLUME => self.show_hud(Hud::Volume),
                WM_POWERBROADCAST if wp.0 as u32 == PBT_APMPOWERSTATUSCHANGE => {
                    // pesan ini juga datang tiap persen berubah; HUD hanya saat charger dicolok atau dicabut
                    match power::status() {
                        Some((plugged, pct)) if self.plugged.replace(plugged) != Some(plugged) => self.show_hud(Hud::Power(plugged, pct)),
                        _ => self.draw(),
                    }
                }
                settings::WM_APP_SETTINGS => {
                    self.reload();
                    let _ = self.tx.send(Msg::Refresh);
                    if let Ok((gfx, scale)) = Self::place(self.hwnd) {
                        (self.gfx, self.scale) = (gfx, scale);
                    }
                    self.update();
                }
                WM_DISPLAYCHANGE | WM_DPICHANGED => {
                    if let Ok((gfx, scale)) = Self::place(self.hwnd) {
                        (self.gfx, self.scale) = (gfx, scale);
                        self.draw();
                    }
                }
                _ => {}
            }
        }
    }
}

/// Cincin baterai: hijau saat dicas, merah saat hampir habis, dengan ikon petir saat dicas.
fn battery_ring(g: &Gfx, cx: f32, cy: f32, rad: f32, plugged: bool, pct: u8) {
    let color = if plugged { GREEN } else if pct <= 20 { 0xff453a } else { WHITE };
    g.ring(cx, cy, rad, 2.2, pct as f32 / 100.0, color, 0x333333);
    if plugged {
        g.text("\u{E945}", &g.f.icon_small, [cx - rad, cy - rad, cx + rad, cy + rad], color);
    }
}

/// Ikon gelombang audio diam (tidak beranimasi, supaya CPU tetap nol saat musik diputar).
fn wave(g: &Gfx, cx: f32, cy: f32) {
    for (i, bar) in [5.0f32, 9.0, 13.0, 9.0, 6.0].into_iter().enumerate() {
        let x = cx - 11.0 + i as f32 * 4.4;
        g.rrect([x, cy - bar / 2.0, x + 2.2, cy + bar / 2.0], 1.1, WHITE);
    }
}

/// Monitor yang tersambung: monitor utama selalu di urutan pertama.
fn monitors() -> Vec<(HMONITOR, RECT, bool)> {
    extern "system" fn each(mon: HMONITOR, _: HDC, _: *mut RECT, data: LPARAM) -> BOOL {
        let list = unsafe { &mut *(data.0 as *mut Vec<(HMONITOR, RECT, bool)>) };
        let mut info = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
        if unsafe { GetMonitorInfoW(mon, &mut info) }.as_bool() {
            list.push((mon, info.rcMonitor, info.dwFlags & MONITORINFOF_PRIMARY != 0));
        }
        TRUE
    }
    let mut list: Vec<(HMONITOR, RECT, bool)> = Vec::new();
    let _ = unsafe { EnumDisplayMonitors(None, None, Some(each), LPARAM(&mut list as *mut _ as isize)) };
    list.sort_by_key(|m| (!m.2, m.1.left, m.1.top));
    list
}

/// Isi jendela Pengaturan. Tiap fitur menambahkan bagiannya di sini.
fn sections() -> Vec<Section> {
    let mut page_items: Vec<Item> = pages::ALL.iter().map(|p| Item::Check(p.key(), p.title(), true)).collect();
    page_items.insert(0, Item::Note("Halaman yang tampil saat notch dibuka. Gulir roda mouse atau klik panah untuk berpindah."));
    vec![
        Section {
            name: "Umum",
            items: vec![
                Item::Check(settings::AUTOSTART, "Jalankan saat Windows menyala", false),
                Item::Check("HideFullscreen", "Sembunyikan saat ada app layar penuh (game, video, presentasi)", true),
                Item::Check("HideNotch", "Sembunyikan notch", false),
            ],
        },
        Section {
            name: "Tampilan",
            items: vec![
                Item::Choice("Size", "Ukuran notch", || opts(&["Kecil", "Sedang", "Besar", "Sangat besar"]), 1),
                Item::Choice(
                    "Monitor",
                    "Layar",
                    || monitors().iter().enumerate().map(|(i, m)| format!("Layar {}{}", i + 1, if m.2 { " (utama)" } else { "" })).collect(),
                    0,
                ),
            ],
        },
        Section {
            name: "Musik",
            items: vec![
                Item::Check("ShowLyrics", "Tampilkan lirik", true),
                Item::Note("Lirik diambil dari LRCLIB (lrclib.net). Judul dan artis lagu dikirim ke sana."),
                Item::Choice("LyricOffset", "Majukan lirik", || opts(&["Mati", "0,3 detik", "0,6 detik", "1 detik"]), 0),
            ],
        },
        Section { name: "Halaman", items: page_items },
    ]
}

fn tray_menu(hwnd: HWND) {
    let hidden = settings::flag("HideNotch", false);
    // menu menjalankan message loop sendiri, jadi APP tidak boleh sedang dipinjam di sini
    match tray::menu(hwnd, hidden) {
        tray::CMD_SETTINGS => settings::open(),
        tray::CMD_HIDE => {
            settings::set_dword("HideNotch", !hidden as u32);
            let _ = unsafe { PostMessageW(Some(hwnd), settings::WM_APP_SETTINGS, WPARAM(0), LPARAM(0)) };
        }
        tray::CMD_QUIT => drop(unsafe { PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0)) }),
        _ => {}
    }
}

extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    thread_local! {
        // Explorer mengirim pesan ini setelah restart; ikon tray harus didaftarkan ulang
        static TASKBAR_CREATED: u32 = unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) };
    }
    unsafe {
        match msg {
            WM_DESTROY => {
                tray::remove(hwnd);
                PostQuitMessage(0);
                return LRESULT(0);
            }
            // klik pada notch tidak boleh mengambil fokus dari app yang sedang dipakai
            WM_MOUSEACTIVATE => return LRESULT(MA_NOACTIVATE as isize),
            WM_APP_OPEN_SETTINGS => {
                settings::open();
                return LRESULT(0);
            }
            WM_APP_TRAY => {
                match lp.0 as u32 {
                    WM_LBUTTONDBLCLK => settings::open(),
                    WM_LBUTTONUP | WM_RBUTTONUP => tray_menu(hwnd),
                    _ => {}
                }
                return LRESULT(0);
            }
            _ if msg == TASKBAR_CREATED.with(|m| *m) => tray::add(hwnd),
            _ => {}
        }
        APP.with(|app| {
            if let Ok(mut app) = app.try_borrow_mut()
                && let Some(app) = app.as_mut()
            {
                app.on(msg, wp, lp);
            }
        });
        DefWindowProcW(hwnd, msg, wp, lp)
    }
}

fn main() -> Result<()> {
    unsafe {
        // satu instance saja; autostart dan klik manual bisa bertabrakan
        let _instance = CreateMutexW(None, true, w!("Ponian.SingleInstance"))?;
        let open_settings = std::env::args().any(|a| a == "--settings");
        if GetLastError() == ERROR_ALREADY_EXISTS {
            // instance kedua dengan --settings meminta instance yang berjalan membuka Pengaturan
            if open_settings && let Ok(running) = FindWindowW(w!("Ponian"), None) {
                let _ = PostMessageW(Some(running), WM_APP_OPEN_SETTINGS, WPARAM(0), LPARAM(0));
            }
            return Ok(());
        }
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);

        let instance: HINSTANCE = GetModuleHandleW(None)?.into();
        let class = w!("Ponian");
        RegisterClassW(&WNDCLASSW { lpfnWndProc: Some(wndproc), hInstance: instance, lpszClassName: class, hCursor: LoadCursorW(None, IDC_ARROW)?, ..Default::default() });
        let hwnd = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            class,
            class,
            WS_POPUP,
            0,
            0,
            1,
            1,
            None,
            None,
            Some(instance),
            None,
        )?;
        let (gfx, scale) = App::place(hwnd)?;
        settings::NOTCH.store(hwnd.0 as isize, Relaxed);
        let _ = settings::SECTIONS.set(sections);
        tray::add(hwnd);
        SetTimer(Some(hwnd), T_POLL, 1000, None);

        let mut app = App {
            hwnd,
            gfx,
            scale,
            tx: media::spawn(hwnd.0 as isize),
            media: Media::default(),
            last_track: String::new(),
            timer: Timer::Off,
            hud: None,
            plugged: power::status().map(|s| s.0),
            inside: false,
            hover: false,
            user_hidden: false,
            fullscreen: false,
            ticking: false,
            pages: vec![Page::Music],
            page: 0,
            lyric_offset_ms: 0,
            hide_fullscreen: true,
            minute: GetLocalTime().wMinute,
            view: View::Idle,
            cur: (IDLE.0, 0.0),
            from: (IDLE.0, 0.0),
            to: (IDLE.0, 0.0),
            fade: false,
            anim_start: None,
            born: Instant::now(),
            buttons: Vec::new(),
        };
        // mulai dari tinggi 0 supaya notch "turun" dari tepi layar saat app dibuka
        app.view = View::Hidden;
        app.reload();
        app.update();
        APP.set(Some(app));
        if open_settings {
            settings::open();
        }

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            DispatchMessageW(&msg);
        }
    }
    Ok(())
}
