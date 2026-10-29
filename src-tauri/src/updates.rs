//! Sucht auf GitHub nach einer neueren Version, lädt sie im Hintergrund vorab und
//! installiert sie auf Knopfdruck: über den Installer oder, bei der portablen Version,
//! durch Austausch der eigenen Dateien.

use crate::downloads::{self, DownloadProgress, DownloadRequest};
use crate::state::state;
use anyhow::Context;
use semver::Version;
use serde::{Deserialize, Serialize};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::Duration;
use tauri::Emitter;

const RELEASES_API: &str = "https://api.github.com/repos/CodingEmil/whisper-bird/releases?per_page=30";
pub const RELEASES_PAGE: &str = "https://github.com/CodingEmil/whisper-bird/releases";
pub const DOWNLOAD_ID: &str = "app-update";
/// Die App läuft meist tagelang im Tray; deshalb wird nicht nur beim Start gesucht.
const INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
/// Neue Exe der portablen Version, die beim Beenden gestartet wird.
static REPLACEMENT: Mutex<Option<PathBuf>> = Mutex::new(None);

#[derive(Clone, Debug, Default, Serialize)]
pub struct UpdateStatus {
    pub current: String,
    /// Neuere Version, falls es eine gibt.
    pub latest: Option<String>,
    /// Release-Seite der neueren Version.
    pub url: Option<String>,
    /// Die App kann sich selbst aktualisieren (Installer bzw. portable Zip im Release).
    pub installable: bool,
    /// Das Update ist heruntergeladen und geprüft.
    pub ready: bool,
    /// „Installieren“ wurde gedrückt; es geht los, sobald der Download fertig ist.
    pub installing: bool,
    pub checking: bool,
    /// Letzte erfolgreiche Suche (ms seit 1970).
    pub checked_at: Option<i64>,
    pub error: Option<String>,
    /// Seit dem letzten Start kam ein Update; die Oberfläche meldet es einmal.
    pub just_updated: bool,
    /// Neuerungen der laufenden Version aus ihrem Release-Text.
    pub notes: Vec<String>,
}

/// Wie sich diese Kopie aktualisiert.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    /// Installiert: über den Installer des Releases.
    Installer,
    /// Portable Zip: Die App ersetzt ihre eigenen Dateien.
    Portable,
}

#[derive(Clone)]
struct Package {
    url: String,
    name: String,
    size: u64,
    sha256: Option<String>,
}

pub struct Updates {
    current: Version,
    /// Keiner im Entwicklungsbuild: Der aktualisiert sich nie selbst.
    mode: Option<Mode>,
    status: Mutex<UpdateStatus>,
    package: Mutex<Option<Package>>,
}

impl Updates {
    pub fn new(current: Version) -> Self {
        let status = UpdateStatus {
            current: current.to_string(),
            ..Default::default()
        };
        Self {
            current,
            mode: detect_mode(),
            status: Mutex::new(status),
            package: Mutex::new(None),
        }
    }

    pub fn status(&self) -> UpdateStatus {
        self.status.lock().unwrap().clone()
    }

    fn update(&self, f: impl FnOnce(&mut UpdateStatus)) {
        f(&mut self.status.lock().unwrap());
        self.changed();
    }

    fn changed(&self) {
        let _ = state().app.emit("update-status", self.status());
    }
}

fn detect_mode() -> Option<Mode> {
    if cfg!(debug_assertions) {
        return None;
    }
    let exe = std::env::current_exe().ok()?;
    // Der Installer legt seine uninstall.exe neben die App.
    Some(if exe.parent()?.join("uninstall.exe").exists() {
        Mode::Installer
    } else {
        Mode::Portable
    })
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<Asset>,
    body: Option<String>,
}

impl Release {
    fn version(&self) -> Option<Version> {
        Version::parse(self.tag_name.trim_start_matches('v')).ok()
    }
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
    size: u64,
    /// „sha256:…“, von GitHub berechnet.
    digest: Option<String>,
}

