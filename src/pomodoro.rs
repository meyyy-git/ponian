//! Pomodoro: fokus dan istirahat bergantian, 4 ronde lalu istirahat panjang.
use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Phase {
    Focus,
    Short,
    Long,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Run {
    Ready,
    Running(Instant),
    Paused(Duration),
    Ringing,
}

/// Durasi dalam menit.
#[derive(Clone, Copy)]
pub struct Durations {
    pub focus: u64,
    pub short: u64,
    pub long: u64,
}

pub const ROUNDS: u8 = 4;

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Pomo {
    pub phase: Phase,
    /// 1..=ROUNDS
    pub round: u8,
    pub run: Run,
}

impl Default for Pomo {
    fn default() -> Self {
        Pomo { phase: Phase::Focus, round: 1, run: Run::Ready }
    }
}

impl Pomo {
    pub fn len(&self, d: Durations) -> Duration {
        let min = match self.phase {
            Phase::Focus => d.focus,
            Phase::Short => d.short,
            Phase::Long => d.long,
        };
        Duration::from_secs(min * 60)
    }

    /// Tombol utama: mulai, jeda, lanjut, atau (saat berbunyi) pindah ke fase berikutnya.
    pub fn toggle(&mut self, now: Instant, d: Durations) {
        self.run = match self.run {
            Run::Ready => Run::Running(now + self.len(d)),
            Run::Running(end) => Run::Paused(end.saturating_duration_since(now)),
            Run::Paused(left) => Run::Running(now + left),
            Run::Ringing => {
                self.advance();
                return;
            }
        };
    }

    pub fn reset(&mut self) {
        self.run = Run::Ready;
    }

    /// Mengembalikan fase yang baru selesai, tepat sekali.
    pub fn tick(&mut self, now: Instant) -> Option<Phase> {
        match self.run {
            Run::Running(end) if now >= end => {
                self.run = Run::Ringing;
                Some(self.phase)
            }
            _ => None,
        }
    }

    pub fn advance(&mut self) {
        (self.phase, self.round) = match (self.phase, self.round) {
            (Phase::Focus, r) if r >= ROUNDS => (Phase::Long, r),
            (Phase::Focus, r) => (Phase::Short, r),
            (Phase::Short, r) => (Phase::Focus, r + 1),
            (Phase::Long, _) => (Phase::Focus, 1),
        };
        self.run = Run::Ready;
    }

    pub fn remaining(&self, now: Instant, d: Durations) -> Duration {
        match self.run {
            Run::Ready => self.len(d),
            Run::Running(end) => end.saturating_duration_since(now),
            Run::Paused(left) => left,
            Run::Ringing => Duration::ZERO,
        }
    }

    pub fn running(&self) -> bool {
        matches!(self.run, Run::Running(_))
    }

    /// Lama satu siklus penuh: 4 fokus, 3 istirahat pendek, 1 istirahat panjang.
    pub fn cycle_minutes(d: Durations) -> u64 {
        ROUNDS as u64 * d.focus + (ROUNDS as u64 - 1) * d.short + d.long
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const D: Durations = Durations { focus: 25, short: 5, long: 15 };

    #[test]
    fn start_pause_resume_finish() {
        let t0 = Instant::now();
        let mut p = Pomo::default();
        p.toggle(t0, D);
        assert_eq!(p.remaining(t0, D), Duration::from_secs(1500));
        p.toggle(t0 + Duration::from_secs(100), D);
        assert_eq!(p.run, Run::Paused(Duration::from_secs(1400)));
        // waktu jeda tidak dihitung
        p.toggle(t0 + Duration::from_secs(500), D);
        assert_eq!(p.tick(t0 + Duration::from_secs(1899)), None);
        assert_eq!(p.tick(t0 + Duration::from_secs(1900)), Some(Phase::Focus));
        assert_eq!(p.run, Run::Ringing);
        assert_eq!(p.tick(t0 + Duration::from_secs(1901)), None);
        p.toggle(t0, D);
        assert_eq!((p.phase, p.round, p.run), (Phase::Short, 1, Run::Ready));
    }

    #[test]
    fn full_cycle_order() {
        let mut p = Pomo::default();
        let mut seen = vec![];
        for _ in 0..9 {
            seen.push((p.phase, p.round));
            p.advance();
        }
        use Phase::*;
        assert_eq!(seen, vec![(Focus, 1), (Short, 1), (Focus, 2), (Short, 2), (Focus, 3), (Short, 3), (Focus, 4), (Long, 4), (Focus, 1)]);
        assert_eq!(Pomo::cycle_minutes(D), 130);
    }
}
