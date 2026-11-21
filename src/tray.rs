//! Ikon tray, menunya, dan settings di registry.
use crate::WM_APP_TRAY;
use windows::{
    Win32::{Foundation::*, System::Registry::*, UI::Shell::*, UI::WindowsAndMessaging::*},
    core::*,
};

const KEY: PCWSTR = w!("Software\\Ponian");
const RUN: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const APP: PCWSTR = w!("Ponian");

pub const HIDE_NOTCH: PCWSTR = w!("HideNotch");
pub const SHOW_LYRICS: PCWSTR = w!("ShowLyrics");

pub const CMD_AUTOSTART: i32 = 1;
pub const CMD_LYRICS: i32 = 2;
pub const CMD_HIDE: i32 = 3;
pub const CMD_QUIT: i32 = 4;

pub fn get(name: PCWSTR, default: bool) -> bool {
    let (mut value, mut len) = (0u32, 4u32);
    let res = unsafe { RegGetValueW(HKEY_CURRENT_USER, KEY, name, RRF_RT_REG_DWORD, None, Some(&mut value as *mut _ as _), Some(&mut len)) };
    if res.is_ok() { value != 0 } else { default }
}

pub fn set(name: PCWSTR, on: bool) {
    let value = on as u32;
    let _ = unsafe { RegSetKeyValueW(HKEY_CURRENT_USER, KEY, name, REG_DWORD.0, Some(&value as *const _ as _), 4) };
}

pub fn autostart() -> bool {
    unsafe { RegGetValueW(HKEY_CURRENT_USER, RUN, APP, RRF_RT_REG_SZ, None, None, None) }.is_ok()
}

// shortcut: path exe dicatat saat opsi dinyalakan; kalau exe dipindah, opsi harus dinyalakan ulang.
pub fn set_autostart(on: bool) {
    unsafe {
        if !on {
            let _ = RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN, APP);
        } else if let Ok(exe) = std::env::current_exe() {
            let cmd: Vec<u16> = format!("\"{}\"\0", exe.display()).encode_utf16().collect();
            let _ = RegSetKeyValueW(HKEY_CURRENT_USER, RUN, APP, REG_SZ.0, Some(cmd.as_ptr() as _), (cmd.len() * 2) as u32);
        }
    }
}

fn icon_data(hwnd: HWND) -> NOTIFYICONDATAW {
    let mut d = NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: 1,
        uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
        uCallbackMessage: WM_APP_TRAY,
        hIcon: unsafe { LoadIconW(None, IDI_APPLICATION) }.unwrap_or_default(),
        ..Default::default()
    };
    for (dst, src) in d.szTip.iter_mut().zip("Ponian".encode_utf16()) {
        *dst = src;
    }
    d
}

pub fn add(hwnd: HWND) {
    let _ = unsafe { Shell_NotifyIconW(NIM_ADD, &icon_data(hwnd)) };
}

pub fn remove(hwnd: HWND) {
    let _ = unsafe { Shell_NotifyIconW(NIM_DELETE, &icon_data(hwnd)) };
}

/// Menampilkan menu di posisi kursor dan mengembalikan perintah yang dipilih (0 kalau batal).
pub fn menu(hwnd: HWND, lyrics: bool, hidden: bool) -> i32 {
    unsafe {
        let Ok(menu) = CreatePopupMenu() else { return 0 };
        let check = |on: bool| if on { MF_CHECKED } else { MF_UNCHECKED };
        let _ = AppendMenuW(menu, MF_STRING | check(autostart()), CMD_AUTOSTART as usize, w!("Jalankan saat startup"));
        let _ = AppendMenuW(menu, MF_STRING | check(lyrics), CMD_LYRICS as usize, w!("Tampilkan lirik"));
        let _ = AppendMenuW(menu, MF_STRING | check(hidden), CMD_HIDE as usize, w!("Sembunyikan notch"));
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, None);
        let _ = AppendMenuW(menu, MF_STRING, CMD_QUIT as usize, w!("Keluar"));
        let mut p = POINT::default();
        let _ = GetCursorPos(&mut p);
        // tanpa ini menu tidak menutup saat user klik di luar
        let _ = SetForegroundWindow(hwnd);
        let cmd = TrackPopupMenu(menu, TPM_RETURNCMD | TPM_RIGHTBUTTON, p.x, p.y, None, hwnd, None);
        let _ = DestroyMenu(menu);
        cmd.0
    }
}
