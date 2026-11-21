//! Settings: disimpan di registry (HKCU\Software\Ponian), plus jendela Pengaturan yang digambar sendiri di atas latar kaca.
//! Data rahasia (API key, link kalender) dienkripsi dengan DPAPI sehingga hanya akun Windows ini yang bisa membacanya.
use crate::{
    input,
    render::{ACCENT, GRAY, Gfx, R, WHITE},
};
use std::{
    cell::RefCell,
    sync::atomic::{AtomicIsize, Ordering::Relaxed},
};
use windows::{
    Win32::{
        Foundation::*,
        Graphics::{Dwm::*, Gdi::*},
        Security::Cryptography::*,
        System::{
            LibraryLoader::{GetModuleHandleW, GetProcAddress, LoadLibraryW},
            Registry::*,
        },
        UI::{
            Controls::MARGINS,
            HiDpi::{AdjustWindowRectExForDpi, GetDpiForWindow},
            Input::KeyboardAndMouse::*,
            WindowsAndMessaging::*,
        },
    },
    core::*,
};

const KEY: PCWSTR = w!("Software\\Ponian");
const RUN: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const APP: PCWSTR = w!("Ponian");
/// Kunci khusus yang disimpan di entri Run Windows, bukan di kunci Ponian.
pub const AUTOSTART: &str = "Autostart";

/// Window notch, untuk diberi tahu kalau ada setting yang berubah.
pub static NOTCH: AtomicIsize = AtomicIsize::new(0);
pub const WM_APP_SETTINGS: u32 = WM_APP + 10;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

pub fn dword(name: &str, default: u32) -> u32 {
    if name == AUTOSTART {
        return autostart() as u32;
    }
    let (mut value, mut len) = (0u32, 4u32);
    let res = unsafe { RegGetValueW(HKEY_CURRENT_USER, KEY, PCWSTR(wide(name).as_ptr()), RRF_RT_REG_DWORD, None, Some(&mut value as *mut _ as _), Some(&mut len)) };
    if res.is_ok() { value } else { default }
}

pub fn flag(name: &str, default: bool) -> bool {
    dword(name, default as u32) != 0
}

pub fn set_dword(name: &str, value: u32) {
    if name == AUTOSTART {
        return set_autostart(value != 0);
    }
    let _ = unsafe { RegSetKeyValueW(HKEY_CURRENT_USER, KEY, PCWSTR(wide(name).as_ptr()), REG_DWORD.0, Some(&value as *const _ as _), 4) };
}

fn bytes(name: &str, kind: REG_ROUTINE_FLAGS) -> Option<Vec<u8>> {
    let name = wide(name);
    let mut len = 0u32;
    unsafe { RegGetValueW(HKEY_CURRENT_USER, KEY, PCWSTR(name.as_ptr()), kind, None, None, Some(&mut len)) }.ok().ok()?;
    let mut buf = vec![0u8; len as usize];
    unsafe { RegGetValueW(HKEY_CURRENT_USER, KEY, PCWSTR(name.as_ptr()), kind, None, Some(buf.as_mut_ptr() as _), Some(&mut len)) }.ok().ok()?;
    buf.truncate(len as usize);
    Some(buf)
}

pub fn string(name: &str) -> String {
    let Some(b) = bytes(name, RRF_RT_REG_SZ) else { return String::new() };
    let units: Vec<u16> = b.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).take_while(|&u| u != 0).collect();
    String::from_utf16_lossy(&units)
}

pub fn set_string(name: &str, value: &str) {
    let v = wide(value);
    let _ = unsafe { RegSetKeyValueW(HKEY_CURRENT_USER, KEY, PCWSTR(wide(name).as_ptr()), REG_SZ.0, Some(v.as_ptr() as _), (v.len() * 2) as u32) };
}