/// Neueste Version über `current`. Vorabversionen zählen nur, wenn schon eine läuft.
fn newer(releases: Vec<Release>, current: &Version) -> Option<(Version, Release)> {
    let beta = !current.pre.is_empty();
    releases
        .into_iter()
        .filter(|r| !r.draft)
        .filter_map(|r| Some((r.version()?, r)))
        .filter(|(v, r)| beta || (v.pre.is_empty() && !r.prerelease))
        .filter(|(v, _)| v > current)
        .max_by(|a, b| a.0.cmp(&b.0))
}

/// Die Stichpunkte über der ersten `---`-Linie; darunter steht der Installationshinweis.
fn release_notes(body: &str) -> Vec<String> {
    body.lines()
        .map(str::trim)
        .take_while(|l| *l != "---")
        .filter_map(|l| l.strip_prefix("- ").or_else(|| l.strip_prefix("* ")))
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

fn pick_package(release: &Release, mode: Mode) -> Option<Package> {
    let suffix = match mode {
        Mode::Installer => "-setup.exe",
        Mode::Portable => "-portable.zip",
    };
    let a = release.assets.iter().find(|a| a.name.ends_with(suffix))?;
    Some(Package {
        url: a.browser_download_url.clone(),
        name: a.name.clone(),
        size: a.size,
        sha256: a.digest.as_deref().and_then(|d| d.strip_prefix("sha256:")).map(str::to_string),
    })
}

fn dir() -> PathBuf {
    state().dirs.data.join("updates")
}

/// Liste der Dateien, die ein portables Update als *.old zurückgelassen hat.
fn leftovers_file() -> PathBuf {
    state().dirs.data.join("update-leftovers.txt")
}

/// Löscht im Update-Ordner alles außer dem Paket, das noch gebraucht wird.
fn prune(keep: Option<&str>) {
    let Ok(entries) = std::fs::read_dir(dir()) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if keep.is_some_and(|k| name == k || name == format!("{k}.part")) {
            continue;
        }
        let _ = std::fs::remove_file(entry.path());
    }
}

/// Ob das Paket schon vollständig daliegt, etwa von einem früheren Start.
fn complete(path: &Path, pkg: &Package) -> bool {
    match &pkg.sha256 {
        Some(sha) => downloads::sha256_file(path).is_ok_and(|h| h.eq_ignore_ascii_case(sha)),
        None => std::fs::metadata(path).is_ok_and(|m| m.len() == pkg.size),
    }
}

async fn fetch(current: &Version) -> anyhow::Result<Vec<Release>> {
    let client = reqwest::Client::builder()
        .user_agent(format!("WhisperBird/{current}"))
        .timeout(Duration::from_secs(20))
        .build()?;
    let releases: Vec<Release> = client
        .get(RELEASES_API)
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(releases)
}

/// Fragt GitHub nach der neuesten Version und lädt sie gleich vorab; läuft schon eine
/// Suche, passiert nichts.
pub async fn check() {
    let up = &state().updates;
    {
        let mut s = up.status.lock().unwrap();
        if s.checking {
            return;
        }
        s.checking = true;
    }
    up.changed();
    let result = fetch(&up.current).await;
    let found = {
        let mut s = up.status.lock().unwrap();
        s.checking = false;
        match result {
            Ok(releases) => {
                s.notes = releases
                    .iter()
                    .find(|r| r.version().as_ref() == Some(&up.current))
                    .and_then(|r| r.body.as_deref())
                    .map(release_notes)
                    .unwrap_or_default();
                let found = newer(releases, &up.current);
                if let Some((v, _)) = &found {
                    log::info!("Update verfügbar: {v}");
                }
                let package = found.as_ref().zip(up.mode).and_then(|((_, r), mode)| pick_package(r, mode));
                let mut current = up.package.lock().unwrap();
                // Ein noch neueres Release ersetzt ein schon geladenes.
                if current.as_ref().map(|p| &p.name) != package.as_ref().map(|p| &p.name) {
                    s.ready = false;
                }
                *current = package.clone();
                s.installable = package.is_some();
                s.latest = found.as_ref().map(|(v, _)| v.to_string());
                s.url = found.map(|(_, r)| r.html_url);
                s.checked_at = Some(chrono::Utc::now().timestamp_millis());
                s.error = None;
                Some(package)
            }
            Err(e) => {
                log::warn!("Update-Suche fehlgeschlagen: {e:#}");
                s.error = Some(format!("{e:#}"));
                None
            }
        }
    };
    up.changed();
    crate::tray::rebuild();
    if let Some(package) = found {
        prune(package.as_ref().map(|p| p.name.as_str()));
        if let Some(pkg) = package {
            prefetch(pkg);
        }
    }
}

