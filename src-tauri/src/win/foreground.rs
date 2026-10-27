//! Welches Programm hat gerade den Fokus? Für app-spezifische Formatierung,
//! Auto-Return und um den Fokus nach dem Kontextmenü zurückzugeben.

use serde::Serialize;

#[derive(Clone, Debug, Default, Serialize)]
pub struct AppContext {
    /// Dateiname der EXE in Kleinbuchstaben, z. B. "outlook.exe".
    pub exe: String,
    pub title: String,
    /// HWND als Zahl (HWND selbst ist nicht Send).
    pub hwnd: isize,
}

impl AppContext {
    /// Grobe Kategorie für die Enhance-Formatierung.
    pub fn category(&self) -> &'static str {
        let exe = self.exe.as_str();
        let title = self.title.to_lowercase();
        let browser = matches!(
            exe,
            "chrome.exe" | "msedge.exe" | "firefox.exe" | "brave.exe" | "opera.exe" | "vivaldi.exe" | "arc.exe"
        );
        let any = |list: &[&str]| list.iter().any(|k| title.contains(k));
        if matches!(exe, "outlook.exe" | "olk.exe" | "thunderbird.exe" | "hxoutlook.exe" | "mailspring.exe" | "emclient.exe")
            || (browser && any(&["gmail", "outlook", "posteingang", "inbox", "gmx", "web.de", "proton mail", "mail"]))
        {
            "email"
        } else if matches!(
            exe,
            "slack.exe" | "teams.exe" | "ms-teams.exe" | "discord.exe" | "whatsapp.exe" | "telegram.exe" | "signal.exe"
        ) || (browser && any(&["whatsapp", "slack", "discord", "teams", "telegram", "messenger"]))
        {
            "chat"
        } else if matches!(
            exe,
            "code.exe" | "cursor.exe" | "windsurf.exe" | "devenv.exe" | "idea64.exe" | "pycharm64.exe" | "rider64.exe"
                | "webstorm64.exe" | "zed.exe" | "sublime_text.exe" | "notepad++.exe" | "windowsterminal.exe"
                | "wt.exe" | "powershell.exe" | "pwsh.exe" | "cmd.exe"
        ) {
            "code"
        } else if browser && any(&["chatgpt", "claude", "gemini", "copilot", "perplexity"]) {
            "ai_prompt"
        } else if matches!(exe, "winword.exe" | "notion.exe" | "obsidian.exe" | "onenote.exe" | "soffice.bin" | "wordpad.exe")
            || (browser && any(&["google docs", "notion", "confluence"]))
        {
            "document"
        } else {
            "general"
        }
    }

    pub fn display_name(&self) -> String {
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
            hwnd: hwnd.0 as isize,
        }
    }
}

#[cfg(not(windows))]
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

#[cfg(not(windows))]
pub fn focus(_hwnd: isize) {}
