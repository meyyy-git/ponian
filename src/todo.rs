//! Daftar tugas dengan jam opsional dan pengingat.
//! Disimpan sebagai teks, satu tugas per baris: `selesai|sudah_diingatkan|tanggal|jam|judul`.

#[derive(Clone, Debug, PartialEq)]
pub struct Task {
    pub title: String,
    /// menit sejak tengah malam, untuk pengingat
    pub time: Option<u32>,
    pub done: bool,
    pub reminded: bool,
    /// hari dibuat atau dijadwalkan (hari sejak 1970)
    pub day: i64,
}

/// "14:30" atau "14.30" -> menit sejak tengah malam.
fn as_time(w: &str) -> Option<u32> {
    let (h, m) = w.split_once([':', '.'])?;
    let (h, m): (u32, u32) = (h.parse().ok()?, m.parse().ok()?);
    (h < 24 && m < 60 && w.len() <= 5).then_some(h * 60 + m)
}

/// "besok", "lusa", "25/12", atau "25/12/2027" -> hari sejak 1970. Tanggal tanpa tahun yang sudah lewat berarti tahun depan.
fn as_day(w: &str, today: i64) -> Option<i64> {
    match w.to_lowercase().as_str() {
        "besok" => return Some(today + 1),
        "lusa" => return Some(today + 2),
        _ => {}
    }
    let mut parts = w.split('/');
    let (d, m): (u32, u32) = (parts.next()?.parse().ok()?, parts.next()?.parse().ok()?);
    let year = parts.next().map(|y| y.parse::<i64>().ok());
    if parts.next().is_some() || !(1..=31).contains(&d) || !(1..=12).contains(&m) {
        return None;
    }
    let this_year = crate::clock::civil_from_days(today).0;
    match year {
        Some(y) => Some(crate::clock::days_from_civil(y?, m, d)),
        None => Some(crate::clock::days_from_civil(this_year, m, d)).filter(|&day| day >= today).or(Some(crate::clock::days_from_civil(this_year + 1, m, d))),
    }
}

/// "Rapat tim besok 14:30" -> ("Rapat tim", jam 870, hari besok). Jam dan hari hanya dikenali di
/// awal atau akhir kalimat, supaya angka di tengah judul ("Versi 1.2 rilis") tidak ikut terbaca.
pub fn parse(input: &str, today: i64) -> Option<(String, Option<u32>, Option<i64>)> {
    let mut words: Vec<&str> = input.split_whitespace().collect();
    let (mut time, mut day) = (None, None);
    while words.len() > 1 {
        let mut took = false;
        for at_end in [true, false] {
            let w = if at_end { words[words.len() - 1] } else { words[0] };
            let hit = if time.is_none() && as_time(w).is_some() {
                time = as_time(w);
                true
            } else if day.is_none() && as_day(w, today).is_some() {
                day = as_day(w, today);
                true
            } else {
                false
            };
            if hit {
                if at_end { words.pop() } else { Some(words.remove(0)) };
                took = true;
                break;
            }
        }
        if !took {
            break;
        }
    }
    let title = words.join(" ");
    (!title.is_empty()).then_some((title, time, day))
}

/// Teks untuk kotak edit: kebalikan dari `parse`.
pub fn to_input(t: &Task, today: i64) -> String {
    let mut s = t.title.clone();
    if t.day != today {
        let (_, m, d) = crate::clock::civil_from_days(t.day);
        s.push_str(&format!(" {d}/{m}"));
    }
    if let Some(min) = t.time {
        s.push_str(&format!(" {}", crate::clock::hhmm(min)));
    }
    s
}

pub fn load(text: &str) -> Vec<Task> {
    text.lines()
        .filter_map(|line| {
            let mut f = line.splitn(5, '|');
            let done = f.next()? == "1";
            let reminded = f.next()? == "1";
            let day = crate::clock::parse_key(f.next()?)?;
            let time = as_time(f.next()?);
            let title = f.next()?.trim().to_string();
            (!title.is_empty()).then_some(Task { title, time, done, reminded, day })
        })
        .collect()
}

