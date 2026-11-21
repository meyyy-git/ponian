use crate::{
    Action, App,
    render::{ACCENT, CARD, DIM, GRAY, Gfx, R, WHITE},
    shelf,
};

const ROWS: usize = 3;

pub fn draw(app: &App, g: &Gfx, [l, t, r, b]: R, buttons: &mut Vec<(R, Action)>) {
    // tab File dan Clipboard
    let mut x = l;
    for (tab, name, count) in [(0u8, "File", app.files.len()), (1, "Clipboard", app.clips.len())] {
        let label = if count > 0 { format!("{name} {count}") } else { name.to_string() };
        let w = g.width(&label, &g.f.small) + 4.0;
        let active = app.shelf_tab == tab && !app.dragging;
        g.text(&label, &g.f.small, [x, t - 3.0, x + w, t + 15.0], if active { WHITE } else { GRAY });
        if active {
            g.rrect([x, t + 14.0, x + w - 4.0, t + 15.5], 0.75, ACCENT);
        }
        buttons.push(([x - 4.0, t - 5.0, x + w + 4.0, t + 17.0], Action::ShelfTab(tab)));
        x += w + 12.0;
    }

    let zone = [l, t + 22.0, r, b - 2.0];
    let empty = |icon: &str, title: &str, sub: &str, color: u32| {
        g.dashed(zone, 14.0, color);
        let cy = (zone[1] + zone[3]) / 2.0;
        g.text(icon, &g.f.icon_big, [l, cy - 34.0, r, cy - 2.0], WHITE);
        g.text(title, &g.f.center, [l, cy + 2.0, r, cy + 20.0], WHITE);
        g.text(sub, &g.f.csmall, [l, cy + 18.0, r, cy + 34.0], GRAY);
    };

    if app.dragging {
        empty("\u{E896}", "Lepas untuk menambahkan", "File tetap di tempat asalnya", ACCENT);
    } else if app.shelf_tab == 0 {
        if app.files.is_empty() {
            return empty("\u{E896}", "Jatuhkan file di sini", "Seret keluar kapan saja \u{B7} klik untuk membuka", DIM);
        }
        for (row, path) in app.files.iter().take(ROWS).enumerate() {
            let y = zone[1] + row as f32 * 26.0;
            let item = [l, y, r - 26.0, y + 23.0];
            g.rrect([l, y, r, y + 23.0], 9.0, CARD);
            g.text("\u{E8A5}", &g.f.icon_small, [l + 6.0, y, l + 24.0, y + 23.0], GRAY);
            g.text(shelf::file_name(path), &g.f.text, [l + 28.0, y, r - 28.0, y + 23.0], WHITE);
            buttons.push((item, Action::ShelfOpen(row)));
            let del = [r - 24.0, y, r, y + 23.0];
            g.text("\u{E711}", &g.f.icon_small, del, DIM);
            buttons.push((del, Action::ShelfRemove(row)));
        }
        more(g, app.files.len(), [l, b - 4.0, r, b + 10.0]);
    } else {
        if app.clips.is_empty() {
            return empty("\u{E8C8}", "Salin teks, gambar, atau file", "Nanti menunggu di sini, siap ditempel lagi", DIM);
        }
        for (row, clip) in app.clips.iter().take(ROWS).enumerate() {
            let y = zone[1] + row as f32 * 26.0;
            let item = [l, y, r, y + 23.0];
            let (icon, line) = clip.label();
            g.rrect(item, 9.0, CARD);
            // teks tampil polos; file dan gambar diberi ikon supaya terbedakan
            let x = if icon.is_empty() { l + 10.0 } else { l + 28.0 };
            g.text(icon, &g.f.icon_small, [l + 6.0, y, l + 24.0, y + 23.0], GRAY);
            g.text(&line, &g.f.text, [x, y, r - 10.0, y + 23.0], WHITE);
            buttons.push((item, Action::ClipCopy(row)));
        }
        more(g, app.clips.len(), [l, b - 4.0, r, b + 10.0]);
    }
}

fn more(g: &Gfx, total: usize, r: R) {
    if total > ROWS {
        g.text(&format!("+{} lagi", total - ROWS), &g.f.rsmall, r, GRAY);
    }
}
