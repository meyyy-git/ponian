#![windows_subsystem = "windows"]
//! Ponian: notch untuk Windows.

mod ai;
mod astro;
mod calendar;
mod clock;
mod currency;
mod features;
mod input;
mod location;
mod lyrics;
mod media;
mod net;
mod notify;
mod ocr;
mod online;
mod pages;
mod pomodoro;
mod power;
mod prayer;
mod render;
mod settings;
mod shelf;
mod state;
mod store;
mod system;
mod todo;
mod tray;
mod volume;
mod water;
mod weather;
mod worldclock;

use features::WaterCfg;
use media::{Media, Msg};
use pages::Page;
use pomodoro::{Durations, Pomo, Run};
use render::{ACCENT, CHIP, DIM, GRAY, GREEN, Gfx, R, WHITE};
use settings::{Item, Section, opts};
use state::{Inputs, View};
use std::{
    cell::RefCell,
    sync::{atomic::Ordering::Relaxed, mpsc::Sender},
    time::Instant,
};
use windows::{
    Win32::{
        Foundation::*,
        System::{LibraryLoader::GetModuleHandleW, SystemInformation::GetLocalTime, Threading::CreateMutexW},
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
pub const WM_APP_DRAG: u32 = WM_APP + 6;
pub const WM_APP_CAPS: u32 = WM_APP + 7;
pub const WM_APP_NOTIFY: u32 = WM_APP + 8;
pub const WM_APP_NET: u32 = WM_APP + 9;
pub const WM_APP_AUDIODEV: u32 = WM_APP + 11;
pub const WM_APP_ONLINE: u32 = WM_APP + 12;
pub const WM_APP_AI: u32 = WM_APP + 13;
pub const WM_APP_OCR: u32 = WM_APP + 14;
const WM_CLIPBOARDUPDATE: u32 = 0x031D;
const PBT_POWERSETTINGCHANGE: u32 = 0x8013;
const WM_APP_OPEN_SETTINGS: u32 = WM_APP + 5;
const WM_MOUSELEAVE: u32 = 0x02A3;
const WM_MOUSEHWHEEL: u32 = 0x020E;

/// Ukuran window dalam DIP. Notch digambar di dalamnya, sisanya transparan dan tembus klik.
const WIN: (f32, f32) = (440.0, 320.0);
const IDLE: (f32, f32) = (152.0, 30.0);
const EXPANDED: (f32, f32) = (405.0, 172.0);
/// Halaman Tanya lebih tinggi supaya jawaban AI muat.
const ASK_HEIGHT: f32 = 300.0;
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

#[derive(Clone, Copy, PartialEq)]
enum Hud {
    Volume,
    Power(bool, u8),
    Track,
    Alert,
    /// popup singkat yang memakai data `alert`
    Info,
    Brightness(u32),
    Caps(bool),
    Lang(isize),
}

/// Peringatan yang tetap tampil sampai diklik (pengingat tugas, minum air, waktu salat).
pub struct Alert {
    icon: &'static str,
    text: String,
    /// judul tugas, kalau peringatan ini bisa ditunda
    task: Option<String>,
}

#[derive(Clone, Copy)]
pub enum Action {
    Media(fn() -> Msg),
    Page(usize),
    PrevPage,
    NextPage,
    PomoToggle,
    PomoReset,
    TodoAdd,
    TodoToggle(usize),
    TodoDelete(usize),
    WaterAdd,
    WaterUndo,
    Snooze,
    ShelfOpen(usize),
    ShelfRemove(usize),
    ClipCopy(usize),
    /// buka kotak Tanya; true = mode terjemah
    AskOpen(bool),
    AskKey,
    AskCopy,
    AskClear,
}

pub struct App {
    hwnd: HWND,
    gfx: Gfx,
    scale: f32,
    tx: Sender<Msg>,
    media: Media,
    last_track: String,
    pomo: Pomo,
    durations: Durations,
    tasks: Vec<todo::Task>,
    today: i64,
    water: u32,
    water_cfg: WaterCfg,
    /// menit terakhir pengingat minum air hari ini
    water_reminded: u32,
    /// menit sejak tengah malam untuk tiap waktu salat hari ini
    prayer: Option<[u32; 6]>,
    prayer_info: String,
    prayer_done: String,
    alert: Option<Alert>,
    widgets: (usize, usize),
    cities: (usize, usize),
    sound: bool,
    files: Vec<String>,
    clips: Vec<String>,
    /// file sedang diseret di atas notch
    dragging: bool,
    /// posisi tombol mouse ditekan di atas file Shelf, untuk mulai menyeret keluar
    press: Option<(f32, f32, usize)>,
    brightness_ready: bool,
    layout: (isize, isize),
    battery_band: u8,
    net: Option<String>,
    notices: Vec<(notify::Notice, Instant)>,
    notify_preview: bool,
    ask: features::AskState,
    has_key: bool,
    /// notch tetap terbuka sampai waktu ini walau kursor tidak di atasnya (setelah bertanya lewat shortcut)
    pin: Option<Instant>,
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
        self.pomo.run == Run::Ringing
    }

    /// Sisa waktu Pomodoro untuk pill kecil, selama sedang berjalan atau dijeda.
    fn pomo_remaining(&self) -> Option<String> {
        matches!(self.pomo.run, Run::Running(_) | Run::Paused(_)).then(|| state::mmss(self.pomo.remaining(Instant::now(), self.durations)))
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
        self.reload_features();
    }

    fn target(&self, view: View) -> (f32, f32) {
        let m = &self.media;
        let peek = if self.inside { PEEK } else { (0.0, 0.0) };
        match view {
            View::Hidden => (IDLE.0, 0.0),
            View::Idle => (IDLE.0 + peek.0, IDLE.1 + peek.1),
            View::Compact => {
                let w = if m.lyrics.is_some() { 360.0 } else { 280.0 } + if self.pomo_remaining().is_some() { 54.0 } else { 0.0 };
                (w + peek.0, IDLE.1 + peek.1)
            }
            View::Hud if matches!(self.hud, Some(Hud::Alert | Hud::Info)) => (380.0, 38.0),
            View::Hud if matches!(self.hud, Some(Hud::Caps(_) | Hud::Lang(_))) => (220.0, 34.0),
            View::Hud => (300.0, 34.0),
            View::Expanded if self.current_page() == Page::Ask => (EXPANDED.0, ASK_HEIGHT),
            View::Expanded => EXPANDED,
        }
    }

    /// Hitung ulang bentuk notch dari keadaan sekarang, lalu gambar.
    fn update(&mut self) {
        let playing = self.media.session && self.media.playing;
        let view = state::view(Inputs {
            hidden: self.user_hidden || self.fullscreen,
            // kotak teks yang terbuka menahan notch tetap terbuka
            hover: self.hover || input::is_open() || self.dragging || self.pin.is_some_and(|t| Instant::now() < t),
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
            let need_tick = self.pomo.running() || self.ringing() || (playing && self.media.lyrics.is_some());
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
        // HUD biasa tidak boleh menimpa peringatan yang belum dibaca
        if self.hud == Some(Hud::Alert) && hud != Hud::Alert {
            return;
        }
        self.show_hud_for(hud, HUD_MS);
    }

    fn show_hud_for(&mut self, hud: Hud, ms: u32) {
        self.hud = Some(hud);
        unsafe { SetTimer(Some(self.hwnd), T_HUD, ms, None) };
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
        let remaining = self.pomo_remaining();
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
                Some(Hud::Brightness(pct)) => {
                    g.text("\u{E706}", &g.f.icon, [x0 + 14.0, 0.0, x0 + 44.0, h], WHITE);
                    let (b0, b1) = (x0 + 50.0, x1 - 60.0);
                    g.rrect([b0, h / 2.0 - 2.0, b1, h / 2.0 + 2.0], 2.0, 0x3a3a3a);
                    g.rrect([b0, h / 2.0 - 2.0, b0 + (b1 - b0) * pct.min(100) as f32 / 100.0, h / 2.0 + 2.0], 2.0, WHITE);
                    g.text(&format!("{pct}%"), &g.f.csmall, [x1 - 56.0, 0.0, x1 - 14.0, h], WHITE);
                }
                Some(Hud::Caps(on)) => {
                    g.text("\u{E84A}", &g.f.icon, [x0 + 12.0, 0.0, x0 + 40.0, h], if on { ACCENT } else { GRAY });
                    g.text(if on { "Caps Lock nyala" } else { "Caps Lock mati" }, &g.f.text, [x0 + 44.0, 0.0, x1 - 12.0, h], WHITE);
                }
                Some(Hud::Lang(layout)) => {
                    g.text("\u{E8C1}", &g.f.icon, [x0 + 12.0, 0.0, x0 + 40.0, h], ACCENT);
                    g.text(&system::layout_name(layout), &g.f.text, [x0 + 44.0, 0.0, x1 - 12.0, h], WHITE);
                }
                Some(Hud::Info) => {
                    let Some(a) = &self.alert else { return };
                    g.text(a.icon, &g.f.icon, [x0 + 12.0, 0.0, x0 + 40.0, h], ACCENT);
                    g.text(&a.text, &g.f.text, [x0 + 44.0, 0.0, x1 - 16.0, h], WHITE);
                }
                Some(Hud::Alert) => {
                    let Some(a) = &self.alert else { return };
                    g.text(a.icon, &g.f.icon, [x0 + 12.0, 0.0, x0 + 40.0, h], ACCENT);
                    let right = if a.task.is_some() { 100.0 } else { 16.0 };
                    g.text(&a.text, &g.f.text, [x0 + 44.0, 0.0, x1 - right, h], WHITE);
                    if a.task.is_some() {
                        let snooze = features::SNOOZE[(settings::dword("Todo.Snooze", 1) as usize).min(2)];
                        pages::chip(g, [x1 - 94.0, 7.0, x1 - 12.0, h - 7.0], &format!("Tunda {snooze} mnt"), CHIP, buttons, Action::Snooze);
                    }
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

                page.draw(self, g, [x0 + 32.0, 42.0, x1 - 32.0, h - 24.0], buttons);

                let n = self.pages.len();
                if n > 1 {
                    for (glyph, x, action) in [("\u{E76B}", x0 + 6.0, Action::PrevPage), ("\u{E76C}", x1 - 28.0, Action::NextPage)] {
                        let btn = [x, h / 2.0 - 8.0, x + 22.0, h / 2.0 + 26.0];
                        g.text(glyph, &g.f.icon_small, btn, DIM);
                        buttons.push((btn, action));
                    }
                    let widths: Vec<f32> = (0..n).map(|i| if i == self.page { 16.0 } else { 5.0 }).collect();
                    let total = widths.iter().sum::<f32>() + 5.0 * (n - 1) as f32;
                    let mut x = cx - total / 2.0;
                    for (i, w) in widths.into_iter().enumerate() {
                        g.rrect([x, h - 14.0, x + w, h - 9.0], 2.5, if i == self.page { WHITE } else { DIM });
                        buttons.push(([x - 3.0, h - 20.0, x + w + 3.0, h - 2.0], Action::Page(i)));
                        x += w + 5.0;
                    }
                }
            }
            View::Hidden => {}
        }
    }

    fn click(&mut self, x: f32, y: f32) {
        let hit = self.buttons.iter().find(|(b, _)| x >= b[0] && x < b[2] && y >= b[1] && y < b[3]).map(|b| b.1);
        // saat Pomodoro berbunyi, klik di mana pun pada notch pindah ke fase berikutnya
        if self.ringing() {
            self.pomo.toggle(Instant::now(), self.durations);
        } else if self.view == View::Hud && matches!(self.hud, Some(Hud::Alert | Hud::Info)) && !matches!(hit, Some(Action::Snooze)) {
            // klik pada peringatan berarti sudah dibaca
            (self.alert, self.hud) = (None, None);
        } else if let Some(action) = hit {
            self.act(action);
        }
        self.update();
    }

    /// Daftarkan shortcut dan beri tahu kalau ada yang bentrok dengan app lain.
    fn hotkeys(&mut self) {
        let failed = register_hotkeys(self.hwnd);
        if !failed.is_empty() {
            self.info("\u{E765}", format!("{} dipakai app lain \u{B7} ganti di Pengaturan", failed.join(" & ")), 6000);
        }
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
                    // begitu user sudah melihat dan menggeser kursor keluar, notch boleh menutup
                    if self.hover {
                        self.pin = None;
                    }
                    (self.inside, self.hover) = (false, false);
                    let _ = KillTimer(Some(self.hwnd), T_HOVER);
                    self.update();
                }
                // menahan tombol di atas file Shelf lalu bergerak = menyeret file keluar
                WM_MOUSEMOVE if (wp.0 & 1) != 0 => {
                    let dip = |v: isize| (v & 0xffff) as i16 as f32 / self.scale;
                    let (x, y) = (dip(lp.0), dip(lp.0 >> 16));
                    if let Some((px, py, i)) = self.press
                        && ((x - px).powi(2) + (y - py).powi(2)) > 36.0
                    {
                        self.press = None;
                        if let Some(path) = self.files.get(i).cloned() {
                            shelf::drag_out(&path);
                        }
                    }
                }
                WM_LBUTTONDOWN => {
                    let dip = |v: isize| (v & 0xffff) as i16 as f32 / self.scale;
                    let (x, y) = (dip(lp.0), dip(lp.0 >> 16));
                    self.press = self.buttons.iter().find(|(b, _)| x >= b[0] && x < b[2] && y >= b[1] && y < b[3]).and_then(|b| match b.1 {
                        Action::ShelfOpen(i) => Some((x, y, i)),
                        _ => None,
                    });
                }
                // setelah menyeret, DoDragDrop sudah memakan pelepasan tombol, jadi pesan ini hanya datang untuk klik biasa
                WM_LBUTTONUP => {
                    let dip = |v: isize| (v & 0xffff) as i16 as f32 / self.scale;
                    self.press = None;
                    self.click(dip(lp.0), dip(lp.0 >> 16));
                }
                WM_CLIPBOARDUPDATE => self.on_clipboard(),
                WM_APP_DRAG => {
                    self.on_drag(wp.0);
                    self.update();
                }
                WM_APP_CAPS if settings::flag("Hud.Caps", true) => self.show_hud(Hud::Caps(system::caps_on())),
                WM_APP_NOTIFY => self.on_notify(),
                WM_APP_NET => self.on_net(),
                WM_APP_AUDIODEV => self.on_audio_device(),
                WM_APP_ONLINE if self.view == View::Expanded => self.draw(),
                WM_APP_AI => {
                    self.on_ai();
                    self.pin = Some(Instant::now() + std::time::Duration::from_secs(12));
                    self.update();
                }
                WM_APP_OCR => self.on_ocr(),
                WM_HOTKEY => match wp.0 {
                    HOTKEY_ASK => self.open_ask_input(features::INPUT_ASK),
                    HOTKEY_OCR => ocr::start(self.hwnd),
                    _ => {}
                },
                WM_POWERBROADCAST if wp.0 as u32 == PBT_POWERSETTINGCHANGE => {
                    // pemberitahuan pertama datang saat didaftarkan, bukan karena user mengubah kecerahan
                    if let Some(pct) = system::brightness(lp) {
                        if self.brightness_ready && settings::flag("Hud.Brightness", true) {
                            self.show_hud(Hud::Brightness(pct));
                        }
                        self.brightness_ready = true;
                    }
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
                        if self.pomo_tick() {
                            self.update();
                        } else if self.anim_start.is_none() && self.view != View::Hidden {
                            self.draw();
                        }
                    }
                    T_POLL => {
                        let fullscreen = self.hide_fullscreen && power::fullscreen();
                        let minute = GetLocalTime().wMinute;
                        let new_minute = minute != self.minute;
                        self.minute = minute;
                        self.check_layout();
                        if self.pin.is_some_and(|t| Instant::now() >= t) {
                            self.pin = None;
                            self.update();
                        }
                        if new_minute {
                            self.minute_jobs();
                        }
                        if fullscreen != self.fullscreen {
                            self.fullscreen = fullscreen;
                            self.update();
                        } else if new_minute && self.view != View::Hidden && self.anim_start.is_none() {
                            self.draw();
                        }
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
                        (self.hud, self.alert) = (None, None);
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
                WM_APP_VOLUME if settings::flag("Hud.Volume", true) => self.show_hud(Hud::Volume),
                WM_POWERBROADCAST if wp.0 as u32 == PBT_APMPOWERSTATUSCHANGE => {
                    // pesan ini juga datang tiap persen berubah; HUD hanya saat charger dicolok atau dicabut
                    match power::status() {
                        Some((plugged, pct)) if self.plugged.replace(plugged) != Some(plugged) => self.show_hud(Hud::Power(plugged, pct)),
                        Some((plugged, pct)) => {
                            self.on_battery(plugged, pct);
                            self.draw();
                        }
                        None => self.draw(),
                    }
                }
                input::WM_APP_INPUT => {
                    self.on_input(wp.0, lp.0);
                    // pertanyaan dari shortcut: biarkan notch terbuka supaya jawabannya terlihat
                    if wp.0 != features::INPUT_TODO && lp.0 != input::CANCEL {
                        self.pin = Some(Instant::now() + std::time::Duration::from_secs(if self.ask.thinking { 30 } else { 10 }));
                    }
                    // kursor mungkin sudah di luar notch saat kotak teks ditutup
                    let mut p = POINT::default();
                    let mut wr = RECT::default();
                    let _ = (GetCursorPos(&mut p), GetWindowRect(self.hwnd, &mut wr));
                    if !(p.x >= wr.left && p.x < wr.right && p.y >= wr.top && p.y < wr.bottom) {
                        (self.inside, self.hover) = (false, false);
                    }
                    self.update();
                }
                settings::WM_APP_SETTINGS => {
                    self.reload();
                    self.hotkeys();
                    online::FORCE.store(true, Relaxed);
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

const HOTKEY_ASK: usize = 1;
const HOTKEY_OCR: usize = 2;

const ASK_KEYS: [&str; 4] = ["Ctrl+Alt+Space", "Ctrl+Alt+A", "Ctrl+Shift+Space", "Alt+Shift+Space"];

/// Daftarkan shortcut Tanya (pilihan user) dan Ctrl+Alt+T untuk menyalin teks dari layar.
/// Mengembalikan nama shortcut yang gagal karena sudah dipakai app lain.
fn register_hotkeys(hwnd: HWND) -> Vec<&'static str> {
    use windows::Win32::UI::Input::KeyboardAndMouse::{MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, RegisterHotKey, UnregisterHotKey, VK_SPACE};
    let mut failed = Vec::new();
    unsafe {
        let _ = UnregisterHotKey(Some(hwnd), HOTKEY_ASK as i32);
        let _ = UnregisterHotKey(Some(hwnd), HOTKEY_OCR as i32);
        let pick = (settings::dword("Ask.Key", 0) as usize).min(ASK_KEYS.len() - 1);
        let (mods, vk) = [
            (MOD_CONTROL | MOD_ALT, VK_SPACE.0 as u32),
            (MOD_CONTROL | MOD_ALT, 'A' as u32),
            (MOD_CONTROL | MOD_SHIFT, VK_SPACE.0 as u32),
            (MOD_ALT | MOD_SHIFT, VK_SPACE.0 as u32),
        ][pick];
        if settings::flag("Ask.On", true) && RegisterHotKey(Some(hwnd), HOTKEY_ASK as i32, mods | MOD_NOREPEAT, vk).is_err() {
            failed.push(ASK_KEYS[pick]);
        }
        if settings::flag("Ocr.On", true) && RegisterHotKey(Some(hwnd), HOTKEY_OCR as i32, MOD_CONTROL | MOD_ALT | MOD_NOREPEAT, 'T' as u32).is_err() {
            failed.push("Ctrl+Alt+T");
        }
    }
    failed
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
        Section {
            name: "Pomodoro",
            items: vec![
                Item::Choice("Pomo.Focus", "Fokus", || opts(&["15 menit", "20 menit", "25 menit", "30 menit", "45 menit", "50 menit", "60 menit"]), 2),
                Item::Choice("Pomo.Short", "Istirahat pendek", || opts(&["3 menit", "5 menit", "10 menit"]), 1),
                Item::Choice("Pomo.Long", "Istirahat panjang", || opts(&["10 menit", "15 menit", "20 menit", "30 menit"]), 1),
                Item::Check("Sound", "Bunyi untuk Pomodoro dan semua pengingat", true),
            ],
        },
        Section {
            name: "To-do",
            items: vec![
                Item::Note("Tulis jam di awal atau akhir tugas untuk pengingat, misalnya \u{201C}Rapat 14:30\u{201D}."),
                Item::Check("Todo.Remind", "Ingatkan saat jam tugas tiba", true),
                Item::Choice("Todo.Snooze", "Tunda pengingat selama", || opts(&["5 menit", "10 menit", "15 menit"]), 1),
            ],
        },
        Section {
            name: "Minum air",
            items: vec![
                Item::Check("Water.On", "Ingatkan minum air, dibagi rata sepanjang jam aktif", true),
                Item::Choice("Water.Goal", "Target harian", || opts(&["1.500 ml", "2.000 ml", "2.500 ml", "3.000 ml", "3.500 ml"]), 1),
                Item::Choice("Water.Glass", "Ukuran gelas", || opts(&["200 ml", "250 ml", "330 ml", "500 ml"]), 1),
                Item::Choice("Water.From", "Aktif mulai", || (5..=12).map(clock_label).collect(), 3),
                Item::Choice("Water.Until", "Aktif sampai", || (17..=24).map(clock_label).collect(), 4),
            ],
        },
        Section {
            name: "Salat",
            items: vec![
                Item::Choice("Prayer.Method", "Metode perhitungan", || prayer::METHODS.iter().map(|m| m.name.to_string()).collect(), 0),
                Item::Check("Prayer.Hanafi", "Asar mazhab Hanafi (bayangan dua kali panjang benda)", false),
                Item::Check("UseWindowsLocation", "Pakai lokasi Windows (perlu izin lokasi di Pengaturan Windows)", true),
                Item::Text("Location", "Atau tulis koordinat sendiri: lintang, bujur (contoh: -6.2088, 106.8456)", false),
                Item::Choice("Prayer.Notify", "Pengingat", || opts(&["Mati", "Saat waktu salat", "10 menit sebelumnya"]), 1),
                Item::Check("Prayer.Pause", "Jeda musik saat waktu salat", false),
                Item::Note("Jadwal dihitung di komputer ini dari posisi matahari, tanpa internet."),
            ],
        },
        Section {
            name: "Sistem",
            items: vec![
                Item::Note("Indikator yang muncul di notch:"),
                Item::Check("Hud.Volume", "Volume", true),
                Item::Check("Hud.Brightness", "Kecerahan layar", true),
                Item::Check("Hud.Caps", "Caps Lock", true),
                Item::Check("Hud.Lang", "Bahasa input", true),
                Item::Note("Popup:"),
                Item::Check("Popup.Battery", "Baterai lemah, kritis, dan penuh", true),
                Item::Check("Popup.Wifi", "Wi-Fi tersambung atau terputus", true),
                Item::Check("Popup.Audio", "Perangkat audio berganti (headset Bluetooth, speaker)", true),
            ],
        },
        Section {
            name: "Notifikasi",
            items: vec![
                Item::Check("Notify.On", "Tampilkan notifikasi app di notch", true),
                Item::Check("Notify.Preview", "Tampilkan isi pesan", false),
                Item::Note("Kalau isi pesan dimatikan, notch hanya menampilkan nama app. Notifikasi dibaca di komputer ini dan tidak dikirim ke mana pun."),
            ],
        },
        Section {
            name: "Shelf",
            items: vec![
                Item::Check("Shelf.Clear", "Kosongkan Shelf setiap Ponian dibuka", true),
                Item::Check("Clip.On", "Simpan riwayat clipboard (teks yang Anda salin)", true),
                Item::Choice("Clip.Limit", "Jumlah riwayat clipboard", || opts(&["5", "10", "20", "50"]), 1),
                Item::Check("Clip.Keep", "Ingat riwayat clipboard setelah Ponian ditutup", false),
                Item::Note("Teks dari password manager yang menandai isinya rahasia tidak ikut disimpan."),
            ],
        },
        Section {
            name: "AI & OCR",
            items: vec![
                Item::Check("Ask.On", "Shortcut untuk membuka Tanya dari mana saja", true),
                Item::Choice("Ask.Key", "Shortcut Tanya", || opts(&ASK_KEYS), 0),
                Item::Text("Gemini", "API key Gemini (gratis di aistudio.google.com)", true),
                Item::Note("Pertanyaan, teks terjemahan, dan teks dari layar yang Anda tanyakan dikirim ke Google Gemini. Dengan key gratis, Google bisa memakainya untuk meningkatkan layanan. Hitung cepat dan kurs dihitung di komputer ini."),
                Item::Check("Ocr.On", "Salin teks dari layar (OCR) dengan Ctrl+Alt+T: pilih area, teksnya disalin", true),
                Item::Note("OCR memakai mesin bawaan Windows dan berjalan offline."),
            ],
        },
        Section {
            name: "Internet",
            items: vec![
                Item::Check("Weather.On", "Cuaca (data MET Norway, memakai lokasi dari bagian Salat)", true),
                Item::Check("Rates.On", "Kurs mata uang (data Bank Sentral Eropa lewat Frankfurter)", true),
                Item::Choice("Rates.Main", "Mata uang utama", || currency::MAIN.iter().map(|c| c.to_string()).collect(), 0),
                Item::Text("Calendar", "Link iCal rahasia (Google Calendar: Setelan \u{203A} Alamat rahasia dalam format iCal)", true),
                Item::Note("Link kalender disimpan terenkripsi. Acara hari ini tampil di halaman To-do."),
            ],
        },
        Section {
            name: "Widget",
            items: vec![
                Item::Choice("Widget.Left", "Widget kiri", pages::widgets::names, 0),
                Item::Choice("Widget.Right", "Widget kanan", pages::widgets::names, 1),
                Item::Choice("Clock.City1", "Jam dunia, kota pertama", worldclock::names, 5),
                Item::Choice("Clock.City2", "Jam dunia, kota kedua", worldclock::names, 10),
                Item::Text("Countdown", "Hitung mundur: tanggal dan nama acara (contoh: 2027-01-17 Ulang tahun). Kosongkan untuk hari besar terdekat.", false),
            ],
        },
    ]
}

fn clock_label(h: u32) -> String {
    clock::hhmm(h * 60)
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
            pomo: Pomo::default(),
            durations: Durations { focus: 25, short: 5, long: 15 },
            tasks: Vec::new(),
            today: 0,
            water: 0,
            water_cfg: WaterCfg::default(),
            water_reminded: 0,
            prayer: None,
            prayer_info: String::new(),
            prayer_done: String::new(),
            alert: None,
            widgets: (0, 1),
            cities: (5, 10),
            sound: true,
            files: Vec::new(),
            clips: Vec::new(),
            dragging: false,
            press: None,
            brightness_ready: false,
            layout: (0, 0),
            battery_band: 0,
            net: None,
            notices: Vec::new(),
            notify_preview: false,
            ask: features::AskState::default(),
            has_key: false,
            pin: None,
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
        app.load_data();
        app.reload();
        app.minute_jobs();
        app.hotkeys();
        location::refresh(hwnd.0 as isize);
        shelf::register(hwnd);
        system::register(hwnd);
        notify::spawn(hwnd.0 as isize);
        online::spawn(hwnd.0 as isize);
        app.update();
        APP.set(Some(app));
        if open_settings {
            settings::open();
        }

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            // TranslateMessage mengubah tombol menjadi karakter (WM_CHAR) untuk kotak teks
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    Ok(())
}
