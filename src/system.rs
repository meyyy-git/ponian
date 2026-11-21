//! Indikator sistem: kecerahan, Caps Lock, bahasa input, Wi-Fi, dan perangkat audio.
use crate::WM_APP_CAPS;
use std::sync::{
    Mutex,
    atomic::{AtomicIsize, Ordering::Relaxed},
};
use windows::{
    Networking::Connectivity::NetworkInformation,
    Win32::{
        Foundation::*,
        Globalization::LCIDToLocaleName,
        System::{
            Power::{POWERBROADCAST_SETTING, RegisterPowerSettingNotification},
            SystemServices::GUID_DEVICE_POWER_POLICY_VIDEO_BRIGHTNESS,
        },
        UI::{
            Input::KeyboardAndMouse::{GetKeyState, GetKeyboardLayout, VK_CAPITAL},
            WindowsAndMessaging::*,
        },
    },
};

static NOTCH: AtomicIsize = AtomicIsize::new(0);

/// Daftarkan pemberitahuan kecerahan dan pemantau tombol Caps Lock.
pub fn register(hwnd: HWND) {
    NOTCH.store(hwnd.0 as isize, Relaxed);
    unsafe {
        let _ = RegisterPowerSettingNotification(HANDLE(hwnd.0), &GUID_DEVICE_POWER_POLICY_VIDEO_BRIGHTNESS, DEVICE_NOTIFY_WINDOW_HANDLE);
        // hook ini hanya memeriksa tombol Caps Lock; tombol lain langsung diteruskan tanpa dibaca
        let _ = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard), None, 0);
    }
}

extern "system" fn keyboard(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        if code >= 0 && wp.0 as u32 == WM_KEYUP {
            let info = &*(lp.0 as *const KBDLLHOOKSTRUCT);
            if info.vkCode == VK_CAPITAL.0 as u32 {
                let _ = PostMessageW(Some(HWND(NOTCH.load(Relaxed) as _)), WM_APP_CAPS, WPARAM(0), LPARAM(0));
            }
        }
        CallNextHookEx(None, code, wp, lp)
    }
}

pub fn caps_on() -> bool {
    (unsafe { GetKeyState(VK_CAPITAL.0 as i32) } & 1) != 0
}

/// Persen kecerahan dari WM_POWERBROADCAST / PBT_POWERSETTINGCHANGE.
pub fn brightness(lp: LPARAM) -> Option<u32> {
    let s = unsafe { (lp.0 as *const POWERBROADCAST_SETTING).as_ref()? };
    (s.PowerSetting == GUID_DEVICE_POWER_POLICY_VIDEO_BRIGHTNESS && s.DataLength >= 4).then(|| unsafe { std::ptr::read_unaligned(s.Data.as_ptr() as *const u32) })
}

/// (jendela aktif, layout keyboard-nya) untuk mendeteksi pergantian bahasa input.
pub fn input_layout() -> (isize, isize) {
    unsafe {
        let fg = GetForegroundWindow();
        let thread = GetWindowThreadProcessId(fg, None);
        (fg.0 as isize, GetKeyboardLayout(thread).0 as isize)
    }
}

/// "EN-US", "ID-ID" dari layout keyboard.
pub fn layout_name(hkl: isize) -> String {
    let mut buf = [0u16; 85];
    let n = unsafe { LCIDToLocaleName((hkl as u32) & 0xffff, Some(&mut buf), 0) } as usize;
    String::from_utf16_lossy(&buf[..n.saturating_sub(1)]).to_uppercase()
}

/// Status jaringan terakhir yang dibaca thread pekerja: None = belum dibaca, Some("") = tidak ada internet.
pub static NETWORK: Mutex<Option<String>> = Mutex::new(None);

/// Nama Wi-Fi (atau "Ethernet"), atau string kosong saat tidak ada internet. Panggil dari thread pekerja.
pub fn network() -> String {
    (|| -> windows::core::Result<String> {
        let Ok(p) = NetworkInformation::GetInternetConnectionProfile() else { return Ok(String::new()) };
        if p.IsWlanConnectionProfile()? {
            return Ok(p.WlanConnectionProfileDetails()?.GetConnectedSsid()?.to_string());
        }
        Ok("Ethernet".into())
    })()
    .unwrap_or_default()
}

/// Nama perangkat audio default terakhir, diisi thread pekerja.
pub static AUDIO_DEVICE: Mutex<String> = Mutex::new(String::new());
