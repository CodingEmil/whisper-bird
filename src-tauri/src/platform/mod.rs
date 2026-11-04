//! Plattform-Helfer: aktive App, Zwischenablage, Tastatureingaben und
//! Berechtigungen. Die Windows-Teile stehen in den Dateien selbst, die von
//! macOS unter `macos/`.

pub mod clipboard;
pub mod foreground;
pub mod input;
pub mod permissions;

#[cfg(target_os = "macos")]
pub mod macos;
