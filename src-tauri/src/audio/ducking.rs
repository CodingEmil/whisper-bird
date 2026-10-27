//! Audio-Ducking: Systemlautstärke während der Aufnahme absenken und danach
//! wiederherstellen.

use std::sync::Mutex;

static SAVED: Mutex<Option<f32>> = Mutex::new(None);

/// Senkt die Lautstärke auf `level` (0..1) des aktuellen Werts.
pub fn duck(level: f32) {
    let mut saved = SAVED.lock().unwrap();
    if saved.is_some() {
        return;
    }
    match imp::get() {
        Ok(current) => {
            *saved = Some(current);
            if let Err(e) = imp::set(current * level.clamp(0.0, 1.0)) {
                log::warn!("Ducking fehlgeschlagen: {e:#}");
            }
        }
        Err(e) => log::warn!("Lautstärke nicht lesbar: {e:#}"),
    }
}

pub fn restore() {
    if let Some(v) = SAVED.lock().unwrap().take() {
        if let Err(e) = imp::set(v) {
            log::warn!("Lautstärke nicht wiederhergestellt: {e:#}");
        }
    }
}

#[cfg(windows)]
mod imp {
    use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
    use windows::Win32::Media::Audio::{eConsole, eRender, IMMDeviceEnumerator, MMDeviceEnumerator};
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED,
    };

    fn endpoint() -> anyhow::Result<IAudioEndpointVolume> {
        unsafe {
            // S_FALSE/RPC_E_CHANGED_MODE sind hier unkritisch.
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let enumerator: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
            let device = enumerator.GetDefaultAudioEndpoint(eRender, eConsole)?;
            Ok(device.Activate(CLSCTX_ALL, None)?)
        }
    }

    pub fn get() -> anyhow::Result<f32> {
        unsafe { Ok(endpoint()?.GetMasterVolumeLevelScalar()?) }
    }

    pub fn set(v: f32) -> anyhow::Result<()> {
        unsafe { Ok(endpoint()?.SetMasterVolumeLevelScalar(v, std::ptr::null())?) }
    }
}

#[cfg(not(windows))]
mod imp {
    pub fn get() -> anyhow::Result<f32> {
        anyhow::bail!("nicht unterstützt")
    }
    pub fn set(_: f32) -> anyhow::Result<()> {
        anyhow::bail!("nicht unterstützt")
    }
}
