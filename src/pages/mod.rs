//! Halaman-halaman di notch yang terbuka. Satu modul per halaman, dipilih lewat enum.
mod music;
mod pomodoro;
mod prayer;
mod today;
mod todo;
mod water;
pub mod widgets;

use crate::{
    Action, App,
    render::{CHIP, GRAY, Gfx, R, WHITE},
};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Page {
    Music,
    Today,
    Widgets,
    Pomodoro,
    Todo,
    Water,
    Prayer,
}

// shortcut: urutan halaman tetap mengikuti daftar ini; user hanya bisa menyalakan atau mematikan.
pub const ALL: [Page; 7] = [Page::Music, Page::Today, Page::Widgets, Page::Pomodoro, Page::Todo, Page::Water, Page::Prayer];

impl Page {
    pub fn title(self) -> &'static str {
        match self {
            Page::Music => "Musik",
            Page::Today => "Hari ini",
            Page::Widgets => "Widget",
            Page::Pomodoro => "Pomodoro",
            Page::Todo => "To-do",
            Page::Water => "Minum air",
            Page::Prayer => "Salat",
        }
    }

    /// Nama setting registry yang menyalakan halaman ini.
    pub fn key(self) -> &'static str {
        match self {
            Page::Music => "Page.Music",
            Page::Today => "Page.Today",
            Page::Widgets => "Page.Widgets",
            Page::Pomodoro => "Page.Pomodoro",
            Page::Todo => "Page.Todo",
            Page::Water => "Page.Water",
            Page::Prayer => "Page.Prayer",
        }
    }

    pub fn draw(self, app: &App, g: &Gfx, area: R, buttons: &mut Vec<(R, Action)>) {
        match self {
            Page::Music => music::draw(app, g, area, buttons),
            Page::Today => today::draw(app, g, area),
            Page::Widgets => widgets::draw(app, g, area),
            Page::Pomodoro => pomodoro::draw(app, g, area, buttons),
            Page::Todo => todo::draw(app, g, area, buttons),
            Page::Water => water::draw(app, g, area, buttons),
            Page::Prayer => prayer::draw(app, g, area),
        }
    }
}

/// Halaman yang dinyalakan user, minimal satu.
pub fn enabled() -> Vec<Page> {
    let pages: Vec<Page> = ALL.into_iter().filter(|p| crate::settings::flag(p.key(), true)).collect();
    if pages.is_empty() { vec![Page::Music] } else { pages }
}

/// Tombol kapsul kecil berlabel.
pub fn chip(g: &Gfx, r: R, label: &str, color: u32, buttons: &mut Vec<(R, Action)>, action: Action) {
    g.rrect(r, (r[3] - r[1]) / 2.0, color);
    g.text(label, if color == CHIP { &g.f.csmall } else { &g.f.cbold }, r, WHITE);
    buttons.push((r, action));
}

/// Kartu berlatar abu-abu gelap dengan label kecil di atas, nilai besar, dan keterangan.
pub fn card(g: &Gfx, [l, t, r, b]: R, label: &str, value: &str, sub: &str) {
    g.rrect([l, t, r, b], 12.0, 0x161616);
    g.text(label, &g.f.small, [l + 12.0, t + 6.0, r - 10.0, t + 24.0], GRAY);
    g.text(value, &g.f.bold, [l + 12.0, t + 24.0, r - 10.0, t + 46.0], WHITE);
    g.text(sub, &g.f.small, [l + 12.0, t + 44.0, r - 10.0, b - 4.0], GRAY);
}
