//! Jam dunia memakai data zona waktu Windows, jadi pergantian musim panas (DST) ikut benar.
use std::{cell::RefCell, collections::HashMap};
use windows::Win32::{
    Foundation::SYSTEMTIME,
    System::{
        SystemInformation::GetSystemTime,
        Time::{DYNAMIC_TIME_ZONE_INFORMATION, EnumDynamicTimeZoneInformation, SystemTimeToTzSpecificLocalTimeEx},
    },
};

/// (nama kota, nama kunci zona waktu Windows)
pub const CITIES: [(&str, &str); 16] = [
    ("Jakarta", "SE Asia Standard Time"),
    ("Makassar", "Singapore Standard Time"),
    ("Jayapura", "Tokyo Standard Time"),
    ("Singapore","Singapore Standard Time"),
    ("Kuala Lumpur", "Singapore Standard Time"),
    ("Tokyo", "Tokyo Standard Time"),
    ("Seoul", "Korea Standard Time"),
    ("Beijing", "China Standard Time"),
    ("Mecca","Arab Standard Time"),
    ("Dubai", "Arabian Standard Time"),
    ("London", "GMT Standard Time"),
    ("Paris", "Romance Standard Time"),
    ("Berlin", "W. Europe Standard Time"),
    ("New York", "Eastern Standard Time"),
    ("Los Angeles", "Pacific Standard Time"),
    ("Sydney", "AUS Eastern Standard Time"),
];

pub fn names() -> Vec<String> {
    CITIES.iter().map(|c| c.0.to_string()).collect()
}

thread_local! {
    static ZONES: RefCell<HashMap<usize, Option<DYNAMIC_TIME_ZONE_INFORMATION>>> = RefCell::new(HashMap::new());
}

fn zone(key: &str) -> Option<DYNAMIC_TIME_ZONE_INFORMATION> {
    for index in 0.. {
        let mut tz = DYNAMIC_TIME_ZONE_INFORMATION::default();
        if unsafe { EnumDynamicTimeZoneInformation(index, &mut tz) } != 0 {
            return None;
        }
        // Windows tidak membersihkan sisa buffer setelah NUL, jadi potong di NUL pertama
        let name = &tz.TimeZoneKeyName;
        let end = name.iter().position(|&c| c == 0).unwrap_or(name.len());
        if String::from_utf16_lossy(&name[..end]) == key {
            return Some(tz);
        }
    }
    None
}

/// Jam dan menit sekarang di kota ke-`i`.
pub fn time(i: usize) -> Option<(u16, u16)> {
    let key = CITIES.get(i)?.1;
    let tz = ZONES.with_borrow_mut(|z| *z.entry(i).or_insert_with(|| zone(key)))?;
    let utc = unsafe { GetSystemTime() };
    let mut local = SYSTEMTIME::default();
    unsafe { SystemTimeToTzSpecificLocalTimeEx(Some(&tz), &utc, &mut local) }.ok()?;
    Some((local.wHour, local.wMinute))
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_city_has_a_windows_time_zone() {
        for (i, c) in super::CITIES.iter().enumerate() {
            assert!(super::time(i).is_some(), "zona waktu {} ({}) tidak ditemukan", c.0, c.1);
        }
    }
}
