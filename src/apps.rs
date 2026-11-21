//! Pintasan aplikasi pilihan user: daftar path, nama, dan ikonnya.
use crate::store;
use windows::{
    Win32::{
        Foundation::HWND,
        Graphics::Gdi::*,
        Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES,
        UI::{
            Controls::Dialogs::{GetOpenFileNameW, OFN_EXPLORER, OFN_FILEMUSTEXIST, OFN_NODEREFERENCELINKS, OPENFILENAMEW},
            Shell::{SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON, SHGetFileInfoW},
            WindowsAndMessaging::{DI_NORMAL, DestroyIcon, DrawIconEx, GetSystemMetrics, SM_CXICON},
        },
    },
    core::{PCWSTR, PWSTR, w},
};

/// Sampai 11 pintasan tampil besar dengan nama (6 x 2); lebih dari itu tampil kecil tanpa nama (8 x 3).
pub const MAX: usize = 23;

pub struct Shortcut {
    pub path: String,
    pub name: String,
    /// piksel BGRA premultiplied berukuran `side` x `side`
    pub icon: Option<Vec<u8>>,
    pub side: u32,
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

pub fn make(path: &str) -> Shortcut {
    let file = path.rsplit(['\\', '/']).next().unwrap_or(path);
    let name = file.rsplit_once('.').map_or(file, |(stem, _)| stem).to_string();
    let (icon, side) = icon(path).map_or((None, 0), |(px, side)| (Some(px), side));
    Shortcut { path: path.to_string(), name, icon, side }
}

pub fn load() -> Vec<Shortcut> {
    // tiap baris: path, lalu (opsional) tab dan nama buatan user
    store::read("apps.txt")
        .lines()
        .filter(|l| !l.trim().is_empty())
        .take(MAX)
        .map(|l| match l.split_once('\t') {
            Some((path, name)) => Shortcut { name: name.trim().to_string(), ..make(path.trim()) },
            None => make(l.trim()),
        })
        .collect()
}

pub fn save(list: &[Shortcut]) {
    store::write("apps.txt", &list.iter().map(|s| format!("{}\t{}\n", s.path, s.name.replace(['\t', '\n'], " "))).collect::<String>());
}

/// Ikon file seperti yang ditampilkan Explorer, digambar ke bitmap 32-bit.
fn icon(path: &str) -> Option<(Vec<u8>, u32)> {
    unsafe {
        let mut info = SHFILEINFOW::default();
        let found = SHGetFileInfoW(PCWSTR(wide(path).as_ptr()), FILE_FLAGS_AND_ATTRIBUTES(0), Some(&mut info), size_of::<SHFILEINFOW>() as u32, SHGFI_ICON | SHGFI_LARGEICON);
        if found == 0 || info.hIcon.is_invalid() {
            return None;
        }
        let side = GetSystemMetrics(SM_CXICON).max(16);
        let dc = CreateCompatibleDC(None);
        let header = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER { biSize: size_of::<BITMAPINFOHEADER>() as u32, biWidth: side, biHeight: -side, biPlanes: 1, biBitCount: 32, ..Default::default() },
            ..Default::default()
        };
        let mut bits = std::ptr::null_mut();
        let pixels = CreateDIBSection(Some(dc), &header, DIB_RGB_COLORS, &mut bits, None, 0).ok().map(|bmp| {
            let old = SelectObject(dc, bmp.into());
            let _ = DrawIconEx(dc, 0, 0, info.hIcon, side, side, 0, None, DI_NORMAL);
            let mut px = std::slice::from_raw_parts(bits as *const u8, (side * side * 4) as usize).to_vec();
            SelectObject(dc, old);
            let _ = DeleteObject(bmp.into());
            // ikon lama tanpa kanal alpha keluar dengan alpha 0 semua: anggap piksel berwarna itu padat
            if px.chunks_exact(4).all(|p| p[3] == 0) {
                for p in px.chunks_exact_mut(4).filter(|p| p[..3] != [0, 0, 0]) {
                    p[3] = 255;
                }
            }
            px
        });
        let _ = DeleteDC(dc);
        let _ = DestroyIcon(info.hIcon);
        pixels.map(|px| (px, side as u32))
    }
}

/// Dialog pilih file. Memblokir sampai ditutup, jadi jangan dipanggil saat state app sedang dipinjam.
pub fn pick(owner: HWND) -> Option<String> {
    let mut file = [0u16; 1024];
    let mut dialog = OPENFILENAMEW {
        lStructSize: size_of::<OPENFILENAMEW>() as u32,
        hwndOwner: owner,
        lpstrFilter: w!("Aplikasi dan pintasan\0*.exe;*.lnk;*.url\0Semua file\0*.*\0"),
        lpstrFile: PWSTR(file.as_mut_ptr()),
        nMaxFile: file.len() as u32,
        lpstrTitle: w!("Pilih aplikasi untuk Ponian"),
        // pintasan (.lnk) disimpan apa adanya, tidak diganti dengan file tujuannya
        Flags: OFN_EXPLORER | OFN_FILEMUSTEXIST | OFN_NODEREFERENCELINKS,
        ..Default::default()
    };
    unsafe { GetOpenFileNameW(&mut dialog) }.as_bool().then(|| {
        let end = file.iter().position(|&c| c == 0).unwrap_or(file.len());
        String::from_utf16_lossy(&file[..end])
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn names_come_from_the_file_stem() {
        assert_eq!(super::make(r"Z:\tidak\ada\Visual Studio Code.lnk").name, "Visual Studio Code");
        assert_eq!(super::make(r"Z:\tidak\ada\tanpa-ekstensi").name, "tanpa-ekstensi");
    }
}
