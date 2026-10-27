//! Zwischenablage über Win32: Text setzen/lesen und den kompletten Inhalt
//! sichern und wiederherstellen (Text, Bilder, Dateien, Formatierungen).

#[derive(Default)]
pub struct Snapshot {
    formats: Vec<(u32, Vec<u8>)>,
}

impl Snapshot {
    pub fn is_empty(&self) -> bool {
        self.formats.is_empty()
    }
}

#[cfg(windows)]
mod imp {
    use super::Snapshot;
    use std::time::Duration;
    use windows::core::w;
    use windows::Win32::Foundation::{HANDLE, HGLOBAL};
    use windows::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, EnumClipboardFormats, GetClipboardData,
        GetClipboardSequenceNumber, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
    };
    use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE};
    use windows::Win32::System::Ole::CF_UNICODETEXT;

    /// Formate, die GDI-Handles statt Speicher enthalten – nicht kopierbar.
    fn is_handle_format(f: u32) -> bool {
        matches!(f, 2 | 3 | 9 | 14 | 0x80 | 0x81 | 0x82 | 0x83 | 0x8E) || (0x300..=0x3FF).contains(&f)
    }

    struct Guard;
    impl Drop for Guard {
        fn drop(&mut self) {
            unsafe {
                let _ = CloseClipboard();
            }
        }
    }

    /// Öffnet die Zwischenablage mit Wiederholungen (andere Apps halten sie oft kurz).
    fn open() -> anyhow::Result<Guard> {
        for _ in 0..25 {
            if unsafe { OpenClipboard(None) }.is_ok() {
                return Ok(Guard);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        anyhow::bail!("Zwischenablage ist blockiert")
    }

    unsafe fn read_global(h: HANDLE) -> Option<Vec<u8>> {
        let hg = HGLOBAL(h.0);
        let size = GlobalSize(hg);
        if size == 0 {
            return None;
        }
        let ptr = GlobalLock(hg) as *const u8;
        if ptr.is_null() {
            return None;
        }
        let data = std::slice::from_raw_parts(ptr, size).to_vec();
        let _ = GlobalUnlock(hg);
        Some(data)
    }

    unsafe fn alloc_global(data: &[u8]) -> anyhow::Result<HANDLE> {
        let hg = GlobalAlloc(GMEM_MOVEABLE, data.len().max(1))?;
        let ptr = GlobalLock(hg) as *mut u8;
        if ptr.is_null() {
            anyhow::bail!("GlobalLock fehlgeschlagen");
        }
        std::ptr::copy_nonoverlapping(data.as_ptr(), ptr, data.len());
        let _ = GlobalUnlock(hg);
        Ok(HANDLE(hg.0))
    }

    pub fn sequence() -> u32 {
        unsafe { GetClipboardSequenceNumber() }
    }

    pub fn snapshot() -> Snapshot {
        let mut snap = Snapshot::default();
        let Ok(_guard) = open() else { return snap };
        unsafe {
            let mut f = EnumClipboardFormats(0);
            while f != 0 {
                if !is_handle_format(f) {
                    if let Ok(h) = GetClipboardData(f) {
                        if let Some(data) = read_global(h) {
                            snap.formats.push((f, data));
                        }
                    }
                }
                f = EnumClipboardFormats(f);
            }
        }
        snap
    }

    pub fn restore(snap: &Snapshot) {
        let Ok(_guard) = open() else { return };
        unsafe {
            let _ = EmptyClipboard();
            for (f, data) in &snap.formats {
                if let Ok(h) = alloc_global(data) {
                    let _ = SetClipboardData(*f, Some(h));
                }
            }
        }
    }

    /// Setzt Text. `transient`: von Zwischenablage-Verlauf (Win+V) und Cloud ausnehmen.
    pub fn set_text(text: &str, transient: bool) -> anyhow::Result<()> {
        let _guard = open()?;
        unsafe {
            EmptyClipboard()?;
            let mut wide: Vec<u16> = text.encode_utf16().collect();
            wide.push(0);
            let bytes = std::slice::from_raw_parts(wide.as_ptr() as *const u8, wide.len() * 2);
            SetClipboardData(CF_UNICODETEXT.0 as u32, Some(alloc_global(bytes)?))?;
            if transient {
                let zero = 0u32.to_le_bytes();
                for name in [
                    w!("ExcludeClipboardContentFromMonitorProcessing"),
                    w!("CanIncludeInClipboardHistory"),
                    w!("CanUploadToCloudClipboard"),
                ] {
                    let fmt = RegisterClipboardFormatW(name);
                    if fmt != 0 {
                        if let Ok(h) = alloc_global(&zero) {
                            let _ = SetClipboardData(fmt, Some(h));
                        }
                    }
                }
            }
        }
        Ok(())
    }

    pub fn get_text() -> Option<String> {
        let _guard = open().ok()?;
        unsafe {
            let h = GetClipboardData(CF_UNICODETEXT.0 as u32).ok()?;
            let data = read_global(h)?;
            let units: Vec<u16> = data
                .chunks_exact(2)
                .map(|b| u16::from_le_bytes([b[0], b[1]]))
                .take_while(|&u| u != 0)
                .collect();
            Some(String::from_utf16_lossy(&units))
        }
    }

    pub fn clear() {
        if let Ok(_guard) = open() {
            unsafe {
                let _ = EmptyClipboard();
            }
        }
    }
}

#[cfg(windows)]
pub use imp::{clear, get_text, restore, sequence, set_text, snapshot};

#[cfg(not(windows))]
pub fn snapshot() -> Snapshot {
    Snapshot::default()
}
#[cfg(not(windows))]
pub fn restore(_: &Snapshot) {}
#[cfg(not(windows))]
pub fn set_text(_: &str, _: bool) -> anyhow::Result<()> {
    Ok(())
}
#[cfg(not(windows))]
pub fn get_text() -> Option<String> {
    None
}
#[cfg(not(windows))]
pub fn sequence() -> u32 {
    0
}
#[cfg(not(windows))]
pub fn clear() {}