pub fn secret(name: &str) -> Option<String> {
    let enc = bytes(name, RRF_RT_REG_BINARY)?;
    let input = CRYPT_INTEGER_BLOB { cbData: enc.len() as u32, pbData: enc.as_ptr() as *mut u8 };
    let mut out = CRYPT_INTEGER_BLOB::default();
    unsafe { CryptUnprotectData(&input, None, None, None, None, CRYPTPROTECT_UI_FORBIDDEN, &mut out) }.ok()?;
    let plain = unsafe { std::slice::from_raw_parts(out.pbData, out.cbData as usize) }.to_vec();
    unsafe { LocalFree(Some(HLOCAL(out.pbData as _))) };
    String::from_utf8(plain).ok().filter(|s| !s.is_empty())
}

pub fn set_secret(name: &str, value: &str) {
    let name_w = wide(name);
    if value.is_empty() {
        let _ = unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, KEY, PCWSTR(name_w.as_ptr())) };
        return;
    }
    let input = CRYPT_INTEGER_BLOB { cbData: value.len() as u32, pbData: value.as_ptr() as *mut u8 };
    let mut out = CRYPT_INTEGER_BLOB::default();
    if unsafe { CryptProtectData(&input, w!("Ponian"), None, None, None, CRYPTPROTECT_UI_FORBIDDEN, &mut out) }.is_ok() {
        let _ = unsafe { RegSetKeyValueW(HKEY_CURRENT_USER, KEY, PCWSTR(name_w.as_ptr()), REG_BINARY.0, Some(out.pbData as _), out.cbData) };
        unsafe { LocalFree(Some(HLOCAL(out.pbData as _))) };
    }
}

pub fn autostart() -> bool {
    unsafe { RegGetValueW(HKEY_CURRENT_USER, RUN, APP, RRF_RT_REG_SZ, None, None, None) }.is_ok()
}

// shortcut: path exe dicatat saat opsi dinyalakan; kalau exe dipindah, opsi harus dinyalakan ulang.
fn set_autostart(on: bool) {
    unsafe {
        if !on {
            let _ = RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN, APP);
        } else if let Ok(exe) = std::env::current_exe() {
            let cmd = wide(&format!("\"{}\"", exe.display()));
            let _ = RegSetKeyValueW(HKEY_CURRENT_USER, RUN, APP, REG_SZ.0, Some(cmd.as_ptr() as _), (cmd.len() * 2) as u32);
        }
    }
}

// ---------- isi jendela Pengaturan ----------

