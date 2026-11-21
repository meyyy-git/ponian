//! HTTP GET sederhana lewat WinHTTP. Dipilih karena jauh lebih ringan dari Windows.Web.Http, yang
//! ikut memuat WinINet dan puluhan DLL lain. Semua fungsi memblokir; panggil dari thread pekerja.
use std::ffi::c_void;
use windows::{
    Win32::Networking::WinHttp::*,
    core::{HSTRING, PCWSTR, w},
};

/// Identitas app untuk layanan luar. MET Norway mewajibkan kontak di sini.
// shortcut: kontak masih placeholder; ganti dengan URL repo atau email resmi sebelum rilis publik.
pub const USER_AGENT: &str = "Ponian/0.1 (+https://github.com/ponian-app)";

/// Respons dibatasi supaya server yang salah tidak menghabiskan memori.
const MAX_BODY: usize = 4 * 1024 * 1024;

/// Handle WinHTTP yang ditutup otomatis.
struct Handle(*mut c_void);

impl Drop for Handle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            let _ = unsafe { WinHttpCloseHandle(self.0) };
        }
    }
}

/// "https://host:port/path?query" -> (https?, host, port, path dengan query)
fn split(url: &str) -> Option<(bool, &str, u16, String)> {
    let (secure, rest) = match url.split_once("://")? {
        ("https", rest) => (true, rest),
        ("http", rest) => (false, rest),
        _ => return None,
    };
    let at = rest.find(['/', '?']).unwrap_or(rest.len());
    let (authority, path) = rest.split_at(at);
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) => (host, port.parse().ok()?),
        None => (authority, if secure { 443 } else { 80 }),
    };
    // path harus diawali "/" walau URL-nya langsung berisi query
    let path = if path.starts_with('/') { path.to_string() } else { format!("/{path}") };
    (!host.is_empty()).then_some((secure, host, port, path))
}

/// Isi respons kalau statusnya 200; selain itu None.
pub fn get(url: &str) -> Option<String> {
    let (secure, host, port, path) = split(url)?;
    unsafe {
        let session = Handle(WinHttpOpen(&HSTRING::from(USER_AGENT), WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, PCWSTR::null(), PCWSTR::null(), 0));
        if session.0.is_null() {
            return None;
        }
        let _ = WinHttpSetTimeouts(session.0, 5000, 5000, 8000, 8000);
        let connect = Handle(WinHttpConnect(session.0, &HSTRING::from(host), port, 0));
        if connect.0.is_null() {
            return None;
        }
        let flags = if secure { WINHTTP_FLAG_SECURE } else { WINHTTP_OPEN_REQUEST_FLAGS(0) };
        let request = Handle(WinHttpOpenRequest(connect.0, w!("GET"), &HSTRING::from(path.as_str()), PCWSTR::null(), PCWSTR::null(), std::ptr::null(), flags));
        if request.0.is_null() {
            return None;
        }
        WinHttpSendRequest(request.0, None, None, 0, 0, 0).ok()?;
        WinHttpReceiveResponse(request.0, std::ptr::null_mut()).ok()?;
        let (mut status, mut len) = (0u32, 4u32);
        WinHttpQueryHeaders(request.0, WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER, PCWSTR::null(), Some(&mut status as *mut _ as _), &mut len, std::ptr::null_mut()).ok()?;
        if status != 200 {
            return None;
        }
        let mut body = Vec::new();
        let mut chunk = [0u8; 16 * 1024];
        loop {
            let mut read = 0u32;
            WinHttpReadData(request.0, chunk.as_mut_ptr() as _, chunk.len() as u32, &mut read).ok()?;
            if read == 0 {
                break;
            }
            body.extend_from_slice(&chunk[..read as usize]);
            if body.len() > MAX_BODY {
                return None;
            }
        }
        String::from_utf8(body).ok()
    }
}

/// Percent-encoding untuk nilai parameter URL.
pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_urls() {
        assert_eq!(split("https://lrclib.net/api/get?a=1"), Some((true, "lrclib.net", 443, "/api/get?a=1".into())));
        assert_eq!(split("http://localhost:8080"), Some((false, "localhost", 8080, "/".into())));
        assert_eq!(split("https://x.test?q=1"), Some((true, "x.test", 443, "/?q=1".into())));
        assert_eq!(split("ftp://x.test/"), None);
        assert_eq!(split("https:///nohost"), None);
    }

    #[test]
    fn escapes_query_values() {
        assert_eq!(escape("Lampu Merah & Co."), "Lampu%20Merah%20%26%20Co.");
        assert_eq!(escape("caf\u{e9}"), "caf%C3%A9");
    }
}
