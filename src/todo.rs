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

/// "Rapat tim 14:30" -> ("Rapat tim", 870). Jam boleh ditulis 14:30 atau 14.30, di awal atau akhir.
pub fn parse(input: &str) -> Option<(String, Option<u32>)> {
    let words: Vec<&str> = input.split_whitespace().collect();
    let as_time = |w: &str| -> Option<u32> {
        let (h, m) = w.split_once([':', '.'])?;
        let (h, m): (u32, u32) = (h.parse().ok()?, m.parse().ok()?);
        (h < 24 && m < 60 && w.len() <= 5).then_some(h * 60 + m)
    };
    let (time, rest): (Option<u32>, Vec<&str>) = match (words.first().and_then(|w| as_time(w)), words.last().and_then(|w| as_time(w))) {
        (_, Some(t)) if words.len() > 1 => (Some(t), words[..words.len() - 1].to_vec()),
        (Some(t), _) if words.len() > 1 => (Some(t), words[1..].to_vec()),
        _ => (None, words),
    };
    let title = rest.join(" ");
    (!title.is_empty()).then_some((title, time))
}

pub fn load(text: &str) -> Vec<Task> {
    text.lines()
        .filter_map(|line| {
            let mut f = line.splitn(5, '|');
            let done = f.next()? == "1";
            let reminded = f.next()? == "1";
            let day = crate::clock::parse_key(f.next()?)?;
            let time = parse(&format!("{} x", f.next()?)).and_then(|(_, t)| t);
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

/// Urutan tampil: belum selesai dulu, yang berjam diurutkan menurut jam.
pub fn sorted(tasks: &[Task]) -> Vec<usize> {
    let mut idx: Vec<usize> = (0..tasks.len()).collect();
    idx.sort_by_key(|&i| (tasks[i].done, tasks[i].time.unwrap_or(u32::MAX), i));
    idx
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_times() {
        assert_eq!(parse("Rapat tim 14:30"), Some(("Rapat tim".into(), Some(870))));
        assert_eq!(parse("09.05 Minum obat"), Some(("Minum obat".into(), Some(545))));
        assert_eq!(parse("Beli susu"), Some(("Beli susu".into(), None)));
        assert_eq!(parse("Versi 1.2 rilis"), Some(("Versi 1.2 rilis".into(), None)));
        assert_eq!(parse("25:00 bukan jam"), Some(("25:00 bukan jam".into(), None)));
        assert_eq!(parse("   "), None);
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
