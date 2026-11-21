//! Lokasi untuk jadwal salat: koordinat manual dari Pengaturan, atau lokasi Windows kalau diizinkan.
use crate::settings;
use windows::{
    Devices::Geolocation::{GeolocationAccessStatus, Geolocator},
    Win32::{
        Foundation::{HWND, LPARAM, WPARAM},
        System::Com::{COINIT_MULTITHREADED, CoInitializeEx},
        UI::WindowsAndMessaging::PostMessageW,
    },
};

/// "-6.2, 106.8" -> (-6.2, 106.8)
pub fn parse(s: &str) -> Option<(f64, f64)> {
    let (a, b) = s.split_once(',')?;
    let (lat, lon): (f64, f64) = (a.trim().parse().ok()?, b.trim().parse().ok()?);
    ((-90.0..=90.0).contains(&lat) && (-180.0..=180.0).contains(&lon)).then_some((lat, lon))
}

/// Koordinat yang dipakai sekarang. Koordinat manual menang atas lokasi Windows.
pub fn current() -> Option<(f64, f64)> {
    parse(&settings::string("Location")).or_else(|| parse(&settings::string("WindowsLocation")))
}

/// Minta lokasi Windows di thread terpisah, simpan, lalu beri tahu window notch.
pub fn refresh(notch: isize) {
    if !settings::flag("UseWindowsLocation", true) {
        return;
    }
    std::thread::spawn(move || {
        let found = (|| -> windows::core::Result<(f64, f64)> {
            unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).ok()? };
            if Geolocator::RequestAccessAsync()?.join()? != GeolocationAccessStatus::Allowed {
                return Err(windows::core::Error::empty());
            }
            let p = Geolocator::new()?.GetGeopositionAsync()?.join()?.Coordinate()?.Point()?.Position()?;
            Ok((p.Latitude, p.Longitude))
        })();
        // dibulatkan ke 2 desimal (sekitar 1 km) supaya geseran kecil tidak memicu muat ulang
        if let Ok((lat, lon)) = found
            && let text = format!("{lat:.2}, {lon:.2}")
            && text != settings::string("WindowsLocation")
        {
            settings::set_string("WindowsLocation", &text);
            let _ = unsafe { PostMessageW(Some(HWND(notch as _)), settings::WM_APP_SETTINGS, WPARAM(0), LPARAM(0)) };
        }
    });
}

#[cfg(test)]
mod tests {
    #[test]
    fn parses_coordinates() {
        assert_eq!(super::parse("-6.2088, 106.8456"), Some((-6.2088, 106.8456)));
        assert_eq!(super::parse("91, 0"), None);
        assert_eq!(super::parse("Jakarta"), None);
    }
}
