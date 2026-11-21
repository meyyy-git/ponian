//! Ikon tray dan menunya. Semua opsi lain ada di jendela Pengaturan.
use crate::WM_APP_TRAY;
use windows::{
    Win32::{Foundation::*, UI::Shell::*, UI::WindowsAndMessaging::*},
    core::*,
};

pub const CMD_SETTINGS: i32 = 1;
pub const CMD_HIDE: i32 = 2;
pub const CMD_QUIT: i32 = 3;

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
pub fn menu(hwnd: HWND, hidden: bool) -> i32 {
    unsafe {
        let Ok(menu) = CreatePopupMenu() else { return 0 };
        let _ = AppendMenuW(menu, MF_STRING, CMD_SETTINGS as usize, w!("Pengaturan\u{2026}"));
        let _ = AppendMenuW(menu, MF_STRING | if hidden { MF_CHECKED } else { MF_UNCHECKED }, CMD_HIDE as usize, w!("Sembunyikan notch"));
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
