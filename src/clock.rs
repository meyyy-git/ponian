//! Tanggal dan jam lokal tanpa crate tambahan.
use windows::Win32::System::SystemInformation::{GetLocalTime, GetSystemTime};

/// Hari sejak 1970-01-01 untuk tanggal kalender Gregorian (algoritma Howard Hinnant).
pub fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m as i64 + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

pub fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { yoe + era * 400 + 1 } else { yoe + era * 400 }, m, d)
}

/// 0 = Minggu
pub fn weekday(days: i64) -> u32 {
    (days + 4).rem_euclid(7) as u32
}

pub fn key(days: i64) -> String {
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02}")
}

pub fn parse_key(s: &str) -> Option<i64> {
    let mut it = s.trim().splitn(3, '-');
    let y = it.next()?.parse().ok()?;
    let m = it.next()?.parse().ok()?;
    let d = it.next()?.parse().ok()?;
    ((1..=12).contains(&m) && (1..=31).contains(&d)).then(|| days_from_civil(y, m, d))
}

#[derive(Clone, Copy, Debug)]
pub struct Now {
    /// hari lokal sejak 1970-01-01
    pub day: i64,
    /// menit sejak tengah malam lokal
    pub min: u32,

    /// selisih jam lokal terhadap UTC, dalam menit
    pub offset: i64,
}

pub fn now() -> Now {
    let (l, u) = unsafe { (GetLocalTime(), GetSystemTime()) };
    let day = days_from_civil(l.wYear as i64, l.wMonth as u32, l.wDay as u32);
    let utc_day = days_from_civil(u.wYear as i64, u.wMonth as u32, u.wDay as u32);
    let min = l.wHour as u32 * 60 + l.wMinute as u32;
    let utc_min = u.wHour as i64 * 60 + u.wMinute as i64;
    Now { day, min, offset: (day * 1440 + min as i64) - (utc_day * 1440 + utc_min) }
}

pub fn hhmm(min: u32) -> String {
    format!("{:02}:{:02}", (min / 60) % 24, min % 60)
}

/// "1 j 20 mnt", "45 mnt"
pub fn span(min: u32) -> String {
    match (min / 60, min % 60) {
        (0, m) => format!("{m} mnt"),
        (h, 0) => format!("{h} j"),
        (h, m) => format!("{h} j {m} mnt"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_roundtrip() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(2000, 3, 1), 11_017);
        for z in [-1000, 0, 11_017, 20_000, 20_778] {
            let (y, m, d) = civil_from_days(z);
            assert_eq!(days_from_civil(y, m, d), z);
        }
        assert_eq!(key(days_from_civil(2026, 11, 21)), "2026-11-21");
        assert_eq!(parse_key("2026-11-21"), Some(days_from_civil(2026, 11, 21)));
        assert_eq!(parse_key("2026-13-01"), None);
        // 21 Nov 2026 adalah hari Sabtu
        assert_eq!(weekday(days_from_civil(2026, 11, 21)), 6);
    }

    #[test]
    fn formats() {
        assert_eq!(hhmm(905), "15:05");
        assert_eq!(span(80), "1 j 20 mnt");
        assert_eq!(span(45), "45 mnt");
        assert_eq!(span(120), "2 j");
    }
}

/// Jam sekarang menurut format Windows milik user: ("3:18", "PM") atau ("15:18", "").
pub fn time_text() -> (String, String) {
    use windows::{
        Win32::Globalization::{GetTimeFormatEx, TIME_NOSECONDS},
        core::PCWSTR,
    };
    let mut buf = [0u16; 32];
    let n = unsafe { GetTimeFormatEx(PCWSTR::null(), TIME_NOSECONDS, None, PCWSTR::null(), Some(&mut buf)) } as usize;
    let s = String::from_utf16_lossy(&buf[..n.saturating_sub(1)]);
    match s.split_once(' ') {
        Some((time, ampm)) => (time.to_string(), ampm.to_string()),
        None => (s, String::new()),
    }
}
