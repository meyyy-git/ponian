//! Shelf: tempat menaruh file sementara (seret masuk, seret keluar, klik untuk membuka) dan riwayat clipboard.
use crate::{WM_APP_DRAG, settings, store};
use std::sync::Mutex;
use windows::{
    Win32::{
        Foundation::*,
        System::{
            Com::{DVASPECT_CONTENT, FORMATETC, IDataObject, TYMED_HGLOBAL},
            DataExchange::*,
            Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock},
            Ole::*,
            SystemServices::MODIFIERKEYS_FLAGS,
        },
        UI::{
            Shell::{BHID_DataObject, DragQueryFileW, HDROP, IShellItem, SHCreateItemFromParsingName, ShellExecuteW},
            WindowsAndMessaging::{PostMessageW, SW_SHOWNORMAL},
        },
    },
    core::*,
};

const CF_HDROP: u16 = 15;
const CF_UNICODETEXT: u32 = 13;
const MK_LBUTTON: u32 = 1;

/// File yang baru dijatuhkan ke notch, diambil oleh thread UI lewat WM_APP_DRAG.
pub static DROPPED: Mutex<Vec<String>> = Mutex::new(Vec::new());

pub fn load_files() -> Vec<String> {
    if settings::flag("Shelf.Clear", true) {
        return Vec::new();
    }
    store::read("shelf.txt").lines().filter(|l| std::path::Path::new(l).exists()).map(String::from).collect()
}

pub fn save_files(files: &[String]) {
    store::write("shelf.txt", &files.iter().map(|f| format!("{f}\n")).collect::<String>());
}

pub fn file_name(path: &str) -> &str {
    path.rsplit(['\\', '/']).next().unwrap_or(path)
}

pub fn open(path: &str) {
    let p: Vec<u16> = path.encode_utf16().chain(Some(0)).collect();
    unsafe { ShellExecuteW(None, w!("open"), PCWSTR(p.as_ptr()), None, None, SW_SHOWNORMAL) };
}

// ---------- seret masuk ----------

#[implement(IDropTarget)]
struct Target(isize);

fn post(hwnd: isize, state: usize) {
    let _ = unsafe { PostMessageW(Some(HWND(hwnd as _)), WM_APP_DRAG, WPARAM(state), LPARAM(0)) };
}

fn paths(data: &IDataObject) -> Vec<String> {
    let format = FORMATETC { cfFormat: CF_HDROP, ptd: std::ptr::null_mut(), dwAspect: DVASPECT_CONTENT.0, lindex: -1, tymed: TYMED_HGLOBAL.0 as u32 };
    let Ok(mut medium) = (unsafe { data.GetData(&format) }) else { return Vec::new() };
    let list = drop_paths(HDROP(unsafe { medium.u.hGlobal.0 }));
    unsafe { ReleaseStgMedium(&mut medium) };
    list
}

impl IDropTarget_Impl for Target_Impl {
    fn DragEnter(&self, _: Ref<IDataObject>, _: MODIFIERKEYS_FLAGS, _: &POINTL, effect: *mut DROPEFFECT) -> Result<()> {
        unsafe { *effect = DROPEFFECT_COPY };
        post(self.0, 1);
        Ok(())
    }
    fn DragOver(&self, _: MODIFIERKEYS_FLAGS, _: &POINTL, effect: *mut DROPEFFECT) -> Result<()> {
        unsafe { *effect = DROPEFFECT_COPY };
        Ok(())
    }
    fn DragLeave(&self) -> Result<()> {
        post(self.0, 0);
        Ok(())
    }
    fn Drop(&self, data: Ref<IDataObject>, _: MODIFIERKEYS_FLAGS, _: &POINTL, effect: *mut DROPEFFECT) -> Result<()> {
        if let Some(data) = data.as_ref() {
            DROPPED.lock().unwrap().extend(paths(data));
        }
        unsafe { *effect = DROPEFFECT_COPY };
        post(self.0, 2);
        Ok(())
    }
}

/// Daftarkan notch sebagai tempat menjatuhkan file. Butuh OleInitialize di thread UI.
pub fn register(hwnd: HWND) {
    unsafe {
        let _ = OleInitialize(None);
        let target: IDropTarget = Target(hwnd.0 as isize).into();
        let _ = RegisterDragDrop(hwnd, &target);
        let _ = AddClipboardFormatListener(hwnd);
    }
}

// ---------- seret keluar ----------

#[implement(IDropSource)]
struct Source;

impl IDropSource_Impl for Source_Impl {
    fn QueryContinueDrag(&self, escape: BOOL, keys: MODIFIERKEYS_FLAGS) -> HRESULT {
        if escape.as_bool() {
            DRAGDROP_S_CANCEL
        } else if keys.0 & MK_LBUTTON == 0 {
            DRAGDROP_S_DROP
        } else {
            S_OK
        }
    }
    fn GiveFeedback(&self, _: DROPEFFECT) -> HRESULT {
        DRAGDROP_S_USEDEFAULTCURSORS
    }
}

