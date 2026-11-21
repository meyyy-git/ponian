//! Memantau volume perangkat output default.
use crate::{WM_APP_VOLUME, media::Msg};
use std::sync::{
    Mutex,
    atomic::{AtomicU32, Ordering::Relaxed},
    mpsc::Sender,
};
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, PROPERTYKEY, WPARAM},
        Media::Audio::{Endpoints::*, *},
        System::Com::{CLSCTX_ALL, CoCreateInstance},
        UI::WindowsAndMessaging::PostMessageW,
    },
    core::*,
};

pub const MUTE: u32 = 1 << 31;
/// persen volume, ditambah bit MUTE
pub static VOLUME: AtomicU32 = AtomicU32::new(0);

#[implement(IAudioEndpointVolumeCallback)]
struct OnVolume(isize);

impl IAudioEndpointVolumeCallback_Impl for OnVolume_Impl {
    fn OnNotify(&self, data: *mut AUDIO_VOLUME_NOTIFICATION_DATA) -> Result<()> {
        let Some(d) = (unsafe { data.as_ref() }) else { return Ok(()) };
        let v = (d.fMasterVolume * 100.0).round() as u32 | if d.bMuted.as_bool() { MUTE } else { 0 };
        if VOLUME.swap(v, Relaxed) != v {
            let _ = unsafe { PostMessageW(Some(HWND(self.0 as _)), WM_APP_VOLUME, WPARAM(0), LPARAM(0)) };
        }
        Ok(())
    }
}

#[implement(IMMNotificationClient)]
struct OnDevice(Mutex<Sender<Msg>>);

impl IMMNotificationClient_Impl for OnDevice_Impl {
    fn OnDefaultDeviceChanged(&self, flow: EDataFlow, role: ERole, _: &PCWSTR) -> Result<()> {
        if flow == eRender && role == eMultimedia {
            let _ = self.0.lock().unwrap().send(Msg::AudioDevice);
        }
        Ok(())
    }
    fn OnDeviceStateChanged(&self, _: &PCWSTR, _: DEVICE_STATE) -> Result<()> {
        Ok(())
    }
    fn OnDeviceAdded(&self, _: &PCWSTR) -> Result<()> {
        Ok(())
    }
    fn OnDeviceRemoved(&self, _: &PCWSTR) -> Result<()> {
        Ok(())
    }
    fn OnPropertyValueChanged(&self, _: &PCWSTR, _: &PROPERTYKEY) -> Result<()> {
        Ok(())
    }
}

/// Pendaftaran callback berlaku selama nilai ini hidup.
pub struct Watch {
    devices: IMMDeviceEnumerator,
    on_device: IMMNotificationClient,
    endpoint: Option<(IAudioEndpointVolume, IAudioEndpointVolumeCallback)>,
}

impl Watch {
    pub fn new(hwnd: isize, tx: &Sender<Msg>) -> Result<Self> {
        unsafe {
            let devices: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
            let on_device: IMMNotificationClient = OnDevice(Mutex::new(tx.clone())).into();
            devices.RegisterEndpointNotificationCallback(&on_device)?;
            let mut w = Self { devices, on_device, endpoint: None };
            w.rebind(hwnd);
            Ok(w)
        }
    }

    /// Pindah ke perangkat output default yang sekarang (misalnya setelah headphone dicolok).
    pub fn rebind(&mut self, hwnd: isize) {
        self.unbind();
        self.endpoint = unsafe {
            (|| -> Result<_> {
                let vol: IAudioEndpointVolume = self.devices.GetDefaultAudioEndpoint(eRender, eMultimedia)?.Activate(CLSCTX_ALL, None)?;
                let cb: IAudioEndpointVolumeCallback = OnVolume(hwnd).into();
                vol.RegisterControlChangeNotify(&cb)?;
                let v = (vol.GetMasterVolumeLevelScalar()? * 100.0).round() as u32;
                VOLUME.store(v | if vol.GetMute()?.as_bool() { MUTE } else { 0 }, Relaxed);
                Ok((vol, cb))
            })()
            .ok()
        };
    }

    fn unbind(&mut self) {
        if let Some((vol, cb)) = self.endpoint.take() {
            let _ = unsafe { vol.UnregisterControlChangeNotify(&cb) };
        }
    }
}

impl Drop for Watch {
    fn drop(&mut self) {
        self.unbind();
        let _ = unsafe { self.devices.UnregisterEndpointNotificationCallback(&self.on_device) };
    }
}
