//! Halaman-halaman di notch yang terbuka. Satu modul per halaman, dipilih lewat enum.
mod music;
mod timer;

use crate::{
    Action, App,
    render::{CHIP, Gfx, R, WHITE},
};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Page {
    Music,
    Timer,
}

// shortcut: urutan halaman tetap mengikuti daftar ini; user hanya bisa menyalakan atau mematikan.
pub const ALL: [Page; 2] = [Page::Music, Page::Timer];

impl Page {
    pub fn title(self) -> &'static str {
        match self {
            Page::Music => "Musik",
            Page::Timer => "Timer",
        }
    }

    /// Nama setting registry yang menyalakan halaman ini.
    pub fn key(self) -> &'static str {
        match self {
            Page::Music => "Page.Music",
            Page::Timer => "Page.Timer",
        }
    }

    pub fn draw(self, app: &App, g: &Gfx, area: R, buttons: &mut Vec<(R, Action)>) {
        match self {
            Page::Music => music::draw(app, g, area, buttons),
            Page::Timer => timer::draw(app, g, area, buttons),
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
