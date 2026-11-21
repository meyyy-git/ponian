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
    let drop = HDROP(unsafe { medium.u.hGlobal.0 });
    let count = unsafe { DragQueryFileW(drop, u32::MAX, None) };
    let list = (0..count)
        .map(|i| {
            let mut buf = vec![0u16; unsafe { DragQueryFileW(drop, i, None) } as usize + 1];
            let n = unsafe { DragQueryFileW(drop, i, Some(&mut buf)) } as usize;
            String::from_utf16_lossy(&buf[..n])
        })
        .collect();
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

fn format(name: PCWSTR) -> u32 {
    unsafe { RegisterClipboardFormatW(name) }
}

/// Teks clipboard sekarang, kecuali yang ditandai rahasia oleh app asalnya (misalnya password manager).
pub fn read_text(hwnd: HWND) -> Option<String> {
    unsafe {
        let excluded = IsClipboardFormatAvailable(format(w!("ExcludeClipboardContentFromMonitorProcessing"))).is_ok()
            || IsClipboardFormatAvailable(format(w!("Clipboard Viewer Ignore"))).is_ok();
        let history_off = format(w!("CanIncludeInClipboardHistory"));
        if excluded {
            return None;
        }
        OpenClipboard(Some(hwnd)).ok()?;
        let text = (|| {
            // app yang melarang riwayat clipboard menaruh DWORD 0 di format ini
            if let Ok(h) = GetClipboardData(history_off) {
                let p = GlobalLock(HGLOBAL(h.0)) as *const u32;
                let allowed = p.is_null() || *p != 0;
                let _ = GlobalUnlock(HGLOBAL(h.0));
                if !allowed {
                    return None;
                }
            }
            let h = GetClipboardData(CF_UNICODETEXT).ok()?;
            let p = GlobalLock(HGLOBAL(h.0)) as *const u16;
            if p.is_null() {
                return None;
            }
            let len = GlobalSize(HGLOBAL(h.0)) / 2;
            let units = std::slice::from_raw_parts(p, len);
            let end = units.iter().position(|&u| u == 0).unwrap_or(len);
            let s = String::from_utf16_lossy(&units[..end]);
            let _ = GlobalUnlock(HGLOBAL(h.0));
            Some(s)
        })();
        let _ = CloseClipboard();
        text.filter(|t| !t.trim().is_empty())
    }
}

pub fn write_text(hwnd: HWND, text: &str) {
    let units: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
    unsafe {
        if OpenClipboard(Some(hwnd)).is_err() {
            return;
        }
        let _ = EmptyClipboard();
        if let Ok(mem) = GlobalAlloc(GMEM_MOVEABLE, units.len() * 2) {
            let p = GlobalLock(mem) as *mut u16;
            if !p.is_null() {
                std::ptr::copy_nonoverlapping(units.as_ptr(), p, units.len());
                let _ = GlobalUnlock(mem);
                let _ = SetClipboardData(CF_UNICODETEXT, Some(HANDLE(mem.0)));
            }
        }
        let _ = CloseClipboard();
    }
}

/// Taruh teks di depan riwayat, buang duplikat, dan batasi jumlahnya.
pub fn remember(history: &mut Vec<String>, text: String, limit: usize) {
    history.retain(|t| *t != text);
    history.insert(0, text);
    history.truncate(limit);
}

pub fn load_clips() -> Vec<String> {
    if !settings::flag("Clip.Keep", false) {
        return Vec::new();
    }
    // satu entri per baris; baris baru di dalam teks disimpan sebagai \n
    store::read("clipboard.txt").lines().map(|l| l.replace("\\n", "\n").replace("\\\\", "\\")).collect()
}

pub fn save_clips(clips: &[String]) {
    if settings::flag("Clip.Keep", false) {
        store::write("clipboard.txt", &clips.iter().map(|c| format!("{}\n", c.replace('\\', "\\\\").replace('\n', "\\n").replace('\r', ""))).collect::<String>());
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn history_dedupes_and_limits() {
        let mut h = vec!["b".to_string(), "a".to_string()];
        super::remember(&mut h, "a".into(), 2);
        assert_eq!(h, vec!["a", "b"]);
        super::remember(&mut h, "c".into(), 2);
        assert_eq!(h, vec!["c", "a"]);
        assert_eq!(super::file_name(r"C:\Users\x\Laporan akhir.pdf"), "Laporan akhir.pdf");
    }
}
