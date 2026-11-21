use super::chip;
use crate::{
    Action, App, clock,
    pomodoro::{Phase, Pomo, ROUNDS, Run},
    render::{ACCENT, CHIP, DIM, GRAY, Gfx, R, WHITE},
    state::mmss,
};
use std::time::Instant;

pub fn draw(app: &App, g: &Gfx, [l, t, r, b]: R, buttons: &mut Vec<(R, Action)>) {
    let p = &app.pomo;
    let d = app.durations;
    let mid = (l + r) / 2.0 + 10.0;
    let phase = match p.phase {
        Phase::Focus => format!("Fokus \u{B7} {} dari {ROUNDS}", p.round),
        Phase::Short => "Istirahat pendek".into(),
        Phase::Long => "Istirahat panjang".into(),
    };
    g.text(&phase, &g.f.small, [l, t, mid - 12.0, t + 18.0], GRAY);
    g.text(&mmss(p.remaining(Instant::now(), d)), &g.f.big, [l, t + 18.0, mid - 12.0, t + 58.0], WHITE);
    let (status, color) = match p.run {
        Run::Ready => ("Siap", ACCENT),
        Run::Running(_) => ("Berjalan", WHITE),
        Run::Paused(_) => ("Dijeda", GRAY),
        Run::Ringing => (if p.phase == Phase::Focus { "Waktunya istirahat" } else { "Kembali fokus" }, ACCENT),
    };
    g.text(&format!("{} mnt", p.len(d).as_secs() / 60), &g.f.cbold, [l, t + 60.0, l + 52.0, t + 80.0], WHITE);
    g.text(status, &g.f.small, [l + 58.0, t + 60.0, mid - 12.0, t + 80.0], color);

    // garis pemisah kolom
    g.rrect([mid - 1.0, t + 4.0, mid, b - 8.0], 0.5, 0x2a2a2a);

    let (rl, rr) = (mid + 14.0, r);
    let label = match p.run {
        Run::Ready => "Mulai",
        Run::Running(_) => "Jeda",
        Run::Paused(_) => "Lanjut",
        Run::Ringing => "Lanjut",
    };
    chip(g, [rr - 110.0, t, rr - 36.0, t + 26.0], label, ACCENT, buttons, Action::PomoToggle);
    chip(g, [rr - 28.0, t, rr, t + 26.0], "", CHIP, buttons, Action::PomoReset);
    g.text("\u{E72C}", &g.f.icon_small, [rr - 28.0, t, rr, t + 26.0], WHITE);

    // satu segmen per ronde fokus, lalu segmen istirahat panjang
    let y = t + 50.0;
    let seg = (rr - rl - 4.0 * 6.0) / 5.0;
    for i in 0..=ROUNDS {
        let x = rl + i as f32 * (seg + 6.0);
        let done = match p.phase {
            Phase::Focus => i + 1 < p.round,
            Phase::Short => i < p.round,
            Phase::Long => i < ROUNDS,
        };
        let current = (p.phase == Phase::Long && i == ROUNDS) || (p.phase != Phase::Long && i + 1 == p.round && i < ROUNDS);
        g.rrect([x, y, x + seg, y + 5.0], 2.5, if done { ACCENT } else if current { WHITE } else { DIM });
        // segmen terakhir: istirahat panjang
        let label = if i == ROUNDS { "\u{2615}".to_string() } else { (i + 1).to_string() };
        g.text(&label, &g.f.csmall, [x - 6.0, y + 8.0, x + seg + 6.0, y + 24.0], if current { WHITE } else { GRAY });
    }
    let cycle = Pomo::cycle_minutes(d) as u32;
    g.text(&format!("{} \u{B7} {ROUNDS} sesi", clock::span(cycle)), &g.f.small, [rl, y + 28.0, rr, y + 46.0], GRAY);
}
