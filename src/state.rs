//! Logika murni: bentuk pill dan timer. Tidak menyentuh API Windows supaya bisa diuji.
use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum View {
    Hidden,
    Line,
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
    pub timer: bool,
}

pub fn view(i: Inputs) -> View {
    if i.hidden {
        View::Hidden
    } else if i.ringing || i.hover {
        View::Expanded
    } else if i.hud {
        View::Hud
    } else if i.playing || i.timer {
        View::Compact
    } else {
        View::Line
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Timer {
    Off,
    Running(Instant),
    Ringing,
}

impl Timer {
    pub fn start(minutes: u64, now: Instant) -> Self {
        Timer::Running(now + Duration::from_secs(minutes * 60))
    }

    /// Mengembalikan true tepat saat timer habis.
    pub fn tick(&mut self, now: Instant) -> bool {
        let done = matches!(*self, Timer::Running(end) if now >= end);
        if done {
            *self = Timer::Ringing;
        }
        done
    }

    pub fn remaining(&self, now: Instant) -> Option<Duration> {
        match self {
            Timer::Running(end) => Some(end.saturating_duration_since(now)),
            _ => None,
        }
    }
}

pub fn mmss(d: Duration) -> String {
    let s = (d.as_millis() as u64).div_ceil(1000);
    format!("{}:{:02}", s / 60, s % 60)
}

pub fn ease(t: f32) -> f32 {
    1.0 - (1.0 - t.clamp(0.0, 1.0)).powi(3)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn view_priority() {
        let i = Inputs::default();
        assert_eq!(view(i), View::Line);
        assert_eq!(view(Inputs { playing: true, ..i }), View::Compact);
        assert_eq!(view(Inputs { timer: true, ..i }), View::Compact);
        assert_eq!(view(Inputs { playing: true, hud: true, ..i }), View::Hud);
        assert_eq!(view(Inputs { hud: true, hover: true, ..i }), View::Expanded);
        assert_eq!(view(Inputs { ringing: true, ..i }), View::Expanded);
        assert_eq!(view(Inputs { ringing: true, hover: true, hidden: true, ..i }), View::Hidden);
    }

    #[test]
    fn timer_lifecycle() {
        let t0 = Instant::now();
        let mut t = Timer::start(5, t0);
        assert_eq!(t.remaining(t0), Some(Duration::from_secs(300)));
        assert!(!t.tick(t0 + Duration::from_secs(299)));
        assert!(t.tick(t0 + Duration::from_secs(300)));
        assert_eq!(t, Timer::Ringing);
        assert!(!t.tick(t0 + Duration::from_secs(301)));
        assert_eq!(t.remaining(t0), None);
    }

    #[test]
    fn mmss_rounds_up() {
        assert_eq!(mmss(Duration::from_secs(300)), "5:00");
        assert_eq!(mmss(Duration::from_millis(299_100)), "5:00");
        assert_eq!(mmss(Duration::from_millis(900)), "0:01");
        assert_eq!(mmss(Duration::ZERO), "0:00");
    }
}
