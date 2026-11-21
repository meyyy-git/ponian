fn main() {
    // Common Controls v6: tampilan kontrol modern dan placeholder (cue banner) di kotak teks
    println!("cargo:rustc-link-arg-bins=/MANIFEST:EMBED");
    println!(
        "cargo:rustc-link-arg-bins=/MANIFESTDEPENDENCY:type='win32' name='Microsoft.Windows.Common-Controls' version='6.0.0.0' processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'"
    );
    // ikon aplikasi dan keterangan versi, sudah dikompilasi supaya build tidak butuh rc.exe.
    // shortcut: ponian.res dibuat oleh assets/make-icon.ps1 dan tidak ikut berubah saat versi di Cargo.toml
    // dinaikkan; peringatan di bawah mengingatkan untuk menjalankan skrip itu lagi.
    println!("cargo:rustc-link-arg-bins={}\\assets\\ponian.res", std::env::var("CARGO_MANIFEST_DIR").unwrap());
    println!("cargo:rerun-if-changed=assets/ponian.res");
    println!("cargo:rerun-if-changed=assets/ponian.rc");
    let rc = std::fs::read_to_string("assets/ponian.rc").unwrap_or_default();
    if !rc.contains(&format!("\"{}\"", env!("CARGO_PKG_VERSION"))) {
        println!("cargo:warning=version info in assets/ponian.res does not match Cargo.toml; run assets/make-icon.ps1");
    }
}
