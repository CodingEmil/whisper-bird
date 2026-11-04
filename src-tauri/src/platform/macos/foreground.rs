//! Aktive App über NSWorkspace, Fenstertitel über die Bedienungshilfen-API.

use super::CfOwned;
use crate::platform::foreground::AppContext;
use objc2::rc::{autoreleasepool, Retained};
use objc2_app_kit::{NSApplicationActivationOptions, NSRunningApplication, NSWorkspace};
use objc2_foundation::NSString;
use std::ffi::c_void;

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXUIElementCreateApplication(pid: i32) -> *const c_void;
    fn AXUIElementCopyAttributeValue(element: *const c_void, attribute: *const c_void, value: *mut *const c_void) -> i32;
    fn AXUIElementSetMessagingTimeout(element: *const c_void, seconds: f32) -> i32;
}

/// Wert eines Attributs als neue Referenz (oder `None`).
fn ax_attribute(element: *const c_void, name: &str) -> Option<CfOwned> {
    let attr: Retained<NSString> = NSString::from_str(name);
    let mut value: *const c_void = std::ptr::null();
    let err = unsafe { AXUIElementCopyAttributeValue(element, Retained::as_ptr(&attr) as *const c_void, &mut value) };
    if err != 0 {
        return None;
    }
    CfOwned::new(value)
}

/// Titel des Fensters mit dem Fokus. Ohne Bedienungshilfen-Recht leer.
fn window_title(pid: i32) -> String {
    let Some(app) = CfOwned::new(unsafe { AXUIElementCreateApplication(pid) }) else {
        return String::new();
    };
    // Hängt die Ziel-App, nicht sekundenlang auf sie warten.
    unsafe { AXUIElementSetMessagingTimeout(app.0, 0.25) };
    ax_attribute(app.0, "AXFocusedWindow")
        .and_then(|window| ax_attribute(window.0, "AXTitle"))
        .and_then(|title| title.to_string())
        .unwrap_or_default()
}

pub fn active_app() -> AppContext {
    autoreleasepool(|_| {
        let Some(app) = NSWorkspace::sharedWorkspace().frontmostApplication() else {
            return AppContext::default();
        };
        let pid = app.processIdentifier();
        let name = app.localizedName().map(|s| s.to_string()).unwrap_or_default();
        AppContext {
            exe: name.to_lowercase(),
            bundle_id: app.bundleIdentifier().map(|s| s.to_string().to_lowercase()).unwrap_or_default(),
            title: window_title(pid),
            handle: pid as isize,
            name,
        }
    })
}

/// Holt die App mit dieser Prozess-ID wieder nach vorn.
pub fn focus(handle: isize) {
    if handle <= 0 || handle == std::process::id() as isize {
        return;
    }
    autoreleasepool(|_| {
        let Some(app) = NSRunningApplication::runningApplicationWithProcessIdentifier(handle as i32) else {
            return;
        };
        if !app.isActive() {
            // Ab macOS 14 ohne Wirkung, davor nötig, damit die App wirklich nach vorn kommt.
            #[allow(deprecated)]
            let _ = app.activateWithOptions(NSApplicationActivationOptions::ActivateIgnoringOtherApps);
        }
    })
}
