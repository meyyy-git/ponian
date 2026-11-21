use super::chip;
use crate::{
    Action, App,
    render::{ACCENT, CHIP, GRAY, Gfx, R, TRACK, WHITE},
    state::mmss,
    timer::{PRESETS, Run},
};
use std::time::Instant;

pub fn draw(app: &App, g: &Gfx, [l, t, r, _b]: R, buttons: &mut Vec<(R, Action)>) {
    let tm = &app.timer;
    let mid = l + 148.0;
    let remaining = tm.remaining(Instant::now());
    g.text("Hitung mundur", &g.f.small, [l + 2.0, t, mid - 8.0, t + 16.0], GRAY);
    g.text(&mmss(remaining), &g.f.big, [l, t + 16.0, mid - 8.0, t + 52.0], WHITE);
    let (status, color) = match tm.run {
        Run::Ready => ("Siap", ACCENT),
        Run::Running(_) => ("Berjalan", WHITE),
        Run::Paused(_) => ("Dijeda", GRAY),
        Run::Ringing => ("Waktu habis", ACCENT),
    };
    let minutes = format!("{} mnt", tm.minutes);
    let mw = g.width(&minutes, &g.f.bold);
    g.text(&minutes, &g.f.bold, [l + 2.0, t + 58.0, l + 4.0 + mw, t + 78.0], WHITE);
    g.text(status, &g.f.bold, [l + 9.0 + mw, t + 58.0, mid - 8.0, t + 78.0], color);
    let frac = 1.0 - remaining.as_secs_f32() / tm.len().as_secs_f32().max(1.0);
    g.rrect([l + 2.0, t + 83.0, l + 130.0, t + 86.0], 1.5, TRACK);
    if frac > 0.0 {
        g.rrect([l + 2.0, t + 83.0, l + 2.0 + 128.0 * frac.min(1.0), t + 86.0], 1.5, WHITE);
    }

    g.rrect([mid, t + 4.0, mid + 1.0, t + 92.0], 0.5, 0x2a2a2a);

    let rl = mid + 14.0;
    g.text("Durasi", &g.f.small, [rl, t + 5.0, rl + 70.0, t + 23.0], GRAY);
    let label = match tm.run {
        Run::Ready => "Mulai",
        Run::Running(_) => "Jeda",
        Run::Paused(_) => "Lanjut",
        Run::Ringing => "Selesai",
    };
    chip(g, [r - 84.0, t + 4.0, r - 24.0, t + 24.0], label, ACCENT, buttons, Action::TimerToggle);
    chip(g, [r - 19.0, t + 4.5, r, t + 23.5], "", CHIP, buttons, Action::TimerReset);
    g.text("\u{E72C}", &g.f.icon_small, [r - 19.0, t + 4.5, r, t + 23.5], WHITE);

    // pilihan durasi: dua baris berisi tiga
    let w = (r - rl - 12.0) / 3.0;
    for (i, minutes) in PRESETS.into_iter().enumerate() {
        let x = rl + (i % 3) as f32 * (w + 6.0);
        let y = t + 34.0 + (i / 3) as f32 * 30.0;
        let cell = [x, y, x + w, y + 24.0];
        let selected = tm.minutes == minutes;
        g.rrect(cell, 12.0, if selected { 0x34285e } else { CHIP });
        g.text(&format!("{minutes} mnt"), &g.f.csmall, cell, if selected { WHITE } else { GRAY });
        buttons.push((cell, Action::TimerSet(minutes)));
    }
}
