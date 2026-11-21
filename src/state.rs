//! Logika murni: bentuk notch, animasi, timer, dan format tanggal. Tidak menyentuh API Windows supaya bisa diuji.
use std::time::Duration;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum View {
    Hidden,
    Idle,
    Compact,
    Hud,
    Expanded,
}

#[derive(Clone, Copy, Default)]
pub struct Inputs {
    pub hidden: bool,
    pub hover: bool,
    pub ringing: bool,
    pub hud: bool,
    pub playing: bool,
}

pub fn view(i: Inputs) -> View {
    if i.hidden {
        View::Hidden
    } else if i.ringing || i.hover {
        View::Expanded
    } else if i.hud {
        View::Hud
    } else if i.playing {
        View::Compact
    } else {
        View::Idle
    }
}

pub fn mmss(d: Duration) -> String {
    let s = (d.as_millis() as u64).div_ceil(1000);
    format!("{}:{:02}", s / 60, s % 60)
}

/// Lama animasi pegas sampai dianggap diam.
pub const SPRING_SECS: f32 = 0.45;

/// Respons pegas teredam (zeta 0.8): sedikit melewati tujuan (~1,5%) lalu kembali.
/// Nilainya diukur dari animasi notch yang terasa "kenyal" tapi tidak berlebihan.
pub fn spring(t: f32) -> f32 {
    const ZETA: f32 = 0.8;
    const OMEGA: f32 = 18.0;
    if t >= SPRING_SECS {
        return 1.0;
    }
    let t = t.max(0.0);
    let wd = OMEGA * (1.0 - ZETA * ZETA).sqrt();
    let decay = (-ZETA * OMEGA * t).exp();
    1.0 - decay * ((wd * t).cos() + ZETA / (1.0 - ZETA * ZETA).sqrt() * (wd * t).sin())
}

const DAYS: [&str; 7] = ["Min", "Sen", "Sel", "Rab", "Kam", "Jum", "Sab"];
const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "Mei", "Jun", "Jul", "Agu", "Sep", "Okt", "Nov", "Des"];

/// `dow` 0 = Minggu, `month` 1..=12, sesuai SYSTEMTIME.
pub fn date_id(dow: u16, day: u16, month: u16, year: u16) -> String {
    let d = DAYS.get(dow as usize).unwrap_or(&"");
    let m = MONTHS.get(month.wrapping_sub(1) as usize).unwrap_or(&"");
    format!("{d} {day} {m} {year}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn view_priority() {
        let i = Inputs::default();
        assert_eq!(view(i), View::Idle);
        assert_eq!(view(Inputs { playing: true, ..i }), View::Compact);
        assert_eq!(view(Inputs { playing: true, hud: true, ..i }), View::Hud);
        assert_eq!(view(Inputs { hud: true, hover: true, ..i }), View::Expanded);
        assert_eq!(view(Inputs { ringing: true, ..i }), View::Expanded);
        assert_eq!(view(Inputs { ringing: true, hover: true, hidden: true, ..i }), View::Hidden);
    }

    #[test]
    fn mmss_rounds_up() {
        assert_eq!(mmss(Duration::from_secs(300)), "5:00");
        assert_eq!(mmss(Duration::from_millis(299_100)), "5:00");
        assert_eq!(mmss(Duration::from_millis(900)), "0:01");
        assert_eq!(mmss(Duration::ZERO), "0:00");
    }

    #[test]
    fn spring_overshoots_slightly_then_settles() {
        assert!(spring(0.0).abs() < 1e-6);
        let peak = (0..450).map(|ms| spring(ms as f32 / 1000.0)).fold(0.0, f32::max);
        assert!((1.01..1.025).contains(&peak), "puncak {peak}");
        assert!((spring(SPRING_SECS - 0.001) - 1.0).abs() < 0.005);
        assert_eq!(spring(SPRING_SECS), 1.0);
    }

    #[test]
    fn indonesian_date() {
        assert_eq!(date_id(6, 21, 11, 2026), "Sab 21 Nov 2026");
        assert_eq!(date_id(0, 1, 5, 2027), "Min 1 Mei 2027");
    }
}
