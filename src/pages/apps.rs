use crate::{
    Action, App,
    apps::MAX,
    render::{CARD, DIM, GRAY, Gfx, R, WHITE},
};

pub fn draw(app: &App, g: &Gfx, [l, t, r, _b]: R, buttons: &mut Vec<(R, Action)>) {
    // sampai 11 pintasan: ikon besar dengan nama; lebih dari itu: ikon kecil tanpa nama supaya muat
    let roomy = app.apps.len() <= 11;
    let (cols, row_h, half) = if roomy { (6, 51.0, 15.0) } else { (8, 34.0, 12.0) };
    let cw = (r - l) / cols as f32;
    let cell = |i: usize| {
        let (x, y) = (l + (i % cols) as f32 * cw, t + (i / cols) as f32 * row_h);
        let top = if roomy { y + 3.0 } else { y + 4.0 };
        ([x, y, x + cw, y + row_h - 2.0], [x + cw / 2.0 - half, top, x + cw / 2.0 + half, top + half * 2.0], [x + 1.0, y + 33.0, x + cw - 1.0, y + 48.0])
    };
    for (i, s) in app.apps.iter().enumerate() {
        let (hit, icon, label) = cell(i);
        match &s.icon {
            Some(px) => g.image(px, s.side, icon, 5.0),
            None => {
                g.rrect(icon, 8.0, CARD);
                g.text("\u{E7B8}", &g.f.icon_small, icon, GRAY);
            }
        }
        if roomy {
            g.text(&s.name, &g.f.csmall, label, 0xd8d8d8);
        }
        buttons.push((hit, Action::AppOpen(i)));
    }
    if app.apps.len() < MAX {
        let (hit, icon, label) = cell(app.apps.len());
        g.dashed(icon, 9.0, DIM);
        g.text("\u{E710}", &g.f.icon_small, icon, GRAY);
        if roomy {
            g.text("Tambah", &g.f.csmall, label, GRAY);
        }
        buttons.push((hit, Action::AppAdd));
    }
    if app.apps.is_empty() {
        g.text("Tambah aplikasi favorit Anda", &g.f.text, [l + cw + 8.0, t + 4.0, r, t + 24.0], WHITE);
        g.text("Klik + atau seret file .exe / pintasan ke sini.", &g.f.small, [l + cw + 8.0, t + 22.0, r, t + 38.0], GRAY);
        g.text("Klik kanan ikon untuk mengatur atau menghapusnya.", &g.f.small, [l + cw + 8.0, t + 37.0, r, t + 53.0], GRAY);
    }
}
