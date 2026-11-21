use crate::{
    App, clock, prayer,
    render::{ACCENT, GRAY, Gfx, R, WHITE},
};

/// Indeks waktu salat berikutnya (melewati waktu Terbit) dan menit menuju ke sana.
pub fn next(times: &[u32; 6], now_min: u32) -> (usize, u32) {
    let order = [0, 2, 3, 4, 5];
    for i in order {
        if times[i] > now_min {
            return (i, times[i] - now_min);
        }
    }
    (0, times[0] + 1440 - now_min)
}

pub fn draw(app: &App, g: &Gfx, [l, t, r, b]: R) {
    let Some(times) = app.prayer else {
        g.text("Atur lokasi di Pengaturan \u{203A} Salat untuk melihat jadwal.", &g.f.csmall, [l, t, r, b], GRAY);
        return;
    };
    let now = clock::now();
    let (n, wait) = next(&times, now.min);
    g.text(&format!("{} dalam {}", prayer::NAMES[n], clock::span(wait)), &g.f.bold, [l, t, r, t + 24.0], WHITE);
    g.text(&app.prayer_info, &g.f.small, [l, t + 24.0, r, t + 42.0], GRAY);

    let cols = [0, 2, 3, 4, 5];
    let w = (r - l) / cols.len() as f32;
    for (c, &i) in cols.iter().enumerate() {
        let x = l + c as f32 * w;
        let box_ = [x + 2.0, t + 52.0, x + w - 2.0, t + 100.0];
        let current = i == n;
        g.rrect(box_, 10.0, if current { ACCENT } else { 0x161616 });
        g.text(prayer::NAMES[i], &g.f.csmall, [box_[0], box_[1] + 4.0, box_[2], box_[1] + 22.0], if current { WHITE } else { GRAY });
        g.text(&clock::hhmm(times[i]), &g.f.cbold, [box_[0], box_[1] + 22.0, box_[2], box_[3] - 4.0], WHITE);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn next_prayer_skips_sunrise_and_wraps() {
        let times = [245, 323, 702, 904, 1074, 1148];
        assert_eq!(super::next(&times, 300), (2, 402));
        assert_eq!(super::next(&times, 702), (3, 202));
        assert_eq!(super::next(&times, 1200), (0, 485));
    }
}
