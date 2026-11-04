//! Welches Programm hat gerade den Fokus? Für app-spezifische Formatierung,
//! Auto-Return und um den Fokus nach dem Kontextmenü zurückzugeben.

use serde::Serialize;

#[derive(Clone, Debug, Default, Serialize)]
pub struct AppContext {
    /// Windows: Dateiname der EXE in Kleinbuchstaben, z. B. "outlook.exe".
    /// macOS: App-Name in Kleinbuchstaben, z. B. "slack" oder "microsoft outlook".
    pub exe: String,
    /// Anzeigename der App (macOS); unter Windows leer, dort zählt `exe`.
    pub name: String,
    /// Bundle-ID in Kleinbuchstaben, z. B. "com.tinyspeck.slackmacgap" (nur macOS).
    pub bundle_id: String,
    pub title: String,
    /// Windows: HWND als Zahl (HWND selbst ist nicht Send). macOS: Prozess-ID.
    pub handle: isize,
}

/// Bundle-IDs der Mac-Apps je Kategorie (Kleinbuchstaben; Einträge mit Punkt am
/// Ende gelten als Präfix, etwa für alle JetBrains-IDEs).
const MAC_BROWSERS: &[&str] = &[
    "com.google.chrome", "com.apple.safari", "org.mozilla.firefox", "com.brave.browser", "com.microsoft.edgemac",
    "company.thebrowser.browser", "com.operasoftware.opera", "com.vivaldi.vivaldi", "app.zen-browser.zen",
];
const MAC_EMAIL: &[&str] = &[
    "com.apple.mail", "com.microsoft.outlook", "org.mozilla.thunderbird", "com.readdle.smartemail-mac",
    "com.mailspring.mailspring", "com.superhuman.electron", "com.mimestream.mimestream",
];
const MAC_CHAT: &[&str] = &[
    "com.tinyspeck.slackmacgap", "com.microsoft.teams2", "com.microsoft.teams", "com.hnc.discord",
    "net.whatsapp.whatsapp", "desktop.whatsapp", "ru.keepcoder.telegram", "org.telegram.desktop",
    "org.whispersystems.signal-desktop", "com.apple.mobilesms",
];
const MAC_CODE: &[&str] = &[
    "com.microsoft.vscode", "com.todesktop.230313mzl4w4u92", "com.exafunction.windsurf", "dev.zed.zed",
    "com.sublimetext.4", "com.apple.dt.xcode", "com.jetbrains.", "com.google.android.studio", "com.apple.terminal",
    "com.googlecode.iterm2", "dev.warp.warp-stable", "com.mitchellh.ghostty", "net.kovidgoyal.kitty",
];
const MAC_DOCUMENT: &[&str] = &[
    "com.microsoft.word", "notion.id", "md.obsidian", "com.microsoft.onenote.mac", "com.apple.iwork.pages",
    "org.libreoffice.script", "com.apple.textedit", "com.apple.notes",
];

impl AppContext {
    fn bundle_in(&self, list: &[&str]) -> bool {
        let id = self.bundle_id.as_str();
        !id.is_empty() && list.iter().any(|&b| if b.ends_with('.') { id.starts_with(b) } else { id == b })
    }

    /// Grobe Kategorie für die Enhance-Formatierung.
    pub fn category(&self) -> &'static str {
        let exe = self.exe.as_str();
        let title = self.title.to_lowercase();
        let browser = matches!(
            exe,
            "chrome.exe" | "msedge.exe" | "firefox.exe" | "brave.exe" | "opera.exe" | "vivaldi.exe" | "arc.exe"
        ) || self.bundle_in(MAC_BROWSERS);
        let any = |list: &[&str]| list.iter().any(|k| title.contains(k));
        if matches!(exe, "outlook.exe" | "olk.exe" | "thunderbird.exe" | "hxoutlook.exe" | "mailspring.exe" | "emclient.exe")
            || self.bundle_in(MAC_EMAIL)
            || (browser && any(&["gmail", "outlook", "posteingang", "inbox", "gmx", "web.de", "proton mail", "mail"]))
        {
            "email"
        } else if matches!(
            exe,
            "slack.exe" | "teams.exe" | "ms-teams.exe" | "discord.exe" | "whatsapp.exe" | "telegram.exe" | "signal.exe"
        ) || self.bundle_in(MAC_CHAT)
            || (browser && any(&["whatsapp", "slack", "discord", "teams", "telegram", "messenger"]))
        {
            "chat"
        } else if matches!(
            exe,
            "code.exe" | "cursor.exe" | "windsurf.exe" | "devenv.exe" | "idea64.exe" | "pycharm64.exe" | "rider64.exe"
                | "webstorm64.exe" | "zed.exe" | "sublime_text.exe" | "notepad++.exe" | "windowsterminal.exe"
                | "wt.exe" | "powershell.exe" | "pwsh.exe" | "cmd.exe"
        ) || self.bundle_in(MAC_CODE)
        {
            "code"
        } else if browser && any(&["chatgpt", "claude", "gemini", "copilot", "perplexity"]) {
            "ai_prompt"
        } else if matches!(exe, "winword.exe" | "notion.exe" | "obsidian.exe" | "onenote.exe" | "soffice.bin" | "wordpad.exe")
            || self.bundle_in(MAC_DOCUMENT)
            || (browser && any(&["google docs", "notion", "confluence"]))
        {
            "document"
        } else {
            "general"
        }
    }

    pub fn display_name(&self) -> String {
        if !self.name.is_empty() {
            return self.name.clone();
        }
        let name = self.exe.trim_end_matches(".exe");
        let mut c = name.chars();
        match c.next() {
            Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            None => String::new(),
        }
    }
}

#[cfg(windows)]
pub fn active_app() -> AppContext {
    use windows::core::PWSTR;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId,
    };
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() {
            return AppContext::default();
        }
        let mut title_buf = [0u16; 512];
        let len = GetWindowTextW(hwnd, &mut title_buf).max(0) as usize;
        let title = String::from_utf16_lossy(&title_buf[..len]);
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        let mut exe = String::new();
        if let Ok(process) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
            let mut buf = [0u16; 1024];
            let mut size = buf.len() as u32;
            if QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut size).is_ok() {
                let path = String::from_utf16_lossy(&buf[..size as usize]);
                exe = path.rsplit(['\\', '/']).next().unwrap_or("").to_lowercase();
            }
            let _ = CloseHandle(process);
        }
        AppContext {
            exe,
            title,
            handle: hwnd.0 as isize,
            ..Default::default()
        }
    }
}

#[cfg(target_os = "macos")]
pub use super::macos::foreground::{active_app, focus};

#[cfg(not(any(windows, target_os = "macos")))]
pub fn active_app() -> AppContext {
    AppContext::default()
}

/// Gibt einem zuvor aktiven Fenster den Fokus zurück.
#[cfg(windows)]
pub fn focus(hwnd: isize) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{IsWindow, SetForegroundWindow};
    if hwnd == 0 {
        return;
    }
    unsafe {
        let h = HWND(hwnd as *mut _);
        if IsWindow(Some(h)).as_bool() {
            let _ = SetForegroundWindow(h);
        }
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn focus(_handle: isize) {}
