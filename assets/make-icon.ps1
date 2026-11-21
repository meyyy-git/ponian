# Menggambar ikon Ponian dan menyusunnya menjadi ponian.ico + ponian.res (resource yang ditautkan build.rs).
# Jalankan dari folder mana pun: powershell -File assets\make-icon.ps1
Add-Type -AssemblyName System.Drawing
$dir = $PSScriptRoot
function RoundPath([float]$x, [float]$y, [float]$w, [float]$h, [float]$r) {
    $p = New-Object System.Drawing.Drawing2D.GraphicsPath; $d = $r * 2
    $p.AddArc($x, $y, $d, $d, 180, 90); $p.AddArc($x + $w - $d, $y, $d, $d, 270, 90)
    $p.AddArc($x + $w - $d, $y + $h - $d, $d, $d, 0, 90); $p.AddArc($x, $y + $h - $d, $d, $d, 90, 90)
    $p.CloseFigure(); $p
}
function Rgb($a, $r, $g, $b) { [System.Drawing.Color]::FromArgb($a, $r, $g, $b) }
function Pt($x, $y) { New-Object System.Drawing.PointF $x, $y }

# gambar induk 1024 px: badan membulat bergradasi biru, notch hitam menggantung di atas, empat batang audio
$S = 1024; $m = 64
$big = New-Object System.Drawing.Bitmap $S, $S, ([System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
$g = [System.Drawing.Graphics]::FromImage($big); $g.SmoothingMode = 'AntiAlias'; $g.Clear([System.Drawing.Color]::Transparent)
$body = RoundPath $m $m ($S - 2 * $m) ($S - 2 * $m) 205
$g.FillPath((New-Object System.Drawing.Drawing2D.LinearGradientBrush (Pt 0 $m), (Pt 0 ($S - $m)), (Rgb 255 100 190 255), (Rgb 255 0 78 216)), $body)
$g.SetClip($body)
# kilau lembut di bagian atas
$g.FillEllipse((New-Object System.Drawing.Drawing2D.LinearGradientBrush (Pt 0 $m), (Pt 0 560), (Rgb 60 255 255 255), (Rgb 0 255 255 255)), -200, -420, 1424, 980)
# notch: persegi hitam menempel di tepi atas dengan sudut bawah membulat
$nw = 520; $nh = 200
$g.FillPath((New-Object System.Drawing.SolidBrush (Rgb 255 10 10 12)), (RoundPath (($S - $nw) / 2) ($m - 120) $nw ($nh + 120) 78))
$g.ResetClip()
$bars = 62, 104, 78, 40; $bw = 30; $gap = 26; $bx = ($S - (4 * $bw + 3 * $gap)) / 2; $cy = $m + $nh / 2 - 6
for ($i = 0; $i -lt 4; $i++) { $g.FillPath([System.Drawing.Brushes]::White, (RoundPath ($bx + $i * ($bw + $gap)) ($cy - $bars[$i] / 2) $bw $bars[$i] 15)) }
# garis tepi tipis supaya tegas di latar gelap
$g.DrawPath((New-Object System.Drawing.Pen (Rgb 50 255 255 255), 6), $body)
$g.Dispose(); $big.Save("$dir\ponian-1024.png")

# ICO: 256 px sebagai PNG, ukuran lain sebagai bitmap 32-bit (bentuk yang diterima semua versi rc.exe)
$sizes = 256, 128, 64, 48, 32, 24, 16; $images = @()
foreach ($s in $sizes) {
    $bmp = New-Object System.Drawing.Bitmap $s, $s, ([System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $gg = [System.Drawing.Graphics]::FromImage($bmp); $gg.InterpolationMode = 'HighQualityBicubic'; $gg.PixelOffsetMode = 'HighQuality'; $gg.CompositingQuality = 'HighQuality'
    $gg.DrawImage($big, 0, 0, $s, $s); $gg.Dispose()
    $ms = New-Object IO.MemoryStream
    if ($s -eq 256) { $bmp.Save($ms, [System.Drawing.Imaging.ImageFormat]::Png) }
    else {
        $data = $bmp.LockBits((New-Object System.Drawing.Rectangle 0, 0, $s, $s), 'ReadOnly', 'Format32bppArgb')
        $px = New-Object byte[] ($s * $s * 4); [Runtime.InteropServices.Marshal]::Copy($data.Scan0, $px, 0, $px.Length); $bmp.UnlockBits($data)
        $w = New-Object IO.BinaryWriter $ms; $maskRow = [int]([math]::Floor(($s + 31) / 32) * 4)
        # BITMAPINFOHEADER (tinggi digandakan untuk mask), piksel dari baris bawah, lalu mask kosong
        $w.Write([int]40); $w.Write([int]$s); $w.Write([int]($s * 2)); $w.Write([int16]1); $w.Write([int16]32); $w.Write([int]0)
        $w.Write([int]($px.Length + $maskRow * $s)); $w.Write([int]0); $w.Write([int]0); $w.Write([int]0); $w.Write([int]0)
        for ($y = $s - 1; $y -ge 0; $y--) { $w.Write($px, $y * $s * 4, $s * 4) }
        $w.Write((New-Object byte[] ($maskRow * $s))); $w.Flush()
    }
    $images += , ($ms.ToArray()); $bmp.Dispose()
}
$big.Dispose()
$out = New-Object IO.MemoryStream; $w = New-Object IO.BinaryWriter $out
$w.Write([int16]0); $w.Write([int16]1); $w.Write([int16]$sizes.Count); $offset = 6 + 16 * $sizes.Count
for ($i = 0; $i -lt $sizes.Count; $i++) {
    $s = $sizes[$i]; $w.Write([byte]($s % 256)); $w.Write([byte]($s % 256)); $w.Write([byte]0); $w.Write([byte]0)
    $w.Write([int16]1); $w.Write([int16]32); $w.Write([int]$images[$i].Length); $w.Write([int]$offset); $offset += $images[$i].Length
}
foreach ($img in $images) { $w.Write($img) }; $w.Flush(); [IO.File]::WriteAllBytes("$dir\ponian.ico", $out.ToArray())

$rc = Get-ChildItem "C:\Program Files (x86)\Windows Kits\10\bin" -Recurse -Filter rc.exe -ErrorAction SilentlyContinue | Where-Object { $_.FullName -match '\\x64\\' } | Sort-Object FullName -Descending | Select-Object -First 1
if (-not $rc) { Write-Error "rc.exe (Windows SDK) tidak ditemukan; ponian.res tidak diperbarui"; exit 1 }
Push-Location $dir; & $rc.FullName /nologo /fo ponian.res ponian.rc; Pop-Location
"ponian.ico dan ponian.res diperbarui"