/// Beim Start: erkennt ein gerade eingespieltes Update, räumt auf und sucht dann
/// täglich nach neuen Versionen, solange das nicht abgeschaltet ist.
pub fn init() {
    detect_update();
    remove_leftovers();
    if !state().settings().check_updates {
        let _ = std::fs::remove_dir_all(dir());
    }
    tauri::async_runtime::spawn(async {
        loop {
            if state().settings().check_updates {
                check().await;
            }
            tokio::time::sleep(INTERVAL).await;
        }
    });
}

/// Läuft eine neuere Version als beim letzten Mal, meldet die Oberfläche das, bis es
/// quittiert ist. Egal, ob über die App, den Installer oder von Hand aktualisiert wurde.
fn detect_update() {
    // Der Entwicklungsbuild teilt sich die Einstellungen mit der installierten App; seine
    // Versionsnummer (0.1.0 gilt als neuer als jede Beta) würde deren Hinweis verschlucken.
    if cfg!(debug_assertions) {
        return;
    }
    let st = state();
    let s = st.settings();
    let up = &st.updates;
    if s.last_version == up.status().current {
        return;
    }
    let updated = match Version::parse(&s.last_version) {
        Ok(last) => last < up.current,
        // Noch nichts gemerkt: eine Neuinstallation – oder ein Update von einer Version,
        // die sich das noch nicht gemerkt hat; dann ist die Einrichtung schon erledigt.
        Err(_) => s.onboarding_done,
    };
    if updated {
        up.status.lock().unwrap().just_updated = true;
    } else {
        notice_seen();
    }
}

/// Der Hinweis auf das Update ist gelesen.
pub fn notice_seen() {
    let st = state();
    let current = st.updates.status().current;
    st.updates.update(|s| s.just_updated = false);
    st.update_settings(|s| s.last_version = current);
}

/// Lädt das Paket im Hintergrund, damit „Installieren“ sofort loslegen kann.
fn prefetch(pkg: Package) {
    let st = state();
    if st.updates.status().ready || st.downloads.is_active(DOWNLOAD_ID) {
        return;
    }
    let dest = dir().join(&pkg.name);
    if complete(&dest, &pkg) {
        finished(&st.app, true);
        return;
    }
    let req = DownloadRequest {
        id: DOWNLOAD_ID.into(),
        url: pkg.url,
        dest,
        size: pkg.size,
        sha256: pkg.sha256,
    };
    st.downloads.start_with(st.app.clone(), req, finished);
}

fn finished(_app: &tauri::AppHandle, ok: bool) {
    let up = &state().updates;
    let install = {
        let mut s = up.status.lock().unwrap();
        if ok {
            s.ready = true;
        } else {
            s.installing = false;
        }
        ok && s.installing
    };
    up.changed();
    if install {
        apply();
    }
}

