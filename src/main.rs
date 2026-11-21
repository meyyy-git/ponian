#![windows_subsystem = "windows"]
//! Ponian: notch untuk Windows.

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
use render::{ACCENT, CHIP, DIM, GRAY, GREEN, Gfx, R, TRACK, WHITE};
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
pub const WM_APP_OCR: u32 = WM_APP + 14;
const WM_CLIPBOARDUPDATE: u32 = 0x031D;
const PBT_POWERSETTINGCHANGE: u32 = 0x8013;
pub const WM_APP_OPEN_SETTINGS: u32 = WM_APP + 5;
const WM_MOUSELEAVE: u32 = 0x02A3;
const WM_MOUSEHWHEEL: u32 = 0x020E;

/// Ukuran window dalam DIP. Notch digambar di dalamnya, sisanya transparan dan tembus klik.
const WIN: (f32, f32) = (440.0, 190.0);
const IDLE: (f32, f32) = (152.0, 30.0);
const EXPANDED: (f32, f32) = (396.0, 171.0);
/// Notch sedikit membesar begitu kursor masuk, sebelum jeda hover selesai.
const PEEK: (f32, f32) = (8.0, 3.0);
/// Pill lirik saat musik berjalan.
const LYRIC: (f32, f32) = (300.0, 34.0);
const HUD_MS: u32 = 3000;
const HOVER_MS: u32 = 240;
const FADE_SECS: f32 = 0.22;

