//! Overlay und Kontextmenü als NSPanel. Normale Fenster würden beim Klicken die
//! ganze App aktivieren, und der diktierte Text landete nicht mehr in der App,
//! in der man gerade schreibt. Panels schweben über allen Apps (auch im
//! Vollbild und auf allen Schreibtischen), ohne die App im Vordergrund abzulösen.

use tauri::{AppHandle, Manager, WebviewWindow};
use tauri_nspanel::{CollectionBehavior, ManagerExt, PanelLevel, StyleMask, WebviewWindowExt};

mod overlay {
    tauri_nspanel::tauri_panel! {
        panel!(OverlayPanel {
            config: {
                can_become_key_window: false,
                is_floating_panel: true
            }
        })
    }
}

mod menu {
    tauri_nspanel::tauri_panel! {
        panel!(MenuPanel {
            config: {
                // Nimmt Tastatureingaben an, ohne die App zu aktivieren (wie Spotlight).
                can_become_key_window: true,
                is_floating_panel: true
            }
        })
    }
}

/// Overlay: nie Tastaturfokus, über allem, auf jedem Schreibtisch.
pub fn make_overlay(window: &WebviewWindow) -> tauri::Result<()> {
    let panel = window.to_panel::<overlay::OverlayPanel>()?;
    panel.set_level(PanelLevel::Status.value());
    if let Err(e) = panel.set_style_mask(StyleMask::empty().borderless().nonactivating_panel().value()) {
        log::warn!("Overlay-Stil nicht gesetzt: {e:?}");
    }
    panel.set_collection_behavior(
        CollectionBehavior::new()
            .can_join_all_spaces()
            .stationary()
            .full_screen_auxiliary()
            .value(),
    );
    panel.set_hides_on_deactivate(false);
    Ok(())
}

/// Kontextmenü „Text bearbeiten“: bekommt Tastatureingaben, die Ziel-App bleibt aktiv.
pub fn make_menu(window: &WebviewWindow) -> tauri::Result<()> {
    let panel = window.to_panel::<menu::MenuPanel>()?;
    panel.set_level(PanelLevel::Floating.value());
    if let Err(e) = panel.set_style_mask(StyleMask::empty().borderless().nonactivating_panel().value()) {
        log::warn!("Menü-Stil nicht gesetzt: {e:?}");
    }
    panel.set_collection_behavior(
        CollectionBehavior::new()
            .move_to_active_space()
            .full_screen_auxiliary()
            .value(),
    );
    panel.set_hides_on_deactivate(false);
    Ok(())
}

/// Zeigt das Panel ganz oben, ohne Fokus und ohne die App zu aktivieren.
/// `false`, wenn es kein Panel ist (dann bitte normal zeigen).
pub fn show(app: &AppHandle, label: &str) -> bool {
    match app.get_webview_panel(label) {
        Ok(panel) => {
            panel.show();
            true
        }
        Err(_) => false,
    }
}

/// Zeigt das Panel und gibt ihm die Tastatur, ohne die App zu aktivieren.
pub fn focus(window: &WebviewWindow) -> bool {
    let Ok(panel) = window.app_handle().get_webview_panel(window.label()) else {
        return false;
    };
    panel.order_front_regardless();
    panel.make_key_window();
    // Eingaben an die WebView statt an die leere Inhaltsfläche des Panels.
    let _ = AsRef::<tauri::Webview>::as_ref(window).set_focus();
    true
}
