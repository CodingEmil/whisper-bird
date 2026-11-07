//! macOS: Tastatureingaben (CGEvent), Zwischenablage (NSPasteboard), aktive App
//! (NSWorkspace und Bedienungshilfen-API), Berechtigungen, Ton und Lautstärke.

pub mod audio;
pub mod clipboard;
pub mod foreground;
pub mod input;
pub mod panel;
pub mod permissions;
pub mod shell;

use std::ffi::{c_int, c_void};

extern "C" {
    fn atexit(callback: extern "C" fn()) -> c_int;
    fn _exit(status: c_int) -> !;
}

/// Lässt den Prozess beim folgenden `exit` enden, ohne die statischen C++-Destruktoren
/// auszuführen. Der Destruktor des Metal-Geräts in ggml bricht mit `GGML_ASSERT` ab, solange
/// noch Puffer eines Modells leben. Das lässt sich hier nicht zuverlässig vermeiden: Das
/// Sortformer-Modell gehört einem Meeting-Thread, und das Spracherkennungsmodell bleibt
/// während einer Transkription gesperrt, sodass ein Entladen das Beenden blockieren würde.
///
/// `atexit` arbeitet in umgekehrter Reihenfolge. Das hier zuletzt Registrierte läuft also vor
/// den Destruktoren, die ggml beim ersten Gebrauch registriert hat. Alles, was vor `exit`
/// geschieht (Aufräumen von Tauri, Benachrichtigungen von AppKit), bleibt davon unberührt.
/// Der Exit-Code ist immer 0, denn `atexit` kennt den Code nicht und die App beendet sich
/// nur mit 0.
pub fn skip_static_destructors() {
    extern "C" fn finish() {
        unsafe { _exit(0) }
    }
    if unsafe { atexit(finish) } != 0 {
        log::warn!("atexit fehlgeschlagen");
    }
}

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

#[cfg(test)]
mod tests {
    /// Ein vorher registrierter Handler steht für die Destruktoren von ggml: Er darf nicht mehr
    /// laufen. Geprüft im eigenen Prozess, weil der Test `exit` aufruft.
    #[test]
    fn beenden_ueberspringt_frueher_registrierte_handler() {
        const CHILD: &str = "WB_TEST_SKIP_DESTRUCTORS";
        if std::env::var_os(CHILD).is_some() {
            extern "C" fn stands_in_for_ggml() {
                std::process::abort();
            }
            unsafe { super::atexit(stands_in_for_ggml) };
            super::skip_static_destructors();
            println!("{CHILD}");
            std::process::exit(0);
        }
        let child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "--nocapture", "platform::macos::tests::beenden_ueberspringt_frueher_registrierte_handler"])
            .env(CHILD, "1")
            .output()
            .unwrap();
        // Ohne die Ausgabe könnte der Filter auch einfach keinen Test getroffen haben.
        assert!(String::from_utf8_lossy(&child.stdout).contains(CHILD));
        assert_eq!(child.status.code(), Some(0));
    }
}
