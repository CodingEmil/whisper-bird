//! Datenschutz-Freigaben unter macOS: Bedienungshilfen (Shortcuts, Einfügen,
//! Markierung lesen) und Mikrofon.

use objc2::rc::autoreleasepool;
use objc2::runtime::{AnyObject, Bool};
use objc2_av_foundation::{AVCaptureDevice, AVMediaTypeAudio};
use objc2_foundation::{NSDictionary, NSNumber, NSString};
use std::ffi::c_void;

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrusted() -> bool;
    fn AXIsProcessTrustedWithOptions(options: *const c_void) -> bool;
}

pub fn accessibility_granted() -> bool {
    unsafe { AXIsProcessTrusted() }
}

/// Zeigt die Systemabfrage (nur beim ersten Mal) und öffnet die Einstellungen.
pub fn request_accessibility() {
    autoreleasepool(|_| {
        let key = NSString::from_str("AXTrustedCheckOptionPrompt");
        let value = NSNumber::new_bool(true);
        let options: objc2::rc::Retained<NSDictionary<NSString, AnyObject>> =
            NSDictionary::from_slices(&[&*key], &[value.as_ref() as &AnyObject]);
        unsafe { AXIsProcessTrustedWithOptions(objc2::rc::Retained::as_ptr(&options) as *const c_void) };
    });
    if !accessibility_granted() {
        open_settings("accessibility");
    }
}

/// Fragt nach dem Mikrofon. Die Antwort kommt asynchron; das Frontend fragt den Status ab.
pub fn request_microphone() {
    autoreleasepool(|_| {
        let Some(audio) = (unsafe { AVMediaTypeAudio }) else { return };
        let done = block2::RcBlock::new(|granted: Bool| {
            log::info!("Mikrofon-Freigabe: {}", granted.as_bool());
        });
        unsafe { AVCaptureDevice::requestAccessForMediaType_completionHandler(audio, &done) };
    });
}

/// Öffnet die passende Seite unter Systemeinstellungen → Datenschutz & Sicherheit.
pub fn open_settings(kind: &str) {
    let pane = match kind {
        "accessibility" => "Privacy_Accessibility",
        "microphone" => "Privacy_Microphone",
        "system_audio" => "Privacy_AudioCapture",
        _ => "Privacy",
    };
    let url = format!("x-apple.systempreferences:com.apple.preference.security?{pane}");
    if let Err(e) = std::process::Command::new("open").arg(url).spawn() {
        log::warn!("Systemeinstellungen ließen sich nicht öffnen: {e}");
    }
}
