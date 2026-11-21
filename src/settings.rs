//! Settings: disimpan di registry (HKCU\Software\Ponian), plus jendela Pengaturan berisi kontrol Windows biasa.
//! Data rahasia (API key, link kalender) dienkripsi dengan DPAPI sehingga hanya akun Windows ini yang bisa membacanya.
use std::{
    cell::RefCell,
    sync::atomic::{AtomicIsize, Ordering::Relaxed},
};
use windows::{
    Win32::{
        Foundation::*,
        Graphics::Gdi::*,
        Security::Cryptography::*,
        System::{LibraryLoader::GetModuleHandleW, Registry::*},
        UI::{HiDpi::{AdjustWindowRectExForDpi, GetDpiForWindow}, Input::KeyboardAndMouse::SetFocus, WindowsAndMessaging::*},
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
    /// kotak centang dengan tombol naik/turun: kunci, label, default, fungsi pemindah (indeks item, arah)
    Movable(&'static str, &'static str, bool, fn(usize, i32)),
}

pub struct Section {
    pub name: &'static str,
    pub items: Vec<Item>,
}

/// Daftar bagian Pengaturan disusun oleh main.rs, supaya tiap fitur menambahkan opsinya sendiri.
pub static SECTIONS: std::sync::OnceLock<fn() -> Vec<Section>> = std::sync::OnceLock::new();

fn list(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

pub fn opts(items: &'static [&'static str]) -> Vec<String> {
    list(items)
}

struct Ui {
    hwnd: HWND,
    sections: Vec<Section>,
    current: usize,
    font: HFONT,
    scale: f32,
}

thread_local! {
    static UI: RefCell<Option<Ui>> = const { RefCell::new(None) };
}

const ID_LIST: i32 = 100;
const ID_ITEM: i32 = 1000;
const ID_MOVE: i32 = 3000;

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
        RegisterClassW(&WNDCLASSW {
            lpfnWndProc: Some(proc),
            hInstance: instance,
            lpszClassName: class,
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            hbrBackground: GetSysColorBrush(COLOR_WINDOW),
            ..Default::default()
        });
        let Ok(hwnd) = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            class,
            w!("Pengaturan Ponian"),
            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            100,
            100,
            None,
            None,
            Some(instance),
            None,
        ) else {
            return;
        };
        let scale = GetDpiForWindow(hwnd).max(96) as f32 / 96.0;
        let px = |v: f32| (v * scale) as i32;
        let mut rc = RECT { left: 0, top: 0, right: px(660.0), bottom: px(480.0) };
        let _ = AdjustWindowRectExForDpi(&mut rc, WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU, false, WINDOW_EX_STYLE(0), GetDpiForWindow(hwnd));
        let _ = SetWindowPos(hwnd, None, 0, 0, rc.right - rc.left, rc.bottom - rc.top, SWP_NOMOVE | SWP_NOZORDER);
        let font = CreateFontW(-px(14.0), 0, 0, 0, 400, 0, 0, 0, DEFAULT_CHARSET, OUT_DEFAULT_PRECIS, CLIP_DEFAULT_PRECIS, CLEARTYPE_QUALITY, 0, w!("Segoe UI"));
        let sections = build();
        let listbox = child(hwnd, w!("LISTBOX"), w!(""), WS_BORDER | WINDOW_STYLE(LBS_NOTIFY as u32), [16.0, 16.0, 150.0, 448.0], ID_LIST, scale, font);
        for s in &sections {
            let name = wide(s.name);
            SendMessageW(listbox, LB_ADDSTRING, None, Some(LPARAM(name.as_ptr() as isize)));
        }
        SendMessageW(listbox, LB_SETCURSEL, Some(WPARAM(0)), None);
        UI.set(Some(Ui { hwnd, sections, current: 0, font, scale }));
        build_section(hwnd);
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);
    }
}

