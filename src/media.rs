//! Thread pekerja: membaca sesi media Windows, menjalankan kontrol, dan mengambil lirik.
//! Semua panggilan yang bisa memblokir ada di sini supaya thread window tidak pernah macet.
use crate::{
    WM_APP_MEDIA,
    lyrics::{self, Line},
    volume,
};
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        mpsc::{self, Sender},
    },
    thread,
    time::{Instant, SystemTime, UNIX_EPOCH},
};
use windows::{
    Foundation::TypedEventHandler,
    Graphics::Imaging::{BitmapAlphaMode, BitmapBounds, BitmapDecoder, BitmapInterpolationMode, BitmapPixelFormat, BitmapTransform, ColorManagementMode, ExifOrientationMode},
    Media::Control::{
        GlobalSystemMediaTransportControlsSession as Session, GlobalSystemMediaTransportControlsSessionManager as Manager,
        GlobalSystemMediaTransportControlsSessionMediaProperties as Props, GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
    },
    Win32::{
        Foundation::{HWND, LPARAM, WPARAM},
        System::Com::{COINIT_MULTITHREADED, CoInitializeEx},
        UI::WindowsAndMessaging::PostMessageW,
    },
    core::Result,
};

pub enum Msg {
    Refresh,
    /// properti media berubah: cover album perlu dibaca ulang
    Props,
    Prev,
    Toggle,
    Next,
    Pause,
    /// lompat ke posisi 0..1 dari durasi
    Seek(f32),
    Mute,
    /// pindah ke pemutar berikutnya (saat lebih dari satu app memutar media)
    Source,
    Net,
    Lyrics(String, Option<Arc<Vec<Line>>>),
    AudioDevice,
}

/// Sisi cover album dalam piksel setelah diperkecil.
pub const ART: u32 = 96;

/// Cover album: ART x ART piksel BGRA premultiplied.
pub struct Art {
    pub bgra: Vec<u8>,
    /// warna rata-rata cover, dipakai untuk pendar di pill lirik
    pub tint: u32,
}

#[derive(Clone, Default)]
pub struct Media {
    pub session: bool,
    pub title: String,
    pub artist: String,
    pub playing: bool,
    pos_ms: i64,
    at: Option<Instant>,
    pub dur_ms: i64,
    pub lyrics: Option<Arc<Vec<Line>>>,
    pub art: Option<Arc<Art>>,
    /// jumlah pemutar yang sedang terdaftar di Windows, dan nama pemutar yang ditampilkan
    pub sources: usize,
    pub source: String,
}

/// Nama pemutar dari AppUserModelId-nya: "Spotify.exe" -> "Spotify", "Paket_abc!Spotify" -> "Spotify".
fn source_name(id: &str) -> String {
    let name = id.rsplit('!').next().unwrap_or(id);
    let name = name.rsplit(['\\', '/']).next().unwrap_or(name);
    let name = if name.to_ascii_lowercase().ends_with(".exe") { &name[..name.len() - 4] } else { name };
    let mut chars = name.chars();
    chars.next().map_or(String::new(), |c| c.to_uppercase().chain(chars).collect())
}

impl Media {
    /// Posisi putar sekarang. Windows hanya melaporkan posisi sesekali, jadi sisanya dihitung dari jam.
    // shortcut: mengasumsikan kecepatan putar 1x; lirik akan meleset kalau pemutar dipercepat.
    pub fn pos(&self) -> i64 {
        let since = if self.playing { self.at.map_or(0, |t| t.elapsed().as_millis() as i64) } else { 0 };
        let pos = self.pos_ms + since;
        if self.dur_ms > 0 { pos.min(self.dur_ms) } else { pos }
    }
}

pub static MEDIA: Mutex<Option<Media>> = Mutex::new(None);

type Cache = HashMap<String, Option<Arc<Vec<Line>>>>;

pub fn spawn(hwnd: isize) -> Sender<Msg> {
    let (tx, rx) = mpsc::channel();
    let tx2 = tx.clone();
    thread::spawn(move || {
        let _ = run(hwnd, tx2, rx);
    });
    tx
}

macro_rules! ping {
    ($tx:expr, $msg:expr) => {{
        let tx = $tx.clone();
        TypedEventHandler::new(move |_, _| {
            let _ = tx.send($msg);
            Ok(())
        })
    }};
}

