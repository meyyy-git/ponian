//! Widget kecil: dua di halaman Widget, dipilih di Pengaturan.
use crate::{
    App, astro, clock, prayer,
    render::{ACCENT, DIM, GRAY, Gfx, R, WHITE},
    store, water, worldclock,
};

pub const WIDGETS: [&str; 8] = ["Jam dunia", "Hitung mundur", "Bulan", "Minggu ini", "Salat berikutnya", "Fokus hari ini", "Air hari ini", "Tanggal"];

pub fn names() -> Vec<String> {
    WIDGETS.iter().map(|s| s.to_string()).collect()
}

pub fn draw(app: &App, g: &Gfx, [l, t, r, b]: R) {
    let mid = (l + r) / 2.0;
    widget(app, g, app.widgets.0, [l, t, mid - 5.0, b]);
    widget(app, g, app.widgets.1, [mid + 5.0, t, r, b]);
}

const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "Mei", "Jun", "Jul", "Agu", "Sep", "Okt", "Nov", "Des"];
const HIJRI: [&str; 12] = ["Muharam", "Safar", "Rabiulawal", "Rabiulakhir", "Jumadilawal", "Jumadilakhir", "Rajab", "Syakban", "Ramadan", "Syawal", "Zulkaidah", "Zulhijah"];

fn date(day: i64) -> String {
    let (y, m, d) = clock::civil_from_days(day);
    format!("{d} {} {y}", MONTHS[m as usize - 1])
}

/// Acara hitung mundur: buatan user di Pengaturan, atau hari besar terdekat.
pub fn countdown(today: i64) -> (String, i64) {
    let custom = crate::settings::string("Countdown");
    if let Some((d, name)) = custom.split_once(' ')
        && let Some(day) = clock::parse_key(d)
        && day >= today
    {
        return (name.trim().to_string(), day);
    }
    let (y, ..) = clock::civil_from_days(today);
    let fixed = |m, d, name: &str| {
        let day = clock::days_from_civil(y, m, d);
        (name.to_string(), if day >= today { day } else { clock::days_from_civil(y + 1, m, d) })
    };
    [
        fixed(1, 1, "Tahun Baru"),
        fixed(8, 17, "Hari Kemerdekaan"),
        fixed(12, 25, "Natal"),
        ("Awal Ramadan (perkiraan)".into(), astro::next_hijri(today, 9, 1)),
        ("Idulfitri (perkiraan)".into(), astro::next_hijri(today, 10, 1)),
        ("Iduladha (perkiraan)".into(), astro::next_hijri(today, 12, 10)),
    ]
    .into_iter()
    .min_by_key(|e| e.1)
    .unwrap_or(("".into(), today))
}

