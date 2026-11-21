use super::chip;
use crate::{
    Action, App,
    render::{ACCENT, CHIP, GRAY, Gfx, R, WHITE},
};

pub fn draw(app: &App, g: &Gfx, [l, t, r, b]: R, buttons: &mut Vec<(R, Action)>) {
    let a = &app.ask;
    let row = [b - 28.0, b];
    if !app.has_key && a.history.is_empty() && a.error.is_none() {
        g.text("Tanya apa saja ke Gemini", &g.f.bold, [l, t + 4.0, r, t + 28.0], WHITE);
        g.text(
            "Butuh API key Gemini milik Anda (gratis di aistudio.google.com). Salin key-nya, klik \u{201C}Tempel key\u{201D}, tekan Ctrl+V lalu Enter.",
            &g.f.para,
            [l, t + 32.0, r, b - 36.0],
            GRAY,
        );
        g.text("Hitung cepat tetap bisa tanpa key: ketik misalnya \u{201C}12,5 x 4 =\u{201D}.", &g.f.para, [l, t + 84.0, r, b - 36.0], GRAY);
        chip(g, [l, row[0], l + 110.0, row[1]], "Tempel key", ACCENT, buttons, Action::AskKey);
        chip(g, [l + 118.0, row[0], l + 210.0, row[1]], "Hitung", CHIP, buttons, Action::AskOpen(false));
        return;
    }

    let mut y = t;
    if let Some(ctx) = &a.context {
        let line: String = ctx.split_whitespace().collect::<Vec<_>>().join(" ");
        g.rrect([l, y, r, y + 24.0], 8.0, 0x161616);
        g.text(&format!("Dari layar: {line}"), &g.f.small, [l + 8.0, y, r - 8.0, y + 24.0], GRAY);
        y += 30.0;
    }
    let question = a.history.iter().rev().find(|m| m.0).map(|m| m.1.as_str());
    let answer = a.history.last().filter(|m| !m.0).map(|m| m.1.as_str());
    match (question, &a.error) {
        (None, None) if !a.thinking => {
            g.text("Tanya apa saja", &g.f.bold, [l, y + 4.0, r, y + 28.0], WHITE);
            g.text("Ctrl+Alt+Space dari mana saja. Akhiri dengan \u{201C}=\u{201D} untuk hitung cepat atau kurs, misalnya \u{201C}100 usd ke idr =\u{201D}.", &g.f.para, [l, y + 32.0, r, row[0] - 6.0], GRAY);
        }
        _ => {
            if let Some(q) = question {
                g.text(q, &g.f.small, [l, y, r, y + 18.0], GRAY);
                y += 22.0;
            }
            let (text, color) = match (&a.error, a.thinking, answer) {
                (Some(e), _, _) => (e.as_str(), 0xff8a80),
                (_, true, _) => (if a.translate { "Menerjemahkan\u{2026}" } else { "Sedang berpikir\u{2026}" }, GRAY),
                (_, _, Some(ans)) => (ans, WHITE),
                _ => ("", WHITE),
            };
            g.text(text, &g.f.para, [l, y, r, row[0] - 6.0], color);
        }
    }

    if app.has_key {
        chip(g, [l, row[0], l + 84.0, row[1]], "Tanya", ACCENT, buttons, Action::AskOpen(false));
        chip(g, [l + 92.0, row[0], l + 196.0, row[1]], "Terjemahkan", CHIP, buttons, Action::AskOpen(true));
    } else {
        chip(g, [l, row[0], l + 84.0, row[1]], "Hitung", ACCENT, buttons, Action::AskOpen(false));
        chip(g, [l + 92.0, row[0], l + 196.0, row[1]], "Tempel key", CHIP, buttons, Action::AskKey);
    }
    if answer.is_some() {
        chip(g, [r - 136.0, row[0], r - 72.0, row[1]], "Salin", CHIP, buttons, Action::AskCopy);
    }
    if !a.history.is_empty() || a.context.is_some() {
        chip(g, [r - 64.0, row[0], r, row[1]], "Baru", CHIP, buttons, Action::AskClear);
    }
}
