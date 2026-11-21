//! HTTP sederhana lewat Windows.Web.Http (tanpa crate tambahan). Semua fungsi memblokir; panggil dari thread pekerja.
use windows::{Foundation::Uri, Web::Http::HttpClient, core::HSTRING};

/// Identitas app untuk layanan luar. MET Norway mewajibkan kontak di sini.
// shortcut: kontak masih placeholder; ganti dengan URL repo atau email resmi sebelum rilis publik.
pub const USER_AGENT: &str = "Ponian/0.1 (+https://github.com/ponian-app)";

pub fn get(url: &str) -> Option<String> {
    let client = HttpClient::new().ok()?;
    client.DefaultRequestHeaders().ok()?.UserAgent().ok()?.TryParseAdd(&HSTRING::from(USER_AGENT)).ok()?;
    let text = client.GetStringAsync(&Uri::CreateUri(&HSTRING::from(url)).ok()?).ok()?.join().ok()?;
    Some(text.to_string())
}

pub fn escape(s: &str) -> Option<String> {
    Some(Uri::EscapeComponent(&HSTRING::from(s)).ok()?.to_string())
}