pub enum Item {
    Check(&'static str, &'static str, bool),
    /// kunci, label, daftar pilihan, indeks default
    Choice(&'static str, &'static str, fn() -> Vec<String>, u32),
    /// kunci, label, rahasia (dienkripsi dan disamarkan)
    Text(&'static str, &'static str, bool),
    Note(&'static str),
    /// sakelar dengan tombol naik/turun: kunci, label, default, fungsi pemindah (indeks item, arah)
    Movable(&'static str, &'static str, bool, fn(usize, i32)),
}

pub struct Section {
    pub name: &'static str,
    /// ikon di font ikon Windows
    pub icon: &'static str,
    pub items: Vec<Item>,
}

/// Daftar bagian Pengaturan disusun oleh main.rs, supaya tiap fitur menambahkan opsinya sendiri.
pub static SECTIONS: std::sync::OnceLock<fn() -> Vec<Section>> = std::sync::OnceLock::new();

pub fn opts(items: &'static [&'static str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

// Jendela digambar sendiri dengan Direct2D di atas latar akrilik Windows (kartu kaca), bukan kontrol
// Windows biasa: kontrol GDI tidak bisa tampil benar di atas latar tembus pandang.
// shortcut: pembaca layar tidak melihat isi jendela ini; pakai UI Automation kalau itu dibutuhkan.

/// Ukuran isi jendela dalam DIP, lebar panel kiri, dan batas atas-bawah area gulir di kanan.
const W: f32 = 720.0;
const H: f32 = 520.0;
const SIDE: f32 = 186.0;
const VIEW: (f32, f32) = (56.0, H - 12.0);
const WM_MOUSELEAVE: u32 = 0x02A3;

#[derive(Clone, Copy, PartialEq)]
enum Hit {
    Section(usize),
    Toggle(usize),
    Choice(usize),
    Text(usize),
    Move(usize, i32),
}

struct Ui {
    hwnd: HWND,
    g: Gfx,
    sections: Vec<Section>,
    current: usize,
    scale: f32,
    /// latar akrilik aktif (Windows 11); kalau tidak, jendela diberi latar gelap biasa
    glass: bool,
    scroll: f32,
    /// tinggi isi bagian yang sedang tampil, untuk batas gulir
    content: f32,
    /// area yang bisa diklik, dicatat ulang tiap kali menggambar
    hits: Vec<(R, Hit)>,
    /// indeks `hits` di bawah kursor, dan yang dipilih lewat keyboard
    hot: Option<usize>,
    focus: Option<usize>,
}

thread_local! {
    static UI: RefCell<Option<Ui>> = const { RefCell::new(None) };
}

/// Menu bawaan Windows (daftar pilihan di Pengaturan, menu tray) dibuat gelap supaya serasi dengan notch.
// shortcut: memakai dua fungsi uxtheme tanpa nama (ordinal 135 dan 136, ada sejak Windows 10 1903) yang tidak
// didokumentasikan Microsoft; kalau suatu saat hilang, menu kembali terang.
pub fn dark_menus() {
    unsafe {
        let Ok(lib) = LoadLibraryW(w!("uxtheme.dll")) else { return };
        if let (Some(set_mode), Some(flush)) = (GetProcAddress(lib, PCSTR(135 as *const u8)), GetProcAddress(lib, PCSTR(136 as *const u8))) {
            // 2 = selalu gelap, apa pun tema Windows
            std::mem::transmute::<_, extern "system" fn(i32) -> i32>(set_mode)(2);
            std::mem::transmute::<_, extern "system" fn()>(flush)();
        }
    }
}

pub fn open() {
    if let Some(hwnd) = UI.with_borrow(|ui| ui.as_ref().map(|u| u.hwnd)) {
        unsafe {
            let _ = ShowWindow(hwnd, SW_RESTORE);
            let _ = SetForegroundWindow(hwnd);
        }
        return;
    }
    let Some(build) = SECTIONS.get() else { return };
    unsafe {
        let Ok(module) = GetModuleHandleW(None) else { return };
        let instance: HINSTANCE = module.into();
        let class = w!("PonianSettings");
        RegisterClassW(&WNDCLASSW { lpfnWndProc: Some(proc), hInstance: instance, lpszClassName: class, hbrBackground: HBRUSH(GetStockObject(BLACK_BRUSH).0), ..Default::default() });
        let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX;
        let Ok(hwnd) = CreateWindowExW(WINDOW_EX_STYLE(0), class, w!("Pengaturan Ponian"), style, CW_USEDEFAULT, CW_USEDEFAULT, 100, 100, None, None, Some(instance), None) else {
            return;
        };
        // shortcut: ukuran mengikuti DPI layar saat dibuka; pindah ke layar ber-DPI lain tidak mengubahnya.
        let dpi = GetDpiForWindow(hwnd).max(96);
        let scale = dpi as f32 / 96.0;
        let (w, h) = ((W * scale) as i32, (H * scale) as i32);
        let mut rc = RECT { left: 0, top: 0, right: w, bottom: h };
        let _ = AdjustWindowRectExForDpi(&mut rc, style, false, WINDOW_EX_STYLE(0), dpi);
        let _ = SetWindowPos(hwnd, None, 0, 0, rc.right - rc.left, rc.bottom - rc.top, SWP_NOMOVE | SWP_NOZORDER);
        let Ok(g) = Gfx::new(w, h, dpi as f32) else {
            let _ = DestroyWindow(hwnd);
            return;
        };
        // judul gelap dan latar akrilik. Windows 10 menolak atribut latar; jendela lalu memakai warna gelap biasa.
        let (dark, acrylic) = (1i32, DWMSBT_TRANSIENTWINDOW);
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE, &dark as *const _ as _, 4);
        let glass = DwmSetWindowAttribute(hwnd, DWMWA_SYSTEMBACKDROP_TYPE, &acrylic as *const _ as _, 4).is_ok()
            && DwmExtendFrameIntoClientArea(hwnd, &MARGINS { cxLeftWidth: -1, cxRightWidth: -1, cyTopHeight: -1, cyBottomHeight: -1 }).is_ok();
        UI.set(Some(Ui { hwnd, g, sections: build(), current: 0, scale, glass, scroll: 0.0, content: 0.0, hits: Vec::new(), hot: None, focus: None }));
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);
    }
}

