//! Notifikasi Windows di notch. Dibaca dengan UserNotificationListener tiap beberapa detik di thread
//! sendiri, karena event perubahan notifikasi hanya tersedia untuk app ber-package.
use crate::{WM_APP_NOTIFY, settings};
use std::{collections::HashSet, sync::Mutex, thread, time::Duration};
use windows::{
    UI::Notifications::{KnownNotificationBindings, Management::UserNotificationListener, NotificationKinds, UserNotification},
    Win32::{
        Foundation::{HWND, LPARAM, WPARAM},
        System::Com::{COINIT_MULTITHREADED, CoInitializeEx},
        UI::WindowsAndMessaging::PostMessageW,
    },
};

#[derive(Clone)]
pub struct Notice {
    pub app: String,
    pub title: String,
    pub body: String,
    /// AppUserModelId app pengirim, dipakai untuk membuka app itu saat notifikasinya diklik
    pub aumid: String,
}

/// Notifikasi baru yang belum diambil thread UI.
pub static QUEUE: Mutex<Vec<Notice>> = Mutex::new(Vec::new());

fn read(n: &UserNotification) -> windows::core::Result<Notice> {
    let info = n.AppInfo()?;
    let app = info.DisplayInfo()?.DisplayName()?.to_string();
    let aumid = info.AppUserModelId().map(|s| s.to_string()).unwrap_or_default();
    let texts = n.Notification()?.Visual()?.GetBinding(&KnownNotificationBindings::ToastGeneric()?)?.GetTextElements()?;
    let mut lines = (0..texts.Size()?).filter_map(|i| texts.GetAt(i).ok()?.Text().ok().map(|t| t.to_string()));
    Ok(Notice { app, aumid, title: lines.next().unwrap_or_default(), body: lines.collect::<Vec<_>>().join(" ") })
}

pub fn spawn(hwnd: isize) {
    thread::spawn(move || {
        if unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_err() {
            return;
        }
        let Ok(listener) = UserNotificationListener::Current() else { return };
        let mut seen: Option<HashSet<u32>> = None;
        loop {
            thread::sleep(Duration::from_secs(3));
            if !settings::flag("Notify.On", true) {
                seen = None;
                continue;
            }
            let Ok(list) = listener.GetNotificationsAsync(NotificationKinds::Toast).and_then(|op| op.join()) else { continue };
            let current: Vec<UserNotification> = list.into_iter().collect();
            let ids: HashSet<u32> = current.iter().filter_map(|n| n.Id().ok()).collect();
            // putaran pertama hanya mencatat notifikasi lama supaya tidak semuanya muncul sekaligus
            if let Some(old) = &seen {
                let mute = settings::string("Notify.Mute");
                let fresh: Vec<Notice> = current.iter().filter(|n| n.Id().is_ok_and(|id| !old.contains(&id))).filter_map(|n| read(n).ok()).filter(|n| !muted(&n.app, &mute)).collect();
                if !fresh.is_empty() {
                    QUEUE.lock().unwrap().extend(fresh);
                    let _ = unsafe { PostMessageW(Some(HWND(hwnd as _)), WM_APP_NOTIFY, WPARAM(0), LPARAM(0)) };
                }
            }
            seen = Some(ids);
        }
    });
}

/// `list` berisi nama app yang dibisukan, dipisah koma; huruf besar-kecil tidak dibedakan.
fn muted(app: &str, list: &str) -> bool {
    list.split(',').map(str::trim).any(|m| !m.is_empty() && m.eq_ignore_ascii_case(app.trim()))
}

#[cfg(test)]
mod tests {
    #[test]
    fn mute_list_matches_whole_names() {
        assert!(super::muted("WhatsApp", "slack, whatsapp"));
        assert!(!super::muted("WhatsApp", ""));
        assert!(!super::muted("WhatsApp Beta", "whatsapp,"));
    }
}
