//! Lirik tersinkron per baris dari LRCLIB (https://lrclib.net/docs).
use windows::{
    Data::Json::{JsonArray, JsonObject},
    core::HSTRING,
};

/// (waktu mulai dalam ms, teks)
pub type Line = (i64, String);

const BASE: &str = "https://lrclib.net/api";

pub fn parse_lrc(src: &str) -> Vec<Line> {
    let mut out = Vec::new();
    for raw in src.lines() {
        let mut rest = raw.trim();
        let mut stamps = Vec::new();
        while let Some((tag, tail)) = rest.strip_prefix('[').and_then(|r| r.split_once(']')) {
            // tag yang bukan waktu, misalnya [ar:Artis], adalah metadata
            let Some(ms) = stamp(tag) else { break };
            stamps.push(ms);
            rest = tail.trim_start();
        }
        out.extend(stamps.into_iter().map(|ms| (ms, rest.trim().to_string())));
    }
    out.sort_by_key(|l| l.0);
    out
}

fn stamp(tag: &str) -> Option<i64> {
    let (m, s) = tag.split_once(':')?;
    let (s, frac) = s.split_once(['.', ':']).unwrap_or((s, "0"));
    let unit = match frac.len() {
        1 => 100,
        2 => 10,
        3 => 1,
        _ => return None,
    };
    Some(m.parse::<i64>().ok()? * 60_000 + s.parse::<i64>().ok()? * 1000 + frac.parse::<i64>().ok()? * unit)
}

/// Indeks baris yang sedang dinyanyikan pada posisi `pos_ms`.
pub fn active(lines: &[Line], pos_ms: i64) -> Option<usize> {
    lines.partition_point(|l| l.0 <= pos_ms).checked_sub(1)
}

/// Buang embel-embel judul ala YouTube: "(Official Video)", "[Lyrics]", "| Label".
pub fn clean_title(title: &str) -> String {
    const NOISE: [&str; 7] = ["official", "video", "audio", "lyric", "feat", "ft.", "visualizer"];
    let mut rest = title.split(" | ").next().unwrap_or(title);
    let mut out = String::new();
    while let Some(i) = rest.find(['(', '[']) {
        let close = if rest[i..].starts_with('(') { ')' } else { ']' };
        let Some(j) = rest[i..].find(close) else { break };
        let group = &rest[i..=i + j];
        out.push_str(&rest[..i]);
        if !NOISE.iter().any(|n| group.to_lowercase().contains(n)) {
            out.push_str(group);
        }
        rest = &rest[i + j + 1..];
    }
    out.push_str(rest);
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn clean_artist(artist: &str) -> &str {
    artist.strip_suffix(" - Topic").unwrap_or(artist).trim()
}

/// Memblokir sampai server menjawab; panggil dari thread sendiri.
pub fn fetch(title: &str, artist: &str, dur_s: i64) -> Option<Vec<Line>> {
    let (t, a) = (clean_title(title), clean_artist(artist));
    let exact = format!("{BASE}/get?track_name={}&artist_name={}&duration={dur_s}", esc(&t)?, esc(a)?);
    let found = http(&exact).and_then(|b| synced(&JsonObject::Parse(&b).ok()?)).or_else(|| {
        // judul ala YouTube "Artis - Lagu" tidak cocok dengan track_name, jadi pakai pencarian bebas
        let search = if t.contains(" - ") {
            format!("{BASE}/search?q={}", esc(&t)?)
        } else {
            format!("{BASE}/search?track_name={}&artist_name={}", esc(&t)?, esc(a)?)
        };
        let arr = JsonArray::Parse(&http(&search)?).ok()?;
        let mut best: Option<(f64, String)> = None;
        for i in 0..arr.Size().ok()? {
            let Ok(o) = arr.GetObjectAt(i) else { continue };
            let (Some(lrc), Ok(d)) = (synced(&o), o.GetNamedNumber(&HSTRING::from("duration"))) else { continue };
            let diff = (d - dur_s as f64).abs();
            // durasi beda jauh berarti versi lain, waktunya tidak akan pas
            if diff <= 5.0 && best.as_ref().is_none_or(|b| diff < b.0) {
                best = Some((diff, lrc));
            }
        }
        best.map(|b| b.1)
    })?;
    Some(parse_lrc(&found)).filter(|l| !l.is_empty())
}

fn synced(o: &JsonObject) -> Option<String> {
    let s = o.GetNamedString(&HSTRING::from("syncedLyrics")).ok()?.to_string();
    (!s.trim().is_empty()).then_some(s)
}

fn esc(s: &str) -> Option<String> {
    crate::net::escape(s)
}

fn http(url: &str) -> Option<HSTRING> {
    crate::net::get(url).map(HSTRING::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_lrc() {
        let l = parse_lrc("[ar:Someone]\n[00:12.50] Halo\n[01:02.345]Dunia\n[00:05.00][00:20.00] Reff\n[00:30.00]\nteks tanpa waktu");
        assert_eq!(
            l,
            vec![
                (5_000, "Reff".into()),
                (12_500, "Halo".into()),
                (20_000, "Reff".into()),
                (30_000, "".into()),
                (62_345, "Dunia".into()),
            ]
        );
    }

    #[test]
    fn finds_active_line() {
        let l = parse_lrc("[00:10.00]a\n[00:20.00]b");
        assert_eq!(active(&l, 9_999), None);
        assert_eq!(active(&l, 10_000), Some(0));
        assert_eq!(active(&l, 19_999), Some(0));
        assert_eq!(active(&l, 99_000), Some(1));
        assert_eq!(active(&[], 0), None);
    }

    #[test]
    fn cleans_titles() {
        assert_eq!(clean_title("Lagu (Official Music Video) [Lyrics]"), "Lagu");
        assert_eq!(clean_title("Lagu (Live at Wembley) (feat. X)"), "Lagu (Live at Wembley)");
        assert_eq!(clean_title("Artis - Lagu | Label Rekaman"), "Artis - Lagu");
        assert_eq!(clean_title("Lagu (belum ditutup"), "Lagu (belum ditutup");
        assert_eq!(clean_artist("Artis - Topic"), "Artis");
    }

    /// Butuh internet: `cargo test -- --ignored`
    #[test]
    #[ignore]
    fn lrclib_live() {
        let l = fetch("Never Gonna Give You Up (Official Video)", "Rick Astley", 213).expect("lirik tidak ditemukan");
        assert!(l.len() > 10);
    }
}
