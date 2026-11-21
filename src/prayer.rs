//! Jadwal salat dihitung di komputer sendiri dari posisi matahari (rumus umum ala PrayTimes).

pub enum Isha {
    Angle(f64),
    /// menit setelah Magrib
    Minutes(f64),
}

pub struct Method {
    pub name: &'static str,
    pub fajr: f64,
    pub isha: Isha,
    /// menit pengaman (ihtiyat) yang ditambahkan ke setiap waktu
    pub ihtiyat: f64,
}

pub const METHODS: [Method; 8] = [
    Method { name: "Kemenag RI", fajr: 20.0, isha: Isha::Angle(18.0), ihtiyat: 2.0 },
    Method { name: "JAKIM (Malaysia)", fajr: 20.0, isha: Isha::Angle(18.0), ihtiyat: 0.0 },
    Method { name: "MUIS (Singapore)", fajr: 20.0, isha: Isha::Angle(18.0), ihtiyat: 0.0 },
    Method { name: "Muslim World League", fajr: 18.0, isha: Isha::Angle(17.0), ihtiyat: 0.0 },
    Method { name: "Umm al-Qura", fajr: 18.5, isha: Isha::Minutes(90.0), ihtiyat: 0.0 },
    Method { name: "Egypt", fajr: 19.5, isha: Isha::Angle(17.5), ihtiyat: 0.0 },
    Method { name: "Karachi", fajr: 18.0, isha: Isha::Angle(18.0), ihtiyat: 0.0 },
    Method { name: "ISNA (North America)", fajr: 15.0, isha: Isha::Angle(15.0), ihtiyat: 0.0 },
];

pub const NAMES: [&str; 6] = ["Fajr", "Sunrise", "Dhuhr", "Asr", "Maghrib", "Isha"];

fn sin(d: f64) -> f64 {
    d.to_radians().sin()
}
fn cos(d: f64) -> f64 {
    d.to_radians().cos()
}
fn tan(d: f64) -> f64 {
    d.to_radians().tan()
}
fn fix(a: f64, b: f64) -> f64 {
    a.rem_euclid(b)
}

/// (deklinasi matahari, persamaan waktu dalam jam) untuk tanggal Julian `jd`.
fn sun(jd: f64) -> (f64, f64) {
    let d = jd - 2_451_545.0;
    let g = fix(357.529 + 0.985_600_28 * d, 360.0);
    let q = fix(280.459 + 0.985_647_36 * d, 360.0);
    let l = fix(q + 1.915 * sin(g) + 0.020 * sin(2.0 * g), 360.0);
    let e = 23.439 - 0.000_000_36 * d;
    let ra = fix((cos(e) * sin(l)).atan2(cos(l)).to_degrees() / 15.0, 24.0);
    let decl = (sin(e) * sin(l)).asin().to_degrees();
    (decl, q / 15.0 - ra)
}

