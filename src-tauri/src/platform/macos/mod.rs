//! macOS: Tastatureingaben (CGEvent), Zwischenablage (NSPasteboard), aktive App
//! (NSWorkspace und Bedienungshilfen-API), Berechtigungen, Ton und Lautstärke.

pub mod audio;
pub mod clipboard;
pub mod foreground;
pub mod input;
pub mod panel;
pub mod permissions;
pub mod shell;

use std::ffi::c_void;

/// Erste bevorzugte Sprache des Nutzers, z. B. "de-DE".
pub fn preferred_language() -> String {
    objc2::rc::autoreleasepool(|_| {
        objc2_foundation::NSLocale::preferredLanguages()
            .firstObject()
            .map(|s| s.to_string())
            .unwrap_or_default()
    })
}

// CoreFoundation-Grundlagen für die C-APIs (Bedienungshilfen), die objc2 nicht abdeckt.
#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFRelease(cf: *const c_void);
    fn CFGetTypeID(cf: *const c_void) -> usize;
    fn CFStringGetTypeID() -> usize;
}

/// Besitzt eine CF-Referenz aus einer Create/Copy-Funktion und gibt sie frei.
pub(crate) struct CfOwned(pub *const c_void);

impl CfOwned {
    pub fn new(ptr: *const c_void) -> Option<Self> {
        (!ptr.is_null()).then_some(Self(ptr))
    }

    /// Inhalt als String, falls es ein CFString ist (CFString und NSString sind austauschbar).
    pub fn to_string(&self) -> Option<String> {
        unsafe {
            if CFGetTypeID(self.0) != CFStringGetTypeID() {
                return None;
            }
            let s = &*(self.0 as *const objc2_foundation::NSString);
            Some(s.to_string())
        }
    }
}

impl Drop for CfOwned {
    fn drop(&mut self) {
        unsafe { CFRelease(self.0) }
    }
}
