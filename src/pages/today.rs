use super::card;
use crate::{
    App, calendar, clock, online, prayer,
    render::{ACCENT, GRAY, GREEN, Gfx, R, WHITE},
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
    let due = next_task.map_or("Nothing due".into(), |t| format!("{} {}", clock::hhmm(t.time.unwrap_or(0)), t.title));
    card(g, col(0.0), "\u{E9D5}", GREEN, "Tasks", &format!("{done}/{}", today_tasks.len()), &due);

    // kartu tengah: tanggal merah berikutnya; selagi timer dipakai, kartunya menampilkan timer
    let holidays = online::HOLIDAYS.lock().unwrap();
    match calendar::next_holiday(&holidays, now.day).filter(|_| matches!(app.timer.run, Run::Ready)) {
        Some((name, day)) => {
            let when = match day - now.day {
                0 => "Today".to_string(),
                1 => "Tomorrow".to_string(),
                n => format!("In {n} days"),
            };
            // keterangan dalam kurung ("(tentative)", "(Nyepi)") dibuang supaya namanya muat
            card(g, col(1.0), "\u{E7C1}", super::calendar::RED, "Holiday", &when, name.split(" (").next().unwrap_or(name));
        }
        None => {
            let status = match app.timer.run {
                Run::Ready => "Ready",
                Run::Running(_) => "Running",
                Run::Paused(_) => "Paused",
                Run::Ringing => "Time\u{2019}s up",
            };
            card(g, col(1.0), "\u{E916}", ACCENT, "Timer", &mmss(app.timer.remaining(Instant::now())), status);
        }
    }

    match online::WEATHER.lock().unwrap().as_ref() {
        Some(wx) => {
            let (text, emoji) = weather::describe(&wx.symbol);
            card(g, col(2.0), emoji, WHITE, "Weather", &format!("{:.0}\u{B0}", wx.temp), text);
        }
        None => card(g, col(2.0), "\u{E753}", GRAY, "Weather", "\u{2013}", "No data yet"),
    }

    let row = |icon: &str, label: &str, value: &str, top: f32, bottom: f32| {
        g.text(icon, &g.f.icon_small, [l - 1.0, top, l + 13.0, bottom], GRAY);
        g.text(label, &g.f.small, [l + 18.0, top, l + 98.0, bottom], GRAY);
        g.text(value, &g.f.text, [l + 100.0, top, r, bottom], WHITE);
    };
    let line = match app.prayer {
        Some(times) => {
            let (n, wait) = super::prayer::next(&times, now.min);
            format!("{} {} \u{B7} in {}", prayer::NAMES[n], clock::hhmm(times[n]), clock::span(wait))
        }
        None => "Set a location in Settings".into(),
    };
    row("\u{E708}", "Prayer", &line, t + 66.0, t + 86.0);

    let notice = app.notices.last().map_or("None yet".to_string(), |(n, at)| {
        let ago = at.elapsed().as_secs() / 60;
        let when = if ago == 0 { "just now".to_string() } else { format!("{} ago", clock::span(ago as u32)) };
        format!("{} \u{B7} {} \u{B7} {when}", n.app, if app.notify_preview { n.title.as_str() } else { "new message" })
    });
    row("\u{EA8F}", "Notifications", &notice, t + 86.0, b);
}
