//! Kotak teks melayang di atas notch. Memakai kontrol EDIT bawaan Windows (kursor, seleksi, Ctrl+V,
//! IME, pembaca layar), karena layered window tidak bisa menampung kontrol anak.
//! Notch hanya mengambil fokus keyboard selama kotak ini terbuka; Esc atau klik di luar mengembalikannya.
use std::cell::RefCell;
use windows::{
    Win32::{
        Foundation::*,
        Graphics::{Dwm::*, Gdi::*},
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            Controls::EM_SETSEL,
            Input::KeyboardAndMouse::{SetFocus, VK_ESCAPE, VK_RETURN, VK_TAB},
            Shell::{DefSubclassProc, SetWindowSubclass},
            WindowsAndMessaging::*,
        },
    },
    core::*,
};

pub const WM_APP_INPUT: u32 = WM_APP + 4;
/// lParam pesan WM_APP_INPUT
pub const SUBMIT: isize = 1;
pub const CANCEL: isize = 0;
pub const TAB: isize = 2;

const BG: COLORREF = COLORREF(0x00262626);
const EM_SETCUEBANNER: u32 = 0x1501;

struct Input {
    popup: HWND,
    edit: HWND,
    owner: HWND,
    id: usize,
    brush: HBRUSH,
    font: HFONT,
}

thread_local! {
    static INPUT: RefCell<Option<Input>> = const { RefCell::new(None) };
    static LAST: RefCell<Option<String>> = const { RefCell::new(None) };
}

pub fn is_open() -> bool {
    INPUT.with_borrow(|i| i.is_some())
}

/// Teks yang dikirim terakhir kali (diambil sekali setelah WM_APP_INPUT dengan SUBMIT atau TAB).
pub fn take_text() -> Option<String> {
    LAST.with_borrow_mut(|l| l.take())
}

/// Buka kotak teks di `rect` (piksel layar). `id` dikembalikan di wParam WM_APP_INPUT.
pub fn open(owner: HWND, id: usize, rect: RECT, scale: f32, initial: &str, placeholder: &str) {
    close();
    unsafe {
        let Ok(module) = GetModuleHandleW(None) else { return };
        let instance: HINSTANCE = module.into();
        let class = w!("PonianInput");
        let brush = CreateSolidBrush(BG);
        RegisterClassW(&WNDCLASSW { lpfnWndProc: Some(popup_proc), hInstance: instance, lpszClassName: class, hbrBackground: brush, ..Default::default() });
        let (w, h) = (rect.right - rect.left, rect.bottom - rect.top);
        let Ok(popup) = CreateWindowExW(WS_EX_TOPMOST | WS_EX_TOOLWINDOW, class, w!(""), WS_POPUP, rect.left, rect.top, w, h, Some(owner), None, Some(instance), None)
        else {
            return;
        };
        let round = DWMWCP_ROUND;
        let _ = DwmSetWindowAttribute(popup, DWMWA_WINDOW_CORNER_PREFERENCE, &round as *const _ as _, 4);
        let pad = (10.0 * scale) as i32;
        let font_h = (14.0 * scale) as i32;
        let initial_w: Vec<u16> = initial.encode_utf16().chain(Some(0)).collect();
        let Ok(edit) = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("EDIT"),
            PCWSTR(initial_w.as_ptr()),
            WS_CHILD | WS_VISIBLE | WINDOW_STYLE(ES_AUTOHSCROLL as u32),
            pad,
            (h - font_h) / 2 - 2,
            w - 2 * pad,
            font_h + 6,
            Some(popup),
            None,
            Some(instance),
            None,
        ) else {
            let _ = DestroyWindow(popup);
            return;
        };
        let font = CreateFontW(-font_h, 0, 0, 0, 400, 0, 0, 0, DEFAULT_CHARSET, OUT_DEFAULT_PRECIS, CLIP_DEFAULT_PRECIS, CLEARTYPE_QUALITY, 0, w!("Segoe UI"));
        SendMessageW(edit, WM_SETFONT, Some(WPARAM(font.0 as usize)), Some(LPARAM(1)));
        let cue: Vec<u16> = placeholder.encode_utf16().chain(Some(0)).collect();
        SendMessageW(edit, EM_SETCUEBANNER, Some(WPARAM(1)), Some(LPARAM(cue.as_ptr() as isize)));
        SendMessageW(edit, EM_SETSEL, Some(WPARAM(initial.len())), Some(LPARAM(initial.len() as isize)));
        let _ = SetWindowSubclass(edit, Some(edit_proc), 1, 0);
        INPUT.set(Some(Input { popup, edit, owner, id, brush, font }));
        let _ = ShowWindow(popup, SW_SHOW);
        let _ = SetForegroundWindow(popup);
        let _ = SetFocus(Some(edit));
    }
}

pub fn close() {
    let Some(i) = INPUT.take() else { return };
    unsafe {
        let _ = DestroyWindow(i.popup);
        let _ = DeleteObject(i.brush.into());
        let _ = DeleteObject(i.font.into());
    }
}

fn finish(result: isize) {
    let msg = INPUT.with_borrow_mut(|i| {
        let i = i.as_mut()?;
        if result == SUBMIT || result == TAB {
            let mut buf = vec![0u16; unsafe { GetWindowTextLengthW(i.edit) } as usize + 1];
            let n = unsafe { GetWindowTextW(i.edit, &mut buf) } as usize;
            LAST.set(Some(String::from_utf16_lossy(&buf[..n])));
        }
        Some((i.owner, i.id))
    });
    let Some((owner, id)) = msg else { return };
    if result != TAB {
        close();
    }
    let _ = unsafe { PostMessageW(Some(owner), WM_APP_INPUT, WPARAM(id), LPARAM(result)) };
}

extern "system" fn edit_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM, _: usize, _: usize) -> LRESULT {
    unsafe {
        match msg {
            WM_KEYDOWN if wp.0 == VK_RETURN.0 as usize => finish(SUBMIT),
            WM_KEYDOWN if wp.0 == VK_ESCAPE.0 as usize => finish(CANCEL),
            WM_KEYDOWN if wp.0 == VK_TAB.0 as usize => finish(TAB),
            // tanpa ini kontrol EDIT berbunyi "ding" untuk Enter, Esc, dan Tab
            WM_CHAR if matches!(wp.0 as u8, b'\r' | 0x1b | b'\t') => {}
            WM_GETDLGCODE => return LRESULT(DLGC_WANTALLKEYS as isize),
            _ => return DefSubclassProc(hwnd, msg, wp, lp),
        }
        LRESULT(0)
    }
}

extern "system" fn popup_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_CTLCOLOREDIT => {
                let dc = HDC(wp.0 as _);
                SetTextColor(dc, COLORREF(0x00ffffff));
                SetBkColor(dc, BG);
                let brush = INPUT.with_borrow(|i| i.as_ref().map(|i| i.brush)).unwrap_or_default();
                LRESULT(brush.0 as isize)
            }
            // klik di luar kotak teks dianggap batal
            WM_ACTIVATE if (wp.0 & 0xffff) as u32 == WA_INACTIVE => {
                if is_open() {
                    finish(CANCEL);
                }
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wp, lp),
        }
    }
}
