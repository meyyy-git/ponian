use super::chip;
use crate::{
    Action, App, calendar, clock, online,
    render::{ACCENT, DIM, GRAY, GREEN, Gfx, R, WHITE},
    todo,
};

const ROWS: usize = 3;

pub fn draw(app: &App, g: &Gfx, [l, t, r, b]: R, buttons: &mut Vec<(R, Action)>) {
    let today = clock::now().day;
    let tasks = &app.tasks;
    let done = tasks.iter().filter(|t| t.done && t.day == today).count();
    let total = tasks.iter().filter(|t| t.day == today || !t.done).count();
    g.text(&format!("{done}/{total} selesai hari ini"), &g.f.small, [l, t, r - 100.0, t + 24.0], GRAY);
    chip(g, [r - 92.0, t, r, t + 24.0], "+ Tambah", ACCENT, buttons, Action::TodoAdd);

    // acara kalender hari ini tampil di atas tugas, hanya untuk dibaca
    let events = calendar::on(&online::EVENTS.lock().unwrap(), today);
    let order = todo::sorted(tasks);
    if order.is_empty() && events.is_empty() {
        g.text("Belum ada tugas.", &g.f.cbold, [l, t + 34.0, r, t + 56.0], WHITE);
        g.text("Tulis jam untuk pengingat, misalnya \u{201C}Rapat 14:30\u{201D}", &g.f.csmall, [l, t + 56.0, r, t + 76.0], GRAY);
        return;
    }
    let mut row = 0;
    for e in events.iter().take(ROWS) {
        let y = t + 32.0 + row as f32 * 24.0;
        g.text("\u{E787}", &g.f.icon_small, [l, y, l + 16.0, y + 22.0], ACCENT);
        g.text(&e.title, &g.f.text, [l + 22.0, y, r - 80.0, y + 22.0], WHITE);
        g.text(&e.min.map_or("Seharian".into(), clock::hhmm), &g.f.rsmall, [r - 80.0, y, r - 30.0, y + 22.0], GRAY);
        row += 1;
    }
    for &i in order.iter().take(ROWS - row) {
        let task = &tasks[i];
        let y = t + 32.0 + row as f32 * 24.0;
        let check = [l, y, r - 26.0, y + 22.0];
        g.ring(l + 8.0, y + 11.0, 6.5, 1.6, if task.done { 1.0 } else { 0.0 }, GREEN, if task.done { GREEN } else { GRAY });
        g.text(&task.title, &g.f.text, [l + 22.0, y, r - 80.0, y + 22.0], if task.done { DIM } else { WHITE });
        if let Some(m) = task.time {
            g.text(&clock::hhmm(m), &g.f.rsmall, [r - 80.0, y, r - 30.0, y + 22.0], GRAY);
        }
        buttons.push((check, Action::TodoToggle(i)));
        let del = [r - 24.0, y, r, y + 22.0];
        g.text("\u{E711}", &g.f.icon_small, del, DIM);
        buttons.push((del, Action::TodoDelete(i)));
        row += 1;
    }
    let hidden = (events.len() + order.len()).saturating_sub(row);
    if hidden > 0 {
        g.text(&format!("+{hidden} lagi"), &g.f.small, [l + 22.0, b - 18.0, r, b], GRAY);
    }
}
