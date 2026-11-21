//! Widget kecil: dua di halaman Widget, dipilih di Pengaturan.
use crate::{
    App, astro, calendar, clock, currency, online, prayer,
    render::{ACCENT, GRAY, Gfx, R, WHITE},
    weather, worldclock,
};

// indeks 4 (Tanggal) ditangani cabang `_` di fungsi widget
pub const WIDGETS: [&str; 7] = ["World clock", "Countdown", "Moon", "Next prayer", "Date", "Weather", "Exchange rates"];

pub fn names() -> Vec<String> {
    WIDGETS.iter().map(|s| s.to_string()).collect()
}

pub fn draw(app: &App, g: &Gfx, [l, t, r, b]: R) {
    let mid = (l + r) / 2.0;
    widget(app, g, app.widgets.0, [l, t, mid - 5.0, b]);
    widget(app, g, app.widgets.1, [mid + 5.0, t, r, b]);
}

const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
const HIJRI: [&str; 12] = ["Muharram", "Safar", "Rabi I", "Rabi II", "Jumada I", "Jumada II", "Rajab", "Shaban", "Ramadan", "Shawwal", "Dhu al-Qadah", "Dhu al-Hijjah"];

fn date(day: i64) -> String {
    let (y, m, d) = clock::civil_from_days(day);
    format!("{d} {} {y}", MONTHS[m as usize - 1])
}

/// Acara hitung mundur: buatan user di Pengaturan, lalu tanggal merah berikutnya dari kalender hari
/// libur; tanpa keduanya, hari besar terdekat dari daftar bawaan (tanggal Islam hanya perkiraan).
pub fn countdown(today: i64, custom: &str, holiday: Option<(String, i64)>) -> (String, i64) {
    if let Some((d, name)) = custom.split_once(' ')
        && let Some(day) = clock::parse_key(d)
        && day >= today
    {
        return (name.trim().to_string(), day);
    }
    if let Some(holiday) = holiday {
        return holiday;
    }
    let (y, ..) = clock::civil_from_days(today);
    let fixed = |m, d, name: &str| {
        let day = clock::days_from_civil(y, m, d);
        (name.to_string(), if day >= today { day } else { clock::days_from_civil(y + 1, m, d) })
    };
    [
        fixed(1, 1, "New Year\u{2019}s Day"),
        fixed(8, 17, "Independence Day"),
        fixed(12, 25, "Christmas"),
        ("Start of Ramadan (estimated)".into(), astro::next_hijri(today, 9, 1)),
        ("Eid al-Fitr (estimated)".into(), astro::next_hijri(today, 10, 1)),
        ("Eid al-Adha (estimated)".into(), astro::next_hijri(today, 12, 10)),
    ]
    .into_iter()
    .min_by_key(|e| e.1)
    .unwrap_or(("".into(), today))
}