/// Seret file keluar dari notch ke Explorer, email, chat, dan lain-lain. Memblokir sampai dilepas.
pub fn drag_out(path: &str) {
    let p: Vec<u16> = path.encode_utf16().chain(Some(0)).collect();
    unsafe {
        let Ok(item) = SHCreateItemFromParsingName::<_, _, IShellItem>(PCWSTR(p.as_ptr()), None) else { return };
        let Ok(data) = item.BindToHandler::<_, IDataObject>(None, &BHID_DataObject) else { return };
        let source: IDropSource = Source.into();
        let mut effect = DROPEFFECT_NONE;
        let _ = DoDragDrop(&data, &source, DROPEFFECT_COPY | DROPEFFECT_MOVE | DROPEFFECT_LINK, &mut effect);
    }
}

// ---------- clipboard ----------

const CF_DIB: u32 = 8;
/// Gambar disimpan mentah di memori, jadi jumlah dan ukurannya dibatasi supaya RAM tetap kecil.
// shortcut: gambar di atas 16 MB dilewati dan hanya 2 gambar terakhir yang diingat; simpan ke file kalau perlu lebih.
const IMAGE_BYTES: usize = 16 << 20;
const IMAGES: usize = 2;

#[derive(Clone, PartialEq, Debug)]
pub enum Clip {
    Text(String),
    Files(Vec<String>),
    /// isi CF_DIB apa adanya: BITMAPINFOHEADER diikuti piksel
    Image(Vec<u8>),
}

impl Clip {
    /// Ikon dan satu baris ringkasan untuk daftar di Shelf.
    pub fn label(&self) -> (&'static str, String) {
        match self {
            Clip::Text(t) => ("", t.split_whitespace().collect::<Vec<_>>().join(" ")),
            Clip::Files(f) if f.len() == 1 => ("\u{E8A5}", file_name(&f[0]).to_string()),
            Clip::Files(f) => ("\u{E8A5}", format!("{} +{} file lain", f.first().map_or("", |p| file_name(p)), f.len().saturating_sub(1))),
            Clip::Image(dib) => {
                // lebar di byte 4..8 dan tinggi di 8..12 (negatif kalau barisnya dari atas)
                let side = |at: usize| dib.get(at..at + 4).map_or(0, |b| i32::from_le_bytes(b.try_into().unwrap()).unsigned_abs());
                ("\u{EB9F}", format!("Gambar {} \u{D7} {}", side(4), side(8)))
            }
        }
    }
}

fn format(name: PCWSTR) -> u32 {
    unsafe { RegisterClipboardFormatW(name) }
}

fn drop_paths(drop: HDROP) -> Vec<String> {
    let count = unsafe { DragQueryFileW(drop, u32::MAX, None) };
    (0..count)
        .map(|i| {
            let mut buf = vec![0u16; unsafe { DragQueryFileW(drop, i, None) } as usize + 1];
            let n = unsafe { DragQueryFileW(drop, i, Some(&mut buf)) } as usize;
            String::from_utf16_lossy(&buf[..n])
        })
        .collect()
}

/// Salinan isi satu format clipboard. Clipboard harus sedang terbuka.
unsafe fn bytes(format: u32, limit: usize) -> Option<Vec<u8>> {
    unsafe {
        let h = HGLOBAL(GetClipboardData(format).ok()?.0);
        let len = GlobalSize(h);
        let p = GlobalLock(h) as *const u8;
        if p.is_null() {
            return None;
        }
        let data = (len <= limit).then(|| std::slice::from_raw_parts(p, len).to_vec());
        let _ = GlobalUnlock(h);
        data
    }
}

/// Isi clipboard sekarang (file, teks, atau gambar), kecuali yang ditandai rahasia oleh app asalnya
/// (misalnya password manager).
pub fn read(hwnd: HWND) -> Option<Clip> {
    unsafe {
        let excluded = IsClipboardFormatAvailable(format(w!("ExcludeClipboardContentFromMonitorProcessing"))).is_ok()
            || IsClipboardFormatAvailable(format(w!("Clipboard Viewer Ignore"))).is_ok();
        if excluded {
            return None;
        }
        OpenClipboard(Some(hwnd)).ok()?;
        let clip = (|| {
            // app yang melarang riwayat clipboard menaruh DWORD 0 di format ini
            if bytes(format(w!("CanIncludeInClipboardHistory")), 64).is_some_and(|b| b.len() >= 4 && b[..4] == [0; 4]) {
                return None;
            }
            if let Ok(h) = GetClipboardData(CF_HDROP as u32) {
                let files = drop_paths(HDROP(h.0));
                return (!files.is_empty()).then_some(Clip::Files(files));
            }
            if let Some(b) = bytes(CF_UNICODETEXT, usize::MAX) {
                let units: Vec<u16> = b.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).take_while(|&u| u != 0).collect();
                let text = String::from_utf16_lossy(&units);
                return (!text.trim().is_empty()).then_some(Clip::Text(text));
            }
            bytes(CF_DIB, IMAGE_BYTES).filter(|b| b.len() > 40).map(Clip::Image)
        })();
        let _ = CloseClipboard();
        clip
    }
}