fn widget(app: &App, g: &Gfx, kind: usize, [l, t, r, b]: R) {
    g.rrect([l, t, r, b], 14.0, 0x161616);
    let (l, t, r, b) = (l + 12.0, t + 8.0, r - 12.0, b - 8.0);
    let now = clock::now();
    let title = |s: &str| g.text(s, &g.f.small, [l, t, r, t + 18.0], GRAY);
    match kind {
        0 => {
            title("JAM DUNIA");
            for (row, city) in [app.cities.0, app.cities.1].into_iter().enumerate() {
                let y = t + 24.0 + row as f32 * 32.0;
                let name = worldclock::CITIES.get(city).map_or("", |c| c.0);
                g.text(name, &g.f.text, [l, y, r - 50.0, y + 28.0], WHITE);
                let time = worldclock::time(city).map_or("--:--".into(), |(h, m)| format!("{h:02}:{m:02}"));
                g.text(&time, &g.f.rbold, [r - 60.0, y, r, y + 28.0], WHITE);
            }
        }
        1 => {
            title("HITUNG MUNDUR");
            let (name, day) = countdown(now.day);
            let left = day - now.day;
            g.text(&if left == 0 { "Hari ini".into() } else { format!("{left} hari") }, &g.f.big, [l, t + 18.0, r, t + 56.0], WHITE);
            g.text(&name, &g.f.text, [l, t + 56.0, r, t + 74.0], WHITE);
            g.text(&date(day), &g.f.small, [l, t + 72.0, r, b], GRAY);
        }
        2 => {
            title("BULAN");
            let utc = now.day as f64 + (now.min as f64 - now.offset as f64) / 1440.0;
            let age = astro::moon_age(utc);
            let lit = astro::illumination(age);
            // lingkaran gelap dengan bagian terang sebanding iluminasi
            let (cx, cy, rad) = (r - 24.0, t + 44.0, 20.0);
            g.ring(cx, cy, rad - 1.0, 2.0, 1.0, 0x3a3a3a, 0x3a3a3a);
            g.ring(cx, cy, rad - 1.0, 2.0, lit as f32, 0xf5e6b8, 0x3a3a3a);
            g.text(astro::phase_name(age), &g.f.bold, [l, t + 22.0, r - 50.0, t + 44.0], WHITE);
            g.text(&format!("{:.0}% terang", lit * 100.0), &g.f.small, [l, t + 44.0, r - 50.0, t + 62.0], GRAY);
            let full = (astro::next_full_moon(utc) - utc).ceil() as i64;
            g.text(&if full <= 1 { "Purnama malam ini".into() } else { format!("Purnama dalam {full} hari") }, &g.f.small, [l, t + 62.0, r, b], GRAY);
        }
        3 => {
            title("FOKUS 7 HARI");
            let log = store::daily("focus.txt");
            let days: Vec<u32> = (0..7).map(|i| log.iter().find(|e| e.0 == now.day - 6 + i).map_or(0, |e| e.1)).collect();
            let max = days.iter().copied().max().unwrap_or(0).max(1);
            let w = (r - l) / 7.0;
            for (i, v) in days.iter().enumerate() {
                let x = l + i as f32 * w + 3.0;
                let h = 44.0 * *v as f32 / max as f32;
                g.rrect([x, t + 70.0 - h.max(3.0), x + w - 6.0, t + 70.0], 2.5, if i == 6 { ACCENT } else { DIM });
                let wd = ["M", "S", "S", "R", "K", "J", "S"][clock::weekday(now.day - 6 + i as i64) as usize];
                g.text(wd, &g.f.csmall, [x - 2.0, t + 72.0, x + w - 4.0, b], GRAY);
            }
        }
        4 => {
            title("SALAT BERIKUTNYA");
            match app.prayer {
                Some(times) => {
                    let (n, wait) = super::prayer::next(&times, now.min);
                    g.text(prayer::NAMES[n], &g.f.bold, [l, t + 22.0, r, t + 44.0], WHITE);
                    g.text(&clock::hhmm(times[n]), &g.f.big, [l, t + 42.0, r, t + 78.0], WHITE);
                    g.text(&format!("dalam {}", clock::span(wait)), &g.f.small, [l, t + 76.0, r, b], GRAY);
                }
                None => g.text("Atur lokasi di Pengaturan", &g.f.small, [l, t + 22.0, r, b], GRAY),
            }
        }
        5 => {
            title("FOKUS HARI INI");
            let min = store::daily_get("focus.txt", now.day);
            g.text(&clock::span(min), &g.f.big, [l, t + 22.0, r, t + 60.0], WHITE);
            let sessions = min / app.durations.focus.max(1) as u32;
            g.text(&format!("{sessions} sesi fokus"), &g.f.small, [l, t + 60.0, r, b], GRAY);
        }
        6 => {
            title("AIR HARI INI");
            g.text(&water::ml(app.water), &g.f.big, [l, t + 22.0, r, t + 60.0], WHITE);
            g.text(&format!("dari {}", water::ml(app.water_cfg.goal)), &g.f.small, [l, t + 60.0, r, b], GRAY);
        }
        _ => {
            let (y, m, d) = astro::hijri(now.day);
            g.text(["MINGGU", "SENIN", "SELASA", "RABU", "KAMIS", "JUMAT", "SABTU"][clock::weekday(now.day) as usize], &g.f.small, [l, t, r, t + 18.0], ACCENT);
            let (_, gm, gd) = clock::civil_from_days(now.day);
            g.text(&format!("{gd} {}", MONTHS[gm as usize - 1]), &g.f.big, [l, t + 18.0, r, t + 56.0], WHITE);
            g.text(&format!("{d} {} {y} H", HIJRI[(m - 1).clamp(0, 11) as usize]), &g.f.small, [l, t + 58.0, r, b], GRAY);
        }
    }
}
