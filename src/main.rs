#![windows_subsystem = "windows"]
//! Ponian: notch untuk Windows.

mod lyrics;
mod media;
mod power;
mod render;
mod state;
mod tray;
mod volume;

use media::{Media, Msg};
use render::{GRAY, Gfx, R, WHITE};
use state::{Inputs, Timer, View};
use std::{
    cell::RefCell,
    sync::{atomic::Ordering::Relaxed, mpsc::Sender},
    time::Instant,
};
use windows::{
    Win32::{
        Foundation::*,
        System::{
            Diagnostics::Debug::MessageBeep,
            LibraryLoader::GetModuleHandleW,
            Threading::CreateMutexW,
        },
        UI::{
            HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForWindow, SetProcessDpiAwarenessContext},
            Input::KeyboardAndMouse::{TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent},
            WindowsAndMessaging::*,
        },
    },
    core::*,
};

pub const WM_APP_MEDIA: u32 = WM_APP + 1;
pub const WM_APP_VOLUME: u32 = WM_APP + 2;
pub const WM_APP_TRAY: u32 = WM_APP + 3;
const WM_MOUSELEAVE: u32 = 0x02A3;

/// Ukuran window dalam DIP. Pill digambar di dalamnya, sisanya transparan dan tembus klik.
const WIN: (f32, f32) = (420.0, 200.0);
const LINE: (f32, f32) = (120.0, 4.0);
const HUD_MS: u32 = 3000;
const HOVER_MS: u32 = 150;
const ANIM_MS: f32 = 220.0;

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
enum Action {
    Media(fn() -> Msg),
    Timer(u64),
    StopTimer,
}

struct App {
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
    view: View,
    cur: (f32, f32),
    from: (f32, f32),
    to: (f32, f32),
    /// isi pill di-fade hanya kalau bentuknya berganti, bukan saat ukurannya saja yang berubah
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

    fn target(&self, view: View) -> (f32, f32) {
        let m = &self.media;
        let timer = matches!(self.timer, Timer::Running(_));
        match view {
            View::Hidden => (LINE.0, 0.0),
            View::Line => LINE,
            View::Hud => (280.0, 36.0),
            View::Compact if !m.playing => (110.0, 32.0),
            View::Compact => (if m.lyrics.is_some() { 380.0 } else { 260.0 } + if timer { 50.0 } else { 0.0 }, 32.0),
            View::Expanded => {
                let media = match (m.session, m.lyrics.is_some()) {
                    (false, _) => 36.0,
                    (true, false) => 90.0,
                    (true, true) => 134.0,
                };
                (360.0, media + 40.0)
            }
        }
    }

