//! Töne (NSSound) und Systemlautstärke (Core Audio) unter macOS.

use objc2::rc::{autoreleasepool, Retained};
use objc2::AnyThread;
use objc2_app_kit::NSSound;
use objc2_foundation::NSData;
use std::cell::RefCell;
use std::ffi::c_void;

thread_local! {
    /// Laufende Töne; NSSound verstummt, sobald das Objekt freigegeben wird.
    static PLAYING: RefCell<Vec<Retained<NSSound>>> = const { RefCell::new(Vec::new()) };
}

/// Spielt eine WAV-Datei aus dem Speicher ab, ohne zu warten.
pub fn play_wav(data: &'static [u8]) {
    let _ = crate::state::state().app.run_on_main_thread(move || {
        autoreleasepool(|_| {
            let bytes = NSData::with_bytes(data);
            let Some(sound) = NSSound::initWithData(NSSound::alloc(), &bytes) else {
                log::debug!("Ton konnte nicht geladen werden");
                return;
            };
            if sound.play() {
                PLAYING.with(|p| {
                    let mut p = p.borrow_mut();
                    p.retain(|s| s.isPlaying());
                    p.push(sound);
                });
            }
        })
    });
}

// ---------- Lautstärke ----------

#[repr(C)]
struct PropertyAddress {
    selector: u32,
    scope: u32,
    element: u32,
}

const fn fourcc(code: &[u8; 4]) -> u32 {
    u32::from_be_bytes(*code)
}

const SYSTEM_OBJECT: u32 = 1;
const DEFAULT_OUTPUT_DEVICE: u32 = fourcc(b"dOut");
const SCOPE_GLOBAL: u32 = fourcc(b"glob");
const SCOPE_OUTPUT: u32 = fourcc(b"outp");
const ELEMENT_MAIN: u32 = 0;
/// kAudioHardwareServiceDeviceProperty_VirtualMainVolume: die Lautstärke, die
/// auch der Regler in der Menüleiste zeigt.
const VIRTUAL_MAIN_VOLUME: u32 = fourcc(b"vmvc");

#[link(name = "CoreAudio", kind = "framework")]
extern "C" {
    fn AudioObjectGetPropertyData(
        id: u32,
        address: *const PropertyAddress,
        qualifier_size: u32,
        qualifier: *const c_void,
        size: *mut u32,
        data: *mut c_void,
    ) -> i32;
}

#[link(name = "AudioToolbox", kind = "framework")]
extern "C" {
    fn AudioHardwareServiceGetPropertyData(
        id: u32,
        address: *const PropertyAddress,
        qualifier_size: u32,
        qualifier: *const c_void,
        size: *mut u32,
        data: *mut c_void,
    ) -> i32;
    fn AudioHardwareServiceSetPropertyData(
        id: u32,
        address: *const PropertyAddress,
        qualifier_size: u32,
        qualifier: *const c_void,
        size: u32,
        data: *const c_void,
    ) -> i32;
}

fn output_device() -> anyhow::Result<u32> {
    let address = PropertyAddress {
        selector: DEFAULT_OUTPUT_DEVICE,
        scope: SCOPE_GLOBAL,
        element: ELEMENT_MAIN,
    };
    let mut device: u32 = 0;
    let mut size = std::mem::size_of::<u32>() as u32;
    let status = unsafe {
        AudioObjectGetPropertyData(SYSTEM_OBJECT, &address, 0, std::ptr::null(), &mut size, &mut device as *mut u32 as *mut c_void)
    };
    anyhow::ensure!(status == 0 && device != 0, "kein Ausgabegerät (Status {status})");
    Ok(device)
}

const VOLUME: PropertyAddress = PropertyAddress {
    selector: VIRTUAL_MAIN_VOLUME,
    scope: SCOPE_OUTPUT,
    element: ELEMENT_MAIN,
};

pub fn get_volume() -> anyhow::Result<f32> {
    let device = output_device()?;
    let mut volume: f32 = 0.0;
    let mut size = std::mem::size_of::<f32>() as u32;
    let status = unsafe {
        AudioHardwareServiceGetPropertyData(device, &VOLUME, 0, std::ptr::null(), &mut size, &mut volume as *mut f32 as *mut c_void)
    };
    anyhow::ensure!(status == 0, "Lautstärke nicht lesbar (Status {status})");
    Ok(volume)
}

pub fn set_volume(volume: f32) -> anyhow::Result<()> {
    let device = output_device()?;
    let volume = volume.clamp(0.0, 1.0);
    let status = unsafe {
        AudioHardwareServiceSetPropertyData(
            device,
            &VOLUME,
            0,
            std::ptr::null(),
            std::mem::size_of::<f32>() as u32,
            &volume as *const f32 as *const c_void,
        )
    };
    anyhow::ensure!(status == 0, "Lautstärke nicht setzbar (Status {status})");
    Ok(())
}