/// Perkecil cover ke ART x ART (dipotong tengah supaya thumbnail 16:9 tidak gepeng).
fn thumbnail(props: &Props) -> Result<Art> {
    let stream = props.Thumbnail()?.OpenReadAsync()?.join()?;
    let dec = BitmapDecoder::CreateAsync(&stream)?.join()?;
    let (w, h) = (dec.PixelWidth()?.max(1) as f32, dec.PixelHeight()?.max(1) as f32);
    let scale = ART as f32 / w.min(h);
    let (sw, sh) = (((w * scale).ceil() as u32).max(ART), ((h * scale).ceil() as u32).max(ART));
    let t = BitmapTransform::new()?;
    t.SetScaledWidth(sw)?;
    t.SetScaledHeight(sh)?;
    t.SetInterpolationMode(BitmapInterpolationMode::Fant)?;
    t.SetBounds(BitmapBounds { X: (sw - ART) / 2, Y: (sh - ART) / 2, Width: ART, Height: ART })?;
    let data = dec
        .GetPixelDataTransformedAsync(BitmapPixelFormat::Bgra8, BitmapAlphaMode::Premultiplied, &t, ExifOrientationMode::IgnoreExifOrientation, ColorManagementMode::DoNotColorManage)?
        .join()?
        .DetachPixelData()?;
    let bgra = data.to_vec();
    let n = (bgra.len() / 4).max(1) as u64;
    let sum = bgra.chunks_exact(4).fold([0u64; 3], |s, p| [s[0] + p[0] as u64, s[1] + p[1] as u64, s[2] + p[2] as u64]);
    let (b, g, r) = (sum[0] / n, sum[1] / n, sum[2] / n);
    // terangkan sampai kanal terbesar 220 supaya pendar tetap terlihat untuk cover gelap
    let k = 220.0 / r.max(g).max(b).max(1) as f32;
    let ch = |v: u64| ((v as f32 * k).min(255.0)) as u32;
    Ok(Art { bgra, tint: ch(r) << 16 | ch(g) << 8 | ch(b) })
}

fn run(hwnd: isize, tx: Sender<Msg>, rx: mpsc::Receiver<Msg>) -> Result<()> {
    unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).ok()? };
    let mut volume = volume::Watch::new(hwnd, &tx).ok();
    let manager = Manager::RequestAsync()?.join()?;
    manager.CurrentSessionChanged(&ping!(tx, Msg::Props))?;
    manager.SessionsChanged(&ping!(tx, Msg::Props))?;
    let net_tx = tx.clone();
    let _ = windows::Networking::Connectivity::NetworkInformation::NetworkStatusChanged(&windows::Networking::Connectivity::NetworkStatusChangedEventHandler::new(move |_| {
        let _ = net_tx.send(Msg::Net);
        Ok(())
    }));
    let _ = tx.send(Msg::Net);
    let mut session: Option<Session> = None;
    // pemutar pilihan user; None = ikut sesi yang dianggap aktif oleh Windows
    let mut chosen: Option<windows::core::HSTRING> = None;
    let id = |s: &Option<Session>| s.as_ref().and_then(|s| s.SourceAppUserModelId().ok());
    let mut cache = Cache::new();
    // (judul + artis, cover): cover hanya dibaca ulang saat properti media berubah
    let mut art: (String, Option<Arc<Art>>) = Default::default();
    let _ = tx.send(Msg::Props);

    for msg in rx {
        let mut reload_art = false;
        match msg {
            Msg::Refresh => {}
            Msg::Props => reload_art = true,
            Msg::Prev => drop(session.as_ref().map(|s| s.TrySkipPreviousAsync())),
            Msg::Toggle => drop(session.as_ref().map(|s| s.TryTogglePlayPauseAsync())),
            Msg::Next => drop(session.as_ref().map(|s| s.TrySkipNextAsync())),
            Msg::Pause => drop(session.as_ref().map(|s| s.TryPauseAsync())),
            Msg::Seek(frac) => {
                let _ = session.as_ref().map(|s| -> Result<()> {
                    let tl = s.GetTimelineProperties()?;
                    let (a, b) = (tl.StartTime()?.Duration, tl.EndTime()?.Duration);
                    s.TryChangePlaybackPositionAsync(a + ((b - a) as f64 * frac.clamp(0.0, 1.0) as f64) as i64)?;
                    Ok(())
                });
            }
            Msg::Source => {
                let ids: Vec<_> = manager.GetSessions().into_iter().flatten().filter_map(|s| s.SourceAppUserModelId().ok()).collect();
                let next = ids.iter().position(|i| Some(i) == id(&session).as_ref()).map_or(0, |i| i + 1);
                chosen = ids.get(next % ids.len().max(1)).cloned();
            }
            Msg::Mute => {
                if let Some(v) = &volume {
                    v.toggle_mute();
                }
                continue;
            }
            Msg::Lyrics(key, lines) => drop(cache.insert(key, lines)),
            Msg::AudioDevice => {
                if let Some(v) = &mut volume {
                    v.rebind(hwnd);
                    *crate::system::AUDIO_DEVICE.lock().unwrap() = v.device_name();
                    let _ = unsafe { PostMessageW(Some(HWND(hwnd as _)), crate::WM_APP_AUDIODEV, WPARAM(0), LPARAM(0)) };
                }
                continue;
            }
            Msg::Net => {
                *crate::system::NETWORK.lock().unwrap() = Some(crate::system::network());
                let _ = unsafe { PostMessageW(Some(HWND(hwnd as _)), crate::WM_APP_NET, WPARAM(0), LPARAM(0)) };
                continue;
            }
        }
        let all: Vec<Session> = manager.GetSessions().into_iter().flatten().collect();
        // pilihan user dipakai selama pemutarnya masih ada
        let picked = all.iter().find(|s| chosen.is_some() && s.SourceAppUserModelId().ok() == chosen).cloned();
        let current = picked.or_else(|| manager.GetCurrentSession().ok());
        if id(&current) != id(&session) {
            session = current;
            if let Some(s) = &session {
                let _ = s.MediaPropertiesChanged(&ping!(tx, Msg::Props));
                let _ = s.PlaybackInfoChanged(&ping!(tx, Msg::Refresh));
                let _ = s.TimelinePropertiesChanged(&ping!(tx, Msg::Refresh));
            }
        }
        let mut media = session.as_ref().and_then(|s| read(s, &mut cache, &mut art, reload_art, &tx).ok()).unwrap_or_default();
        (media.sources, media.source) = (all.len(), id(&session).map_or(String::new(), |i| source_name(&i.to_string())));
        *MEDIA.lock().unwrap() = Some(media);
        let _ = unsafe { PostMessageW(Some(HWND(hwnd as _)), WM_APP_MEDIA, WPARAM(0), LPARAM(0)) };
    }
    Ok(())
}