fn child(parent: HWND, class: PCWSTR, text: PCWSTR, style: WINDOW_STYLE, [x, y, w, h]: [f32; 4], id: i32, scale: f32, font: HFONT) -> HWND {
    unsafe {
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            class,
            text,
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | style,
            (x * scale) as i32,
            (y * scale) as i32,
            (w * scale) as i32,
            (h * scale) as i32,
            Some(parent),
            Some(HMENU(id as _)),
            None,
            None,
        )
        .unwrap_or_default();
        SendMessageW(hwnd, WM_SETFONT, Some(WPARAM(font.0 as usize)), Some(LPARAM(1)));
        hwnd
    }
}

/// Buang kontrol bagian lama dan buat kontrol untuk bagian yang dipilih.
fn build_section(hwnd: HWND) {
    extern "system" fn destroy(child: HWND, _: LPARAM) -> BOOL {
        if unsafe { GetDlgCtrlID(child) } >= ID_ITEM {
            let _ = unsafe { DestroyWindow(child) };
        }
        TRUE
    }
    UI.with_borrow(|ui| {
        let Some(ui) = ui else { return };
        unsafe {
            let _ = EnumChildWindows(Some(hwnd), Some(destroy), LPARAM(0));
        }
        let (x, w) = (176.0, 460.0);
        let mut y = 16.0;
        let sec = &ui.sections[ui.current];
        let mk = |class: PCWSTR, text: &str, style: WINDOW_STYLE, r: [f32; 4], id: i32| {
            let t = wide(text);
            child(hwnd, class, PCWSTR(t.as_ptr()), style, r, id, ui.scale, ui.font)
        };
        mk(w!("STATIC"), sec.name, WINDOW_STYLE(0), [x, y, w, 24.0], ID_ITEM + 999);
        y += 34.0;
        for (i, item) in sec.items.iter().enumerate() {
            let id = ID_ITEM + i as i32;
            match item {
                Item::Check(key, label, default) => {
                    let c = mk(w!("BUTTON"), label, WINDOW_STYLE(BS_AUTOCHECKBOX as u32), [x, y, w, 24.0], id);
                    let on = dword(key, *default as u32) != 0;
                    unsafe { SendMessageW(c, BM_SETCHECK, Some(WPARAM(on as usize)), None) };
                    y += 30.0;
                }
                Item::Choice(key, label, options, default) => {
                    mk(w!("STATIC"), label, WINDOW_STYLE(0), [x, y + 3.0, 200.0, 22.0], ID_ITEM + 500 + i as i32);
                    let c = mk(w!("COMBOBOX"), "", WINDOW_STYLE((CBS_DROPDOWNLIST | WS_VSCROLL.0 as i32) as u32), [x + 210.0, y, 240.0, 300.0], id);
                    for o in options() {
                        let o = wide(&o);
                        unsafe { SendMessageW(c, CB_ADDSTRING, None, Some(LPARAM(o.as_ptr() as isize))) };
                    }
                    unsafe { SendMessageW(c, CB_SETCURSEL, Some(WPARAM(dword(key, *default) as usize)), None) };
                    y += 34.0;
                }
                Item::Text(key, label, secret_) => {
                    mk(w!("STATIC"), label, WINDOW_STYLE(0), [x, y, w, 22.0], ID_ITEM + 500 + i as i32);
                    let style = WS_BORDER | WINDOW_STYLE((ES_AUTOHSCROLL | if *secret_ { ES_PASSWORD } else { 0 }) as u32);
                    let value = if *secret_ { secret(key).unwrap_or_default() } else { string(key) };
                    mk(w!("EDIT"), &value, style, [x, y + 24.0, w - 10.0, 26.0], id);
                    y += 60.0;
                }
                Item::Movable(key, label, default, _) => {
                    let c = mk(w!("BUTTON"), label, WINDOW_STYLE(BS_AUTOCHECKBOX as u32), [x, y, 300.0, 24.0], id);
                    unsafe { SendMessageW(c, BM_SETCHECK, Some(WPARAM((dword(key, *default as u32) != 0) as usize)), None) };
                    mk(w!("BUTTON"), "\u{25B2}", WINDOW_STYLE(0), [x + 330.0, y - 1.0, 30.0, 24.0], ID_MOVE + i as i32 * 2);
                    mk(w!("BUTTON"), "\u{25BC}", WINDOW_STYLE(0), [x + 364.0, y - 1.0, 30.0, 24.0], ID_MOVE + i as i32 * 2 + 1);
                    y += 30.0;
                }
                Item::Note(text) => {
                    let lines = (text.len() as f32 / 70.0).ceil().max(1.0);
                    mk(w!("STATIC"), text, WINDOW_STYLE(0), [x, y, w - 10.0, 20.0 * lines], ID_ITEM + 500 + i as i32);
                    y += 20.0 * lines + 8.0;
                }
            }
        }
    });
}

