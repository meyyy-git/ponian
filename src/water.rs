//! Pengingat minum air: target harian dibagi rata sepanjang jam aktif.

/// Saatnya mengingatkan? Ya kalau asupan sudah tertinggal satu gelas dari jadwal
/// dan pengingat terakhir sudah lewat 45 menit.
pub fn due(now_min: u32, from_h: u32, until_h: u32, goal: u32, intake: u32, glass: u32, since_last_min: u32) -> bool {
    let (from, until) = (from_h * 60, until_h * 60);
    if now_min < from || now_min >= until || until <= from || intake >= goal || since_last_min < 45 {
        return false;
    }
    let expected = goal as u64 * (now_min - from) as u64 / (until - from) as u64;
    intake as u64 + glass as u64 <= expected
}

/// Format "1.250 ml".
pub fn ml(v: u32) -> String {
    if v >= 1000 { format!("{}.{:03} ml", v / 1000, v % 1000) } else { format!("{v} ml") }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reminds_when_behind() {
        // jam aktif 08-20, target 2400 ml, gelas 250 ml; jam 14:00 seharusnya sudah 1200 ml
        assert!(due(14 * 60, 8, 20, 2400, 900, 250, 60));
        assert!(!due(14 * 60, 8, 20, 2400, 1000, 250, 60));
        assert!(!due(14 * 60, 8, 20, 2400, 900, 250, 30));
        assert!(!due(7 * 60, 8, 20, 2400, 0, 250, 999));
        assert!(!due(21 * 60, 8, 20, 2400, 0, 250, 999));
        assert!(!due(14 * 60, 8, 20, 2400, 2400, 250, 999));
    }

    #[test]
    fn formats_ml() {
        assert_eq!(ml(250), "250 ml");
        assert_eq!(ml(1250), "1.250 ml");
        assert_eq!(ml(2000), "2.000 ml");
    }
}
