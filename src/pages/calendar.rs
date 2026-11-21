use super::chip;
use crate::{
    Action, App, calendar, clock, online,
    render::{ACCENT, CHIP, GRAY, Gfx, R, WHITE},
    state,
};

const MONTHS: [&str; 12] = ["Januari", "Februari", "Maret", "April", "Mei", "Juni", "Juli", "Agustus", "September", "Oktober", "November", "Desember"];
const INITIALS: [&str; 7] = ["M", "S", "S", "R", "K", "J", "S"];

/// (tahun, bulan 1..=12) yang berjarak `offset` bulan dari bulan pada hari `today`.
pub fn month_at(today: i64, offset: i32) -> (i64, u32) {
    let (y, m, _) = clock::civil_from_days(today);
    let months = y * 12 + m as i64 - 1 + offset as i64;
    (months.div_euclid(12), (months.rem_euclid(12) + 1) as u32)
}

/// (hari pertama, jumlah hari, kolom hari pertama) untuk kisi bulan.
pub fn grid(y: i64, m: u32, week_monday: bool) -> (i64, i64, u32) {
    let first = clock::days_from_civil(y, m, 1);
    let next = if m == 12 { clock::days_from_civil(y + 1, 1, 1) } else { clock::days_from_civil(y, m + 1, 1) };
    (first, next - first, (clock::weekday(first) + 7 - week_monday as u32) % 7)
}

pub fn draw(app: &App, g: &Gfx, [l, t, r, _b]: R, buttons: &mut Vec<(R, Action)>) {
    let today = clock::now().day;
    let (y, m) = month_at(today, app.cal_offset);
    let (first, days, lead) = grid(y, m, app.week_monday);
    let selected = app.cal_day.unwrap_or(today);
    let events = online::EVENTS.lock().unwrap();

    // kiri: bulan, navigasi, dan acara pada hari yang dipilih
    g.text(&format!("{} {y}", MONTHS[m as usize - 1]), &g.f.bold, [l, t - 2.0, l + 122.0, t + 18.0], WHITE);
    chip(g, [l, t + 22.0, l + 26.0, t + 44.0], "\u{2039}", CHIP, buttons, Action::CalMove(-1));
    chip(g, [l + 30.0, t + 22.0, l + 56.0, t + 44.0], "\u{203A}", CHIP, buttons, Action::CalMove(1));
    chip(g, [l + 60.0, t + 22.0, l + 96.0, t + 44.0], "Kini", CHIP, buttons, Action::CalToday);
    chip(g, [l + 100.0, t + 22.0, l + 122.0, t + 44.0], "+", CHIP, buttons, Action::CalAdd);
    let (sy, sm, sd) = clock::civil_from_days(selected);
    g.text(&state::date_id(clock::weekday(selected) as u16, sd as u16, sm as u16, sy as u16), &g.f.small, [l, t + 50.0, l + 120.0, t + 66.0], GRAY);
    // acara kalender dan tugas yang dijadwalkan pada hari itu
    let mut list: Vec<String> = calendar::on(&events, selected).iter().map(|e| e.min.map_or(e.title.clone(), |min| format!("{} {}", clock::hhmm(min), e.title))).collect();
    list.extend(app.tasks.iter().filter(|t| t.day == selected && !t.done).map(|t| t.time.map_or(t.title.clone(), |min| format!("{} {}", clock::hhmm(min), t.title))));
    if list.is_empty() {
        g.text("Tidak ada acara", &g.f.small, [l, t + 66.0, l + 120.0, t + 82.0], GRAY);
    }
    for (row, label) in list.iter().take(2).enumerate() {
        let y = t + 66.0 + row as f32 * 17.0;
        g.text(label, &g.f.small, [l, y, l + 120.0, y + 17.0], WHITE);
    }

    // kanan: kisi bulan
    let gx = l + 128.0;
    let cw = (r - gx) / 7.0;
    let start = if app.week_monday { 1 } else { 0 };
    for col in 0..7 {
        let x = gx + col as f32 * cw;
        g.text(INITIALS[(col + start) % 7], &g.f.csmall, [x, t - 4.0, x + cw, t + 10.0], GRAY);
    }
    for d in 0..days {
        let (day, cell_no) = (first + d, lead as i64 + d);
        let (x, y) = (gx + (cell_no % 7) as f32 * cw, t + 12.0 + (cell_no / 7) as f32 * 15.0);
        let cell = [x, y, x + cw, y + 15.0];
        let pill = [x + cw / 2.0 - 11.0, y + 0.5, x + cw / 2.0 + 11.0, y + 14.5];
        let has_event = !calendar::on(&events, day).is_empty() || app.tasks.iter().any(|t| t.day == day && !t.done);
        let color = if day == today {
            g.rrect(pill, 7.0, ACCENT);
            WHITE
        } else {
            if day == selected {
                g.rrect(pill, 7.0, 0x2e2e2e);
            }
            // hari dengan acara berwarna aksen; Minggu merah seperti kalender pada umumnya
            if has_event { 0xb9a4ff } else if clock::weekday(day) == 0 { 0xff8a80 } else { 0xd8d8d8 }
        };
        g.text(&(d + 1).to_string(), &g.f.csmall, cell, color);
        buttons.push((cell, Action::CalDay(day)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn month_navigation_and_grid() {
        let today = clock::days_from_civil(2026, 11, 21);
        assert_eq!(month_at(today, 0), (2026, 11));
        assert_eq!(month_at(today, 2), (2027, 1));
        assert_eq!(month_at(today, -11), (2025, 12));
        // 1 November 2026 hari Minggu: kolom 0 kalau minggu mulai Minggu, kolom 6 kalau mulai Senin
        assert_eq!(grid(2026, 11, false), (clock::days_from_civil(2026, 11, 1), 30, 0));
        assert_eq!(grid(2026, 11, true).2, 6);
        assert_eq!(grid(2028, 2, true).1, 29);
    }
}
