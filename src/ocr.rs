//! Salin teks dari layar: pilih area, teksnya dibaca dengan mesin OCR bawaan Windows (offline) lalu disalin.
use crate::WM_APP_OCR;
use std::{cell::RefCell, sync::Mutex};
use windows::{
    Graphics::Imaging::{BitmapPixelFormat, SoftwareBitmap},
    Media::Ocr::OcrEngine,
    Security::Cryptography::CryptographicBuffer,
    Win32::{
        Foundation::*,
        Graphics::Gdi::*,
        System::{
            Com::{COINIT_MULTITHREADED, CoInitializeEx},
            LibraryLoader::GetModuleHandleW,
        },
        UI::{
            Input::KeyboardAndMouse::{ReleaseCapture, SetCapture, SetFocus, VK_ESCAPE},
            WindowsAndMessaging::*,
        },
    },
    core::*,
};

/// Hasil OCR terakhir untuk thread UI: Ok(teks) atau Err(pesan).
pub static RESULT: Mutex<Option<std::result::Result<String, String>>> = Mutex::new(None);

struct Shot {
    owner: HWND,
    overlay: HWND,
    /// piksel BGRA layar penuh, baris dari atas
    pixels: Vec<u8>,
    w: i32,
    h: i32,
    bright: HBITMAP,
    dim: HBITMAP,
    start: Option<POINT>,
    end: POINT,
}

thread_local! {
    static SHOT: RefCell<Option<Shot>> = const { RefCell::new(None) };
}

fn bitmap(dc: HDC, w: i32, h: i32, pixels: &[u8]) -> HBITMAP {
    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER { biSize: size_of::<BITMAPINFOHEADER>() as u32, biWidth: w, biHeight: -h, biPlanes: 1, biBitCount: 32, ..Default::default() },
        ..Default::default()
    };
    unsafe { CreateDIBitmap(dc, Some(&info.bmiHeader), CBM_INIT as u32, Some(pixels.as_ptr() as _), Some(&info), DIB_RGB_COLORS) }
}

/// Ambil gambar layar lalu tampilkan lapisan untuk memilih area. Esc atau klik kanan membatalkan.
pub fn start(owner: HWND) {
    if SHOT.with_borrow(|s| s.is_some()) {
        return;
    }
    unsafe {
        let (x, y) = (GetSystemMetrics(SM_XVIRTUALSCREEN), GetSystemMetrics(SM_YVIRTUALSCREEN));
        let (w, h) = (GetSystemMetrics(SM_CXVIRTUALSCREEN), GetSystemMetrics(SM_CYVIRTUALSCREEN));
        let screen = GetDC(None);
        let mem = CreateCompatibleDC(Some(screen));
        let cap = CreateCompatibleBitmap(screen, w, h);
        let old = SelectObject(mem, cap.into());
        let _ = BitBlt(mem, 0, 0, w, h, Some(screen), x, y, SRCCOPY);
        SelectObject(mem, old);
        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER { biSize: size_of::<BITMAPINFOHEADER>() as u32, biWidth: w, biHeight: -h, biPlanes: 1, biBitCount: 32, ..Default::default() },
            ..Default::default()
        };
        let mut pixels = vec![0u8; (w * h * 4) as usize];
        GetDIBits(mem, cap, 0, h as u32, Some(pixels.as_mut_ptr() as _), &mut info, DIB_RGB_COLORS);
        let _ = DeleteObject(cap.into());
        let _ = DeleteDC(mem);
        // salinan gelap sebagai latar; area terpilih memakai gambar asli
        let dim_pixels: Vec<u8> = pixels.iter().enumerate().map(|(i, &c)| if i % 4 == 3 { c } else { (c as u16 * 45 / 100) as u8 }).collect();
        let bright = bitmap(screen, w, h, &pixels);
        let dim = bitmap(screen, w, h, &dim_pixels);
        ReleaseDC(None, screen);

        let Ok(module) = GetModuleHandleW(None) else { return };
        let instance: HINSTANCE = module.into();
        let class = w!("PonianOcr");
        RegisterClassW(&WNDCLASSW { lpfnWndProc: Some(proc), hInstance: instance, lpszClassName: class, hCursor: LoadCursorW(None, IDC_CROSS).unwrap_or_default(), ..Default::default() });
        let Ok(overlay) = CreateWindowExW(WS_EX_TOPMOST | WS_EX_TOOLWINDOW, class, w!("Select text area"), WS_POPUP, x, y, w, h, None, None, Some(instance), None) else {
            let _ = (DeleteObject(bright.into()), DeleteObject(dim.into()));
            return;
        };
        SHOT.set(Some(Shot { owner, overlay, pixels, w, h, bright, dim, start: None, end: POINT::default() }));
        let _ = ShowWindow(overlay, SW_SHOW);
        let _ = SetForegroundWindow(overlay);
        let _ = SetFocus(Some(overlay));
    }
}

fn selection(s: &Shot) -> Option<RECT> {
    let a = s.start?;
    let r = RECT { left: a.x.min(s.end.x).max(0), top: a.y.min(s.end.y).max(0), right: a.x.max(s.end.x).min(s.w), bottom: a.y.max(s.end.y).min(s.h) };
    (r.right - r.left > 4 && r.bottom - r.top > 4).then_some(r)
}