const T_ANIM: usize = 1;
const T_TICK: usize = 2;
const T_POLL: usize = 3;
const T_HOVER: usize = 4;
const T_HUD: usize = 5;
/// menyala tepat di awal baris lirik berikutnya
const T_LYRIC: usize = 6;
/// lama transisi antarbaris lirik
const LYRIC_SECS: f32 = 0.42;

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
    /// klik pada batang progres; x0..x1 adalah rentang batangnya
    Seek { x0: f32, x1: f32 },
    Mute,
    LyricsToggle,
    WaterToggle,
    OpenSettings,
    ShelfTab(u8),
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
    /// tab halaman Shelf: 0 = File, 1 = Clipboard
    shelf_tab: u8,
    /// baris lirik yang tampil, baris sebelumnya, dan kapan pergantiannya dimulai
    lyric_idx: Option<usize>,
    lyric_cur: String,
    lyric_prev: String,
    lyric_since: Option<Instant>,
    brightness_ready: bool,
    layout: (isize, isize),
    battery_band: u8,
    net: Option<String>,
    notices: Vec<(notify::Notice, Instant)>,
    notify_preview: bool,
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
        let peek = if self.inside { PEEK } else { (0.0, 0.0) };
        match view {
            View::Hidden => (IDLE.0, 0.0),
            View::Idle => (IDLE.0 + peek.0, IDLE.1 + peek.1),
            View::Compact => (LYRIC.0 + peek.0, LYRIC.1 + peek.1),
            View::Hud if matches!(self.hud, Some(Hud::Alert | Hud::Info)) => (380.0, 38.0),
            View::Hud if matches!(self.hud, Some(Hud::Caps(_) | Hud::Lang(_))) => (220.0, 34.0),
            View::Hud if matches!(self.hud, Some(Hud::Volume | Hud::Brightness(_))) => (320.0, 35.0),
            View::Hud => (300.0, 34.0),
            View::Expanded => EXPANDED,
        }
    }

    /// Hitung ulang bentuk notch dari keadaan sekarang, lalu gambar.
    fn update(&mut self) {
        let playing = self.media.session && self.media.playing;
        let view = state::view(Inputs {
            hidden: self.user_hidden || self.fullscreen,
            // kotak teks yang terbuka menahan notch tetap terbuka
            hover: self.hover || input::is_open() || self.dragging,
            ringing: self.ringing(),
            hud: self.hud.is_some(),
            // pill melebar hanya kalau ada lirik untuk ditampilkan; tanpa lirik cukup ikon audio yang bergerak
            playing: playing && self.media.lyrics.is_some(),
        });
        let to = self.target(view);
        unsafe {
            if view != self.view || to != self.to {
                self.fade = view != self.view;
                (self.view, self.from, self.to) = (view, self.cur, to);
                self.anim_start = Some(Instant::now());
                SetTimer(Some(self.hwnd), T_ANIM, 16, None);
            }
            // selama musik berjalan: ikon audio, baris lirik, dan batang progres ikut bergerak
            let need_tick = self.pomo.running() || self.ringing() || playing;
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
            }
        }
        if self.lyric_since.is_some_and(|s| s.elapsed().as_secs_f32() >= LYRIC_SECS) {
            self.lyric_since = None;
        }
        // timer animasi hidup selama masih ada yang bergerak: ukuran notch atau baris lirik
        if self.anim_start.is_none() && self.lyric_since.is_none() {
            let _ = unsafe { KillTimer(Some(self.hwnd), T_ANIM) };
        }

        let mut buttons = Vec::new();
        let (w, h) = self.cur;
        let cx = WIN.0 / 2.0;
        let g = &self.gfx;
        g.begin();
        if h > 0.1 {
            let blink = self.ringing() && (self.born.elapsed().as_millis() / 500) % 2 == 1;
            // radius sudut bawah dan "telinga" ikut membesar bersama tinggi notch
            let grow = ((h - IDLE.1) / (EXPANDED.1 - IDLE.1)).clamp(0.0, 1.0);
            let (radius, ear) = (11.0 + grow * 13.0, 8.0 + grow * 10.0);
            g.alpha.set(1.0);
            g.notch(cx - w / 2.0, cx + w / 2.0, h, radius, ear, if blink { 0x8a3a00 } else { 0x000000 });
            g.clip([cx - w / 2.0, 0.0, cx + w / 2.0, h]);
            g.alpha.set(k);
            let musical = self.view == View::Compact || (self.view == View::Hud && self.hud == Some(Hud::Track));
            if let (true, Some(art)) = (musical, &self.media.art) {
                g.glow(cx - w / 2.0, cx + w / 2.0, h, radius, art.tint);
            }
            self.content(cx, &mut buttons);
            g.unclip();
        }
        g.end(self.hwnd);
        self.buttons = buttons;
    }

    /// Cocokkan baris lirik dengan posisi putar. Kalau barisnya berganti saat pill lirik tampil, mulai
    /// transisi; lalu pasang timer tepat di awal baris berikutnya supaya pergantiannya tidak telat.
    fn sync_lyric(&mut self) {
        let m = &self.media;
        let Some(lines) = m.lyrics.clone().filter(|_| m.session && m.playing) else {
            let _ = unsafe { KillTimer(Some(self.hwnd), T_LYRIC) };
            (self.lyric_idx, self.lyric_since) = (None, None);
            self.lyric_cur.clear();
            return;
        };
        let pos = self.lyric_pos();
        let mut idx = lyrics::active(&lines, pos);
        // laporan posisi dari pemutar bisa mundur sepersekian detik; jangan bolak-balik ke baris sebelumnya
        if let Some(shown) = self.lyric_idx
            && idx.map_or(0, |i| i + 1) == shown
            && lines.get(shown).is_some_and(|l| l.0 - pos < 500)
        {
            idx = Some(shown);
        }
        // jeda instrumental (baris kosong) menampilkan judul lagu
        let text = idx.map(|i| lines[i].1.as_str()).filter(|s| !s.is_empty()).unwrap_or(&m.title).to_string();
        if idx != self.lyric_idx || text != self.lyric_cur {
            self.lyric_idx = idx;
            let prev = std::mem::replace(&mut self.lyric_cur, text);
            if self.view == View::Compact && !prev.is_empty() {
                (self.lyric_prev, self.lyric_since) = (prev, Some(Instant::now()));
                unsafe { SetTimer(Some(self.hwnd), T_ANIM, 16, None) };
            }
        }
        match lines.get(idx.map_or(0, |i| i + 1)) {
            Some(next) => drop(unsafe { SetTimer(Some(self.hwnd), T_LYRIC, (next.0 - pos).clamp(15, 60_000) as u32, None) }),
            None => drop(unsafe { KillTimer(Some(self.hwnd), T_LYRIC) }),
        }
    }

    /// Jam dengan AM/PM kecil (kalau format Windows memakainya), ditengahkan di `cx`.
    fn clock(&self, g: &Gfx, cx: f32, cy: f32) {
        let (time, ampm) = clock::time_text();
        let (tw, aw) = (g.width(&time, &g.f.time), g.width(&ampm, &g.f.tiny));
        let x = cx - (tw + if ampm.is_empty() { 0.0 } else { 3.0 + aw }) / 2.0;
        g.text(&time, &g.f.time, [x, cy - 11.0, x + tw + 2.0, cy + 11.0], WHITE);
        g.text(&ampm, &g.f.tiny, [x + tw + 3.0, cy - 8.5, x + tw + 5.0 + aw, cy + 11.5], WHITE);
    }

    /// Isi notch diletakkan menurut ukuran tujuan supaya tidak ikut bergeser selama animasi.
    fn content(&self, cx: f32, buttons: &mut Vec<(R, Action)>) {
        let (g, m) = (&self.gfx, &self.media);
        let (x0, x1, h) = (cx - self.to.0 / 2.0, cx + self.to.0 / 2.0, self.to.1);
        let remaining = self.pomo_remaining();
        let battery = power::status();
        let playing = m.session && m.playing;
        let tint = m.art.as_ref().map_or(GRAY, |a| a.tint);
        // batang level untuk HUD volume dan kecerahan
        let level = |glyph: &str, frac: f32| {
            g.text(glyph, &g.f.icon, [x0 + 14.0, 0.0, x0 + 44.0, h], WHITE);
            let (b0, b1) = (x0 + 50.0, x1 - 22.0);
            g.rrect([b0, h / 2.0 - 2.5, b1, h / 2.0 + 2.5], 2.5, TRACK);
            if frac > 0.0 {
                g.rrect([b0, h / 2.0 - 2.5, b0 + (b1 - b0) * frac.min(1.0), h / 2.0 + 2.5], 2.5, WHITE);
            }
        };

        match self.view {
            // pill diam: cincin baterai, jam (atau sisa Pomodoro), ikon audio
            View::Idle => {
                if let Some((plugged, pct)) = battery {
                    battery_ring(g, x0 + 18.0, h / 2.0, 7.0, plugged, pct);
                }
                match &remaining {
                    Some(rem) => g.text(rem, &g.f.cbold, [x0 + 30.0, 0.0, x1 - 36.0, h], WHITE),
                    None => self.clock(g, cx + 1.0, h / 2.0),
                }
                wave(g, x1 - 24.0, h / 2.0, playing.then(|| self.born.elapsed().as_secs_f32()));
            }
            // pill lirik: baris yang sedang dinyanyikan, dengan pendar warna cover
            View::Compact => {
                g.text(NOTE, &g.f.icon, [x0 + 12.0, 0.0, x0 + 38.0, h], tint);
                let (l, r) = (x0 + 40.0, x1 - 22.0);
                let cur = if self.lyric_cur.is_empty() { &m.title } else { &self.lyric_cur };
                // baris lama naik sambil memudar, baris baru naik dari bawah sambil muncul
                let e = self.lyric_since.map_or(1.0, |s| 1.0 - (1.0 - (s.elapsed().as_secs_f32() / LYRIC_SECS).min(1.0)).powi(3));
                let (k, shift) = (g.alpha.get(), h * 0.5);
                if e < 1.0 {
                    g.alpha.set(k * (1.0 - e).powi(2));
                    g.text(&self.lyric_prev, &g.f.cbold, [l, -shift * e, r, h - shift * e], WHITE);
                }
                g.alpha.set(k * e);
                g.text(cur, &g.f.cbold, [l, shift * (1.0 - e), r, h + shift * (1.0 - e)], WHITE);
                g.alpha.set(k);
            }
            View::Hud => match self.hud {
                Some(Hud::Volume) => {
                    let v = volume::VOLUME.load(Relaxed);
                    let muted = v & volume::MUTE != 0;
                    level(if muted { "\u{E74F}" } else { "\u{E767}" }, if muted { 0.0 } else { (v & !volume::MUTE) as f32 / 100.0 });
                }
                Some(Hud::Power(plugged, pct)) => {
                    battery_ring(g, x0 + 22.0, h / 2.0, 8.0, plugged, pct);
                    let label = if plugged { "Mengisi daya" } else { "Memakai baterai" };
                    g.text(&format!("{label} \u{B7} {pct}%"), &g.f.text, [x0 + 40.0, 0.0, x1 - 14.0, h], WHITE);
                }
                Some(Hud::Track) => {
                    let track = if m.artist.is_empty() { m.title.clone() } else { format!("{} \u{2014} {}", m.title, m.artist) };
                    g.text(NOTE, &g.f.icon, [x0 + 12.0, 0.0, x0 + 38.0, h], tint);
                    g.text(&track, &g.f.cbold, [x0 + 40.0, 0.0, x1 - 22.0, h], WHITE);
                }
                Some(Hud::Brightness(pct)) => level("\u{E706}", pct as f32 / 100.0),
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
                // baris status di tengah-atas: cincin baterai, tanggal, jam; ikon audio di kanan
                let page = self.current_page();
                let c = unsafe { GetLocalTime() };
                let date = state::date_id(c.wDayOfWeek, c.wDay, c.wMonth, c.wYear);
                let (time, ampm) = clock::time_text();
                let ring = if battery.is_some() { 24.0 } else { 0.0 };
                let (dw, tw, aw) = (g.width(&date, &g.f.text), g.width(&time, &g.f.time), g.width(&ampm, &g.f.tiny));
                let mut x = cx - (ring + dw + 9.0 + tw + 3.0 + aw) / 2.0;
                if let Some((plugged, pct)) = battery {
                    battery_ring(g, x + 8.0, 27.0, 7.0, plugged, pct);
                    x += ring;
                }
                g.text(&date, &g.f.text, [x, 16.0, x + dw + 2.0, 38.0], 0xe8e8e8);
                x += dw + 9.0;
                g.text(&time, &g.f.time, [x, 16.0, x + tw + 2.0, 38.0], WHITE);
                g.text(&ampm, &g.f.tiny, [x + tw + 3.0, 18.5, x + tw + 3.0 + aw + 2.0, 38.5], WHITE);
                wave(g, x1 - 40.0, 27.0, playing.then(|| self.born.elapsed().as_secs_f32()));
                if page != Page::Music {
                    g.text(page.title(), &g.f.bold, [x0 + 32.0, 21.0, x0 + 124.0, 43.0], WHITE);
                }

                page.draw(self, g, [x0 + 32.0, 50.0, x1 - 32.0, 152.0], buttons);

                let n = self.pages.len();
                if n > 1 {
                    for (glyph, x, action) in [("\u{E76B}", x0 + 2.0, Action::PrevPage), ("\u{E76C}", x1 - 24.0, Action::NextPage)] {
                        let btn = [x, 78.0, x + 22.0, 108.0];
                        g.text(glyph, &g.f.icon_small, btn, DIM);
                        buttons.push((btn, action));
                    }
                    let widths: Vec<f32> = (0..n).map(|i| if i == self.page { 14.0 } else { 3.0 }).collect();
                    let total = widths.iter().sum::<f32>() + 5.0 * (n - 1) as f32;
                    let mut x = cx - total / 2.0;
                    for (i, w) in widths.into_iter().enumerate() {
                        g.rrect([x, 163.5, x + w, 166.5], 1.5, if i == self.page { WHITE } else { DIM });
                        buttons.push(([x - 2.5, 157.0, x + w + 2.5, h], Action::Page(i)));
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
        } else if let Some(Action::Seek { x0, x1 }) = hit {
            let _ = self.tx.send(Msg::Seek((x - x0) / (x1 - x0)));
        } else if let Some(action) = hit {
            self.act(action);
        }
        self.update();
    }

    /// Pindah halaman dengan isi yang muncul perlahan.
    fn go(&mut self, page: usize) {
        if page != self.page && self.view == View::Expanded {
            (self.fade, self.from, self.anim_start) = (true, self.cur, Some(Instant::now()));
            unsafe { SetTimer(Some(self.hwnd), T_ANIM, 16, None) };
        }
        self.page = page;
    }

    /// Daftarkan shortcut dan beri tahu kalau ada yang bentrok dengan app lain.
    fn hotkeys(&mut self) {
        let failed = register_hotkeys(self.hwnd);
        if !failed.is_empty() {
            self.info("\u{E765}", format!("{} dipakai app lain \u{B7} matikan di Pengaturan", failed.join(" & ")), 6000);
        }
    }

    fn turn(&mut self, by: isize) {
        self.go((self.page as isize + by).rem_euclid(self.pages.len() as isize) as usize);
    }

    /// Sesuaikan posisi dan ukuran setelah setting, monitor, atau DPI berubah.
    /// Permukaan gambar diubah ukurannya di tempat: membuat ulang Direct2D menahan belasan MB yang tidak kembali.
    fn relayout(&mut self) {
        if let Ok((w, h, dpi)) = Self::place(self.hwnd)
            && self.gfx.resize(w, h, dpi).is_ok()
        {
            self.scale = dpi / 96.0;
        }
    }

    /// Letakkan window di tengah-atas monitor pilihan; mengembalikan (lebar, tinggi, dpi) permukaan gambarnya.
    fn place(hwnd: HWND) -> Result<(i32, i32, f32)> {
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
            Ok((w, h, dpi))
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
                WM_APP_OCR => self.on_ocr(),
                WM_HOTKEY if wp.0 == HOTKEY_OCR => ocr::start(self.hwnd),
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
                        } else if self.anim_start.is_none() && !matches!(self.view, View::Hidden | View::Compact) {
                            // pill lirik digambar ulang oleh T_LYRIC tepat saat barisnya berganti
                            self.draw();
                        }
                    }
                    T_LYRIC => {
                        self.sync_lyric();
                        if self.lyric_since.is_none() && self.view == View::Compact {
                            self.draw();
                        }
                    }
                    T_POLL => {
                        let fullscreen = self.hide_fullscreen && power::fullscreen();
                        let minute = GetLocalTime().wMinute;
                        let new_minute = minute != self.minute;
                        self.minute = minute;
                        self.check_layout();
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
                    self.sync_lyric();
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
                    self.sync_lyric();
                    self.hotkeys();
                    online::FORCE.store(true, Relaxed);
                    let _ = self.tx.send(Msg::Refresh);
                    self.relayout();
                    self.update();
                }
                WM_DISPLAYCHANGE | WM_DPICHANGED => {
                    self.relayout();
                    self.draw();
                }
                _ => {}
            }
        }
    }
}

