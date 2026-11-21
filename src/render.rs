//! Menggambar dengan Direct2D ke bitmap di memori, lalu mengirimnya ke layered window.
//! Piksel dengan alpha 0 otomatis tembus klik, jadi area di luar notch tidak menghalangi app lain.
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
use windows_numerics::Vector2;

/// kiri, atas, kanan, bawah dalam DIP
pub type R = [f32; 4];

pub const WHITE: u32 = 0xffffff;
pub const GRAY: u32 = 0x9a9a9a;
pub const DIM: u32 = 0x5a5a5a;
pub const CHIP: u32 = 0x262626;
pub const ACCENT: u32 = 0x7b61ff;
pub const GREEN: u32 = 0x34d058;

pub struct Fonts {
    pub text: IDWriteTextFormat,
    pub bold: IDWriteTextFormat,
    pub small: IDWriteTextFormat,
    pub center: IDWriteTextFormat,
    pub cbold: IDWriteTextFormat,
    pub csmall: IDWriteTextFormat,
    pub rbold: IDWriteTextFormat,
    pub rsmall: IDWriteTextFormat,
    pub big: IDWriteTextFormat,
    pub icon: IDWriteTextFormat,
    pub icon_small: IDWriteTextFormat,
}

pub struct Gfx {
    factory: ID2D1Factory,
    rt: ID2D1DCRenderTarget,
    brush: ID2D1SolidColorBrush,
    pub f: Fonts,
    /// pengali alpha untuk isi notch, dipakai saat animasi fade
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
            let font = |family: PCWSTR, size: f32, weight: DWRITE_FONT_WEIGHT, align: DWRITE_TEXT_ALIGNMENT| -> Result<IDWriteTextFormat> {
                let f = dw.CreateTextFormat(family, None, weight, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_STRETCH_NORMAL, size, w!(""))?;
                f.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
                f.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
                f.SetTextAlignment(align)?;
                let trim = DWRITE_TRIMMING { granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER, ..Default::default() };
                f.SetTrimming(&trim, &dw.CreateEllipsisTrimmingSign(&f)?)?;
                Ok(f)
            };
            let (ui, mdl) = (w!("Segoe UI"), w!("Segoe MDL2 Assets"));
            let (normal, semi) = (DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_WEIGHT_SEMI_BOLD);
            let (left, center, right) = (DWRITE_TEXT_ALIGNMENT_LEADING, DWRITE_TEXT_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_TRAILING);
            let f = Fonts {
                text: font(ui, 13.0, normal, left)?,
                bold: font(ui, 15.0, semi, left)?,
                small: font(ui, 12.0, normal, left)?,
                center: font(ui, 13.0, normal, center)?,
                cbold: font(ui, 14.0, semi, center)?,
                csmall: font(ui, 12.0, normal, center)?,
                rbold: font(ui, 14.0, semi, right)?,
                rsmall: font(ui, 12.0, normal, right)?,
                big: font(ui, 30.0, semi, left)?,
                // font ikon bawaan Windows 10 dan 11
                icon: font(mdl, 15.0, normal, center)?,
                icon_small: font(mdl, 9.0, normal, center)?,
            };
            Ok(Self { factory, rt, brush, f, alpha: Cell::new(1.0), dc, bmp, old, size: SIZE { cx: w, cy: h } })
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

    fn color(&self, rgb: u32) {
        let ch = |shift: u32| ((rgb >> shift) & 255) as f32 / 255.0;
        unsafe { self.brush.SetColor(&D2D1_COLOR_F { r: ch(16), g: ch(8), b: ch(0), a: self.alpha.get() }) }
    }

    pub fn rrect(&self, r: R, radius: f32, rgb: u32) {
        self.color(rgb);
        unsafe { self.rt.FillRoundedRectangle(&D2D1_ROUNDED_RECT { rect: rect(r), radiusX: radius, radiusY: radius }, &self.brush) }
    }

    pub fn text(&self, s: &str, format: &IDWriteTextFormat, r: R, rgb: u32) {
        let utf16: Vec<u16> = s.encode_utf16().collect();
        self.color(rgb);
        unsafe {
            self.rt.DrawText(&utf16, format, &rect(r), &self.brush, D2D1_DRAW_TEXT_OPTIONS_CLIP | D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT, DWRITE_MEASURING_MODE_NATURAL)
        }
    }