fn close(result: Option<std::result::Result<RECT, ()>>) {
    let Some(shot) = SHOT.take() else { return };
    unsafe {
        let _ = ReleaseCapture();
        let _ = DestroyWindow(shot.overlay);
        let _ = DeleteObject(shot.bright.into());
        let _ = DeleteObject(shot.dim.into());
    }
    let Some(Ok(r)) = result else { return };
    let (w, h) = (r.right - r.left, r.bottom - r.top);
    let mut crop = Vec::with_capacity((w * h * 4) as usize);
    for row in r.top..r.bottom {
        let at = ((row * shot.w + r.left) * 4) as usize;
        crop.extend_from_slice(&shot.pixels[at..at + (w * 4) as usize]);
    }
    // GDI mengisi alpha dengan 0; tanpa ini mesin OCR melihat gambar transparan (kosong)
    crop.iter_mut().skip(3).step_by(4).for_each(|a| *a = 255);
    let owner = shot.owner.0 as isize;
    // mesin OCR adalah API WinRT yang memblokir, jadi dijalankan di thread sendiri
    std::thread::spawn(move || {
        let text = recognize(&crop, w, h);
        *RESULT.lock().unwrap() = Some(text);
        let _ = unsafe { PostMessageW(Some(HWND(owner as _)), WM_APP_OCR, WPARAM(0), LPARAM(0)) };
    });
}

fn recognize(bgra: &[u8], w: i32, h: i32) -> std::result::Result<String, String> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
    let max = OcrEngine::MaxImageDimension().unwrap_or(2600) as i32;
    if w > max || h > max {
        return Err("Area too large. Select a smaller part.".into());
    }
    let engine = OcrEngine::TryCreateFromUserProfileLanguages().map_err(|_| "Windows has no OCR language. Add one in Settings \u{203A} Time & language.".to_string())?;
    let run = || -> windows::core::Result<String> {
        let buffer = CryptographicBuffer::CreateFromByteArray(bgra)?;
        let bmp = SoftwareBitmap::CreateCopyFromBuffer(&buffer, BitmapPixelFormat::Bgra8, w, h)?;
        let result = engine.RecognizeAsync(&bmp)?.join()?;
        Ok(result.Lines()?.into_iter().filter_map(|l| l.Text().ok().map(|t| t.to_string())).collect::<Vec<_>>().join("\n"))
    };
    match run() {
        Ok(t) if !t.trim().is_empty() => Ok(t),
        Ok(_) => Err("No text found. Try selecting a clearer area.".into()),
        Err(_) => Err("Couldn\u{2019}t read the image. Try again.".into()),
    }
}

#[cfg(test)]
mod tests {
    /// Gambar BGRA mentah dari PowerShell: `OCR_RAW=path;lebar;tinggi cargo test ocr -- --ignored`
    #[test]
    #[ignore]
    fn recognizes_raw_image() {
        let spec = std::env::var("OCR_RAW").expect("OCR_RAW belum diisi");
        let mut it = spec.split(';');
        let (path, w, h) = (it.next().unwrap(), it.next().unwrap().parse().unwrap(), it.next().unwrap().parse().unwrap());
        let bytes = std::fs::read(path).unwrap();
        let text = super::recognize(&bytes, w, h);
        eprintln!("hasil OCR: {text:?}");
        assert!(text.is_ok());
    }
}

extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let point = || POINT { x: (lp.0 & 0xffff) as i16 as i32, y: ((lp.0 >> 16) & 0xffff) as i16 as i32 };
    unsafe {
        match msg {
            WM_LBUTTONDOWN => {
                let p = point();
                SHOT.with_borrow_mut(|s| {
                    if let Some(s) = s {
                        (s.start, s.end) = (Some(p), p);
                    }
                });
                SetCapture(hwnd);
            }
            WM_MOUSEMOVE => {
                let p = point();
                let dragging = SHOT.with_borrow_mut(|s| s.as_mut().filter(|s| s.start.is_some()).map(|s| s.end = p).is_some());
                if dragging {
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
            }
            WM_LBUTTONUP => {
                let r = SHOT.with_borrow(|s| s.as_ref().and_then(selection));
                close(Some(r.ok_or(())));
            }
            WM_RBUTTONUP => close(None),
            WM_KEYDOWN if wp.0 == VK_ESCAPE.0 as usize => close(None),
            WM_PAINT => {
                let mut ps = PAINTSTRUCT::default();
                let dc = BeginPaint(hwnd, &mut ps);
                SHOT.with_borrow(|s| {
                    let Some(s) = s else { return };
                    let mem = CreateCompatibleDC(Some(dc));
                    let old = SelectObject(mem, s.dim.into());
                    let _ = BitBlt(dc, 0, 0, s.w, s.h, Some(mem), 0, 0, SRCCOPY);
                    if let Some(r) = selection(s) {
                        SelectObject(mem, s.bright.into());
                        let _ = BitBlt(dc, r.left, r.top, r.right - r.left, r.bottom - r.top, Some(mem), r.left, r.top, SRCCOPY);
                        let brush = CreateSolidBrush(COLORREF(0x00ff617b));
                        FrameRect(dc, &r, brush);
                        let _ = DeleteObject(brush.into());
                    }
                    SelectObject(mem, old);
                    let _ = DeleteDC(mem);
                });
                let _ = EndPaint(hwnd, &ps);
            }
            // klik di luar (misalnya pindah app) membatalkan
            WM_ACTIVATE if (wp.0 & 0xffff) as u32 == WA_INACTIVE => close(None),
            _ => return DefWindowProcW(hwnd, msg, wp, lp),
        }
        LRESULT(0)
    }
}
