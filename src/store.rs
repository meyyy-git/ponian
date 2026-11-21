//! Data daftar (tugas, catatan minum, riwayat fokus) disimpan sebagai teks di %APPDATA%\Ponian.
use std::{fs, path::PathBuf};

fn dir() -> Option<PathBuf> {
    let dir = PathBuf::from(std::env::var_os("APPDATA")?).join("Ponian");
    fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

pub fn read(name: &str) -> String {
    dir().and_then(|d| fs::read_to_string(d.join(name)).ok()).unwrap_or_default()
}

/// Tulis ke file sementara lalu ganti nama, supaya file lama tidak rusak kalau penulisan terputus.
pub fn write(name: &str, text: &str) {
    let Some(d) = dir() else { return };
    let tmp = d.join(format!("{name}.tmp"));
    if fs::write(&tmp, text).is_ok() {
        let _ = fs::rename(&tmp, d.join(name));
    }
}

/// Catatan harian "YYYY-MM-DD angka", satu baris per hari (dipakai untuk air dan menit fokus).
pub fn daily(name: &str) -> Vec<(i64, u32)> {
    read(name)
        .lines()
        .filter_map(|l| {
            let (day, v) = l.split_once(' ')?;
            Some((crate::clock::parse_key(day)?, v.trim().parse().ok()?))
        })
        .collect()
}

pub fn daily_get(name: &str, day: i64) -> u32 {
    daily(name).into_iter().find(|e| e.0 == day).map_or(0, |e| e.1)
}

/// Simpan nilai satu hari; hanya 60 hari terakhir yang disimpan.
pub fn daily_set(name: &str, day: i64, value: u32) {
    let mut all: Vec<(i64, u32)> = daily(name).into_iter().filter(|e| e.0 != day && e.0 > day - 60).collect();
    all.push((day, value));
    all.sort();
    write(name, &all.iter().map(|(d, v)| format!("{} {v}\n", crate::clock::key(*d))).collect::<String>());
}
