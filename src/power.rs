//! Status baterai dan deteksi app fullscreen.
use windows::Win32::{
    System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS},
    UI::Shell::{QUNS_BUSY, QUNS_PRESENTATION_MODE, QUNS_RUNNING_D3D_FULL_SCREEN, SHQueryUserNotificationState},
};

/// (sedang dicolok, persen). None di PC tanpa baterai.
pub fn status() -> Option<(bool, u8)> {
    let mut s = SYSTEM_POWER_STATUS::default();
    unsafe { GetSystemPowerStatus(&mut s).ok()? };
    // BatteryFlag 128 = tidak ada baterai, 255 = tidak diketahui
    if s.BatteryFlag & 128 != 0 || s.BatteryFlag == 255 || s.BatteryLifePercent > 100 {
        return None;
    }
    Some((s.ACLineStatus == 1, s.BatteryLifePercent))
}

/// Jendela yang sedang aktif di-maximize di layar yang sama dengan `notch`: tab atau judulnya ada
/// tepat di bawah pill.
pub fn covered(notch: windows::Win32::Foundation::HWND) -> bool {
    use windows::Win32::{
        Graphics::Gdi::{MONITOR_DEFAULTTONEAREST, MonitorFromWindow},
        UI::WindowsAndMessaging::{GetForegroundWindow, IsZoomed},
    };
    unsafe {
        let front = GetForegroundWindow();
        !front.is_invalid() && IsZoomed(front).as_bool() && MonitorFromWindow(front, MONITOR_DEFAULTTONEAREST) == MonitorFromWindow(notch, MONITOR_DEFAULTTONEAREST)
    }
}

pub fn fullscreen() -> bool {
    matches!(unsafe { SHQueryUserNotificationState() }, Ok(QUNS_BUSY | QUNS_RUNNING_D3D_FULL_SCREEN | QUNS_PRESENTATION_MODE))
}