fn draw(ui: &mut Ui) {
    let (current, scroll, hot, focus) = (ui.current, ui.scroll, ui.hot, ui.focus);
    let g = &ui.g;
    let mut hits: Vec<(R, Hit)> = Vec::new();
    // catat area klik; hasilnya true kalau kursor sedang di atasnya
    let mut hit = |r: R, h: Hit| {
        hits.push((r, h));
        hot == Some(hits.len() - 1)
    };
    let fill = |r: R, radius: f32, rgb: u32, a: f32| {
        g.alpha.set(a);
        g.rrect(r, radius, rgb);
        g.alpha.set(1.0);
    };
    let line = |r: R, radius: f32, rgb: u32, a: f32| {
        g.alpha.set(a);
        g.stroke(r, radius, rgb, 1.0);
        g.alpha.set(1.0);
    };
    let toggle = |right: f32, cy: f32, on: bool| {
        let r = [right - 38.0, cy - 10.0, right, cy + 10.0];
        if on {
            g.rrect(r, 10.0, ACCENT);
        } else {
            fill(r, 10.0, WHITE, 0.16);
            line(r, 10.0, WHITE, 0.20);
        }
        let x = if on { right - 17.0 } else { right - 35.0 };
        g.rrect([x, cy - 7.0, x + 14.0, cy + 7.0], 7.0, if on { WHITE } else { 0xd0d0d6 });
    };

    g.begin();
    if ui.glass {
        // lapisan gelap tipis supaya teks tetap terbaca di atas wallpaper terang
        fill([0.0, 0.0, W, H], 0.0, 0x0c0c14, 0.42);
    } else {
        g.rrect([0.0, 0.0, W, H], 0.0, 0x17171d);
    }
    g.blob(W - 70.0, 30.0, 280.0, ACCENT, 0.22);
    g.blob(110.0, H - 10.0, 250.0, 0x22d3ee, 0.10);

    // panel kiri: daftar bagian
    let side = [10.0, 10.0, SIDE, H - 10.0];
    fill(side, 14.0, WHITE, 0.06);
    line(side, 14.0, WHITE, 0.10);
    for (i, s) in ui.sections.iter().enumerate() {
        let y = 18.0 + i as f32 * 34.0;
        let r = [16.0, y, SIDE - 6.0, y + 32.0];
        let over = hit(r, Hit::Section(i));
        if i == current {
            fill(r, 9.0, WHITE, 0.14);
            g.rrect([16.0, y + 9.0, 19.0, y + 23.0], 1.5, ACCENT);
        } else if over {
            fill(r, 9.0, WHITE, 0.07);
        }
        let color = if i == current { WHITE } else { 0xc4c4cc };
        g.text(s.icon, &g.f.icon_mid, [26.0, y, 50.0, y + 32.0], color);
        g.text(s.name, &g.f.text, [56.0, y, SIDE - 10.0, y + 32.0], color);
    }

    // kanan: judul, lalu kartu kaca berisi baris-baris opsi
    let sec = &ui.sections[current];
    let (x0, x1, pad) = (SIDE + 16.0, W - 16.0, 14.0);
    g.text(sec.name, &g.f.title, [x0 + 2.0, 12.0, x1, 50.0], WHITE);
    g.clip([x0 - 2.0, VIEW.0, x1 + 2.0, VIEW.1]);
    let full = x1 - x0 - 2.0 * pad;
    // lebar label: sisa baris setelah kontrol di kanannya
    let label_w = |item: &Item| match item {
        Item::Check(..) => full - 52.0,
        Item::Movable(..) => full - 116.0,
        Item::Choice(..) => full - 200.0,
        Item::Text(..) | Item::Note(_) => full,
    };
    let heights: Vec<f32> = sec
        .items
        .iter()
        .map(|item| match item {
            Item::Note(text) => g.height(text, &g.f.wrap, x1 - x0 - 8.0) + 14.0,
            Item::Text(_, label, _) => g.height(label, &g.f.wrap, full) + 60.0,
            Item::Check(_, label, _) | Item::Movable(_, label, ..) | Item::Choice(_, label, ..) => (g.height(label, &g.f.wrap, label_w(item)) + 24.0).max(42.0),
        })
        .collect();
    let mut y = VIEW.0 + 2.0 - scroll;
    for (i, item) in sec.items.iter().enumerate() {
        let h = heights[i];
        let is_note = |n: usize| matches!(sec.items.get(n), Some(Item::Note(_)));
        if let Item::Note(text) = item {
            g.text(text, &g.f.wrap, [x0 + 4.0, y + 2.0, x1 - 4.0, y + h], 0xb0b0bc);
            y += h;
            continue;
        }
        if i == 0 || is_note(i - 1) {
            // satu kartu membungkus semua baris sampai keterangan berikutnya
            let total: f32 = (i..sec.items.len()).take_while(|&n| !is_note(n)).map(|n| heights[n]).sum();
            fill([x0, y, x1, y + total], 12.0, WHITE, 0.07);
            line([x0, y, x1, y + total], 12.0, WHITE, 0.12);
        } else {
            fill([x0 + pad, y - 0.5, x1 - pad, y + 0.5], 0.0, WHITE, 0.09);
        }
        let (row, cy) = ([x0, y, x1, y + h], y + h / 2.0);
        let hover = [x0 + 4.0, y + 4.0, x1 - 4.0, y + h - 4.0];
        // label di tengah baris; untuk kolom teks label ada di atas kolomnya
        let label = |text: &str, top: f32| g.text(text, &g.f.wrap, [x0 + pad, top, x0 + pad + label_w(item), top + h], WHITE);
        let centered = |text: &str| label(text, y + (h - g.height(text, &g.f.wrap, label_w(item))) / 2.0);
        match item {
            Item::Check(key, text, default) => {
                if hit(row, Hit::Toggle(i)) {
                    fill(hover, 9.0, WHITE, 0.05);
                }
                centered(text);
                toggle(x1 - pad, cy, dword(key, *default as u32) != 0);
            }
            Item::Movable(key, text, default, _) => {
                if hit(row, Hit::Toggle(i)) {
                    fill(hover, 9.0, WHITE, 0.05);
                }
                centered(text);
                toggle(x1 - pad, cy, dword(key, *default as u32) != 0);
                for (k, (icon, dir)) in [("\u{E70E}", -1), ("\u{E70D}", 1)].into_iter().enumerate() {
                    let x = x1 - pad - 110.0 + k as f32 * 30.0;
                    let b = [x, cy - 13.0, x + 26.0, cy + 13.0];
                    let over = hit(b, Hit::Move(i, dir));
                    fill(b, 8.0, WHITE, if over { 0.22 } else { 0.09 });
                    g.text(icon, &g.f.icon_small, b, WHITE);
                }
            }
            Item::Choice(key, text, options, default) => {
                let over = hit(row, Hit::Choice(i));
                if over {
                    fill(hover, 9.0, WHITE, 0.05);
                }
                centered(text);
                let options = options();
                let value = options.get(dword(key, *default) as usize).map_or("", |s| s.as_str());
                let chip = [x1 - pad - g.width(value, &g.f.text).min(150.0) - 36.0, cy - 13.0, x1 - pad, cy + 13.0];
                fill(chip, 8.0, WHITE, if over { 0.18 } else { 0.10 });
                line(chip, 8.0, WHITE, 0.14);
                g.text(value, &g.f.text, [chip[0] + 10.0, chip[1], chip[2] - 24.0, chip[3]], WHITE);
                g.text("\u{E70D}", &g.f.icon_small, [chip[2] - 24.0, chip[1], chip[2] - 6.0, chip[3]], 0xc4c4cc);
            }
            Item::Text(key, text, secret_) => {
                label(text, y + 11.0);
                let field = [x0 + pad, y + h - 42.0, x1 - pad, y + h - 12.0];
                let over = hit(field, Hit::Text(i));
                fill(field, 8.0, 0x000000, 0.30);
                line(field, 8.0, if over { ACCENT } else { WHITE }, if over { 0.9 } else { 0.16 });
                // isi rahasia tidak perlu didekripsi hanya untuk ditampilkan sebagai titik-titik
                let value = if *secret_ { bytes(key, RRF_RT_REG_BINARY).map_or(String::new(), |_| "\u{25CF}".repeat(12)) } else { string(key) };
                let inner = [field[0] + 10.0, field[1], field[2] - 30.0, field[3]];
                if value.is_empty() {
                    g.text("Klik untuk mengisi", &g.f.text, inner, GRAY);
                } else {
                    g.text(&value, &g.f.text, inner, WHITE);
                }
                g.text("\u{E70F}", &g.f.icon_small, [field[2] - 28.0, field[1], field[2] - 6.0, field[3]], GRAY);
            }
            Item::Note(_) => {}
        }
        y += h;
        if is_note(i + 1) {
            y += 8.0;
        }
    }
    g.unclip();

    let (content, view) = (y + scroll - VIEW.0 + 6.0, VIEW.1 - VIEW.0);
    if content > view {
        let top = VIEW.0 + scroll / content * view;
        fill([W - 9.0, top, W - 6.0, top + view * view / content], 1.5, WHITE, 0.30);
    }
    // bingkai fokus untuk pemakaian keyboard
    if let Some((r, _)) = focus.and_then(|i| hits.get(i)) {
        g.stroke([r[0] + 1.0, r[1] + 1.0, r[2] - 1.0, r[3] - 1.0], 9.0, ACCENT, 1.6);
    }
    (ui.hits, ui.content) = (hits, content);
}

