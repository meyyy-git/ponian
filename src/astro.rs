//! Fase bulan dan kalender Hijriah tabular (perkiraan, bisa beda 1-2 hari dari rukyat/sidang isbat).

const SYNODIC: f64 = 29.530_588_853;
/// Bulan baru 2000-01-06 18:14 UTC, dalam hari sejak 1970-01-01.
const NEW_MOON: f64 = 10_962.759_722;

/// Umur bulan (hari sejak bulan baru) pada `days` (hari sejak 1970, UTC, boleh pecahan).
pub fn moon_age(days: f64) -> f64 {
    (days - NEW_MOON).rem_euclid(SYNODIC)
}

/// Bagian bulan yang terang, 0..1.
pub fn illumination(age: f64) -> f64 {
    (1.0 - (age / SYNODIC * std::f64::consts::TAU).cos()) / 2.0
}

pub fn phase_name(age: f64) -> &'static str {
    const NAMES: [&str; 8] = ["New moon", "Waxing crescent", "First quarter", "Waxing gibbous", "Full moon", "Waning gibbous", "Last quarter", "Waning crescent"];
    NAMES[((age / SYNODIC * 8.0 + 0.5).floor() as usize) % 8]
}

/// Hari (sejak 1970, pecahan) purnama berikutnya setelah `days`.
pub fn next_full_moon(days: f64) -> f64 {
    let half = SYNODIC / 2.0;
    let age = moon_age(days);
    days + if age < half { half - age } else { SYNODIC - age + half }
}

/// Hari Julian (tengah hari) untuk hari sejak 1970.
fn jd(days: i64) -> i64 {
    days + 2_440_588
}

/// Tanggal Hijriah (tahun, bulan 1..12, hari) dari hari sejak 1970, kalender tabular.
pub fn hijri(days: i64) -> (i64, i64, i64) {
    let l = jd(days) - 1_948_440 + 10_632;
    let n = (l - 1) / 10_631;
    let l = l - 10_631 * n + 354;
    let j = ((10_985 - l) / 5_316) * ((50 * l) / 17_719) + (l / 5_670) * ((43 * l) / 15_238);
    let l = l - ((30 - j) / 15) * ((17_719 * j) / 50) - (j / 16) * ((15_238 * j) / 43) + 29;
    let m = (24 * l) / 709;
    let d = l - (709 * m) / 24;
    (30 * n + j - 30, m, d)
}

/// Kebalikan `hijri`.
pub fn from_hijri(y: i64, m: i64, d: i64) -> i64 {
    let jd = (11 * y + 3) / 30 + 354 * y + 30 * m - (m - 1) / 2 + d + 1_948_440 - 385;
    jd - 2_440_588
}

/// Hari besar Islam berikutnya (bulan, hari Hijriah) mulai `today`.
pub fn next_hijri(today: i64, month: i64, day: i64) -> i64 {
    let (y, ..) = hijri(today);
    (y..y + 2).map(|y| from_hijri(y, month, day)).find(|&d| d >= today).unwrap_or(today)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::days_from_civil;

    #[test]
    fn moon_phases() {
        // purnama 25 Jan 2024 17:54 UTC
        let full = days_from_civil(2024, 1, 25) as f64 + 17.9 / 24.0;
        assert!((moon_age(full) - SYNODIC / 2.0).abs() < 0.6, "umur {}", moon_age(full));
        assert!(illumination(moon_age(full)) > 0.99);
        assert_eq!(phase_name(moon_age(full)), "Full moon");
        // bulan baru 11 Jan 2024 11:57 UTC
        let new = days_from_civil(2024, 1, 11) as f64 + 0.5;
        assert!(illumination(moon_age(new)) < 0.01);
        let next = next_full_moon(new);
        assert!((next - full).abs() < 0.7, "purnama berikutnya {next}, seharusnya {full}");
    }

    #[test]
    fn hijri_dates() {
        let d = days_from_civil(2024, 3, 12);
        assert_eq!(hijri(d).0, 1445);
        // 1 Ramadan 1445 = 12 Mar 2024 (Indonesia); kalender tabular boleh meleset 2 hari
        assert!((from_hijri(1445, 9, 1) - d).abs() <= 2);
        // 1 Syawal 1445 = 10 Apr 2024
        assert!((from_hijri(1445, 10, 1) - days_from_civil(2024, 4, 10)).abs() <= 2);
        for z in [d, d + 100, d + 1000] {
            let (y, m, dd) = hijri(z);
            assert_eq!(from_hijri(y, m, dd), z);
        }
        assert!(next_hijri(d + 1, 9, 1) > d + 300);
    }
}
