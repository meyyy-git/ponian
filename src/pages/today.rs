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
    card(g, col(0.0), "Tugas", &format!("{done}/{}", today_tasks.len()), &next_task.map_or("Tanpa jadwal".into(), |t| format!("{} {}", clock::hhmm(t.time.unwrap_or(0)), t.title)));

    let status = match app.timer.run {
        Run::Ready => "Siap",
        Run::Running(_) => "Berjalan",
        Run::Paused(_) => "Dijeda",
        Run::Ringing => "Waktu habis",
    };
    card(g, col(1.0), "Timer", &mmss(app.timer.remaining(Instant::now())), status);

    match online::WEATHER.lock().unwrap().as_ref() {
        Some(wx) => card(g, col(2.0), "Cuaca", &format!("{:.0}\u{B0}", wx.temp), weather::describe(&wx.symbol).0),
        None => card(g, col(2.0), "Cuaca", "\u{2013}", "Belum ada data"),
    }

    let line = match app.prayer {
        Some(times) => {
            let (n, wait) = super::prayer::next(&times, now.min);
            format!("{} {} \u{B7} dalam {}", prayer::NAMES[n], clock::hhmm(times[n]), clock::span(wait))
        }
        None => "Jadwal salat: atur lokasi di Pengaturan".into(),
    };
    g.text("Berikutnya", &g.f.small, [l, t + 66.0, l + 80.0, t + 86.0], GRAY);
    g.text(&line, &g.f.text, [l + 80.0, t + 66.0, r, t + 86.0], WHITE);

    let notice = app.notices.last().map_or("Belum ada".to_string(), |(n, at)| {
        let ago = at.elapsed().as_secs() / 60;
        let when = if ago == 0 { "baru saja".to_string() } else { format!("{} lalu", clock::span(ago as u32)) };
        format!("{} \u{B7} {} \u{B7} {when}", n.app, if app.notify_preview { n.title.as_str() } else { "pesan baru" })
    });
    g.text("Notifikasi", &g.f.small, [l, t + 86.0, l + 80.0, b], GRAY);
    g.text(&notice, &g.f.text, [l + 80.0, t + 86.0, r, b], WHITE);
}
