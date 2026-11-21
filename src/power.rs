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

pub fn fullscreen() -> bool {
    matches!(unsafe { SHQueryUserNotificationState() }, Ok(QUNS_BUSY | QUNS_RUNNING_D3D_FULL_SCREEN | QUNS_PRESENTATION_MODE))
}
