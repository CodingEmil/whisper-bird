//! Synthetische Tastatureingaben (Strg+V, Strg+C, Enter, Unicode-Tippen).

use std::time::{Duration, Instant};

#[cfg(windows)]
mod imp {
    use super::*;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS,
        KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, VIRTUAL_KEY, VK_CONTROL, VK_LCONTROL, VK_LMENU,
        VK_LSHIFT, VK_LWIN, VK_MENU, VK_RCONTROL, VK_RETURN, VK_RMENU, VK_RSHIFT, VK_RWIN, VK_SHIFT,
    };

    /// Markierung, damit eigene Eingaben erkennbar sind.
    const EXTRA_INFO: usize = 0x5749_4953; // "WIIS"

    fn key(vk: VIRTUAL_KEY, up: bool) -> INPUT {
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    wScan: 0,
                    dwFlags: if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) },
                    time: 0,
                    dwExtraInfo: EXTRA_INFO,
                },
            },
        }
    }

    fn unicode(unit: u16, up: bool) -> INPUT {
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(0),
                    wScan: unit,
                    dwFlags: if up { KEYEVENTF_UNICODE | KEYEVENTF_KEYUP } else { KEYEVENTF_UNICODE },
                    time: 0,
                    dwExtraInfo: EXTRA_INFO,
                },
            },
        }
    }

    pub fn send(inputs: &[INPUT]) {
        unsafe {
            SendInput(inputs, std::mem::size_of::<INPUT>() as i32);
        }
    }

    const MODIFIERS: [VIRTUAL_KEY; 8] = [
        VK_LCONTROL, VK_RCONTROL, VK_LSHIFT, VK_RSHIFT, VK_LMENU, VK_RMENU, VK_LWIN, VK_RWIN,
    ];

    fn is_down(vk: VIRTUAL_KEY) -> bool {
        unsafe { (GetAsyncKeyState(vk.0 as i32) as u16 & 0x8000) != 0 }
    }

    /// Wartet, bis der Nutzer alle Modifier losgelassen hat (sonst würde z. B.
    /// aus Strg+V ein Strg+Umschalt+V). Hängt ein Modifier, wird er gelöst.
    pub fn wait_modifiers_released(timeout: Duration) {
        let start = Instant::now();
        while start.elapsed() < timeout {
            if !MODIFIERS.iter().any(|&vk| is_down(vk)) {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let ups: Vec<INPUT> = MODIFIERS.iter().filter(|&&vk| is_down(vk)).map(|&vk| key(vk, true)).collect();
        if !ups.is_empty() {
            log::debug!("Löse {} hängende Modifier", ups.len());
            send(&ups);
        }
    }

    pub fn ctrl_combo(letter: u8) {
        let vk = VIRTUAL_KEY(letter as u16);
        send(&[key(VK_CONTROL, false), key(vk, false), key(vk, true), key(VK_CONTROL, true)]);
    }

    pub fn enter() {
        send(&[key(VK_RETURN, false), key(VK_RETURN, true)]);
    }

    pub fn type_text(text: &str) {
        let mut batch = Vec::new();
        for ch in text.chars() {
            if ch == '\n' {
                // Umschalt+Enter: Zeilenumbruch, ohne in Chats abzuschicken.
                batch.push(key(VK_SHIFT, false));
                batch.push(key(VK_RETURN, false));
                batch.push(key(VK_RETURN, true));
                batch.push(key(VK_SHIFT, true));
                continue;
            }
            if ch == '\r' {
                continue;
            }
            let mut buf = [0u16; 2];
            for &unit in ch.encode_utf16(&mut buf).iter() {
                batch.push(unicode(unit, false));
                batch.push(unicode(unit, true));
            }
            if batch.len() >= 64 {
                send(&batch);
                batch.clear();
                std::thread::sleep(Duration::from_millis(2));
            }
        }
        if !batch.is_empty() {
            send(&batch);
        }
    }

    #[allow(dead_code)]
    pub fn any_modifier_down() -> bool {
        [VK_CONTROL, VK_SHIFT, VK_MENU].iter().any(|&vk| is_down(vk))
    }
}

#[cfg(windows)]
pub use imp::{enter, type_text, wait_modifiers_released};

#[cfg(windows)]
pub fn paste() {
    imp::ctrl_combo(b'V');
}

#[cfg(windows)]
pub fn copy() {
    imp::ctrl_combo(b'C');
}

#[cfg(not(windows))]
pub fn paste() {}
#[cfg(not(windows))]
pub fn copy() {}
#[cfg(not(windows))]
pub fn enter() {}
#[cfg(not(windows))]
pub fn type_text(_: &str) {}
#[cfg(not(windows))]
pub fn wait_modifiers_released(_: Duration) {
    let _ = Instant::now();
}