fn changed() {
    let notch = NOTCH.load(Relaxed);
    if notch != 0 {
        let _ = unsafe { PostMessageW(Some(HWND(notch as _)), WM_APP_SETTINGS, WPARAM(0), LPARAM(0)) };
    }
}

fn hit_at(ui: &Ui, x: f32, y: f32) -> Option<usize> {
    // baris yang tergulir keluar area kanan tidak ikut terklik; yang digambar belakangan ada di atas
    let visible = |h: &Hit| matches!(h, Hit::Section(_)) || (y >= VIEW.0 && y < VIEW.1);
    ui.hits.iter().rposition(|(r, h)| x >= r[0] && x < r[2] && y >= r[1] && y < r[3] && visible(h))
}

fn max_scroll(ui: &Ui) -> f32 {
    (ui.content - (VIEW.1 - VIEW.0)).max(0.0)
}

/// Jalankan aksi sebuah area klik. `r` adalah areanya dalam DIP, untuk menempatkan menu atau kotak teks.
fn activate(hwnd: HWND, hit: Hit, r: R) {
    enum Follow {
        Menu(&'static str, Vec<String>, u32),
        Edit(usize, String, bool),
    }
    // menu dan kotak teks menjalankan message loop sendiri, jadi dibuka setelah pinjaman UI dilepas
    let follow = UI.with_borrow_mut(|ui| {
        let ui = ui.as_mut()?;
        let scale = ui.scale;
        let item = match hit {
            Hit::Section(i) => {
                (ui.current, ui.scroll) = (i.min(ui.sections.len() - 1), 0.0);
                return None;
            }
            Hit::Toggle(i) | Hit::Choice(i) | Hit::Text(i) | Hit::Move(i, _) => ui.sections[ui.current].items.get(i)?,
        };
        match (hit, item) {
            (Hit::Toggle(_), Item::Check(key, _, default) | Item::Movable(key, _, default, _)) => {
                set_dword(key, (dword(key, *default as u32) == 0) as u32);
                changed();
                None
            }
            (Hit::Move(i, dir), Item::Movable(.., mover)) => {
                mover(i, dir);
                ui.sections = SECTIONS.get()?();
                changed();
                None
            }
            (Hit::Choice(_), Item::Choice(key, _, options, default)) => Some((Follow::Menu(key, options(), dword(key, *default)), scale)),
            (Hit::Text(i), Item::Text(key, _, secret_)) => {
                let value = if *secret_ { secret(key).unwrap_or_default() } else { string(key) };
                Some((Follow::Edit(i, value, *secret_), scale))
            }
            _ => None,
        }
    });
    unsafe {
        let screen = |x: f32, y: f32, scale: f32| {
            let mut p = POINT { x: (x * scale) as i32, y: (y * scale) as i32 };
            let _ = ClientToScreen(hwnd, &mut p);
            p
        };
        match follow {
            Some((Follow::Menu(key, options, current), scale)) => {
                if let Ok(menu) = CreatePopupMenu() {
                    for (n, option) in options.iter().enumerate() {
                        let _ = AppendMenuW(menu, MF_STRING, n + 1, PCWSTR(wide(option).as_ptr()));
                    }
                    let _ = CheckMenuRadioItem(menu, 1, options.len() as u32, current + 1, MF_BYCOMMAND.0);
                    let p = screen(r[2] - 14.0, r[3] - 6.0, scale);
                    let cmd = TrackPopupMenu(menu, TPM_RETURNCMD | TPM_RIGHTALIGN | TPM_TOPALIGN, p.x, p.y, None, hwnd, None).0;
                    let _ = DestroyMenu(menu);
                    if cmd > 0 {
                        set_dword(key, cmd as u32 - 1);
                        changed();
                    }
                }
            }
            Some((Follow::Edit(i, value, secret_), scale)) => {
                let (a, b) = (screen(r[0], r[1], scale), screen(r[2], r[3], scale));
                input::open(hwnd, i, RECT { left: a.x, top: a.y, right: b.x, bottom: b.y }, scale, &value, "Enter untuk menyimpan");
                if secret_ {
                    input::mask();
                }
            }
            None => {}
        }
        let _ = InvalidateRect(Some(hwnd), None, false);
    }
}

/// Keyboard: Tab dan panah berpindah antar area klik, Spasi atau Enter menjalankannya, Esc menutup.
fn key(hwnd: HWND, vk: VIRTUAL_KEY) {
    let step = match vk {
        VK_TAB if unsafe { GetKeyState(VK_SHIFT.0 as i32) } < 0 => -1,
        VK_UP => -1,
        VK_TAB | VK_DOWN => 1,
        VK_ESCAPE => {
            let _ = unsafe { PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0)) };
            return;
        }
        VK_RETURN | VK_SPACE => {
            let target = UI.with_borrow(|ui| ui.as_ref().and_then(|ui| ui.hits.get(ui.focus?).copied()));
            if let Some((r, hit)) = target {
                activate(hwnd, hit, r);
            }
            return;
        }
        _ => return,
    };
    UI.with_borrow_mut(|ui| {
        let Some(ui) = ui else { return };
        let n = ui.hits.len() as i32;
        if n == 0 {
            return;
        }
        let next = ui.focus.map_or(if step > 0 { 0 } else { n - 1 }, |f| (f as i32 + step).rem_euclid(n)) as usize;
        ui.focus = Some(next);
        // gulir supaya baris yang dipilih terlihat
        let (r, hit) = ui.hits[next];
        if !matches!(hit, Hit::Section(_)) {
            let shift = if r[3] > VIEW.1 { r[3] - VIEW.1 + 8.0 } else if r[1] < VIEW.0 { r[1] - VIEW.0 - 8.0 } else { 0.0 };
            ui.scroll = (ui.scroll + shift).clamp(0.0, max_scroll(ui));
        }
    });
    let _ = unsafe { InvalidateRect(Some(hwnd), None, false) };
}

extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let redraw = || {
        let _ = unsafe { InvalidateRect(Some(hwnd), None, false) };
    };
    // posisi kursor dari lParam, dalam DIP
    let point = |ui: &Ui| ((lp.0 & 0xffff) as i16 as f32 / ui.scale, ((lp.0 >> 16) & 0xffff) as i16 as f32 / ui.scale);
    unsafe {
        match msg {
            // semua piksel digambar di WM_PAINT; menghapus dulu hanya membuat kedip
            WM_ERASEBKGND => return LRESULT(1),
            WM_PAINT => {
                let mut ps = PAINTSTRUCT::default();
                let dc = BeginPaint(hwnd, &mut ps);
                UI.with_borrow_mut(|ui| {
                    if let Some(ui) = ui {
                        draw(ui);
                        ui.g.present(dc);
                    }
                });
                let _ = EndPaint(hwnd, &ps);
            }
            WM_MOUSEMOVE | WM_MOUSELEAVE => {
                let moved = UI.with_borrow_mut(|ui| {
                    let ui = ui.as_mut()?;
                    let hot = if msg == WM_MOUSELEAVE {
                        None
                    } else {
                        let (x, y) = point(ui);
                        hit_at(ui, x, y)
                    };
                    (hot != ui.hot).then(|| ui.hot = hot)
                });
                if msg == WM_MOUSEMOVE {
                    let mut track = TRACKMOUSEEVENT { cbSize: size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE, hwndTrack: hwnd, dwHoverTime: 0 };
                    let _ = TrackMouseEvent(&mut track);
                }
                if moved.is_some() {
                    redraw();
                }
            }
            WM_SETCURSOR if (lp.0 & 0xffff) as u32 == HTCLIENT => {
                let hand = UI.with_borrow(|ui| ui.as_ref().is_some_and(|ui| ui.hot.is_some()));
                SetCursor(LoadCursorW(None, if hand { IDC_HAND } else { IDC_ARROW }).ok());
                return LRESULT(1);
            }
            WM_LBUTTONUP => {
                let target = UI.with_borrow_mut(|ui| {
                    let ui = ui.as_mut()?;
                    ui.focus = None;
                    let (x, y) = point(ui);
                    hit_at(ui, x, y).map(|i| ui.hits[i])
                });
                if let Some((r, hit)) = target {
                    activate(hwnd, hit, r);
                }
            }
            WM_MOUSEWHEEL => {
                let notches = (wp.0 >> 16) as i16 as f32 / 120.0;
                UI.with_borrow_mut(|ui| {
                    if let Some(ui) = ui {
                        ui.scroll = (ui.scroll - notches * 48.0).clamp(0.0, max_scroll(ui));
                    }
                });
                redraw();
            }
            WM_KEYDOWN => key(hwnd, VIRTUAL_KEY(wp.0 as u16)),
            // kotak teks ditutup: Enter, Tab, atau klik di luar menyimpan isinya; Esc membatalkan
            input::WM_APP_INPUT => {
                let text = input::take_text();
                if lp.0 == input::TAB {
                    input::close();
                }
                if lp.0 != input::CANCEL
                    && let Some(text) = text
                {
                    let saved = UI.with_borrow(|ui| {
                        let ui = ui.as_ref()?;
                        let Item::Text(key, _, secret_) = ui.sections[ui.current].items.get(wp.0)? else { return None };
                        if *secret_ { set_secret(key, text.trim()) } else { set_string(key, text.trim()) }
                        Some(())
                    });
                    if saved.is_some() {
                        changed();
                    }
                }
                redraw();
            }
            WM_DESTROY => {
                if input::owner() == Some(hwnd) {
                    input::close();
                }
                drop(UI.take());
            }
            _ => return DefWindowProcW(hwnd, msg, wp, lp),
        }
        LRESULT(0)
    }
}
