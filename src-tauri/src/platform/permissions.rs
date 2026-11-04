//! Berechtigungen, die die App braucht. Unter Windows gibt es nichts freizugeben;
//! unter macOS Bedienungshilfen (Shortcuts und Einfügen) und Mikrofon.

use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct Permissions {
    /// Muss der Nutzer überhaupt etwas freigeben (nur macOS)?
    pub required: bool,
    pub accessibility: bool,
    /// "granted", "denied", "not_determined" oder "unknown".
    pub microphone: &'static str,
}

fn microphone() -> &'static str {
    match handy_recorder::permission_status() {
        handy_recorder::Permission::Granted => "granted",
        handy_recorder::Permission::Denied => "denied",
        handy_recorder::Permission::NotDetermined => "not_determined",
        _ => "unknown",
    }
}

pub fn status() -> Permissions {
    #[cfg(target_os = "macos")]
    return Permissions {
        required: true,
        accessibility: super::macos::permissions::accessibility_granted(),
        microphone: microphone(),
    };
    #[cfg(not(target_os = "macos"))]
    Permissions {
        required: false,
        accessibility: true,
        microphone: microphone(),
    }
}

/// Fragt nach einer Freigabe ("accessibility" oder "microphone").
pub fn request(kind: &str) {
    #[cfg(target_os = "macos")]
    match kind {
        "accessibility" => super::macos::permissions::request_accessibility(),
        "microphone" => super::macos::permissions::request_microphone(),
        _ => {}
    }
    #[cfg(not(target_os = "macos"))]
    let _ = kind;
}

/// Öffnet die Systemeinstellungen zur Freigabe.
pub fn open_settings(kind: &str) {
    #[cfg(target_os = "macos")]
    super::macos::permissions::open_settings(kind);
    #[cfg(not(target_os = "macos"))]
    let _ = kind;
}
