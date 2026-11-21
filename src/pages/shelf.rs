use crate::{
    Action, App,
    render::{DIM, GRAY, Gfx, R, WHITE},
    shelf,
};

const ROWS: usize = 3;

pub fn draw(app: &App, g: &Gfx, [l, t, r, b]: R, buttons: &mut Vec<(R, Action)>) {
    let mid = (l + r) / 2.0;
    let (fl, fr, cl, cr) = (l, mid - 8.0, mid + 8.0, r);

    if app.dragging {
        g.rrect([l, t, r, b], 14.0, 0x1d1a33);
        g.text("Lepas untuk menaruh di Shelf", &g.f.cbold, [l, t, r, b], WHITE);
        return;
    }

    g.text("FILE", &g.f.small, [fl, t, fr, t + 18.0], GRAY);
    if app.files.is_empty() {
        g.text("Seret file ke notch", &g.f.small, [fl, t + 22.0, fr, t + 40.0], WHITE);
        g.text("untuk disimpan sementara", &g.f.small, [fl, t + 38.0, fr, t + 56.0], GRAY);
    }
    for (row, path) in app.files.iter().take(ROWS).enumerate() {
        let y = t + 22.0 + row as f32 * 24.0;
        g.text("\u{E8A5}", &g.f.icon_small, [fl, y, fl + 16.0, y + 22.0], GRAY);
        let item = [fl, y, fr - 22.0, y + 22.0];
        g.text(shelf::file_name(path), &g.f.text, [fl + 20.0, y, fr - 22.0, y + 22.0], WHITE);
        buttons.push((item, Action::ShelfOpen(row)));
        let del = [fr - 20.0, y, fr, y + 22.0];
        g.text("\u{E711}", &g.f.icon_small, del, DIM);
        buttons.push((del, Action::ShelfRemove(row)));
    }
    if app.files.len() > ROWS {
        g.text(&format!("+{} lagi", app.files.len() - ROWS), &g.f.small, [fl + 20.0, b - 16.0, fr, b], GRAY);
    }

    g.rrect([mid - 0.5, t + 2.0, mid + 0.5, b - 6.0], 0.5, 0x2a2a2a);

    g.text("CLIPBOARD", &g.f.small, [cl, t, cr, t + 18.0], GRAY);
    if app.clips.is_empty() {
        g.text("Teks yang Anda salin", &g.f.small, [cl, t + 22.0, cr, t + 40.0], WHITE);
        g.text("muncul di sini", &g.f.small, [cl, t + 38.0, cr, t + 56.0], GRAY);
    }
    for (row, clip) in app.clips.iter().take(ROWS).enumerate() {
        let y = t + 22.0 + row as f32 * 24.0;
        let item = [cl, y, cr, y + 22.0];
        let line: String = clip.split_whitespace().collect::<Vec<_>>().join(" ");
        g.rrect(item, 8.0, 0x161616);
        g.text(&line, &g.f.small, [cl + 8.0, y, cr - 6.0, y + 22.0], WHITE);
        buttons.push((item, Action::ClipCopy(row)));
    }
    if app.clips.len() > ROWS {
        g.text(&format!("+{} lagi", app.clips.len() - ROWS), &g.f.small, [cl, b - 16.0, cr, b], GRAY);
    }
}
