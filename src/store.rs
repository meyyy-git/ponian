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

