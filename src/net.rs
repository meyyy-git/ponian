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

/// Kirim permintaan dengan header tambahan. Hasil: (status HTTP, isi respons), atau None kalau tidak tersambung.
pub fn send(method: &str, url: &str, body: Option<&str>, headers: &[(&str, &str)]) -> Option<(i32, String)> {
    use windows::{
        Storage::Streams::UnicodeEncoding,
        Web::Http::{HttpMethod, HttpRequestMessage, HttpStringContent},
    };
    let client = HttpClient::new().ok()?;
    let method = if method == "POST" { HttpMethod::Post() } else { HttpMethod::Get() }.ok()?;
    let req = HttpRequestMessage::Create(&method, &Uri::CreateUri(&HSTRING::from(url)).ok()?).ok()?;
    let h = req.Headers().ok()?;
    h.UserAgent().ok()?.TryParseAdd(&HSTRING::from(USER_AGENT)).ok()?;
    for (k, v) in headers {
        h.TryAppendWithoutValidation(&HSTRING::from(*k), &HSTRING::from(*v)).ok()?;
    }
    if let Some(body) = body {
        let content = HttpStringContent::CreateFromStringWithEncodingAndMediaType(&HSTRING::from(body), UnicodeEncoding::Utf8, &HSTRING::from("application/json")).ok()?;
        req.SetContent(&content).ok()?;
    }
    let resp = client.SendRequestAsync(&req).ok()?.join().ok()?;
    let status = resp.StatusCode().ok()?.0;
    let text = resp.Content().ok()?.ReadAsStringAsync().ok()?.join().ok()?.to_string();
    Some((status, text))
}

pub fn escape(s: &str) -> Option<String> {
    Some(Uri::EscapeComponent(&HSTRING::from(s)).ok()?.to_string())
}
