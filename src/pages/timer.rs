use super::chip;
use crate::{
    Action, App,
    render::{ACCENT, CHIP, GRAY, Gfx, R, TRACK, WHITE},
    state::mmss,
    timer::{PRESETS, Run},
};
use std::time::Instant;

pub fn draw(app: &App, g: &Gfx, [l, t, r, _b]: R, buttons: &mut Vec<(R, Action)>) {
    let mid = l + 148.0;
    let now = Instant::now();

    // tab Timer dan Stopwatch
    let mut x = l + 2.0;
    for (stopwatch, name) in [(false, "Timer"), (true, "Stopwatch")] {
        let w = g.width(name, &g.f.small) + 4.0;
        let active = app.stopwatch_mode == stopwatch;
        g.text(name, &g.f.small, [x, t - 2.0, x + w, t + 15.0], if active { WHITE } else { GRAY });
        if active {
            g.rrect([x, t + 14.0, x + w - 4.0, t + 15.5], 0.75, ACCENT);
        }
        buttons.push(([x - 4.0, t - 4.0, x + w + 4.0, t + 17.0], Action::TimerMode(stopwatch)));
        x += w + 10.0;
    }
    g.rrect([mid, t + 4.0, mid + 1.0, t + 92.0], 0.5, 0x2a2a2a);
    let rl = mid + 14.0;
    let main = [r - 84.0, t + 4.0, r - 24.0, t + 24.0];
    let reset = [r - 19.0, t + 4.5, r, t + 23.5];

    if app.stopwatch_mode {
        let sw = &app.stopwatch;
        let elapsed = sw.elapsed(now);
        g.text(&mmss(elapsed), &g.f.big, [l, t + 18.0, mid - 8.0, t + 54.0], WHITE);
        let (status, color) = if sw.running() { ("Running", WHITE) } else if elapsed.is_zero() { ("Ready", ACCENT) } else { ("Paused", GRAY) };
        g.text(status, &g.f.bold, [l + 2.0, t + 58.0, mid - 8.0, t + 78.0], color);
        g.text("Count up",&g.f.small, [rl, t + 5.0, rl + 80.0, t + 23.0], GRAY);
        chip(g, main, if sw.running() { "Pause" } else if elapsed.is_zero() { "Start" } else { "Resume" }, ACCENT, buttons, Action::StopwatchToggle);
        chip(g, reset, "", CHIP, buttons, Action::StopwatchReset);
        g.text("\u{E72C}", &g.f.icon_small, reset, WHITE);
        g.text("Also shown in the pill when the notch is closed.", &g.f.small, [rl, t + 36.0, r, t + 54.0], GRAY);
        return;
    }

    let tm = &app.timer;
    let remaining = tm.remaining(now);
    g.text(&mmss(remaining), &g.f.big, [l, t + 18.0, mid - 8.0, t + 54.0], WHITE);
    let (status, color) = match tm.run {
        Run::Ready => ("Ready", ACCENT),
        Run::Running(_) => ("Running", WHITE),
        Run::Paused(_) => ("Paused", GRAY),
        Run::Ringing => ("Time\u{2019}s up", ACCENT),
    };
    let minutes = format!("{} min", tm.minutes);
    let mw = g.width(&minutes, &g.f.bold);
    g.text(&minutes, &g.f.bold, [l + 2.0, t + 58.0, l + 4.0 + mw, t + 78.0], WHITE);
    g.text(status, &g.f.bold, [l + 9.0 + mw, t + 58.0, mid - 8.0, t + 78.0], color);
    let frac = 1.0 - remaining.as_secs_f32() / tm.len().as_secs_f32().max(1.0);
    g.rrect([l + 2.0, t + 83.0, l + 130.0, t + 86.0], 1.5, TRACK);
    if frac > 0.0 {
        g.rrect([l + 2.0, t + 83.0, l + 2.0 + 128.0 * frac.min(1.0), t + 86.0], 1.5, WHITE);
    }

    g.text("Duration", &g.f.small, [rl, t + 5.0, rl + 70.0, t + 23.0], GRAY);
    let label = match tm.run {
        Run::Ready => "Start",
        Run::Running(_) => "Pause",
        Run::Paused(_) => "Resume",
        Run::Ringing => "Done",
    };
    chip(g, main, label, ACCENT, buttons, Action::TimerToggle);
    chip(g, reset, "", CHIP, buttons, Action::TimerReset);
    g.text("\u{E72C}", &g.f.icon_small, reset, WHITE);

    // lima pilihan cepat, dan satu sel untuk durasi yang diketik sendiri
    let w = (r - rl - 12.0) / 3.0;
    let custom = !PRESETS.contains(&tm.minutes);
    for i in 0..6 {
        let x = rl + (i % 3) as f32 * (w + 6.0);
        let y = t + 34.0 + (i / 3) as f32 * 30.0;
        let cell = [x, y, x + w, y + 24.0];
        let (text, selected, action) = match PRESETS.get(i) {
            Some(&m) => (format!("{m} min"), tm.minutes == m, Action::TimerSet(m)),
            None => (if custom { format!("{} min", tm.minutes) } else { "Custom\u{2026}".into() }, custom, Action::TimerCustom),
        };
        g.rrect(cell, 12.0, if selected { 0x34285e } else { CHIP });
        g.text(&text, &g.f.csmall, cell, if selected { WHITE } else { GRAY });
        buttons.push((cell, action));
    }
}
