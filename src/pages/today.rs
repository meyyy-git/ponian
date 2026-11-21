use super::card;
use crate::{
    App, clock, online, prayer,
    render::{GRAY, Gfx, R, WHITE},
    state::mmss,
    timer::Run,
    weather,
};
use std::time::Instant;

pub fn draw(app: &App, g: &Gfx, [l, t, r, b]: R) {
    let now = clock::now();
    let w = (r - l - 16.0) / 3.0;
    let col = |i: f32| [l + i * (w + 8.0), t, l + i * (w + 8.0) + w, t + 60.0];

    let today_tasks: Vec<_> = app.tasks.iter().filter(|t| t.day == now.day || !t.done).collect();
    let done = today_tasks.iter().filter(|t| t.done).count();
    let next_task = today_tasks.iter().filter(|t| !t.done && t.time.is_some_and(|m| m >= now.min)).min_by_key(|t| t.time);
    card(g, col(0.0), "Tasks", &format!("{done}/{}", today_tasks.len()), &next_task.map_or("Nothing due".into(), |t| format!("{} {}", clock::hhmm(t.time.unwrap_or(0)), t.title)));

    let status = match app.timer.run {
        Run::Ready => "Ready",
        Run::Running(_) => "Running",
        Run::Paused(_) => "Paused",
        Run::Ringing => "Time\u{2019}s up",
    };
    card(g, col(1.0), "Timer", &mmss(app.timer.remaining(Instant::now())), status);

    match online::WEATHER.lock().unwrap().as_ref() {
        Some(wx) => card(g, col(2.0), "Weather", &format!("{:.0}\u{B0}", wx.temp), weather::describe(&wx.symbol).0),
        None => card(g, col(2.0), "Weather", "\u{2013}", "No data yet"),
    }

    let line = match app.prayer {
        Some(times) => {
            let (n, wait) = super::prayer::next(&times, now.min);
            format!("{} {} \u{B7} in {}", prayer::NAMES[n], clock::hhmm(times[n]), clock::span(wait))
        }
        None => "Prayer times: set a location in Settings".into(),
    };
    g.text("Next",&g.f.small, [l, t + 66.0, l + 80.0, t + 86.0], GRAY);
    g.text(&line, &g.f.text, [l + 80.0, t + 66.0, r, t + 86.0], WHITE);

    let notice = app.notices.last().map_or("None yet".to_string(), |(n, at)| {
        let ago = at.elapsed().as_secs() / 60;
        let when = if ago == 0 { "just now".to_string() } else { format!("{} ago", clock::span(ago as u32)) };
        format!("{} \u{B7} {} \u{B7} {when}", n.app, if app.notify_preview { n.title.as_str() } else { "new message" })
    });
    g.text("Notifications",&g.f.small, [l, t + 86.0, l + 80.0, b], GRAY);
    g.text(&notice, &g.f.text, [l + 80.0, t + 86.0, r, b], WHITE);
}
