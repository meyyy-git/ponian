use crate::{
    Action, App, lyrics,
    media::Msg,
    render::{GRAY, Gfx, R, WHITE},
};

pub fn draw(app: &App, g: &Gfx, [l, t, r, b]: R, buttons: &mut Vec<(R, Action)>) {
    let m = &app.media;
    if !m.session {
        g.text("Tidak ada media diputar", &g.f.csmall, [l, t, r, b], GRAY);
        return;
    }
    g.text(&m.title, &g.f.bold, [l, t, r, t + 22.0], WHITE);
    g.text(&m.artist, &g.f.small, [l, t + 22.0, r, t + 40.0], GRAY);

    let cx = (l + r) / 2.0;
    let y = t + 42.0;
    let toggle = if m.playing { "\u{E769}" } else { "\u{E768}" };
    let controls: [(&str, fn() -> Msg); 3] = [("\u{E892}", || Msg::Prev), (toggle, || Msg::Toggle), ("\u{E893}", || Msg::Next)];
    for (i, (glyph, msg)) in controls.into_iter().enumerate() {
        let x = cx - 72.0 + i as f32 * 48.0;
        let btn = [x, y, x + 48.0, y + 30.0];
        g.text(glyph, &g.f.icon, btn, WHITE);
        buttons.push((btn, Action::Media(msg)));
    }

    let Some(lines) = &m.lyrics else { return };
    let i = lyrics::active(lines, app.lyric_pos());
    let text = |i: Option<usize>| i.and_then(|i| lines.get(i)).map_or("", |l| l.1.as_str());
    let y = (y + 34.0).min(b - 36.0);
    g.text(text(i), &g.f.center, [l, y, r, y + 20.0], WHITE);
    g.text(text(Some(i.map_or(0, |i| i + 1))), &g.f.csmall, [l, y + 18.0, r, y + 36.0], GRAY);
}
