//! Menggambar dengan Direct2D ke bitmap di memori, lalu mengirimnya ke layered window.
//! Piksel dengan alpha 0 otomatis tembus klik, jadi area di luar notch tidak menghalangi app lain.
use std::cell::{Cell, RefCell};
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
use windows_numerics::{Matrix3x2, Vector2};

/// kiri, atas, kanan, bawah dalam DIP
pub type R = [f32; 4];

pub const WHITE: u32 = 0xffffff;
pub const GRAY: u32 = 0x9a9a9a;
pub const DIM: u32 = 0x5a5a5a;
pub const CHIP: u32 = 0x1f1f1f;
pub const CARD: u32 = 0x1a1a1a;
pub const TRACK: u32 = 0x333333;
pub const ACCENT: u32 = 0x8b5cf6;
pub const GREEN: u32 = 0x30d158;

pub struct Fonts {
    pub text: IDWriteTextFormat,
    pub bold: IDWriteTextFormat,
    pub small: IDWriteTextFormat,
    pub center: IDWriteTextFormat,
    pub cbold: IDWriteTextFormat,
    pub csmall: IDWriteTextFormat,
    pub rbold: IDWriteTextFormat,
    pub rsmall: IDWriteTextFormat,
    /// angka waktu di halaman musik
    pub num: IDWriteTextFormat,
    pub rnum: IDWriteTextFormat,
    /// angka besar (timer, hitungan)
    pub big: IDWriteTextFormat,
    /// jam di baris status
    pub time: IDWriteTextFormat,
    pub tiny: IDWriteTextFormat,
    pub icon: IDWriteTextFormat,
    pub icon_small: IDWriteTextFormat,
    pub icon_mid: IDWriteTextFormat,
    pub icon_big: IDWriteTextFormat,
    /// judul bagian di jendela Pengaturan
    pub title: IDWriteTextFormat,
    /// teks yang boleh turun baris (label dan keterangan di Pengaturan), rata atas
    pub wrap: IDWriteTextFormat,
}

pub struct Gfx {
    factory: ID2D1Factory,
    dw: IDWriteFactory,
    rt: ID2D1DCRenderTarget,
    brush: ID2D1SolidColorBrush,
    dash: Option<ID2D1StrokeStyle>,
    /// bitmap gambar yang pernah digambar (cover album, ikon aplikasi)
    art: RefCell<Vec<(usize, ID2D1Bitmap)>>,
    pub f: Fonts,
    /// pengali alpha untuk isi notch, dipakai saat animasi fade
    pub alpha: Cell<f32>,
    dc: HDC,
    bmp: HBITMAP,
    old: HGDIOBJ,
    size: SIZE,
    /// piksel bitmap, dan jumlah baris teratas yang sedang diikat ke Direct2D
    bits: *mut u8,
    bound: Cell<i32>,
}