fn widget(app: &App, g: &Gfx, kind: usize, [l, t, r, b]: R) {
    g.rrect([l, t, r, b], 14.0, 0x161616);
    let (l, t, r, b) = (l + 12.0, t + 8.0, r - 12.0, b - 8.0);
    let now = clock::now();
    let title = |icon: &str, s: &str| {
        g.text(icon, &g.f.icon_small, [l - 1.0, t, l + 13.0, t + 18.0], ACCENT);
        g.text(s, &g.f.small, [l + 18.0, t, r, t + 18.0], GRAY);
    };
    match kind {
        0 => {
            title("\u{E774}", "WORLD CLOCK");
            for (row, city) in [app.cities.0, app.cities.1].into_iter().enumerate() {
                let y = t + 24.0 + row as f32 * 32.0;
                let name = worldclock::CITIES.get(city).map_or("", |c| c.0);
                g.text(name, &g.f.text, [l, y, r - 50.0, y + 28.0], WHITE);
                let time = worldclock::time(city).map_or("--:--".into(), |(h, m)| format!("{h:02}:{m:02}"));
                g.text(&time, &g.f.rbold, [r - 60.0, y, r, y + 28.0], WHITE);
            }
        }
        1 => {
            title("\u{E787}", "COUNTDOWN");
            let holiday = calendar::next_holiday(&online::HOLIDAYS.lock().unwrap(), now.day).map(|(name, day)| (name.to_string(), day));
            let (name, day) = countdown(now.day, &app.countdown, holiday);
            let left = day - now.day;
            g.text(&if left == 0 { "Today".into() } else { format!("{left} day{}", if left == 1 { "" } else { "s" }) }, &g.f.big, [l, t + 18.0, r, t + 56.0], WHITE);
            g.text(&name, &g.f.text, [l, t + 56.0, r, t + 74.0], WHITE);
            g.text(&date(day), &g.f.small, [l, t + 72.0, r, b], GRAY);
        }
        2 => {
            title("\u{E708}", "MOON");
            let utc = now.day as f64 + (now.min as f64 - now.offset as f64) / 1440.0;
            let age = astro::moon_age(utc);
            let lit = astro::illumination(age);
            // lingkaran gelap dengan bagian terang sebanding iluminasi
            let (cx, cy, rad) = (r - 24.0, t + 44.0, 20.0);
            g.ring(cx, cy, rad - 1.0, 2.0, 1.0, 0x3a3a3a, 0x3a3a3a);
            g.ring(cx, cy, rad - 1.0, 2.0, lit as f32, 0xf5e6b8, 0x3a3a3a);
            g.text(astro::phase_name(age), &g.f.bold, [l, t + 22.0, r - 50.0, t + 44.0], WHITE);
            g.text(&format!("{:.0}% lit", lit * 100.0), &g.f.small, [l, t + 44.0, r - 50.0, t + 62.0], GRAY);
            let full = (astro::next_full_moon(utc) - utc).ceil() as i64;
            g.text(&if full <= 1 { "Full moon tonight".into() } else { format!("Full moon in {full} days") }, &g.f.small, [l, t + 62.0, r, b], GRAY);
        }
        3 => {
            title("\u{E823}", "NEXT PRAYER");
            match app.prayer {
                Some(times) => {
                    let (n, wait) = super::prayer::next(&times, now.min);
                    g.text(prayer::NAMES[n], &g.f.bold, [l, t + 22.0, r, t + 44.0], WHITE);
                    g.text(&clock::hhmm(times[n]), &g.f.big, [l, t + 42.0, r, t + 78.0], WHITE);
                    g.text(&format!("in {}", clock::span(wait)), &g.f.small, [l, t + 76.0, r, b], GRAY);
                }
                None => g.text("Set a location in Settings", &g.f.small, [l, t + 22.0, r, b], GRAY),
            }
        }
        5 => {
            title("\u{E753}", "WEATHER");
            match online::WEATHER.lock().unwrap().clone() {
                Some(w) => {
                    let (desc, icon) = weather::describe(&w.symbol);
                    g.text(icon, &g.f.big, [r - 44.0, t + 18.0, r, t + 56.0], WHITE);
                    g.text(&format!("{:.0}\u{B0}", w.temp), &g.f.big, [l, t + 18.0, r - 36.0, t + 56.0], WHITE);
                    g.text(desc, &g.f.text, [l, t + 56.0, r, t + 74.0], WHITE);
                    g.text(&format!("{:.0}\u{B0} \u{2013} {:.0}\u{B0} \u{B7} MET Norway", w.min, w.max), &g.f.small, [l, t + 72.0, r, b], GRAY);
                }
                None => g.text(if app.prayer.is_some() { "Loading weather\u{2026}" } else { "Set a location in Settings" }, &g.f.small, [l, t + 22.0, r, b], GRAY),
            }
        }
        6 => {
            title("\u{E8AB}", "EXCHANGE RATES");
            let main = currency::MAIN[app.rates_main];
            match online::RATES.lock().unwrap().clone() {
                Some(rates) => {
                    for (row, code) in currency::MAIN.iter().filter(|c| **c != main).take(3).enumerate() {
                        let y = t + 22.0 + row as f32 * 22.0;
                        g.text(&format!("1 {code}"), &g.f.text, [l, y, l + 60.0, y + 22.0], GRAY);
                        let v = currency::convert(&rates, 1.0, code, main).map_or("-".into(), |v| currency::format(v, main));
                        g.text(&v, &g.f.rbold, [l + 50.0, y, r, y + 22.0], WHITE);
                    }
                }
                None => g.text("Loading rates\u{2026}", &g.f.small, [l, t + 22.0, r, b], GRAY),
            }
        }
        _ => {
            let (y, m, d) = astro::hijri(now.day);
            g.text(["SUNDAY", "MONDAY", "TUESDAY", "WEDNESDAY", "THURSDAY", "FRIDAY", "SATURDAY"][clock::weekday(now.day) as usize], &g.f.small, [l, t, r, t + 18.0], ACCENT);
            let (_, gm, gd) = clock::civil_from_days(now.day);
            g.text(&format!("{gd} {}", MONTHS[gm as usize - 1]), &g.f.big, [l, t + 18.0, r, t + 56.0], WHITE);
            g.text(&format!("{d} {} {y} AH", HIJRI[(m - 1).clamp(0, 11) as usize]), &g.f.small, [l, t + 58.0, r, b], GRAY);
        }
    }
}
