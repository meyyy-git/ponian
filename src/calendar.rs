//! Kalender dari link iCal rahasia (misalnya Google Calendar "Alamat rahasia dalam format iCal").
use crate::clock;

#[derive(Clone, Debug, PartialEq)]
pub struct Event {
    /// hari lokal sejak 1970
    pub day: i64,
    /// menit sejak tengah malam lokal; None untuk acara sepanjang hari
    pub min: Option<u32>,
    pub title: String,
    /// tanggal merah: acara berketerangan "Public holiday" di kalender hari libur
    pub holiday: bool,
    repeat: Option<(Freq, Option<i64>)>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Freq {
    Daily,
    Weekly,
    Yearly,
}

/// "20261121T073000Z" -> (hari, menit) dalam waktu lokal. Tanpa Z dianggap sudah waktu lokal.
// shortcut: parameter TZID diabaikan (dianggap zona waktu komputer ini); cukup untuk kalender pribadi satu zona.
fn stamp(v: &str, offset_min: i64) -> Option<(i64, Option<u32>)> {
    let v = v.trim();
    let num = |a: usize, b: usize| v.get(a..b)?.parse::<i64>().ok();
    let day = clock::days_from_civil(num(0, 4)?, num(4, 6)? as u32, num(6, 8)? as u32);
    if v.len() < 15 {
        return Some((day, None));
    }
    let mut total = day * 1440 + num(9, 11)? * 60 + num(11, 13)?;
    if v.ends_with('Z') {
        total += offset_min;
    }
    Some((total.div_euclid(1440), Some(total.rem_euclid(1440) as u32)))
}

pub fn parse(ics: &str, offset_min: i64) -> Vec<Event> {
    // baris yang diawali spasi adalah sambungan baris sebelumnya
    let unfolded = ics.replace("\r\n ", "").replace("\n ", "").replace("\r\n\t", "").replace("\n\t", "");
    let mut events = Vec::new();
    let (mut start, mut title, mut repeat, mut holiday) = (None, String::new(), None, false);
    for line in unfolded.lines() {
        let Some((key, value)) = line.split_once(':') else { continue };
        let name = key.split(';').next().unwrap_or(key);
        match name {
            "BEGIN" if value == "VEVENT" => (start, title, repeat, holiday) = (None, String::new(), None, false),
            "DTSTART" => start = stamp(value, offset_min),
            // kalender hari libur Google membedakan "Public holiday" (tanggal merah) dari "Observance"
            "DESCRIPTION" => holiday = value.starts_with("Public holiday"),
            "SUMMARY" => title = value.replace("\\,", ",").replace("\\;", ";").replace("\\n", " "),
            "RRULE" => {
                let part = |k: &str| value.split(';').find_map(|p| p.strip_prefix(k));
                let freq = match part("FREQ=") {
                    Some("DAILY") => Some(Freq::Daily),
                    Some("WEEKLY") => Some(Freq::Weekly),
                    Some("YEARLY") => Some(Freq::Yearly),
                    _ => None,
                };
                repeat = freq.map(|f| (f, part("UNTIL=").and_then(|u| stamp(u, offset_min)).map(|s| s.0)));
            }
            "END" if value == "VEVENT" => {
                if let Some((day, min)) = start {
                    events.push(Event { day, min, title: std::mem::take(&mut title), holiday, repeat });
                }
            }
            _ => {}
        }
    }
    events
}

/// Acara pada `day`, urut menurut jam.
// shortcut: aturan ulang hanya FREQ DAILY/WEEKLY/YEARLY dengan UNTIL; INTERVAL, COUNT, BYDAY, dan EXDATE diabaikan.
pub fn on(events: &[Event], day: i64) -> Vec<Event> {
    let mut out: Vec<Event> = events
        .iter()
        .filter(|e| {
            if e.day == day {
                return true;
            }
            let Some((freq, until)) = e.repeat else { return false };
            if day < e.day || until.is_some_and(|u| day > u) {
                return false;
            }
            match freq {
                Freq::Daily => true,
                Freq::Weekly => (day - e.day) % 7 == 0,
                Freq::Yearly => {
                    let (_, m1, d1) = clock::civil_from_days(e.day);
                    let (_, m2, d2) = clock::civil_from_days(day);
                    (m1, d1) == (m2, d2)
                }
            }
        })
        .cloned()
        .collect();
    out.sort_by_key(|e| e.min.unwrap_or(0));
    out
}

/// Tanggal merah berikutnya (hari ini atau sesudahnya): (nama, hari).
pub fn next_holiday(events: &[Event], today: i64) -> Option<(&str, i64)> {
    events.iter().filter(|e| e.holiday && e.day >= today).min_by_key(|e| e.day).map(|e| (e.title.as_str(), e.day))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn holidays_are_told_apart_from_observances() {
        let ics = "BEGIN:VEVENT\r\nDTSTART;VALUE=DATE:20261231\r\nDESCRIPTION:Observance\\nTo hide observances\r\nSUMMARY:New Year's Eve\r\nEND:VEVENT\r\n\
                   BEGIN:VEVENT\r\nDTSTART;VALUE=DATE:20270101\r\nDESCRIPTION:Public holiday\r\nSUMMARY:New Year's Day\r\nEND:VEVENT\r\n\
                   BEGIN:VEVENT\r\nDTSTART;VALUE=DATE:20261225\r\nDESCRIPTION:Public holiday\r\nSUMMARY:Christmas Day\r\nEND:VEVENT\r\n";
        let events = parse(ics, 7 * 60);
        assert_eq!(events.iter().map(|e| e.holiday).collect::<Vec<_>>(), [false, true, true]);
        let day = |m, d| clock::days_from_civil(2026, m, d);
        assert_eq!(next_holiday(&events, day(11, 21)), Some(("Christmas Day", day(12, 25))));
        assert_eq!(next_holiday(&events, day(12, 25)), Some(("Christmas Day", day(12, 25))));
        assert_eq!(next_holiday(&events, day(12, 26)), Some(("New Year's Day", clock::days_from_civil(2027, 1, 1))));
        assert_eq!(next_holiday(&events, clock::days_from_civil(2027, 1, 2)), None);
    }

    #[test]
    fn parses_and_repeats() {
        let ics = "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nDTSTART:20261121T020000Z\r\nSUMMARY:Rapat\\, tim\r\nEND:VEVENT\r\n\
                   BEGIN:VEVENT\r\nDTSTART;VALUE=DATE:20261120\r\nSUMMARY:Ulang tahun yang\r\n  panjang\r\nRRULE:FREQ=YEARLY\r\nEND:VEVENT\r\n\
                   BEGIN:VEVENT\r\nDTSTART:20261116T190000\r\nSUMMARY:Olahraga\r\nRRULE:FREQ=WEEKLY;UNTIL=20261201T000000Z\r\nEND:VEVENT\r\nEND:VCALENDAR";
        // WIB = UTC+7
        let events = parse(ics, 7 * 60);
        assert_eq!(events.len(), 3);
        let today = clock::days_from_civil(2026, 11, 21);
        assert_eq!((events[0].day, events[0].min, events[0].title.as_str()), (today, Some(9 * 60), "Rapat, tim"));
        assert_eq!(events[1].title, "Ulang tahun yang panjang");
        assert_eq!(on(&events, today).len(), 1);
        assert_eq!(on(&events, clock::days_from_civil(2027, 11, 20))[0].title, "Ulang tahun yang panjang");
        assert_eq!(on(&events, clock::days_from_civil(2026, 11, 23))[0].title, "Olahraga");
        assert!(on(&events, clock::days_from_civil(2026, 12, 7)).is_empty());
    }
}
