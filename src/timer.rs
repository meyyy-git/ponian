//! Timer hitung mundur dengan durasi pilihan.
use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Run {
    Ready,
    Running(Instant),
    Paused(Duration),
    Ringing,
}

/// Pilihan durasi dalam menit.
pub const PRESETS: [u64; 6] = [1, 5, 10, 15, 25, 45];

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Timer {
    pub minutes: u64,
    pub run: Run,
}

impl Timer {
    pub fn new(minutes: u64) -> Self {
        Timer { minutes: minutes.max(1), run: Run::Ready }
    }

    pub fn len(&self) -> Duration {
        Duration::from_secs(self.minutes * 60)
    }

    /// Tombol utama: mulai, jeda, lanjut, atau (saat berbunyi) kembali siap.
    pub fn toggle(&mut self, now: Instant) {
        self.run = match self.run {
            Run::Ready => Run::Running(now + self.len()),
            Run::Running(end) => Run::Paused(end.saturating_duration_since(now)),
            Run::Paused(left) => Run::Running(now + left),
            Run::Ringing => Run::Ready,
        };
    }

    /// Ganti durasi; timer yang sedang berjalan dihentikan.
    pub fn set(&mut self, minutes: u64) {
        *self = Timer::new(minutes);
    }

    pub fn reset(&mut self) {
        self.run = Run::Ready;
    }

    /// Mengembalikan true tepat saat waktu habis.
    pub fn tick(&mut self, now: Instant) -> bool {
        let done = matches!(self.run, Run::Running(end) if now >= end);
        if done {
            self.run = Run::Ringing;
        }
        done
    }

    pub fn remaining(&self, now: Instant) -> Duration {
        match self.run {
            Run::Ready => self.len(),
            Run::Running(end) => end.saturating_duration_since(now),
            Run::Paused(left) => left,
            Run::Ringing => Duration::ZERO,
        }
    }

    pub fn running(&self) -> bool {
        matches!(self.run, Run::Running(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_pause_resume_finish() {
        let t0 = Instant::now();
        let mut t = Timer::new(5);
        assert_eq!(t.remaining(t0), Duration::from_secs(300));
        t.toggle(t0);
        t.toggle(t0 + Duration::from_secs(100));
        assert_eq!(t.run, Run::Paused(Duration::from_secs(200)));
        // waktu jeda tidak dihitung
        t.toggle(t0 + Duration::from_secs(500));
        assert!(!t.tick(t0 + Duration::from_secs(699)));
        assert!(t.tick(t0 + Duration::from_secs(700)));
        assert_eq!(t.run, Run::Ringing);
        assert!(!t.tick(t0 + Duration::from_secs(701)));
        t.toggle(t0);
        assert_eq!((t.run, t.minutes), (Run::Ready, 5));
    }

    #[test]
    fn changing_duration_stops_the_timer() {
        let t0 = Instant::now();
        let mut t = Timer::new(5);
        t.toggle(t0);
        t.set(25);
        assert_eq!((t.run, t.remaining(t0)), (Run::Ready, Duration::from_secs(1500)));
        assert_eq!(Timer::new(0).minutes, 1);
    }
}