fn put(hwnd: HWND, format: u32, data: &[u8]) {
    unsafe {
        if OpenClipboard(Some(hwnd)).is_err() {
            return;
        }
        let _ = EmptyClipboard();
        if let Ok(mem) = GlobalAlloc(GMEM_MOVEABLE, data.len()) {
            let p = GlobalLock(mem) as *mut u8;
            if !p.is_null() {
                std::ptr::copy_nonoverlapping(data.as_ptr(), p, data.len());
                let _ = GlobalUnlock(mem);
                let _ = SetClipboardData(format, Some(HANDLE(mem.0)));
            }
        }
        let _ = CloseClipboard();
    }
}

fn utf16(text: &str) -> impl Iterator<Item = u8> + '_ {
    text.encode_utf16().chain(Some(0)).flat_map(u16::to_le_bytes)
}

pub fn write_text(hwnd: HWND, text: &str) {
    put(hwnd, CF_UNICODETEXT, &utf16(text).collect::<Vec<u8>>());
}

/// Isi CF_HDROP: header DROPFILES 20 byte (offset daftar = 20, fWide = 1), lalu path UTF-16 yang
/// masing-masing diakhiri nol, ditutup satu nol lagi.
fn drop_list(files: &[String]) -> Vec<u8> {
    let mut data = vec![0u8; 20];
    data[0] = 20;
    data[16] = 1;
    data.extend(files.iter().flat_map(|f| utf16(f)));
    data.extend([0, 0]);
    data
}

/// Taruh lagi satu entri riwayat ke clipboard.
pub fn write(hwnd: HWND, clip: &Clip) {
    match clip {
        Clip::Text(t) => write_text(hwnd, t),
        Clip::Files(f) => put(hwnd, CF_HDROP as u32, &drop_list(f)),
        Clip::Image(dib) => put(hwnd, CF_DIB, dib),
    }
}

/// Taruh entri di depan riwayat, buang duplikat, dan batasi jumlahnya (gambar dibatasi lebih ketat).
pub fn remember(history: &mut Vec<Clip>, clip: Clip, limit: usize) {
    history.retain(|c| *c != clip);
    history.insert(0, clip);
    history.truncate(limit);
    let mut images = 0;
    history.retain(|c| {
        images += matches!(c, Clip::Image(_)) as usize;
        images <= IMAGES || !matches!(c, Clip::Image(_))
    });
}

// shortcut: hanya teks yang diingat setelah Ponian ditutup; gambar dan file hilang bersama prosesnya.
pub fn load_clips() -> Vec<Clip> {
    if !settings::flag("Clip.Keep", false) {
        return Vec::new();
    }
    // satu entri per baris; baris baru di dalam teks disimpan sebagai \n
    store::read("clipboard.txt").lines().map(|l| Clip::Text(l.replace("\\n", "\n").replace("\\\\", "\\"))).collect()
}

pub fn save_clips(clips: &[Clip]) {
    if settings::flag("Clip.Keep", false) {
        let line = |c: &Clip| match c {
            Clip::Text(t) => format!("{}\n", t.replace('\\', "\\\\").replace('\n', "\\n").replace('\r', "")),
            _ => String::new(),
        };
        store::write("clipboard.txt", &clips.iter().map(line).collect::<String>());
    }
}

#[cfg(test)]
mod tests {
    use super::Clip::{self, *};

    #[test]
    fn history_dedupes_and_limits() {
        let t = |s: &str| Text(s.to_string());
        let mut h = vec![t("b"), t("a")];
        super::remember(&mut h, t("a"), 2);
        assert_eq!(h, vec![t("a"), t("b")]);
        super::remember(&mut h, t("c"), 2);
        assert_eq!(h, vec![t("c"), t("a")]);
        assert_eq!(super::file_name(r"C:\Users\x\Laporan akhir.pdf"), "Laporan akhir.pdf");
    }

    #[test]
    fn only_the_newest_images_are_kept() {
        let mut h: Vec<Clip> = Vec::new();
        for i in 0..4u8 {
            super::remember(&mut h, Image(vec![i; 44]), 10);
        }
        super::remember(&mut h, Text("x".into()), 10);
        assert_eq!(h, vec![Text("x".into()), Image(vec![3; 44]), Image(vec![2; 44])]);
    }

    #[test]
    fn labels_and_drop_list() {
        let mut dib = vec![0u8; 44];
        dib[4..8].copy_from_slice(&640i32.to_le_bytes());
        dib[8..12].copy_from_slice(&(-480i32).to_le_bytes());
        assert_eq!(Image(dib).label().1, "Gambar 640 \u{D7} 480");
        assert_eq!(Files(vec![r"C:\a\satu.txt".into(), r"C:\a\dua.txt".into()]).label().1, "satu.txt +1 file lain");
        // "C:\a" = 4 unit + nol = 10 byte, lalu nol penutup 2 byte
        let list = super::drop_list(&[r"C:\a".to_string()]);
        assert_eq!((list.len(), list[0], list[16], list[20]), (32, 20, 1, b'C'));
    }
}