pub fn save(tasks: &[Task]) -> String {
    tasks
        .iter()
        .map(|t| {
            let time = t.time.map(crate::clock::hhmm).unwrap_or_default();
            // judul tidak boleh berisi baris baru atau pemisah, supaya format satu baris tetap utuh
            let title = t.title.replace(['\n', '\r'], " ");
            format!("{}|{}|{}|{}|{}\n", t.done as u8, t.reminded as u8, crate::clock::key(t.day), time, title)
        })
        .collect()
}

/// Tugas yang selesai di hari sebelumnya dibuang; yang belum selesai tetap tampil.
pub fn cleanup(tasks: &mut Vec<Task>, today: i64) {
    tasks.retain(|t| !(t.done && t.day < today));
}

/// Indeks tugas hari ini yang jamnya sudah lewat dan belum diingatkan.
pub fn due(tasks: &[Task], today: i64, now_min: u32) -> Option<usize> {
    tasks.iter().position(|t| !t.done && !t.reminded && t.day == today && t.time.is_some_and(|m| m <= now_min))
}

/// Urutan tampil: belum selesai dulu, lalu menurut hari dan jam.
pub fn sorted(tasks: &[Task]) -> Vec<usize> {
    let mut idx: Vec<usize> = (0..tasks.len()).collect();
    idx.sort_by_key(|&i| (tasks[i].done, tasks[i].day, tasks[i].time.unwrap_or(u32::MAX), i));
    idx
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_times() {
        let today = crate::clock::days_from_civil(2026, 11, 21);
        let p = |s: &str| parse(s, today);
        assert_eq!(p("Rapat tim 14:30"), Some(("Rapat tim".into(), Some(870), None)));
        assert_eq!(p("09.05 Minum obat"), Some(("Minum obat".into(), Some(545), None)));
        assert_eq!(p("Beli susu"), Some(("Beli susu".into(), None, None)));
        assert_eq!(p("Versi 1.2 rilis"), Some(("Versi 1.2 rilis".into(), None, None)));
        assert_eq!(p("25:00 bukan jam"), Some(("25:00 bukan jam".into(), None, None)));
        assert_eq!(p("   "), None);
        // hari lain: kata "besok"/"lusa" atau tanggal, di awal atau akhir, boleh digabung dengan jam
        assert_eq!(p("Rapat tim besok 14:30"), Some(("Rapat tim".into(), Some(870), Some(today + 1))));
        assert_eq!(p("lusa Bayar listrik"), Some(("Bayar listrik".into(), None, Some(today + 2))));
        assert_eq!(p("Natal 25/12"), Some(("Natal".into(), None, Some(crate::clock::days_from_civil(2026, 12, 25)))));
        // tanggal tanpa tahun yang sudah lewat berarti tahun depan
        assert_eq!(p("Tahun baru 1/1"), Some(("Tahun baru".into(), None, Some(crate::clock::days_from_civil(2027, 1, 1)))));
        assert_eq!(p("Skor 3/45 aneh"), Some(("Skor 3/45 aneh".into(), None, None)));
        let task = Task { title: "Rapat".into(), time: Some(870), done: false, reminded: false, day: today + 1 };
        assert_eq!(to_input(&task, today), "Rapat 22/11 14:30");
        assert_eq!(p(&to_input(&task, today)), Some(("Rapat".into(), Some(870), Some(today + 1))));
    }

    #[test]
    fn roundtrip_and_due() {
        let today = crate::clock::days_from_civil(2026, 11, 21);
        let tasks = vec![
            Task { title: "Rapat|tim".into(), time: Some(870), done: false, reminded: false, day: today },
            Task { title: "Kemarin".into(), time: None, done: true, reminded: false, day: today - 1 },
        ];
        let mut back = load(&save(&tasks));
        assert_eq!(back, tasks);
        assert_eq!(due(&back, today, 869), None);
        assert_eq!(due(&back, today, 870), Some(0));
        cleanup(&mut back, today);
        assert_eq!(back.len(), 1);
        assert_eq!(load("rusak\n1|0|bukan-tanggal||x\n"), vec![]);
    }
}