/// Cincin baterai: hijau saat dicas, merah saat hampir habis, dengan ikon petir saat dicas.
fn battery_ring(g: &Gfx, cx: f32, cy: f32, rad: f32, plugged: bool, pct: u8) {
    let color = if plugged { GREEN } else if pct <= 20 { 0xff453a } else { WHITE };
    g.ring(cx, cy, rad, 2.2, pct as f32 / 100.0, color, TRACK);
    if plugged {
        g.text("\u{E945}", &g.f.icon_small, [cx - rad, cy - rad, cx + rad, cy + rad], color);
    }
}

/// Ikon not musik di font ikon Windows.
pub const NOTE: &str = "\u{E8D6}";

/// Ikon gelombang audio. `t` (detik) menggerakkan batangnya selama musik berjalan; None = diam dan redup.
fn wave(g: &Gfx, cx: f32, cy: f32, t: Option<f32>) {
    for (i, bar) in [5.0f32, 9.0, 13.0, 9.0, 6.0].into_iter().enumerate() {
        let x = cx - 9.0 + i as f32 * 3.8;
        let bar = match t {
            Some(t) => 3.0 + (bar - 3.0) * (0.55 + 0.45 * (t * 7.0 + i as f32 * 1.9).sin()),
            None => 2.0 + bar * 0.35,
        };
        g.rrect([x, cy - bar / 2.0, x + 2.0, cy + bar / 2.0], 1.0, if t.is_some() { WHITE } else { GRAY });
    }
}

const HOTKEY_OCR: usize = 2;

/// Daftarkan Ctrl+Alt+T untuk menyalin teks dari layar.
/// Mengembalikan nama shortcut yang gagal karena sudah dipakai app lain.
fn register_hotkeys(hwnd: HWND) -> Vec<&'static str> {
    use windows::Win32::UI::Input::KeyboardAndMouse::{MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, RegisterHotKey, UnregisterHotKey};
    let mut failed = Vec::new();
    unsafe {
        let _ = UnregisterHotKey(Some(hwnd), HOTKEY_OCR as i32);
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
            name: "Salin teks layar",
            items: vec![
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
        let (w, h, dpi) = App::place(hwnd)?;
        let (gfx, scale) = (Gfx::new(w, h, dpi)?, dpi / 96.0);
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
            shelf_tab: 0,
            lyric_idx: None,
            lyric_cur: String::new(),
            lyric_prev: String::new(),
            lyric_since: None,
            brightness_ready: false,
            layout: (0, 0),
            battery_band: 0,
            net: None,
            notices: Vec::new(),
            notify_preview: false,
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
