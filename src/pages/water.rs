use super::chip;
use crate::{
    Action, App, clock,
    render::{ACCENT, CHIP, GRAY, Gfx, R, WHITE},
    water::ml,
};

pub fn draw(app: &App, g: &Gfx, [l, t, r, _b]: R, buttons: &mut Vec<(R, Action)>) {
    let w = app.water_cfg;
    g.text(&ml(app.water), &g.f.big, [l, t + 2.0, l + 190.0, t + 42.0], WHITE);
    g.text(&format!("dari {}", ml(w.goal)), &g.f.small, [l, t + 42.0, l + 190.0, t + 60.0], GRAY);

    let frac = (app.water as f32 / w.goal.max(1) as f32).min(1.0);
    let y = t + 70.0;
    g.rrect([l, y, r, y + 8.0], 4.0, 0x2a2a2a);
    if frac > 0.0 {
        g.rrect([l, y, l + (r - l) * frac, y + 8.0], 4.0, 0x3aa0ff);
    }
    let left = w.goal.saturating_sub(app.water);
    let info = if left == 0 { "Target hari ini tercapai".to_string() } else { format!("Sisa {} \u{B7} aktif {}\u{2013}{}", ml(left), clock::hhmm(w.from * 60), clock::hhmm(w.until * 60)) };
    g.text(&info, &g.f.small, [l, y + 12.0, r, y + 30.0], GRAY);

    chip(g, [r - 128.0, t + 8.0, r - 40.0, t + 36.0], &format!("+ {} ml", w.glass), ACCENT, buttons, Action::WaterAdd);
    chip(g, [r - 32.0, t + 8.0, r, t + 36.0], "\u{2212}", CHIP, buttons, Action::WaterUndo);
}