/// Waktu salat dalam jam lokal (bisa pecahan), urut seperti `NAMES`.
pub fn times(y: i64, m: u32, d: u32, lat: f64, lon: f64, tz_hours: f64, method: &Method, hanafi: bool) -> [f64; 6] {
    let (yy, mm) = if m <= 2 { (y - 1, m + 12) } else { (y, m) };
    let a = (yy as f64 / 100.0).floor();
    let b = 2.0 - a + (a / 4.0).floor();
    let jd = (365.25 * (yy as f64 + 4716.0)).floor() + (30.6001 * (mm as f64 + 1.0)).floor() + d as f64 + b - 1524.5 - lon / (15.0 * 24.0);

    let noon = |t: f64| fix(12.0 - sun(jd + t).1, 24.0);
    let angle_time = |angle: f64, t: f64, before_noon: bool| {
        let decl = sun(jd + t).0;
        let x = ((-sin(angle) - sin(decl) * sin(lat)) / (cos(decl) * cos(lat))).clamp(-1.0, 1.0);
        let span = x.acos().to_degrees() / 15.0;
        noon(t) + if before_noon { -span } else { span }
    };
    let asr = |factor: f64, t: f64| {
        let decl = sun(jd + t).0;
        let angle = -(1.0 / (factor + tan((lat - decl).abs()))).atan().to_degrees();
        angle_time(angle, t, false)
    };

    // satu iterasi dari tebakan awal sudah cukup teliti untuk ketepatan menit
    let guess = [5.0, 6.0, 12.0, 13.0, 18.0, 18.0].map(|h| h / 24.0);
    let sunset = angle_time(0.833, guess[4], false);
    let mut t = [
        angle_time(method.fajr, guess[0], true),
        angle_time(0.833, guess[1], true),
        noon(guess[2]),
        asr(if hanafi { 2.0 } else { 1.0 }, guess[3]),
        sunset,
        match method.isha {
            Isha::Angle(a) => angle_time(a, guess[5], false),
            Isha::Minutes(min) => sunset + min / 60.0,
        },
    ];
    let shift = tz_hours - lon / 15.0;
    for (i, v) in t.iter_mut().enumerate() {
        // ihtiyat tidak diterapkan ke waktu terbit, yang justru batas akhir Subuh
        let safety = if i == 1 { -method.ihtiyat } else { method.ihtiyat };
        *v += shift + safety / 60.0;
    }
    t
}

/// Jam pecahan -> menit sejak tengah malam, dibulatkan ke atas seperti jadwal resmi.
/// Waktu salat yang pengingatnya sedang jatuh tempo pada menit `now`: mulai `before` menit sebelum
/// waktunya sampai 10 menit sesudahnya. Jendela itu membuat pengingat tidak terlewat saat komputer tidur,
/// Ponian baru dibuka, atau menitnya bertepatan dengan pengingat lain. Indeks 1 (terbit) dilewati.
pub fn due(times: &[u32; 6], now: u32, before: u32) -> Option<usize> {
    [5, 4, 3, 2, 0].into_iter().find(|&i| {
        let from = times[i].saturating_sub(before);
        now >= from && now - from <= before + 10
    })
}

pub fn minutes(h: f64) -> u32 {
    (fix(h, 24.0) * 60.0).ceil() as u32 % 1440
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(y: i64, m: u32, d: u32, expected: [&str; 6]) {
        // Kota Jakarta, dibandingkan dengan jadwal Kemenag dari api.myquran.com
        let t = times(y, m, d, -6.1754, 106.8272, 7.0, &METHODS[0], false);
        for (i, want) in expected.iter().enumerate() {
            let (h, mm) = want.split_once(':').unwrap();
            let want = h.parse::<i64>().unwrap() * 60 + mm.parse::<i64>().unwrap();
            let got = minutes(t[i]) as i64;
            assert!((got - want).abs() <= 2, "{} {y}-{m}-{d}: hitung {}, Kemenag {}", NAMES[i], crate::clock::hhmm(got as u32), expected[i]);
        }
    }

    #[test]
    fn reminder_window() {
        // subuh 04:01, terbit 05:20, zuhur 11:38, asar 15:01, magrib 17:52, isya 19:06
        let t = [241, 320, 698, 901, 1072, 1146];
        assert_eq!(due(&t, 1071, 0), None);
        assert_eq!(due(&t, 1072, 0), Some(4));
        // masih diingatkan beberapa menit kemudian (komputer baru bangun, Ponian baru dibuka)
        assert_eq!(due(&t, 1082, 0), Some(4));
        assert_eq!(due(&t, 1083, 0), None);
        // terbit bukan waktu salat
        assert_eq!(due(&t, 320, 0), None);
        // mode "10 menit sebelumnya"
        assert_eq!(due(&t, 1135, 10), None);
        assert_eq!(due(&t, 1136, 10), Some(5));
        assert_eq!(due(&t, 1146, 10), Some(5));
    }

    #[test]
    fn matches_kemenag_jakarta() {
        check(2026, 11, 21, ["04:05", "05:23", "11:42", "15:04", "17:54", "19:08"]);
        check(2026, 6, 15, ["04:39", "05:57", "11:57", "15:18", "17:49", "19:04"]);
    }
}