/// „Installieren“: sofort, wenn das Update schon geladen ist, sonst nach dem Download.
pub fn install() -> anyhow::Result<()> {
    let st = state();
    if st.dictation.is_busy() || st.meeting.is_active() {
        anyhow::bail!(crate::i18n::t(&st.settings().ui_language, "update_busy"));
    }
    let pkg = st.updates.package.lock().unwrap().clone().context("Kein installierbares Update gefunden")?;
    let (already, ready) = {
        let mut s = st.updates.status.lock().unwrap();
        let already = s.installing;
        s.installing = true;
        (already, s.ready)
    };
    if already {
        return Ok(());
    }
    st.updates.changed();
    if ready {
        apply();
    } else {
        prefetch(pkg);
    }
    Ok(())
}

/// Startet den Installer bzw. tauscht die Dateien aus und beendet die App.
fn apply() {
    std::thread::spawn(|| {
        let st = state();
        let result = if st.dictation.is_busy() || st.meeting.is_active() {
            Err(anyhow::anyhow!(crate::i18n::t(&st.settings().ui_language, "update_busy")))
        } else {
            launch()
        };
        match result {
            Ok(()) => {
                log::info!("Update wird installiert, Whisper Bird beendet sich");
                st.enhancer.llama.stop();
                st.app.exit(0);
            }
            Err(e) => {
                log::error!("Update fehlgeschlagen: {e:#}");
                st.updates.update(|s| s.installing = false);
                // Die Oberfläche zeigt den Fehler wie einen gescheiterten Download.
                let _ = st.app.emit(
                    "download-progress",
                    DownloadProgress {
                        id: DOWNLOAD_ID.into(),
                        downloaded: 0,
                        total: 0,
                        state: "error",
                        error: Some(format!("{e:#}")),
                    },
                );
            }
        }
    });
}

fn launch() -> anyhow::Result<()> {
    let up = &state().updates;
    let pkg = up.package.lock().unwrap().clone().context("Kein Update geladen")?;
    let path = dir().join(&pkg.name);
    match up.mode {
        Some(Mode::Installer) => {
            // /P: nur der Fortschritt, keine Seiten (der Autostart bleibt, wie er ist);
            // /UPDATE: über die bestehende Installation; /R: danach neu starten.
            Command::new(&path)
                .args(["/P", "/UPDATE", "/R"])
                .spawn()
                .context("Installer konnte nicht starten")?;
        }
        Some(Mode::Portable) => {
            let exe = std::env::current_exe()?;
            let old = replace_files(&path, &exe).context("Update konnte nicht entpackt werden")?;
            let list: Vec<String> = old.iter().map(|p| p.display().to_string()).collect();
            let _ = std::fs::write(leftovers_file(), list.join("\n"));
            *REPLACEMENT.lock().unwrap() = Some(exe);
        }
        None => anyhow::bail!("Der Entwicklungsbuild aktualisiert sich nicht selbst"),
    }
    Ok(())
}

fn suffixed(path: &Path, suffix: &str) -> PathBuf {
    let mut s = OsString::from(path.as_os_str());
    s.push(suffix);
    PathBuf::from(s)
}

/// Ersetzt die Dateien neben `exe` durch die aus dem Zip; die Exe darf umbenannt worden sein.
/// Windows erlaubt, eine laufende Exe umzubenennen: Die alten Dateien bleiben als *.old
/// liegen (Rückgabe), bis die neue Version startet und sie löscht.
fn replace_files(zip_path: &Path, exe: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let dir = exe.parent().context("Programmordner unbekannt")?;
    let mut archive = zip::ZipArchive::new(std::fs::File::open(zip_path)?)?;

    // Erst alles als *.new entpacken; geht dabei etwas schief, bleibt die alte Version unberührt.
    let mut staged: Vec<(PathBuf, PathBuf)> = Vec::new();
    let unpacked = stage(&mut archive, exe, dir, &mut staged).and_then(|()| {
        anyhow::ensure!(staged.iter().any(|(target, _)| target == exe), "Die Exe fehlt im Update");
        Ok(())
    });
    if let Err(e) = unpacked {
        for (_, new) in &staged {
            let _ = std::fs::remove_file(new);
        }
        return Err(e);
    }

    // Dann tauschen: alt → *.old, neu an seinen Platz.
    let mut old = Vec::new();
    for (i, (target, new)) in staged.iter().enumerate() {
        let backup = suffixed(target, ".old");
        let _ = std::fs::remove_file(&backup);
        let existed = target.exists();
        let result = (if existed { std::fs::rename(target, &backup) } else { Ok(()) })
            .and_then(|()| std::fs::rename(new, target));
        if let Err(e) = result {
            // Zurück zum alten Stand.
            for (target, _) in &staged[..=i] {
                let backup = suffixed(target, ".old");
                if backup.exists() {
                    let _ = std::fs::remove_file(target);
                    let _ = std::fs::rename(&backup, target);
                }
            }
            for (_, new) in &staged {
                let _ = std::fs::remove_file(new);
            }
            return Err(e.into());
        }
        if existed {
            old.push(backup);
        }
    }
    Ok(old)
}