fn changed() {
    let notch = NOTCH.load(Relaxed);
    if notch != 0 {
        let _ = unsafe { PostMessageW(Some(HWND(notch as _)), WM_APP_SETTINGS, WPARAM(0), LPARAM(0)) };
    }
}

fn on_command(id: i32, code: u32, control: HWND) {
    // tombol naik/turun: pindahkan item, lalu susun ulang daftar bagian dan kontrolnya
    if id >= ID_MOVE && code == BN_CLICKED as u32 {
        let (index, dir) = (((id - ID_MOVE) / 2) as usize, if (id - ID_MOVE) % 2 == 0 { -1 } else { 1 });
        let hwnd = UI.with_borrow_mut(|ui| {
            let ui = ui.as_mut()?;
            if let Some(Item::Movable(.., mover)) = ui.sections[ui.current].items.get(index) {
                mover(index, dir);
            }
            ui.sections = SECTIONS.get()?();
            Some(ui.hwnd)
        });
        if let Some(hwnd) = hwnd {
            build_section(hwnd);
            changed();
        }
        return;
    }
    if id == ID_LIST && code == LBN_SELCHANGE {
        let sel = unsafe { SendMessageW(control, LB_GETCURSEL, None, None) }.0;
        let hwnd = UI.with_borrow_mut(|ui| {
            let ui = ui.as_mut()?;
            ui.current = (sel.max(0) as usize).min(ui.sections.len() - 1);
            Some(ui.hwnd)
        });
        if let Some(hwnd) = hwnd {
            build_section(hwnd);
        }
        return;
    }
    let saved = UI.with_borrow(|ui| {
        let ui = ui.as_ref()?;
        let item = ui.sections[ui.current].items.get((id - ID_ITEM) as usize)?;
        unsafe {
            match item {
                Item::Check(key, ..) | Item::Movable(key, ..) if code == BN_CLICKED as u32 => {
                    set_dword(key, (SendMessageW(control, BM_GETCHECK, None, None).0 == 1) as u32);
                }
                Item::Choice(key, ..) if code == CBN_SELCHANGE => {
                    set_dword(key, SendMessageW(control, CB_GETCURSEL, None, None).0.max(0) as u32);
                }
                Item::Text(key, _, secret_) if code == EN_KILLFOCUS => {
                    let mut buf = vec![0u16; GetWindowTextLengthW(control) as usize + 1];
                    let n = GetWindowTextW(control, &mut buf) as usize;
                    let value = String::from_utf16_lossy(&buf[..n]);
                    let value = value.trim();
                    if *secret_ { set_secret(key, value) } else { set_string(key, value) }
                }
                _ => return None,
            }
        }
        Some(())
    });
    if saved.is_some() {
        changed();
    }
}

extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_COMMAND => {
                on_command((wp.0 & 0xffff) as i32, (wp.0 >> 16) as u32, HWND(lp.0 as _));
                LRESULT(0)
            }
            // label dan kotak centang memakai latar jendela, bukan abu-abu bawaan
            WM_CTLCOLORSTATIC | WM_CTLCOLORBTN => {
                SetBkColor(HDC(wp.0 as _), COLORREF(GetSysColor(COLOR_WINDOW)));
                LRESULT(GetSysColorBrush(COLOR_WINDOW).0 as isize)
            }
            // kotak teks rahasia disimpan saat jendela ditutup juga
            WM_CLOSE => {
                let _ = SetFocus(Some(hwnd));
                let _ = DestroyWindow(hwnd);
                LRESULT(0)
            }
            WM_DESTROY => {
                if let Some(ui) = UI.take() {
                    let _ = DeleteObject(ui.font.into());
                }
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wp, lp),
        }
    }
}
