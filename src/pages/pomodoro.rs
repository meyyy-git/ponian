use super::chip;
use crate::{
    Action, App, clock,
    pomodoro::{Phase, Pomo, ROUNDS, Run},
    render::{ACCENT, CHIP, GRAY, Gfx, R, TRACK, WHITE},
    state::mmss,
};
use std::time::Instant;

pub fn draw(app: &App, g: &Gfx, [l, t, r, _b]: R, buttons: &mut Vec<(R, Action)>) {
    let p = &app.pomo;
    let d = app.durations;
    let mid = l + 148.0;
    let phase = match p.phase {
        Phase::Focus => format!("Fokus \u{B7} {} dari {ROUNDS}", p.round),
        Phase::Short => "Istirahat pendek".into(),
        Phase::Long => "Istirahat panjang".into(),
    };
    let remaining = p.remaining(Instant::now(), d);
    g.text(&phase, &g.f.small, [l + 2.0, t, mid - 8.0, t + 16.0], GRAY);
    g.text(&mmss(remaining), &g.f.big, [l, t + 16.0, mid - 8.0, t + 52.0], WHITE);
    let (status, color) = match p.run {
        Run::Ready => ("Siap", ACCENT),
        Run::Running(_) => ("Berjalan", WHITE),
        Run::Paused(_) => ("Dijeda", GRAY),
        Run::Ringing => (if p.phase == Phase::Focus { "Istirahat dulu" } else { "Kembali fokus" }, ACCENT),
    };
    let minutes = format!("{} mnt", p.len(d).as_secs() / 60);
    let mw = g.width(&minutes, &g.f.bold);
    g.text(&minutes, &g.f.bold, [l + 2.0, t + 58.0, l + 4.0 + mw, t + 78.0], WHITE);
    g.text(status, &g.f.bold, [l + 9.0 + mw, t + 58.0, mid - 8.0, t + 78.0], color);
    // progres fase yang sedang berjalan
    let frac = 1.0 - remaining.as_secs_f32() / p.len(d).as_secs_f32().max(1.0);
    g.rrect([l + 2.0, t + 83.0, l + 130.0, t + 86.0], 1.5, TRACK);
    if frac > 0.0 {
        g.rrect([l + 2.0, t + 83.0, l + 2.0 + 128.0 * frac.min(1.0), t + 86.0], 1.5, WHITE);
    }

    g.rrect([mid, t + 4.0, mid + 1.0, t + 92.0], 0.5, 0x2a2a2a);

    let rl = mid + 14.0;
    g.text("Ronde ini", &g.f.small, [rl, t + 5.0, rl + 70.0, t + 23.0], GRAY);
    let label = match p.run {
        Run::Ready => "Mulai",
        Run::Running(_) => "Jeda",
        Run::Paused(_) | Run::Ringing => "Lanjut",
    };
    chip(g, [r - 84.0, t + 4.0, r - 24.0, t + 24.0], label, ACCENT, buttons, Action::PomoToggle);
    chip(g, [r - 19.0, t + 4.5, r, t + 23.5], "", CHIP, buttons, Action::PomoReset);
    g.text("\u{E72C}", &g.f.icon_small, [r - 19.0, t + 4.5, r, t + 23.5], WHITE);

    // empat ronde fokus dipisah titik istirahat pendek, diakhiri istirahat panjang
    let y = t + 48.0;
    let seg = (r - rl - 30.0 - 4.0 * 11.0) / 4.0;
    let color = |done: bool, current: bool| if done { ACCENT } else if current { WHITE } else { 0x3a3a3a };
    let mut x = rl;
    for i in 0..ROUNDS {
        let focus_done = p.round > i + 1 || (p.round == i + 1 && p.phase != Phase::Focus);
        let focus_now = p.round == i + 1 && p.phase == Phase::Focus;
        g.rrect([x, y - 2.0, x + seg, y + 2.0], 2.0, color(focus_done, focus_now));
        g.text(&(i + 1).to_string(), &g.f.csmall, [x, y + 10.0, x + seg, y + 26.0], if focus_now { WHITE } else { GRAY });
        x += seg + 3.0;
        // titik = istirahat setelah ronde ini; setelah ronde terakhir istirahatnya panjang
        let break_done = p.round > i + 1;
        let break_now = p.round == i + 1 && p.phase != Phase::Focus;
        let w = if i + 1 == ROUNDS { 30.0 } else { 5.0 };
        g.rrect([x, y - 2.0, x + w, y + 2.0], 2.0, color(break_done, break_now));
        x += w + 3.0;
    }
    g.text("Panjang", &g.f.rsmall, [r - 50.0, y + 10.0, r, y + 26.0], if p.phase == Phase::Long { WHITE } else { GRAY });
    g.text(&format!("{} \u{B7} {ROUNDS} sesi", clock::span(Pomo::cycle_minutes(d) as u32)), &g.f.small, [rl, t + 75.0, r, t + 91.0], GRAY);
}