    /// Bentuk notch: badan dengan sudut bawah membulat, dan "telinga" cekung di kedua sudut atas
    /// supaya notch terlihat menyatu dengan tepi layar.
    pub fn notch(&self, l: f32, r: f32, h: f32, radius: f32, ear: f32, rgb: u32) {
        let (rad, ear) = (radius.min(h / 2.0).min((r - l) / 2.0), ear.min(h / 2.0));
        let p = |x: f32, y: f32| Vector2 { X: x, Y: y };
        let arc = |x: f32, y: f32, s: f32, dir: D2D1_SWEEP_DIRECTION| D2D1_ARC_SEGMENT {
            point: p(x, y),
            size: D2D_SIZE_F { width: s, height: s },
            rotationAngle: 0.0,
            sweepDirection: dir,
            arcSize: D2D1_ARC_SIZE_SMALL,
        };
        let (cw, ccw) = (D2D1_SWEEP_DIRECTION_CLOCKWISE, D2D1_SWEEP_DIRECTION_COUNTER_CLOCKWISE);
        self.fill_path(|s| unsafe {
            s.BeginFigure(p(l - ear, 0.0), D2D1_FIGURE_BEGIN_FILLED);
            s.AddArc(&arc(l, ear, ear, cw));
            s.AddLine(p(l, h - rad));
            s.AddArc(&arc(l + rad, h, rad, ccw));
            s.AddLine(p(r - rad, h));
            s.AddArc(&arc(r, h - rad, rad, ccw));
            s.AddLine(p(r, ear));
            s.AddArc(&arc(r + ear, 0.0, ear, cw));
            s.EndFigure(D2D1_FIGURE_END_CLOSED);
        }, rgb);
    }

    /// Cincin progres, dimulai dari jam 12 searah jarum jam.
    pub fn ring(&self, cx: f32, cy: f32, rad: f32, width: f32, frac: f32, rgb: u32, track: u32) {
        let circle = D2D1_ELLIPSE { point: Vector2 { X: cx, Y: cy }, radiusX: rad, radiusY: rad };
        self.color(track);
        unsafe { self.rt.DrawEllipse(&circle, &self.brush, width, None::<&ID2D1StrokeStyle>) };
        let frac = frac.clamp(0.0, 1.0);
        if frac >= 0.999 {
            self.color(rgb);
            unsafe { self.rt.DrawEllipse(&circle, &self.brush, width, None::<&ID2D1StrokeStyle>) };
        } else if frac > 0.0 {
            let a = frac * std::f32::consts::TAU;
            let Ok(geom) = (unsafe { self.factory.CreatePathGeometry() }) else { return };
            let ok = (|| unsafe {
                let s = geom.Open()?;
                s.BeginFigure(Vector2 { X: cx, Y: cy - rad }, D2D1_FIGURE_BEGIN_HOLLOW);
                s.AddArc(&D2D1_ARC_SEGMENT {
                    point: Vector2 { X: cx + rad * a.sin(), Y: cy - rad * a.cos() },
                    size: D2D_SIZE_F { width: rad, height: rad },
                    rotationAngle: 0.0,
                    sweepDirection: D2D1_SWEEP_DIRECTION_CLOCKWISE,
                    arcSize: if frac > 0.5 { D2D1_ARC_SIZE_LARGE } else { D2D1_ARC_SIZE_SMALL },
                });
                s.EndFigure(D2D1_FIGURE_END_OPEN);
                s.Close()
            })();
            if ok.is_ok() {
                self.color(rgb);
                unsafe { self.rt.DrawGeometry(&geom, &self.brush, width, None::<&ID2D1StrokeStyle>) };
            }
        }
    }

    fn fill_path(&self, build: impl FnOnce(&ID2D1GeometrySink), rgb: u32) {
        let Ok(geom) = (unsafe { self.factory.CreatePathGeometry() }) else { return };
        let Ok(sink) = (unsafe { geom.Open() }) else { return };
        build(&sink);
        if unsafe { sink.Close() }.is_ok() {
            self.color(rgb);
            unsafe { self.rt.FillGeometry(&geom, &self.brush, None::<&ID2D1Brush>) };
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
