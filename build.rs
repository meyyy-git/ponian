fn main() {
    // Common Controls v6: tampilan kontrol modern dan placeholder (cue banner) di kotak teks
    println!("cargo:rustc-link-arg-bins=/MANIFEST:EMBED");
    println!(
        "cargo:rustc-link-arg-bins=/MANIFESTDEPENDENCY:type='win32' name='Microsoft.Windows.Common-Controls' version='6.0.0.0' processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'"
    );
    // ikon aplikasi (resource nomor 1), sudah dikompilasi supaya build tidak butuh rc.exe.
    // shortcut: setelah mengganti assets/ponian.ico, buat ulang dengan `rc /fo ponian.res ponian.rc` di folder assets.
    println!("cargo:rustc-link-arg-bins={}\\assets\\ponian.res", std::env::var("CARGO_MANIFEST_DIR").unwrap());
    println!("cargo:rerun-if-changed=assets/ponian.res");
}