fn stage(
    archive: &mut zip::ZipArchive<std::fs::File>,
    exe: &Path,
    dir: &Path,
    staged: &mut Vec<(PathBuf, PathBuf)>,
) -> anyhow::Result<()> {
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        if entry.is_dir() {
            continue;
        }
        let Some(rel) = entry.enclosed_name() else { continue };
        let Some(name) = rel.file_name() else { continue };
        let is_exe = Path::new(name).extension().is_some_and(|e| e.eq_ignore_ascii_case("exe"));
        let target = if is_exe { exe.to_path_buf() } else { dir.join(name) };
        let new = suffixed(&target, ".new");
        let mut out = std::fs::File::create(&new)?;
        staged.push((target, new));
        std::io::copy(&mut entry, &mut out)?;
    }
    Ok(())
}

/// Löscht die *.old-Dateien des letzten portablen Updates.
fn remove_leftovers() {
    let file = leftovers_file();
    let Ok(list) = std::fs::read_to_string(&file) else { return };
    for path in list.lines().filter(|l| l.ends_with(".old")) {
        let _ = std::fs::remove_file(path);
    }
    let _ = std::fs::remove_file(file);
}

/// Startet nach einem portablen Update die neue Version. Läuft ganz am Ende des Beendens:
/// Erst dann hat das Single-Instance-Plugin seine Sperre freigegeben, die die neue
/// Version sonst sofort wieder schließen würde.
pub fn start_replacement() {
    if let Some(exe) = REPLACEMENT.lock().unwrap().take() {
        if let Err(e) = Command::new(&exe).spawn() {
            log::error!("Neue Version konnte nicht starten: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn release(tag: &str, prerelease: bool) -> Release {
        Release {
            tag_name: tag.into(),
            html_url: format!("https://example.com/{tag}"),
            draft: false,
            prerelease,
            assets: Vec::new(),
            body: None,
        }
    }

    #[test]
    fn neuerungen_stehen_ueber_der_linie() {
        let body = "## Neu\n\n- Update-Anzeige in der Seitenleiste\n* Hinweis nach dem Update\n-  \n\n---\n\nInstaller oder portable Zip.\n- kein Stichpunkt";
        assert_eq!(release_notes(body), ["Update-Anzeige in der Seitenleiste", "Hinweis nach dem Update"]);
        // So sehen die bisherigen Releases aus: keine Neuerungen.
        let alt = "Installer (`*-setup.exe`) oder portable Zip.\n\n**Full Changelog**: https://github.com/x/y/compare/a...b";
        assert!(release_notes(alt).is_empty());
    }

    fn pick(releases: Vec<Release>, current: &str) -> Option<String> {
        newer(releases, &Version::parse(current).unwrap()).map(|(v, _)| v.to_string())
    }

    #[test]
    fn neueste_version_gewinnt() {
        let all = || vec![release("v0.1.0-beta.4", true), release("v0.1.0-beta.10", true), release("v0.1.0-beta.9", true)];
        assert_eq!(pick(all(), "0.1.0-beta.4").as_deref(), Some("0.1.0-beta.10"));
        assert_eq!(pick(all(), "0.1.0-beta.10"), None);
    }

    #[test]
    fn vorabversionen_nur_fuer_beta_nutzer() {
        let all = || vec![release("v0.2.0", false), release("v0.3.0-beta.1", true)];
        assert_eq!(pick(all(), "0.1.0").as_deref(), Some("0.2.0"));
        assert_eq!(pick(all(), "0.2.0"), None);
        assert_eq!(pick(all(), "0.1.0-beta.4").as_deref(), Some("0.3.0-beta.1"));
        // Die stabile Version folgt auf ihre Betas.
        assert_eq!(pick(vec![release("v0.1.0", false)], "0.1.0-beta.4").as_deref(), Some("0.1.0"));
    }

    #[test]
    fn entwuerfe_und_fremde_tags_zaehlen_nicht() {
        let mut draft = release("v0.5.0", false);
        draft.draft = true;
        assert_eq!(pick(vec![draft, release("nightly", false)], "0.1.0"), None);
    }

    #[test]
    fn paket_passt_zur_art_der_installation() {
        let mut r = release("v0.2.0", false);
        let asset = |name: &str, digest: Option<&str>| Asset {
            name: name.into(),
            browser_download_url: name.into(),
            size: 1,
            digest: digest.map(str::to_string),
        };
        r.assets = vec![
            asset("Whisper-Bird_0.2.0_x64-portable.zip", None),
            asset("Whisper.Bird_0.2.0_x64-setup.exe", Some("sha256:abc")),
        ];
        let setup = pick_package(&r, Mode::Installer).unwrap();
        assert_eq!(setup.url, "Whisper.Bird_0.2.0_x64-setup.exe");
        assert_eq!(setup.sha256.as_deref(), Some("abc"));
        assert_eq!(pick_package(&r, Mode::Portable).unwrap().url, "Whisper-Bird_0.2.0_x64-portable.zip");
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("whisper-bird-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("app")).unwrap();
        dir
    }

    fn write_zip(path: &Path, files: &[(&str, &str)]) {
        let mut zip = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
        for (name, text) in files {
            zip.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
            zip.write_all(text.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
    }

    fn read(path: PathBuf) -> String {
        std::fs::read_to_string(path).unwrap()
    }

    #[test]
    fn portable_dateien_werden_ersetzt() {
        let dir = temp_dir("portable");
        let app = dir.join("app");
        let exe = app.join("Mein Bird.exe");
        std::fs::write(&exe, "alt").unwrap();
        std::fs::write(app.join("runtime.dll"), "alt").unwrap();
        let zip = dir.join("update.zip");
        write_zip(&zip, &[("Whisper Bird.exe", "neu"), ("runtime.dll", "neu"), ("NOTICES.txt", "neu")]);

        let old = replace_files(&zip, &exe).unwrap();
        assert_eq!(read(exe.clone()), "neu");
        assert_eq!(read(app.join("runtime.dll")), "neu");
        assert_eq!(read(app.join("NOTICES.txt")), "neu");
        assert_eq!(read(app.join("Mein Bird.exe.old")), "alt");
        assert_eq!(old.len(), 2);
        assert!(!app.join("Whisper Bird.exe").exists());
        assert!(!app.join("runtime.dll.new").exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn ohne_exe_bleibt_alles_beim_alten() {
        let dir = temp_dir("kaputt");
        let app = dir.join("app");
        let exe = app.join("Whisper Bird.exe");
        std::fs::write(&exe, "alt").unwrap();
        let zip = dir.join("update.zip");
        write_zip(&zip, &[("runtime.dll", "neu")]);

        assert!(replace_files(&zip, &exe).is_err());
        assert_eq!(read(exe), "alt");
        assert_eq!(std::fs::read_dir(&app).unwrap().count(), 1);
        let _ = std::fs::remove_dir_all(dir);
    }
}