fn read(s: &Session, cache: &mut Cache, art: &mut (String, Option<Arc<Art>>), reload_art: bool, tx: &Sender<Msg>) -> Result<Media> {
    let props = s.TryGetMediaPropertiesAsync()?.join()?;
    let (title, artist) = (props.Title()?.to_string(), props.Artist()?.to_string());
    let track = format!("{title}\n{artist}");
    if reload_art || art.0 != track {
        *art = (track, thumbnail(&props).ok().map(Arc::new));
    }
    let playing = s.GetPlaybackInfo()?.PlaybackStatus()? == Status::Playing;
    let tl = s.GetTimelineProperties()?;
    let dur_ms = (tl.EndTime()?.Duration - tl.StartTime()?.Duration) / 10_000;
    let mut pos_ms = tl.Position()?.Duration / 10_000;
    if playing {
        // LastUpdatedTime memakai epoch 1601, SystemTime memakai 1970
        let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64) + 11_644_473_600_000;
        let age = now - tl.LastUpdatedTime()?.UniversalTime / 10_000;
        if (0..dur_ms).contains(&age) {
            pos_ms += age;
        }
    }

    // tanpa durasi berarti pemutar tidak melaporkan timeline, jadi lirik tidak bisa disinkronkan
    let dur_s = dur_ms / 1000;
    let lyrics = if dur_s > 0 && !title.is_empty() && crate::settings::flag("ShowLyrics", true) {
        let key = format!("{title}\n{artist}\n{dur_s}");
        match cache.get(&key) {
            Some(found) => found.clone(),
            None => {
                if cache.len() >= 20 {
                    cache.clear();
                }
                cache.insert(key.clone(), None);
                let (tx, title, artist) = (tx.clone(), title.clone(), artist.clone());
                thread::spawn(move || {
                    let lines = lyrics::fetch(&title, &artist, dur_s).map(Arc::new);
                    let _ = tx.send(Msg::Lyrics(key, lines));
                });
                None
            }
        }
    } else {
        None
    };
    Ok(Media { session: true, title, artist, playing, pos_ms, at: Some(Instant::now()), dur_ms, lyrics, art: art.1.clone(), ..Default::default() })
}

#[cfg(test)]
mod tests {
    #[test]
    fn source_names_are_readable() {
        assert_eq!(super::source_name("Spotify.exe"), "Spotify");
        assert_eq!(super::source_name("SpotifyAB.SpotifyMusic_zpdnekdrzrea0!Spotify"), "Spotify");
        assert_eq!(super::source_name("chrome.exe"), "Chrome");
        assert_eq!(super::source_name(""), "");
    }
}
