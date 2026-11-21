use crate::{
    Action, App, NOTE,
    media::{ART, Msg},
    render::{ACCENT, CARD, DIM, GRAY, Gfx, R, WHITE},
    volume,
};
use std::sync::atomic::Ordering::Relaxed;

fn mmss(ms: i64) -> String {
    let s = ms.max(0) / 1000;
    format!("{:02}:{:02}", s / 60, s % 60)
}

pub fn draw(app: &App, g: &Gfx, [l, t, r, _b]: R, buttons: &mut Vec<(R, Action)>) {
    let m = &app.media;
    // halaman musik tidak punya judul, jadi cover naik ke baris status
    let top = t - 26.0;
    let art = [l, top, l + 62.0, top + 62.0];
    match &m.art {
        Some(a) => g.image(&a.bgra, ART, art, 11.0),
        None => {
            g.rrect(art, 11.0, CARD);
            g.text(NOTE, &g.f.icon_big, art, DIM);
        }
    }
    let tx = l + 78.0;
    // lebih dari satu pemutar: chip nama pemutar di kanan judul, klik untuk berpindah
    let mut title_r = r;
    if m.sources > 1 && !m.source.is_empty() {
        let label = format!("{} \u{203A}", m.source);
        title_r = r - g.width(&label, &g.f.csmall) - 24.0;
        crate::pages::chip(g, [title_r + 8.0, top + 21.0, r, top + 39.0], &label, crate::render::CHIP, buttons, Action::Media(|| Msg::Source));
    }
    if m.session && !m.title.is_empty() {
        g.text(&m.title, &g.f.bold, [tx, top + 20.0, title_r, top + 40.0], WHITE);
        g.text(&m.artist, &g.f.small, [tx, top + 38.0, r, top + 54.0], GRAY);
    } else {
        g.text("Nothing playing", &g.f.bold, [tx, top + 20.0, r, top + 40.0], WHITE);
    }

    // batang progres dengan waktu berjalan dan sisa waktu; klik untuk melompat
    let y = top + 85.0;
    let pos = m.pos().max(0);
    g.text(&mmss(pos), &g.f.num, [l, y - 11.0, l + 48.0, y + 11.0], WHITE);
    g.text(&format!("-{}", mmss(m.dur_ms - pos)), &g.f.rnum, [r - 56.0, y - 11.0, r, y + 11.0], WHITE);
    let (b0, b1) = (l + 54.0, r - 62.0);
    g.rrect([b0, y - 1.5, b1, y + 1.5], 1.5, 0x4a4a4a);
    if m.dur_ms > 0 {
        g.rrect([b0, y - 1.5, b0 + (b1 - b0) * (pos as f32 / m.dur_ms as f32).min(1.0), y + 1.5], 1.5, WHITE);
        buttons.push(([b0, y - 9.0, b1, y + 9.0], Action::Seek { x0: b0, x1: b1 }));
    }

    let cy = top + 113.0;
    let cx = (l + r) / 2.0;
    let hit = |x: f32, half: f32| [x - half, cy - 14.0, x + half, cy + 14.0];

    // tombol lirik: menyalakan atau mematikan lirik di pill
    let lyrics_on = app.lyrics_on;
    g.text("\u{E8BD}", &g.f.icon, hit(l + 30.0, 16.0), if !lyrics_on { DIM } else if m.lyrics.is_some() { ACCENT } else { WHITE });
    buttons.push((hit(l + 30.0, 16.0), Action::LyricsToggle));

    // mundur dan maju: dua segitiga berdampingan
    for (center, back, msg) in [(cx - 47.0, true, (|| Msg::Prev) as fn() -> Msg), (cx + 47.0, false, || Msg::Next)] {
        for k in 0..2 {
            // tiap segitiga menempati kotak selebar 9; (flat, tip) adalah sisi rata dan ujung lancipnya
            let x = center - 9.0 + k as f32 * 9.0;
            let (flat, tip) = if back { (x + 9.0, x) } else { (x, x + 9.0) };
            g.poly(&[(flat, cy - 7.0), (tip, cy), (flat, cy + 7.0)], WHITE);
        }
        buttons.push((hit(center, 20.0), Action::Media(msg)));
    }
    if m.playing {
        g.rrect([cx - 6.0, cy - 8.0, cx - 2.0, cy + 8.0], 1.0, WHITE);
        g.rrect([cx + 2.0, cy - 8.0, cx + 6.0, cy + 8.0], 1.0, WHITE);
    } else {
        g.poly(&[(cx - 5.0, cy - 8.0), (cx + 8.0, cy), (cx - 5.0, cy + 8.0)], WHITE);
    }
    buttons.push((hit(cx, 20.0), Action::Media(|| Msg::Toggle)));

    let muted = volume::VOLUME.load(Relaxed) & volume::MUTE != 0;
    g.text(if muted { "\u{E74F}" } else { "\u{E767}" }, &g.f.icon, hit(r - 8.0, 16.0), WHITE);
    buttons.push((hit(r - 8.0, 16.0), Action::Mute));
}