    /// Hitung ulang bentuk pill dari keadaan sekarang, lalu gambar.
    fn update(&mut self) {
        let playing = self.media.session && self.media.playing;
        let timer = matches!(self.timer, Timer::Running(_));
        let view = state::view(Inputs {
            hidden: self.user_hidden || self.fullscreen,
            hover: self.hover,
            ringing: self.ringing(),
            hud: self.hud.is_some(),
            playing,
            timer,
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
            let t = start.elapsed().as_secs_f32() * 1000.0 / ANIM_MS;
            let e = state::ease(t);
            self.cur = (self.from.0 + (self.to.0 - self.from.0) * e, self.from.1 + (self.to.1 - self.from.1) * e);
            if self.fade {
                k = e;
            }
            if t >= 1.0 {
                (self.anim_start, self.fade, self.cur) = (None, false, self.to);
                let _ = unsafe { KillTimer(Some(self.hwnd), T_ANIM) };
            }
        }

        let (g, m) = (&self.gfx, &self.media);
        let mut buttons = Vec::new();
        let (w, h) = self.cur;
        let cx = WIN.0 / 2.0;
        g.begin();
        if h > 0.1 {
            // bagian atas pill sengaja keluar layar supaya menempel di tepi atas seperti notch
            let radius = h.min(18.0);
            let blink = self.ringing() && (self.born.elapsed().as_millis() / 500) % 2 == 1;
            g.alpha.set(1.0);
            g.rrect([cx - w / 2.0, -radius, cx + w / 2.0, h], radius, if blink { 0x8a3a00 } else { 0x000000 });
            g.clip([cx - w / 2.0, 0.0, cx + w / 2.0, h]);
            g.alpha.set(k);
            // isi diletakkan menurut ukuran tujuan supaya tidak ikut bergeser selama animasi
            let (x0, x1) = (cx - self.to.0 / 2.0, cx + self.to.0 / 2.0);
            let now = Instant::now();
            let remaining = self.timer.remaining(now).map(state::mmss);
            let track = if m.artist.is_empty() { m.title.clone() } else { format!("{} \u{2014} {}", m.title, m.artist) };
            let line = m.lyrics.as_ref().map(|l| (l, lyrics::active(l, m.pos())));

            match self.view {
                View::Compact if m.playing => {
                    let lyric = line.and_then(|(l, i)| Some(l[i?].1.as_str())).filter(|s| !s.is_empty());
                    let right = if remaining.is_some() { 58.0 } else { 0.0 };
                    g.text(lyric.unwrap_or(&track), &g.center, [x0 + 16.0, 0.0, x1 - 16.0 - right, 32.0], WHITE);
                    if let Some(r) = &remaining {
                        g.text(r, &g.csmall, [x1 - 66.0, 0.0, x1 - 12.0, 32.0], GRAY);
                    }
                }
                View::Compact => {
                    g.text("\u{E916}", &g.icon, [x0 + 10.0, 0.0, x0 + 38.0, 32.0], WHITE);
                    g.text(remaining.as_deref().unwrap_or(""), &g.center, [x0 + 34.0, 0.0, x1 - 12.0, 32.0], WHITE);
                }
                View::Hud => match self.hud {
                    Some(Hud::Volume) => {
                        let v = volume::VOLUME.load(Relaxed);
                        let (muted, pct) = (v & volume::MUTE != 0, (v & !volume::MUTE).min(100));
                        g.text(if muted { "\u{E74F}" } else { "\u{E767}" }, &g.icon, [x0 + 12.0, 0.0, x0 + 42.0, 36.0], WHITE);
                        let (b0, b1) = (x0 + 48.0, x1 - 58.0);
                        g.rrect([b0, 16.0, b1, 20.0], 2.0, 0x3a3a3a);
                        if pct > 0 && !muted {
                            g.rrect([b0, 16.0, b0 + (b1 - b0) * pct as f32 / 100.0, 20.0], 2.0, WHITE);
                        }
                        g.text(&format!("{pct}%"), &g.csmall, [x1 - 54.0, 0.0, x1 - 12.0, 36.0], WHITE);
                    }
                    Some(Hud::Power(plugged, pct)) => {
                        g.rrect([x0 + 16.0, 12.0, x0 + 40.0, 24.0], 3.0, 0x555555);
                        g.rrect([x0 + 41.0, 16.0, x0 + 43.0, 20.0], 1.0, 0x555555);
                        let fill = if plugged { 0x4cd964 } else { WHITE };
                        g.rrect([x0 + 18.0, 14.0, x0 + 18.0 + 20.0 * pct.max(8) as f32 / 100.0, 22.0], 1.5, fill);
                        let label = if plugged { "Mengisi daya" } else { "Memakai baterai" };
                        g.text(&format!("{label} \u{B7} {pct}%"), &g.text, [x0 + 54.0, 0.0, x1 - 12.0, 36.0], WHITE);
                    }
                    Some(Hud::Track) => g.text(&track, &g.center, [x0 + 16.0, 0.0, x1 - 16.0, 36.0], WHITE),
                    None => {}
                },
                View::Expanded => {
                    let (l, r) = (x0 + 18.0, x1 - 18.0);
                    let mut y = 10.0;
                    if m.session {
                        g.text(&m.title, &g.bold, [l, y, r, y + 20.0], WHITE);
                        g.text(&m.artist, &g.small, [l, y + 20.0, r, y + 38.0], GRAY);
                        y += 42.0;
                        let toggle = if m.playing { "\u{E769}" } else { "\u{E768}" };
                        let controls: [(&str, fn() -> Msg); 3] = [("\u{E892}", || Msg::Prev), (toggle, || Msg::Toggle), ("\u{E893}", || Msg::Next)];
                        for (i, (glyph, msg)) in controls.into_iter().enumerate() {
                            let x = cx - 66.0 + i as f32 * 44.0;
                            let b = [x, y, x + 44.0, y + 34.0];
                            g.text(glyph, &g.icon, b, WHITE);
                            buttons.push((b, Action::Media(msg)));
                        }
                        y += 38.0;
                        if let Some((lines, i)) = line {
                            let text = |i: Option<usize>| i.and_then(|i| lines.get(i)).map_or("", |l| l.1.as_str());
                            g.text(text(i), &g.center, [l, y, r, y + 22.0], WHITE);
                            g.text(text(Some(i.map_or(0, |i| i + 1))), &g.csmall, [l, y + 22.0, r, y + 40.0], GRAY);
                            y += 44.0;
                        }
                    } else {
                        g.text("Tidak ada media diputar", &g.small, [l, y, r, y + 20.0], GRAY);
                        y += 26.0;
                    }
                    let mut chip = |slot: f32, label: &str, action: Action| {
                        let x = r - 56.0 - slot * 62.0;
                        let b = [x, y + 1.0, x + 56.0, y + 27.0];
                        g.rrect(b, 13.0, 0x2a2a2a);
                        g.text(label, &g.csmall, b, WHITE);
                        buttons.push((b, action));
                    };
                    match (self.timer, &remaining) {
                        (Timer::Ringing, _) => {
                            g.text("Waktu habis", &g.bold, [l, y, r - 70.0, y + 28.0], WHITE);
                            chip(0.0, "OK", Action::StopTimer);
                        }
                        (_, Some(rem)) => {
                            g.text(&format!("Timer {rem}"), &g.text, [l, y, r - 70.0, y + 28.0], WHITE);
                            chip(0.0, "Batal", Action::StopTimer);
                        }
                        _ => {
                            g.text("Timer", &g.small, [l, y, l + 60.0, y + 28.0], GRAY);
                            for (slot, minutes) in [(2.0, 5), (1.0, 15), (0.0, 25)] {
                                chip(slot, &format!("{minutes} mnt"), Action::Timer(minutes));
                            }
                        }
                    }
                }
                View::Hidden | View::Line => {}
            }
            g.unclip();
        }
        g.end(self.hwnd);
        self.buttons = buttons;
    }

    fn click(&mut self, x: f32, y: f32) {
        // saat timer berbunyi, klik di mana pun pada pill menghentikannya
        if self.ringing() {
            self.timer = Timer::Off;
        } else if let Some(&(_, action)) = self.buttons.iter().find(|(b, _)| x >= b[0] && x < b[2] && y >= b[1] && y < b[3]) {
            match action {
                Action::Media(msg) => drop(self.tx.send(msg())),
                Action::Timer(minutes) => self.timer = Timer::start(minutes, Instant::now()),
                Action::StopTimer => self.timer = Timer::Off,
            }
        }
        self.update();
    }

    /// Letakkan window di tengah-atas monitor utama dan buat ulang permukaan gambar untuk DPI sekarang.
    fn place(hwnd: HWND) -> Result<(Gfx, f32)> {
        unsafe {
            let dpi = GetDpiForWindow(hwnd).max(96) as f32;
            let scale = dpi / 96.0;
            let (w, h) = ((WIN.0 * scale).ceil() as i32, (WIN.1 * scale).ceil() as i32);
            let x = (GetSystemMetrics(SM_CXSCREEN) - w) / 2;
            SetWindowPos(hwnd, Some(HWND_TOPMOST), x, 0, w, h, SWP_NOACTIVATE | SWP_SHOWWINDOW)?;
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
                    // jeda singkat supaya pill tidak melebar saat kursor hanya lewat
                    SetTimer(Some(self.hwnd), T_HOVER, HOVER_MS, None);
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
                WM_TIMER => match wp.0 {
                    T_ANIM => self.draw(),
                    T_TICK => {
                        if self.timer.tick(Instant::now()) {
                            let _ = MessageBeep(MB_ICONASTERISK);
                            self.update();
                        } else if self.anim_start.is_none() && !matches!(self.view, View::Hidden | View::Line) {
                            self.draw();
                        }
                    }
                    T_POLL => {
                        let fullscreen = power::fullscreen();
                        if fullscreen != self.fullscreen {
                            self.fullscreen = fullscreen;
                            self.update();
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
                    if let Some((plugged, pct)) = power::status()
                        && self.plugged.replace(plugged) != Some(plugged)
                    {
                        self.show_hud(Hud::Power(plugged, pct));
                    }
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

fn tray_menu(hwnd: HWND) {
    let lyrics = media::SHOW_LYRICS.load(Relaxed);
    let hidden = tray::get(tray::HIDE_NOTCH, false);
    // menu menjalankan message loop sendiri, jadi APP tidak boleh sedang dipinjam di sini
    let cmd = tray::menu(hwnd, lyrics, hidden);
    APP.with_borrow_mut(|app| {
        let Some(app) = app else { return };
        match cmd {
            tray::CMD_AUTOSTART => tray::set_autostart(!tray::autostart()),
            tray::CMD_LYRICS => {
                media::SHOW_LYRICS.store(!lyrics, Relaxed);
                tray::set(tray::SHOW_LYRICS, !lyrics);
                let _ = app.tx.send(Msg::Refresh);
            }
            tray::CMD_HIDE => {
                tray::set(tray::HIDE_NOTCH, !hidden);
                app.user_hidden = !hidden;
                app.update();
            }
            tray::CMD_QUIT => drop(unsafe { PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0)) }),
            _ => {}
        }
    });
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
            WM_APP_TRAY => {
                if matches!(lp.0 as u32, WM_LBUTTONUP | WM_RBUTTONUP) {
                    tray_menu(hwnd);
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
        if GetLastError() == ERROR_ALREADY_EXISTS {
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
        media::SHOW_LYRICS.store(tray::get(tray::SHOW_LYRICS, true), Relaxed);
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
            user_hidden: tray::get(tray::HIDE_NOTCH, false),
            fullscreen: false,
            ticking: false,
            view: View::Line,
            cur: LINE,
            from: LINE,
            to: LINE,
            fade: false,
            anim_start: None,
            born: Instant::now(),
            buttons: Vec::new(),
        };
        app.update();
        APP.set(Some(app));

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            DispatchMessageW(&msg);
        }
    }
    Ok(())
}
