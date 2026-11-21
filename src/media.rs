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
    Media::Control::{
        GlobalSystemMediaTransportControlsSession as Session, GlobalSystemMediaTransportControlsSessionManager as Manager,
        GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
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
    Prev,
    Toggle,
    Next,
    Pause,
    Net,
    Lyrics(String, Option<Arc<Vec<Line>>>),
    AudioDevice,
}

#[derive(Clone, Default)]
pub struct Media {
    pub session: bool,
    pub title: String,
    pub artist: String,
    pub playing: bool,
    pos_ms: i64,
    at: Option<Instant>,
    pub lyrics: Option<Arc<Vec<Line>>>,
}

impl Media {
    /// Posisi putar sekarang. Windows hanya melaporkan posisi sesekali, jadi sisanya dihitung dari jam.
    // shortcut: mengasumsikan kecepatan putar 1x; lirik akan meleset kalau pemutar dipercepat.
    pub fn pos(&self) -> i64 {
        let since = if self.playing { self.at.map_or(0, |t| t.elapsed().as_millis() as i64) } else { 0 };
        self.pos_ms + since
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
    ($tx:expr) => {{
        let tx = $tx.clone();
        TypedEventHandler::new(move |_, _| {
            let _ = tx.send(Msg::Refresh);
            Ok(())
        })
    }};
}

fn run(hwnd: isize, tx: Sender<Msg>, rx: mpsc::Receiver<Msg>) -> Result<()> {
    unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).ok()? };
    let mut volume = volume::Watch::new(hwnd, &tx).ok();
    let manager = Manager::RequestAsync()?.join()?;
    manager.CurrentSessionChanged(&ping!(tx))?;
    let net_tx = tx.clone();
    let _ = windows::Networking::Connectivity::NetworkInformation::NetworkStatusChanged(&windows::Networking::Connectivity::NetworkStatusChangedEventHandler::new(move |_| {
        let _ = net_tx.send(Msg::Net);
        Ok(())
    }));
    let _ = tx.send(Msg::Net);
    let mut session: Option<Session> = None;
    let mut cache = Cache::new();
    let _ = tx.send(Msg::Refresh);

    for msg in rx {
        match msg {
            Msg::Refresh => {}
            Msg::Prev => drop(session.as_ref().map(|s| s.TrySkipPreviousAsync())),
            Msg::Toggle => drop(session.as_ref().map(|s| s.TryTogglePlayPauseAsync())),
            Msg::Next => drop(session.as_ref().map(|s| s.TrySkipNextAsync())),
            Msg::Pause => drop(session.as_ref().map(|s| s.TryPauseAsync())),
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
        let current = manager.GetCurrentSession().ok();
        let id = |s: &Option<Session>| s.as_ref().and_then(|s| s.SourceAppUserModelId().ok());
        if id(&current) != id(&session) {
            session = current;
            if let Some(s) = &session {
                let _ = s.MediaPropertiesChanged(&ping!(tx));
                let _ = s.PlaybackInfoChanged(&ping!(tx));
                let _ = s.TimelinePropertiesChanged(&ping!(tx));
            }
        }
        let media = session.as_ref().and_then(|s| read(s, &mut cache, &tx).ok()).unwrap_or_default();
        *MEDIA.lock().unwrap() = Some(media);
        let _ = unsafe { PostMessageW(Some(HWND(hwnd as _)), WM_APP_MEDIA, WPARAM(0), LPARAM(0)) };
    }
    Ok(())
}

fn read(s: &Session, cache: &mut Cache, tx: &Sender<Msg>) -> Result<Media> {
    let props = s.TryGetMediaPropertiesAsync()?.join()?;
    let (title, artist) = (props.Title()?.to_string(), props.Artist()?.to_string());
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
    Ok(Media { session: true, title, artist, playing, pos_ms, at: Some(Instant::now()), lyrics })
}