const NO_STYLE: Option<&ID2D1StrokeStyle> = None;

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
            let (bmp, bits) = dib(dc, w, h)?;
            let old = SelectObject(dc, bmp.into());
            rt.BindDC(dc, &RECT { left: 0, top: 0, right: w, bottom: h })?;
            rt.SetDpi(dpi, dpi);
            rt.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
            let brush = rt.CreateSolidColorBrush(&D2D1_COLOR_F { r: 0.0, g: 0.0, b: 0.0, a: 1.0 }, None)?;
            let dash = factory.CreateStrokeStyle(&D2D1_STROKE_STYLE_PROPERTIES { dashStyle: D2D1_DASH_STYLE_DASH, dashCap: D2D1_CAP_STYLE_ROUND, ..Default::default() }, None).ok();

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
            let (normal, semi, heavy) = (DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_FONT_WEIGHT_BOLD);
            let (left, center, right) = (DWRITE_TEXT_ALIGNMENT_LEADING, DWRITE_TEXT_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_TRAILING);
            let f = Fonts {
                text: font(ui, 13.0, normal, left)?,
                bold: font(ui, 14.5, heavy, left)?,
                small: font(ui, 11.5, normal, left)?,
                center: font(ui, 13.0, normal, center)?,
                cbold: font(ui, 13.0, semi, center)?,
                csmall: font(ui, 11.5, normal, center)?,
                rbold: font(ui, 13.0, semi, right)?,
                rsmall: font(ui, 11.5, normal, right)?,
                num: font(ui, 15.0, normal, left)?,
                rnum: font(ui, 15.0, normal, right)?,
                big: font(ui, 27.0, heavy, left)?,
                time: font(ui, 13.5, heavy, left)?,
                tiny: font(ui, 9.5, semi, left)?,
                // font ikon bawaan Windows 10 dan 11
                icon: font(mdl, 15.0, normal, center)?,
                icon_small: font(mdl, 9.5, normal, center)?,
                icon_mid: font(mdl, 12.5, normal, center)?,
                icon_big: font(mdl, 24.0, normal, center)?,
                title: font(ui, 16.0, semi, left)?,
                wrap: font(ui, 12.5, normal, left)?,
            };
            f.wrap.SetWordWrapping(DWRITE_WORD_WRAPPING_WRAP)?;
            f.wrap.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_NEAR)?;
            Ok(Self { factory, dw, rt, brush, dash, art: RefCell::new(Vec::new()), f, alpha: Cell::new(1.0), dc, bmp, old, size: SIZE { cx: w, cy: h }, bits, bound: Cell::new(h) })
        }
    }

    /// Ganti ukuran atau DPI permukaan gambar tanpa membuat ulang objek Direct2D.
    pub fn resize(&mut self, w: i32, h: i32, dpi: f32) -> Result<()> {
        unsafe {
            if (w, h) != (self.size.cx, self.size.cy) {
                let (bmp, bits) = dib(self.dc, w, h)?;
                SelectObject(self.dc, bmp.into());
                let _ = DeleteObject(self.bmp.into());
                (self.bmp, self.bits, self.size) = (bmp, bits, SIZE { cx: w, cy: h });
                self.bound.set(h);
                self.rt.BindDC(self.dc, &RECT { left: 0, top: 0, right: w, bottom: h })?;
            }
            self.rt.SetDpi(dpi, dpi);
        }
        Ok(())
    }

    /// Batasi area gambar ke `rows` baris teratas. Direct2D membersihkan dan menyalin seluruh area yang
    /// diikat di setiap frame, jadi saat notch kecil tidak perlu mengikat seluruh window.
    pub fn bind_rows(&self, rows: i32) {
        let rows = rows.clamp(1, self.size.cy);
        let bound = self.bound.get();
        if rows == bound {
            return;
        }
        unsafe {
            if rows < bound {
                // baris di bawah area baru tidak lagi disentuh Direct2D; kosongkan sisa gambar lama
                let row_bytes = self.size.cx as usize * 4;
                std::ptr::write_bytes(self.bits.add(rows as usize * row_bytes), 0, (bound - rows) as usize * row_bytes);
            }
            if self.rt.BindDC(self.dc, &RECT { left: 0, top: 0, right: self.size.cx, bottom: rows }).is_ok() {
                self.bound.set(rows);
            }
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

    /// Selesaikan gambar dan salin ke `dc` jendela biasa (bukan layered). Alpha ikut tersalin, jadi
    /// piksel tembus pandang memperlihatkan latar kaca DWM di belakangnya.
    pub fn present(&self, dc: HDC) {
        unsafe {
            let _ = self.rt.EndDraw(None, None);
            let _ = BitBlt(dc, 0, 0, self.size.cx, self.size.cy, Some(self.dc), 0, 0, SRCCOPY);
        }
    }

    /// Garis tepi persegi membulat.
    pub fn stroke(&self, r: R, radius: f32, rgb: u32, width: f32) {
        self.color(rgb);
        unsafe { self.rt.DrawRoundedRectangle(&D2D1_ROUNDED_RECT { rect: rect(r), radiusX: radius, radiusY: radius }, &self.brush, width, NO_STYLE) }
    }

    /// Tinggi teks dalam DIP kalau dibungkus selebar `width` (untuk format yang boleh turun baris).
    pub fn height(&self, s: &str, format: &IDWriteTextFormat, width: f32) -> f32 {
        let utf16: Vec<u16> = s.encode_utf16().collect();
        let mut m = DWRITE_TEXT_METRICS::default();
        unsafe {
            match self.dw.CreateTextLayout(&utf16, format, width, 2000.0) {
                Ok(layout) if layout.GetMetrics(&mut m).is_ok() => m.height,
                _ => 18.0,
            }
        }
    }

    fn rgba(&self, rgb: u32, a: f32) -> D2D1_COLOR_F {
        let ch = |shift: u32| ((rgb >> shift) & 255) as f32 / 255.0;
        D2D1_COLOR_F { r: ch(16), g: ch(8), b: ch(0), a: a * self.alpha.get() }
    }

    fn color(&self, rgb: u32) {
        unsafe { self.brush.SetColor(&self.rgba(rgb, 1.0)) }
    }

    pub fn rrect(&self, r: R, radius: f32, rgb: u32) {
        self.color(rgb);
        unsafe { self.rt.FillRoundedRectangle(&D2D1_ROUNDED_RECT { rect: rect(r), radiusX: radius, radiusY: radius }, &self.brush) }
    }

    /// Bingkai garis putus-putus (area jatuhkan file).
    pub fn dashed(&self, r: R, radius: f32, rgb: u32) {
        self.color(rgb);
        unsafe { self.rt.DrawRoundedRectangle(&D2D1_ROUNDED_RECT { rect: rect(r), radiusX: radius, radiusY: radius }, &self.brush, 1.2, self.dash.as_ref()) }
    }

    pub fn text(&self, s: &str, format: &IDWriteTextFormat, r: R, rgb: u32) {
        let utf16: Vec<u16> = s.encode_utf16().collect();
        self.color(rgb);
        unsafe {
            self.rt.DrawText(&utf16, format, &rect(r), &self.brush, D2D1_DRAW_TEXT_OPTIONS_CLIP | D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT, DWRITE_MEASURING_MODE_NATURAL)
        }
    }

    /// Lebar teks dalam DIP, untuk menyusun beberapa teks berdampingan.
    pub fn width(&self, s: &str, format: &IDWriteTextFormat) -> f32 {
        let utf16: Vec<u16> = s.encode_utf16().collect();
        let mut m = DWRITE_TEXT_METRICS::default();
        unsafe {
            match self.dw.CreateTextLayout(&utf16, format, 2000.0, 100.0) {
                Ok(layout) if layout.GetMetrics(&mut m).is_ok() => m.widthIncludingTrailingWhitespace,
                _ => s.chars().count() as f32 * 7.0,
            }
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
        self.path(true, rgb, 0.0, |s| unsafe {
            s.BeginFigure(p(l - ear, 0.0), D2D1_FIGURE_BEGIN_FILLED);
            s.AddArc(&arc(l, ear, ear, cw));
            s.AddLine(p(l, h - rad));
            s.AddArc(&arc(l + rad, h, rad, ccw));
            s.AddLine(p(r - rad, h));
            s.AddArc(&arc(r, h - rad, rad, ccw));
            s.AddLine(p(r, ear));
            s.AddArc(&arc(r + ear, 0.0, ear, cw));
            s.EndFigure(D2D1_FIGURE_END_CLOSED);
        });
    }

    /// Pendar warna di tepi bawah notch (mengikuti warna cover album).
    pub fn glow(&self, l: f32, r: f32, h: f32, radius: f32, rgb: u32) {
        unsafe {
            let stops = [D2D1_GRADIENT_STOP { position: 0.0, color: self.rgba(rgb, 0.0) }, D2D1_GRADIENT_STOP { position: 1.0, color: self.rgba(rgb, 0.42) }];
            let Ok(collection) = self.rt.CreateGradientStopCollection(&stops, D2D1_GAMMA_2_2, D2D1_EXTEND_MODE_CLAMP) else { return };
            let props = D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES { startPoint: Vector2 { X: 0.0, Y: h * 0.35 }, endPoint: Vector2 { X: 0.0, Y: h } };
            let Ok(brush) = self.rt.CreateLinearGradientBrush(&props, None, &collection) else { return };
            self.rt.PushAxisAlignedClip(&rect([l, h * 0.35, r, h]), D2D1_ANTIALIAS_MODE_ALIASED);
            self.rt.FillRoundedRectangle(&D2D1_ROUNDED_RECT { rect: rect([l, -radius, r, h]), radiusX: radius, radiusY: radius }, &brush);
            self.rt.PopAxisAlignedClip();
        }
    }

    /// Busur dari sudut `from` sepanjang `sweep` derajat, searah jarum jam dari jam 12.
    pub fn arc(&self, cx: f32, cy: f32, rad: f32, width: f32, from: f32, sweep: f32, rgb: u32) {
        if sweep <= 0.0 {
            return;
        }
        if sweep >= 359.9 {
            self.color(rgb);
            let circle = D2D1_ELLIPSE { point: Vector2 { X: cx, Y: cy }, radiusX: rad, radiusY: rad };
            unsafe { self.rt.DrawEllipse(&circle, &self.brush, width, NO_STYLE) };
            return;
        }
        let at = |deg: f32| Vector2 { X: cx + rad * deg.to_radians().sin(), Y: cy - rad * deg.to_radians().cos() };
        self.path(false, rgb, width, |s| unsafe {
            s.BeginFigure(at(from), D2D1_FIGURE_BEGIN_HOLLOW);
            s.AddArc(&D2D1_ARC_SEGMENT {
                point: at(from + sweep),
                size: D2D_SIZE_F { width: rad, height: rad },
                rotationAngle: 0.0,
                sweepDirection: D2D1_SWEEP_DIRECTION_CLOCKWISE,
                arcSize: if sweep > 180.0 { D2D1_ARC_SIZE_LARGE } else { D2D1_ARC_SIZE_SMALL },
            });
            s.EndFigure(D2D1_FIGURE_END_OPEN);
        });
    }

    /// Cincin progres, dimulai dari jam 12 searah jarum jam.
    pub fn ring(&self, cx: f32, cy: f32, rad: f32, width: f32, frac: f32, rgb: u32, track: u32) {
        self.arc(cx, cy, rad, width, 0.0, 360.0, track);
        self.arc(cx, cy, rad, width, 0.0, 360.0 * frac.clamp(0.0, 1.0), rgb);
    }

    /// Poligon terisi (segitiga tombol putar dan sejenisnya).
    pub fn poly(&self, points: &[(f32, f32)], rgb: u32) {
        let Some(first) = points.first() else { return };
        self.path(true, rgb, 0.0, |s| unsafe {
            s.BeginFigure(Vector2 { X: first.0, Y: first.1 }, D2D1_FIGURE_BEGIN_FILLED);
            for p in &points[1..] {
                s.AddLine(Vector2 { X: p.0, Y: p.1 });
            }
            s.EndFigure(D2D1_FIGURE_END_CLOSED);
        });
    }

    fn path(&self, fill: bool, rgb: u32, width: f32, build: impl FnOnce(&ID2D1GeometrySink)) {
        let Ok(geom) = (unsafe { self.factory.CreatePathGeometry() }) else { return };
        let Ok(sink) = (unsafe { geom.Open() }) else { return };
        build(&sink);
        if unsafe { sink.Close() }.is_ok() {
            self.color(rgb);
            unsafe {
                if fill {
                    self.rt.FillGeometry(&geom, &self.brush, None::<&ID2D1Brush>);
                } else {
                    self.rt.DrawGeometry(&geom, &self.brush, width, NO_STYLE);
                }
            }
        }
    }

    /// Gambar persegi `side` x `side` piksel BGRA ke `r` dengan sudut membulat.
    pub fn image(&self, bgra: &[u8], side: u32, r: R, radius: f32) {
        // kunci = alamat data dicampur cuplikan isinya, supaya alamat yang dipakai ulang tidak salah gambar
        let key = bgra.iter().step_by(61).fold(bgra.as_ptr() as usize, |h, &b| h.wrapping_mul(31).wrapping_add(b as usize));
        let mut cache = self.art.borrow_mut();
        if !cache.iter().any(|c| c.0 == key) {
            let props = D2D1_BITMAP_PROPERTIES { pixelFormat: D2D1_PIXEL_FORMAT { format: DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED }, dpiX: 96.0, dpiY: 96.0 };
            let Ok(made) = (unsafe { self.rt.CreateBitmap(D2D_SIZE_U { width: side, height: side }, Some(bgra.as_ptr() as _), side * 4, &props) }) else { return };
            // cukup untuk cover album dan selusin ikon aplikasi
            if cache.len() >= 16 {
                cache.remove(0);
            }
            cache.push((key, made));
        }
        let Some((_, bitmap)) = cache.iter().find(|c| c.0 == key) else { return };
        unsafe {
            let Ok(brush) = self.rt.CreateBitmapBrush(bitmap, None, None) else { return };
            let s = (r[2] - r[0]) / side as f32;
            brush.SetTransform(&Matrix3x2 { M11: s, M12: 0.0, M21: 0.0, M22: s, M31: r[0], M32: r[1] });
            brush.SetOpacity(self.alpha.get());
            self.rt.FillRoundedRectangle(&D2D1_ROUNDED_RECT { rect: rect(r), radiusX: radius, radiusY: radius }, &brush);
        }
    }

    /// Geser semua gambar berikutnya sejauh (dx, dy) DIP; (0, 0) mengembalikan ke semula.
    pub fn shift(&self, dx: f32, dy: f32) {
        unsafe { self.rt.SetTransform(&Matrix3x2 { M11: 1.0, M12: 0.0, M21: 0.0, M22: 1.0, M31: dx, M32: dy }) }
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

/// Bitmap 32-bit dengan baris dari atas, tempat Direct2D menggambar.
fn dib(dc: HDC, w: i32, h: i32) -> Result<(HBITMAP, *mut u8)> {
    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER { biSize: size_of::<BITMAPINFOHEADER>() as u32, biWidth: w, biHeight: -h, biPlanes: 1, biBitCount: 32, ..Default::default() },
        ..Default::default()
    };
    let mut bits = std::ptr::null_mut();
    let bmp = unsafe { CreateDIBSection(Some(dc), &info, DIB_RGB_COLORS, &mut bits, None, 0) }?;
    Ok((bmp, bits as *mut u8))
}

fn rect(r: R) -> D2D_RECT_F {
    D2D_RECT_F { left: r[0], top: r[1], right: r[2], bottom: r[3] }
}
