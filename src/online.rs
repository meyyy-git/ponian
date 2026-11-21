//! Thread pekerja untuk data internet: cuaca, kurs, dan kalender. Hasilnya disimpan di sini dan
//! window notch diberi tahu lewat WM_APP_ONLINE.
use crate::{WM_APP_ONLINE, calendar, clock, currency, location, settings, weather};
use std::{
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering::Relaxed},
    },
    thread,
    time::{Duration, Instant},
};
use windows::Win32::{
    Foundation::{HWND, LPARAM, WPARAM},
    System::Com::{COINIT_MULTITHREADED, CoInitializeEx},
    UI::WindowsAndMessaging::PostMessageW,
};

pub static WEATHER: Mutex<Option<weather::Weather>> = Mutex::new(None);
pub static RATES: Mutex<Option<currency::Rates>> = Mutex::new(None);
pub static EVENTS: Mutex<Vec<calendar::Event>> = Mutex::new(Vec::new());
/// hari libur nasional dan hari peringatan Indonesia
pub static HOLIDAYS: Mutex<Vec<calendar::Event>> = Mutex::new(Vec::new());
/// Kalender hari libur Indonesia publik milik Google, versi nama berbahasa Inggris. Permintaannya tidak
/// membawa data user apa pun.
const HOLIDAYS_URL: &str = "https://calendar.google.com/calendar/ical/en.indonesian%23holiday%40group.v.calendar.google.com/public/basic.ics";
const HOLIDAYS_EVERY: Duration = Duration::from_secs(24 * 3600);
/// diset saat setting berubah supaya semua data diambil ulang
pub static FORCE: AtomicBool = AtomicBool::new(true);

const WEATHER_EVERY: Duration = Duration::from_secs(30 * 60);
const RATES_EVERY: Duration = Duration::from_secs(12 * 3600);
const CALENDAR_EVERY: Duration = Duration::from_secs(30 * 60);

pub fn spawn(hwnd: isize) {
    thread::spawn(move || {
        if unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_err() {
            return;
        }
        let due = |last: Option<Instant>, every: Duration| last.is_none_or(|t| t.elapsed() >= every);
        let (mut w_at, mut r_at, mut c_at): (Option<Instant>, Option<Instant>, Option<Instant>) = (None, None, None);
        let mut h_at: Option<Instant> = None;
        let holidays = |ics: &str| *HOLIDAYS.lock().unwrap() = calendar::parse(ics, clock::now().offset);
        loop {
            if FORCE.swap(false, Relaxed) {
                (w_at, r_at, c_at) = (None, None, None);
            }
            let mut changed = false;
            if settings::flag("Weather.On", true) && due(w_at, WEATHER_EVERY) {
                if let Some((lat, lon)) = location::current() {
                    let w = weather::fetch(lat, lon);
                    // gagal: coba lagi 5 menit kemudian, bukan 30 menit
                    w_at = Some(if w.is_some() { Instant::now() } else { Instant::now() - WEATHER_EVERY + Duration::from_secs(300) });
                    if w.is_some() {
                        *WEATHER.lock().unwrap() = w;
                        changed = true;
                    }
                }
            }
            if settings::flag("Rates.On", true) && due(r_at, RATES_EVERY) {
                let r = currency::fetch();
                r_at = Some(if r.is_some() { Instant::now() } else { Instant::now() - RATES_EVERY + Duration::from_secs(600) });
                if r.is_some() {
                    *RATES.lock().unwrap() = r;
                    changed = true;
                }
            }
            if due(c_at, CALENDAR_EVERY) {
                c_at = Some(Instant::now());
                let events = match settings::secret("Calendar") {
                    Some(url) => crate::net::get(&url.replacen("webcal://", "https://", 1)).map(|ics| calendar::parse(&ics, clock::now().offset)),
                    None => Some(Vec::new()),
                };
                if let Some(events) = events {
                    *EVENTS.lock().unwrap() = events;
                    changed = true;
                }
            }
            if !settings::flag("Holidays.On", true) {
                h_at = None;
                let mut list = HOLIDAYS.lock().unwrap();
                changed |= !list.is_empty();
                list.clear();
            } else if due(h_at, HOLIDAYS_EVERY) {
                // salinan terakhir dipakai dulu: tanggal merah langsung tampil, juga saat offline
                let cached = crate::store::read("holidays.ics");
                if h_at.is_none() && !cached.is_empty() {
                    holidays(&cached);
                    changed = true;
                }
                let fresh = crate::net::get(HOLIDAYS_URL).filter(|ics| ics.contains("BEGIN:VEVENT"));
                h_at = Some(if fresh.is_some() { Instant::now() } else { Instant::now() - HOLIDAYS_EVERY + Duration::from_secs(600) });
                if let Some(ics) = fresh.filter(|ics| *ics != cached) {
                    crate::store::write("holidays.ics", &ics);
                    holidays(&ics);
                    changed = true;
                }
            }
            if changed {
                let _ = unsafe { PostMessageW(Some(HWND(hwnd as _)), WM_APP_ONLINE, WPARAM(0), LPARAM(0)) };
            }
            thread::sleep(Duration::from_secs(20));
        }
    });
}
