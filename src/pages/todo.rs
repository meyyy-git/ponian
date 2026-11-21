use crate::{
    Action, App, calendar, clock, online,
    render::{ACCENT, CARD, CHIP, DIM, GRAY, GREEN, Gfx, R, WHITE},
    todo,
};

const ROWS: usize = 3;

pub fn draw(app: &App, g: &Gfx, [l, t, r, _b]: R, buttons: &mut Vec<(R, Action)>) {
    let today = clock::now().day;
    let tasks = &app.tasks;
    let done = tasks.iter().filter(|t| t.done && t.day == today).count();
    let total = tasks.iter().filter(|t| t.day == today || !t.done).count();
    g.text(&format!("{done}/{total}"), &g.f.big, [l, t + 4.0, l + 140.0, t + 40.0], WHITE);
    g.text("Selesai hari ini", &g.f.small, [l, t + 44.0, l + 140.0, t + 60.0], GRAY);

    let add = [l - 2.0, t + 68.0, l + 82.0, t + 94.0];
    g.rrect(add, 13.0, CHIP);
    g.text("\u{E710}", &g.f.icon_small, [add[0] + 6.0, add[1], add[0] + 26.0, add[3]], WHITE);
    g.text("Tambah", &g.f.small, [add[0] + 28.0, add[1], add[2], add[3]], WHITE);
    buttons.push((add, Action::TodoAdd));

    // kartu daftar: acara kalender hari ini di atas (hanya dibaca), lalu tugas
    let card = [l + 152.0, t + 6.0, r, t + 98.0];
    g.rrect(card, 16.0, CARD);
    let (cl, cr) = (card[0] + 10.0, card[2] - 6.0);
    let events = calendar::on(&online::EVENTS.lock().unwrap(), today);
    let order = todo::sorted(tasks);
    if order.is_empty() && events.is_empty() {
        g.text("\u{E9D5}", &g.f.icon_big, [card[0], card[1] + 14.0, card[2], card[1] + 48.0], DIM);
        g.text("Tidak ada tugas.", &g.f.center, [card[0], card[1] + 52.0, card[2], card[1] + 72.0], GRAY);
        return;
    }
    let mut row = 0;
    let row_y = |row: usize| card[1] + 7.0 + row as f32 * 26.0;
    for e in events.iter().take(ROWS) {
        let y = row_y(row);
        g.text("\u{E787}", &g.f.icon_small, [cl, y, cl + 16.0, y + 24.0], ACCENT);
        g.text(&e.title, &g.f.small, [cl + 20.0, y, cr - 40.0, y + 24.0], WHITE);
        g.text(&e.min.map_or("Seharian".into(), clock::hhmm), &g.f.rsmall, [cr - 50.0, y, cr - 2.0, y + 24.0], GRAY);
        row += 1;
    }
    for &i in order.iter().take(ROWS - row) {
        let task = &tasks[i];
        let y = row_y(row);
        g.ring(cl + 7.0, y + 12.0, 6.0, 1.5, if task.done { 1.0 } else { 0.0 }, GREEN, if task.done { GREEN } else { GRAY });
        // tugas untuk hari lain menampilkan tanggalnya, tugas hari ini menampilkan jamnya
        let time = if task.day != today {
            let (_, m, d) = clock::civil_from_days(task.day);
            format!("{d}/{m}")
        } else {
            task.time.map(clock::hhmm).unwrap_or_default()
        };
        let tw = if time.is_empty() { 0.0 } else { 34.0 };
        g.text(&task.title, &g.f.small, [cl + 20.0, y, cr - 20.0 - tw, y + 24.0], if task.done { DIM } else { WHITE });
        g.text(&time, &g.f.rsmall, [cr - 20.0 - tw, y, cr - 20.0, y + 24.0], GRAY);
        // lingkaran menandai selesai; judul membuka kotak edit
        buttons.push(([card[0], y, cl + 18.0, y + 24.0], Action::TodoToggle(i)));
        buttons.push(([cl + 18.0, y, cr - 20.0, y + 24.0], Action::TodoEdit(i)));
        let del = [cr - 18.0, y, cr + 2.0, y + 24.0];
        g.text("\u{E711}", &g.f.icon_small, del, DIM);
        buttons.push((del, Action::TodoDelete(i)));
        row += 1;
    }
    let hidden = (events.len() + order.len()).saturating_sub(row);
    if hidden > 0 {
        g.text(&format!("+{hidden} lagi"), &g.f.small, [l + 90.0, t + 72.0, l + 148.0, t + 90.0], GRAY);
    }
}
