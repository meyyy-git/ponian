//! Menggambar dengan Direct2D ke bitmap di memori, lalu mengirimnya ke layered window.
//! Piksel dengan alpha 0 otomatis tembus klik, jadi area di luar pill tidak menghalangi app lain.
use std::cell::Cell;
use windows::{
    Win32::{
        Foundation::*,
        Graphics::{
            Direct2D::{Common::*, *},
            DirectWrite::*,
            Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM,
            Gdi::*,
        },
        UI::WindowsAndMessaging::{ULW_ALPHA, UpdateLayeredWindow},
    },
    core::*,
};

/// kiri, atas, kanan, bawah dalam DIP
pub type R = [f32; 4];

pub const WHITE: u32 = 0xffffff;
pub const GRAY: u32 = 0x9a9a9a;

pub struct Gfx {
    rt: ID2D1DCRenderTarget,
    brush: ID2D1SolidColorBrush,
    pub text: IDWriteTextFormat,
    pub bold: IDWriteTextFormat,
    pub small: IDWriteTextFormat,
    pub center: IDWriteTextFormat,
    pub csmall: IDWriteTextFormat,
    pub icon: IDWriteTextFormat,
    /// pengali alpha untuk isi pill, dipakai saat animasi fade
    pub alpha: Cell<f32>,
    dc: HDC,
    bmp: HBITMAP,
    old: HGDIOBJ,
    size: SIZE,
}

impl Gfx {
    pub fn new(w: i32, h: i32, dpi: f32) -> Result<Self> {
        unsafe {
            let factory: ID2D1Factory = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
            // software: permukaannya kecil, dan tanpa device GPU pemakaian RAM jauh lebih rendah
            let rt = factory.CreateDCRenderTarget(&D2D1_RENDER_TARGET_PROPERTIES {
                r#type: D2D1_RENDER_TARGET_TYPE_SOFTWARE,
                pixelFormat: D2D1_PIXEL_FORMAT { format: DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED },
                ..Default::default()
            })?;
            let dc = CreateCompatibleDC(None);
            let info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: w,
                    biHeight: -h,
                    biPlanes: 1,
                    biBitCount: 32,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut bits = std::ptr::null_mut();
            let bmp = CreateDIBSection(Some(dc), &info, DIB_RGB_COLORS, &mut bits, None, 0)?;
            let old = SelectObject(dc, bmp.into());
            rt.BindDC(dc, &RECT { left: 0, top: 0, right: w, bottom: h })?;
            rt.SetDpi(dpi, dpi);
            rt.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
            let brush = rt.CreateSolidColorBrush(&D2D1_COLOR_F { r: 0.0, g: 0.0, b: 0.0, a: 1.0 }, None)?;

            let dw: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
            let font = |family: PCWSTR, size: f32, weight: DWRITE_FONT_WEIGHT, center: bool| -> Result<IDWriteTextFormat> {
                let f = dw.CreateTextFormat(family, None, weight, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_STRETCH_NORMAL, size, w!(""))?;
                f.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
                f.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
                if center {
                    f.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER)?;
                }
                let trim = DWRITE_TRIMMING { granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER, ..Default::default() };
                f.SetTrimming(&trim, &dw.CreateEllipsisTrimmingSign(&f)?)?;
                Ok(f)
            };
            let ui = w!("Segoe UI");
            Ok(Self {
                text: font(ui, 13.0, DWRITE_FONT_WEIGHT_NORMAL, false)?,
                bold: font(ui, 14.0, DWRITE_FONT_WEIGHT_SEMI_BOLD, false)?,
                small: font(ui, 12.0, DWRITE_FONT_WEIGHT_NORMAL, false)?,
                center: font(ui, 13.0, DWRITE_FONT_WEIGHT_NORMAL, true)?,
                csmall: font(ui, 12.0, DWRITE_FONT_WEIGHT_NORMAL, true)?,
                // font ikon bawaan Windows 10 dan 11
                icon: font(w!("Segoe MDL2 Assets"), 15.0, DWRITE_FONT_WEIGHT_NORMAL, true)?,
                rt,
                brush,
                alpha: Cell::new(1.0),
                dc,
                bmp,
                old,
                size: SIZE { cx: w, cy: h },
            })
        }
    }

    pub fn begin(&self) {
        unsafe {
            self.rt.BeginDraw();
            self.rt.Clear(None);
        }
    }

    pub fn end(&self, hwnd: HWND) {
        unsafe {
            let _ = self.rt.EndDraw(None, None);
            let screen = GetDC(None);
            let blend = BLENDFUNCTION { BlendOp: AC_SRC_OVER as u8, SourceConstantAlpha: 255, AlphaFormat: AC_SRC_ALPHA as u8, ..Default::default() };
            let _ = UpdateLayeredWindow(hwnd, Some(screen), None, Some(&self.size), Some(self.dc), Some(&POINT::default()), COLORREF(0), Some(&blend), ULW_ALPHA);
            ReleaseDC(None, screen);
        }
    }

    fn color(&self, rgb: u32, a: f32) {
        let ch = |shift: u32| ((rgb >> shift) & 255) as f32 / 255.0;
        unsafe { self.brush.SetColor(&D2D1_COLOR_F { r: ch(16), g: ch(8), b: ch(0), a: a * self.alpha.get() }) }
    }

    pub fn rrect(&self, r: R, radius: f32, rgb: u32) {
        self.color(rgb, 1.0);
        unsafe { self.rt.FillRoundedRectangle(&D2D1_ROUNDED_RECT { rect: rect(r), radiusX: radius, radiusY: radius }, &self.brush) }
    }

    pub fn text(&self, s: &str, format: &IDWriteTextFormat, r: R, rgb: u32) {
        let utf16: Vec<u16> = s.encode_utf16().collect();
        self.color(rgb, 1.0);
        unsafe {
            self.rt.DrawText(&utf16, format, &rect(r), &self.brush, D2D1_DRAW_TEXT_OPTIONS_CLIP | D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT, DWRITE_MEASURING_MODE_NATURAL)
        }
    }

    pub fn clip(&self, r: R) {
        unsafe { self.rt.PushAxisAlignedClip(&rect(r), D2D1_ANTIALIAS_MODE_ALIASED) }
    }

    pub fn unclip(&self) {
        unsafe { self.rt.PopAxisAlignedClip() }
    }
}

impl Drop for Gfx {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.dc, self.old);
            let _ = DeleteObject(self.bmp.into());
            let _ = DeleteDC(self.dc);
        }
    }
}

fn rect(r: R) -> D2D_RECT_F {
    D2D_RECT_F { left: r[0], top: r[1], right: r[2], bottom: r[3] }
}
