use super::chip;
use crate::{
    Action, App,
    render::{ACCENT, CHIP, GRAY, Gfx, R, TRACK, WHITE},
};

const BLUE: u32 = 0x4aa8ff;

pub fn draw(app: &App, g: &Gfx, [l, t, r, _b]: R, buttons: &mut Vec<(R, Action)>) {
    let w = app.water_cfg;
    let frac = (app.water as f32 / w.goal.max(1) as f32).min(1.0);
    let liters = |ml: u32| format!("{:.1}", ml as f32 / 1000.0);

    let big = liters(app.water);
    let bw = g.width(&big, &g.f.big);
    g.text(&big, &g.f.big, [l, t + 2.0, l + bw + 2.0, t + 36.0], WHITE);
    g.text(&format!("/ {} L", liters(w.goal)), &g.f.bold, [l + bw + 7.0, t + 12.0, l + 114.0, t + 34.0], GRAY);

    // sakelar pengingat
    let sw = [l + 118.0, t + 9.0, l + 150.0, t + 27.0];
    g.rrect(sw, 9.0, if w.on { ACCENT } else { 0x3a3a3a });
    let knob = if w.on { sw[2] - 16.0 } else { sw[0] + 2.0 };
    g.rrect([knob, sw[1] + 2.0, knob + 14.0, sw[3] - 2.0], 7.0, WHITE);
    buttons.push((sw, Action::WaterToggle));

    let reminders = if w.on { "Pengingat nyala" } else { "Pengingat mati" };
    g.text(&format!("Hari ini \u{B7} {:.0}% \u{B7} {reminders}", frac * 100.0), &g.f.small, [l, t + 36.0, l + 160.0, t + 52.0], GRAY);
    g.rrect([l, t + 55.0, l + 150.0, t + 58.0], 1.5, TRACK);
    if frac > 0.0 {
        g.rrect([l, t + 55.0, l + 150.0 * frac, t + 58.0], 1.5, BLUE);
    }

    let row = [t + 72.0, t + 98.0];
    let drink = [l, row[0], l + 62.0, row[1]];
    g.rrect(drink, 13.0, 0x0e2740);
    g.text("\u{EB42}", &g.f.icon_small, [l + 6.0, row[0], l + 22.0, row[1]], BLUE);
    g.text("Minum", &g.f.small, [l + 22.0, row[0], l + 62.0, row[1]], BLUE);
    buttons.push((drink, Action::WaterAdd));
    chip(g, [l + 68.0, row[0], l + 116.0, row[1]], "Atur", CHIP, buttons, Action::OpenSettings);
    chip(g, [l + 122.0, row[0], l + 150.0, row[1]], "\u{2212}", CHIP, buttons, Action::WaterUndo);

    g.rrect([l + 164.0, t + 4.0, l + 165.0, t + 96.0], 0.5, 0x2a2a2a);

    // meteran: busur 270 derajat yang terbuka di bawah
    let (cx, cy) = ((l + 165.0 + r) / 2.0, t + 52.0);
    g.arc(cx, cy, 34.0, 5.0, 225.0, 270.0, 0x262626);
    g.arc(cx, cy, 34.0, 5.0, 225.0, 270.0 * frac, BLUE);
    g.text(&format!("{:.0}", frac * 100.0), &g.f.cbig, [cx - 34.0, cy - 18.0, cx + 34.0, cy + 12.0], WHITE);
    g.text("%", &g.f.csmall, [cx - 20.0, cy + 9.0, cx + 20.0, cy + 25.0], GRAY);
}
