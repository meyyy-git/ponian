//! Halaman-halaman di notch yang terbuka. Satu modul per halaman, dipilih lewat enum.
mod apps;
mod calendar;
mod music;
mod prayer;
mod shelf;
mod timer;
mod today;
mod todo;
pub mod widgets;

use crate::{
    Action, App,
    render::{CHIP, GRAY, Gfx, R, WHITE},
};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Page {
    Music,
    Apps,
    Timer,
    Shelf,
    Todo,
    Calendar,
    Today,
    Widgets,
    Prayer,
}

// urutan bawaan; urutan pilihan user disimpan di setting "Page.Order"
pub const ALL: [Page; 9] = [Page::Music, Page::Apps, Page::Timer, Page::Shelf, Page::Todo, Page::Calendar, Page::Today, Page::Widgets, Page::Prayer];

impl Page {
    pub fn title(self) -> &'static str {
        match self {
            Page::Music => "Music",
            Page::Apps => "Apps",
            Page::Timer => "Timer",
            Page::Shelf => "Shelf",
            Page::Todo => "To-do",
            Page::Calendar => "Calendar",
            Page::Today => "Today",
            Page::Widgets => "Widgets",
            Page::Prayer => "Prayer",
        }
    }

    /// Ikon halaman (font ikon Windows), tampil saat titik halaman di-hover.
    pub fn icon(self) -> &'static str {
        match self {
            Page::Music => crate::NOTE,
            Page::Apps => "\u{E71D}",
            Page::Timer => "\u{E916}",
            Page::Shelf => "\u{E896}",
            Page::Todo => "\u{E9D5}",
            Page::Calendar => "\u{E787}",
            Page::Today => "\u{E706}",
            Page::Widgets => "\u{E8A9}",
            Page::Prayer => "\u{E708}",
        }
    }

    /// Nama setting registry yang menyalakan halaman ini.
    pub fn key(self) -> &'static str {
        match self {
            Page::Music => "Page.Music",
            Page::Apps => "Page.Apps",
            Page::Timer => "Page.Timer",
            Page::Shelf => "Page.Shelf",
            Page::Todo => "Page.Todo",
            Page::Calendar => "Page.Calendar",
            Page::Today => "Page.Today",
            Page::Widgets => "Page.Widgets",
            Page::Prayer => "Page.Prayer",
        }
    }

    pub fn draw(self, app: &App, g: &Gfx, area: R, buttons: &mut Vec<(R, Action)>) {
        match self {
            Page::Music => music::draw(app, g, area, buttons),
            Page::Apps => apps::draw(app, g, area, buttons),
            Page::Timer => timer::draw(app, g, area, buttons),
            Page::Shelf => shelf::draw(app, g, area, buttons),
            Page::Todo => todo::draw(app, g, area, buttons),
            Page::Calendar => calendar::draw(app, g, area, buttons),
            Page::Today => today::draw(app, g, area),
            Page::Widgets => widgets::draw(app, g, area),
            Page::Prayer => prayer::draw(app, g, area),
        }
    }
}

/// Semua halaman dalam urutan pilihan user (setting "Page.Order"); halaman yang belum tercatat di belakang.
pub fn ordered() -> Vec<Page> {
    let saved = crate::settings::string("Page.Order");
    let mut pages: Vec<Page> = saved.split(',').filter_map(|k| ALL.into_iter().find(|p| p.key() == k.trim())).collect();
    pages.dedup();
    for p in ALL {
        if !pages.contains(&p) {
            pages.push(p);
        }
    }
    pages
}

/// Geser halaman ke-`index` satu langkah (dir -1 = naik) dan simpan urutannya.
pub fn move_page(index: usize, dir: i32) {
    let mut pages = ordered();
    let to = index as i32 + dir;
    if index < pages.len() && to >= 0 && (to as usize) < pages.len() {
        pages.swap(index, to as usize);
        crate::settings::set_string("Page.Order", &pages.iter().map(|p| p.key()).collect::<Vec<_>>().join(","));
    }
}

/// Halaman yang dinyalakan user dalam urutannya, minimal satu.
pub fn enabled() -> Vec<Page> {
    let pages: Vec<Page> = ordered().into_iter().filter(|p| crate::settings::flag(p.key(), true)).collect();
    if pages.is_empty() { vec![Page::Music] } else { pages }
}

/// Tombol kapsul kecil berlabel.
pub fn chip(g: &Gfx, r: R, label: &str, color: u32, buttons: &mut Vec<(R, Action)>, action: Action) {
    g.rrect(r, (r[3] - r[1]) / 2.0, color);
    g.text(label, if color == CHIP { &g.f.csmall } else { &g.f.cbold }, lifted(r), WHITE);
    buttons.push((r, action));
}

/// Kotak teks dinaikkan 1 DIP: huruf Segoe UI duduk sedikit di bawah tengah kotak barisnya, jadi tanpa
/// ini label di dalam kapsul terlihat turun.
pub fn lifted([l, t, r, b]: R) -> R {
    [l, t - 1.0, r, b - 1.0]
}

/// Tombol kapsul berisi satu ikon dari font ikon Windows (panah, tambah), yang tepat di tengah.
pub fn icon_chip(g: &Gfx, r: R, icon: &str, buttons: &mut Vec<(R, Action)>, action: Action) {
    g.rrect(r, (r[3] - r[1]) / 2.0, CHIP);
    g.text(icon, &g.f.icon_small, r, WHITE);
    buttons.push((r, action));
}

/// Kartu berlatar abu-abu gelap dengan label kecil di atas, nilai besar, dan keterangan.
/// `icon` digambar berwarna `tint` di depan label (glyph font ikon, atau emoji untuk cuaca).
pub fn card(g: &Gfx, [l, t, r, b]: R, icon: &str, tint: u32, label: &str, value: &str, sub: &str) {
    g.rrect([l, t, r, b], 12.0, 0x161616);
    // glyph font ikon ada di area pribadi Unicode; selain itu (emoji) digambar dengan font teks
    let glyph = icon.chars().next().is_some_and(|c| ('\u{E000}'..='\u{F8FF}').contains(&c));
    g.text(icon, if glyph { &g.f.icon_small } else { &g.f.csmall }, [l + 8.0, t + 4.0, l + 26.0, t + 21.0], tint);
    g.text(label, &g.f.small, [l + 28.0, t + 4.0, r - 8.0, t + 21.0], GRAY);
    g.text(value, &g.f.bold, [l + 12.0, t + 20.0, r - 10.0, t + 41.0], WHITE);
    g.text(sub, &g.f.small, [l + 12.0, t + 39.0, r - 10.0, b - 3.0], GRAY);
}
