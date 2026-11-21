use super::chip;
use crate::{
    Action, App,
    render::{ACCENT, CHIP, GRAY, Gfx, R, WHITE},
    state::{Timer, mmss},
};
use std::time::Instant;

pub fn draw(app: &App, g: &Gfx, [l, t, r, b]: R, buttons: &mut Vec<(R, Action)>) {
    let cy = (t + b) / 2.0;
    let side = [r - 76.0, cy - 14.0, r, cy + 14.0];
    match app.timer {
        Timer::Ringing => {
            g.text("Waktu habis", &g.f.big, [l, cy - 24.0, r - 84.0, cy + 14.0], WHITE);
            g.text("Klik di mana saja untuk berhenti", &g.f.small, [l, cy + 14.0, r - 84.0, cy + 32.0], GRAY);
            chip(g, side, "OK", ACCENT, buttons, Action::StopTimer);
        }
        Timer::Running(_) => {
            let rem = app.timer.remaining(Instant::now()).map(mmss).unwrap_or_default();
            g.text(&rem, &g.f.big, [l, cy - 24.0, r - 84.0, cy + 14.0], WHITE);
            g.text("Timer berjalan", &g.f.small, [l, cy + 14.0, r - 84.0, cy + 32.0], GRAY);
            chip(g, side, "Batal", CHIP, buttons, Action::StopTimer);
        }
        Timer::Off => {
            g.text("Pilih durasi", &g.f.small, [l, cy - 32.0, r, cy - 12.0], GRAY);
            for (i, minutes) in [5u64, 15, 25].into_iter().enumerate() {
                let x = l + i as f32 * 84.0;
                let color = if i == 2 { ACCENT } else { CHIP };
                chip(g, [x, cy - 6.0, x + 76.0, cy + 22.0], &format!("{minutes} mnt"), color, buttons, Action::Timer(minutes));
            }
        }
    }
}
