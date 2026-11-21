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
pub const PRESETS: [u64; 5] = [5, 10, 15, 25, 45];

/// Durasi ketikan user: "90" = 90 menit, "1:30" = 1 jam 30 menit. Maksimal 24 jam.
pub fn parse_minutes(s: &str) -> Option<u64> {
    let s = s.trim();
    let minutes = match s.split_once([':', '.']) {
        Some((h, m)) => h.trim().parse::<u64>().ok()? * 60 + m.trim().parse::<u64>().ok().filter(|m| *m < 60)?,
        None => s.parse().ok()?,
    };
    (1..=1440).contains(&minutes).then_some(minutes)
}

/// Stopwatch: waktu berjalan sejak dimulai, bisa dijeda.
#[derive(Clone, Copy, Default)]
pub struct Stopwatch {
    since: Option<Instant>,
    banked: Duration,
}

impl Stopwatch {
    pub fn toggle(&mut self, now: Instant) {
        match self.since.take() {
            Some(since) => self.banked += now.saturating_duration_since(since),
            None => self.since = Some(now),
        }
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn elapsed(&self, now: Instant) -> Duration {
        self.banked + self.since.map_or(Duration::ZERO, |s| now.saturating_duration_since(s))
    }

    pub fn running(&self) -> bool {
        self.since.is_some()
    }
}

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
    fn stopwatch_and_custom_duration() {
        let t0 = Instant::now();
        let mut s = Stopwatch::default();
        s.toggle(t0);
        s.toggle(t0 + Duration::from_secs(10));
        // waktu jeda tidak dihitung
        s.toggle(t0 + Duration::from_secs(50));
        assert_eq!(s.elapsed(t0 + Duration::from_secs(55)), Duration::from_secs(15));
        assert!(s.running());
        s.reset();
        assert_eq!(s.elapsed(t0), Duration::ZERO);
        assert_eq!((parse_minutes("90"), parse_minutes(" 1:30 "), parse_minutes("0"), parse_minutes("1:75"), parse_minutes("abc")), (Some(90), Some(90), None, None, None));
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
