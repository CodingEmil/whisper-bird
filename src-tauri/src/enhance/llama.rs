//! Verwaltet einen lokalen llama-server (llama.cpp) als Kindprozess:
//! herunterladen, starten, Gesundheitscheck, beenden. Unter Windows der
//! Vulkan-Build (Zip), unter macOS der Metal-Build (tar.gz).

use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub const LLAMA_TAG: &str = "b11600";

/// Release-Paket dieser Plattform: Endung des Dateinamens, SHA-256, Größe.
#[cfg(windows)]
const PACKAGE: (&str, &str, u64) = (
    "bin-win-vulkan-x64.zip",
    "a66cf6f15e1839a6245a6f54ac130ff5f9475e2b6383f38b77823a9d268ab888",
    32_126_995,
);
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
const PACKAGE: (&str, &str, u64) = (
    "bin-macos-arm64.tar.gz",
    "a9c10e58e42603d67a3ad3cbe991d420e31d350937e11775db55011e25536668",
    12_116_133,
);
#[cfg(all(target_os = "macos", not(target_arch = "aarch64")))]
const PACKAGE: (&str, &str, u64) = (
    "bin-macos-x64.tar.gz",
    "f98d8f17e2bb39027f783cb7750eb498f39da4243c6a29827634568db9f0aa2d",
    11_614_051,
);
#[cfg(not(any(windows, target_os = "macos")))]
const PACKAGE: (&str, &str, u64) = (
    "bin-ubuntu-vulkan-x64.tar.gz",
    "3f4e4f41980170b08d8c64f3fb88a6bacd641f8d628a7246e3519ebc012ff980",
    30_358_849,
);

pub const LLAMA_PACKAGE_SHA256: &str = PACKAGE.1;
pub const LLAMA_PACKAGE_SIZE: u64 = PACKAGE.2;

#[cfg(windows)]
const EXE: &str = "llama-server.exe";
#[cfg(not(windows))]
const EXE: &str = "llama-server";

pub fn package_url() -> String {
    format!(
        "https://github.com/ggml-org/llama.cpp/releases/download/{LLAMA_TAG}/llama-{LLAMA_TAG}-{}",
        PACKAGE.0
    )
}

/// Dateiname für den Download (Zip bzw. tar.gz).
pub fn package_file_name() -> String {
    format!("llama-server.{}", if PACKAGE.0.ends_with(".zip") { "zip" } else { "tar.gz" })
}

struct Running {
    child: Child,
    port: u16,
    model: PathBuf,
    #[cfg(windows)]
    _job: job::Job,
}

pub struct LlamaServer {
    bin_dir: PathBuf,
    running: Mutex<Option<Running>>,
    last_used: Mutex<Instant>,
}

impl LlamaServer {
    pub fn new(bin_dir: PathBuf) -> Self {
        #[cfg(unix)]
        orphan::kill_stale(&bin_dir);
        Self {
            bin_dir,
            running: Mutex::new(None),
            last_used: Mutex::new(Instant::now()),
        }
    }

    pub fn bin_dir(&self) -> &Path {
        &self.bin_dir
    }

    pub fn exe_path(&self) -> PathBuf {
        self.bin_dir.join(LLAMA_TAG).join(EXE)
    }

    pub fn is_installed(&self) -> bool {
        self.exe_path().exists()
    }

    /// Entpackt das heruntergeladene Release-Paket.
    pub fn install(&self, package: &Path) -> anyhow::Result<()> {
        let target = self.bin_dir.join(LLAMA_TAG);
        std::fs::create_dir_all(&target)?;
        if package.extension().is_some_and(|e| e == "zip") {
            Self::unpack_zip(package, &target)?;
        } else {
            Self::unpack_tar(package, &target)?;
        }
        let _ = std::fs::remove_file(package);
        if !self.exe_path().exists() {
            anyhow::bail!("{EXE} fehlt im Archiv");
        }
        Ok(())
    }

    /// tar.gz (macOS/Linux): Alles liegt in einem Unterordner, die Bibliotheken
    /// sind teils Symlinks. Das System-tar behält beides bei.
    fn unpack_tar(package: &Path, target: &Path) -> anyhow::Result<()> {
        let output = Command::new("tar")
            .arg("-xzf")
            .arg(package)
            .arg("-C")
            .arg(target)
            .arg("--strip-components=1")
            .output()?;
        if !output.status.success() {
            anyhow::bail!("tar: {}", String::from_utf8_lossy(&output.stderr).trim());
        }
        Ok(())
    }

    fn unpack_zip(zip_path: &Path, target: &Path) -> anyhow::Result<()> {
        let file = std::fs::File::open(zip_path)?;
        let mut archive = zip::ZipArchive::new(file)?;
        for i in 0..archive.len() {
            let mut entry = archive.by_index(i)?;
            let Some(rel) = entry.enclosed_name() else { continue };
            // Flach entpacken: das Release legt alles in die Wurzel.
            let Some(name) = rel.file_name() else { continue };
            if entry.is_dir() {
                continue;
            }
            let out_path = target.join(name);
            let mut out = std::fs::File::create(&out_path)?;
            std::io::copy(&mut entry, &mut out)?;
        }
        Ok(())
    }

    pub fn is_running(&self) -> bool {
        let mut guard = self.running.lock().unwrap();
        if let Some(r) = guard.as_mut() {
            if matches!(r.child.try_wait(), Ok(None)) {
                return true;
            }
            *guard = None;
        }
        false
    }

    pub fn idle_secs(&self) -> u64 {
        self.last_used.lock().unwrap().elapsed().as_secs()
    }

    pub fn touch(&self) {
        *self.last_used.lock().unwrap() = Instant::now();
    }

    pub fn stop(&self) {
        if let Some(mut r) = self.running.lock().unwrap().take() {
            let _ = r.child.kill();
            let _ = r.child.wait();
            #[cfg(unix)]
            orphan::forget(&self.bin_dir);
            log::info!("llama-server beendet");
        }
    }

    /// Startet den Server für `model` (falls nötig) und liefert den Port,
    /// sobald das Modell geladen ist.
    pub async fn ensure(&self, model: &Path) -> anyhow::Result<u16> {
        self.touch();
        let port = {
            let mut guard = self.running.lock().unwrap();
            let reuse = match guard.as_mut() {
                Some(r) if r.model == model => matches!(r.child.try_wait(), Ok(None)).then_some(r.port),
                _ => None,
            };
            match reuse {
                Some(port) => port,
                None => {
                    if let Some(mut old) = guard.take() {
                        let _ = old.child.kill();
                        let _ = old.child.wait();
                    }
                    let running = self.spawn(model)?;
                    let port = running.port;
                    *guard = Some(running);
                    port
                }
            }
        };
        self.wait_healthy(port).await?;
        Ok(port)
    }

    fn spawn(&self, model: &Path) -> anyhow::Result<Running> {
        let exe = self.exe_path();
        if !exe.exists() {
            anyhow::bail!("llama-server ist nicht installiert");
        }
        let port = free_port()?;
        let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(8);
        let mut cmd = Command::new(&exe);
        cmd.current_dir(exe.parent().unwrap())
            .arg("-m")
            .arg(model)
            .args(["--host", "127.0.0.1", "--port", &port.to_string()])
            .args(["-ngl", "all", "-c", "16384", "-np", "1", "-t", &threads.to_string()])
            .args(["--no-webui", "--reasoning-budget", "0", "--log-disable"])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
        let child = cmd.spawn()?;
        log::info!("llama-server gestartet (Port {port}, Modell {})", model.display());
        #[cfg(windows)]
        let job = job::Job::kill_on_close(&child)?;
        // Ohne Job-Objekte: PID merken, um nach einem Absturz aufzuräumen.
        #[cfg(unix)]
        orphan::remember(&self.bin_dir, child.id());
        Ok(Running {
            child,
            port,
            model: model.to_path_buf(),
            #[cfg(windows)]
            _job: job,
        })
    }

    async fn wait_healthy(&self, port: u16) -> anyhow::Result<()> {
        let client = reqwest::Client::new();
        let url = format!("http://127.0.0.1:{port}/health");
        let deadline = Instant::now() + Duration::from_secs(180);
        loop {
            if let Ok(resp) = client.get(&url).timeout(Duration::from_secs(2)).send().await {
                if resp.status().is_success() {
                    return Ok(());
                }
            }
            if !self.is_running() {
                anyhow::bail!("llama-server ist abgestürzt (zu wenig Speicher oder Treiberproblem?)");
            }
            if Instant::now() > deadline {
                anyhow::bail!("llama-server lädt nicht (Zeitüberschreitung)");
            }
            tokio::time::sleep(Duration::from_millis(150)).await;
        }
    }
}

impl Drop for LlamaServer {
    fn drop(&mut self) {
        self.stop();
    }
}

fn free_port() -> anyhow::Result<u16> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    Ok(listener.local_addr()?.port())
}

#[cfg(windows)]
mod job {
    //! Job-Objekt: Stirbt Whisper Bird, beendet Windows auch den llama-server.
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };

    pub struct Job(HANDLE);
    unsafe impl Send for Job {}

    impl Job {
        pub fn kill_on_close(child: &std::process::Child) -> anyhow::Result<Self> {
            unsafe {
                let handle = CreateJobObjectW(None, None)?;
                let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
                info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
                SetInformationJobObject(
                    handle,
                    JobObjectExtendedLimitInformation,
                    &info as *const _ as *const _,
                    std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                )?;
                AssignProcessToJobObject(handle, HANDLE(child.as_raw_handle() as *mut _))?;
                Ok(Self(handle))
            }
        }
    }

    impl Drop for Job {
        fn drop(&mut self) {
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }
}

#[cfg(unix)]
mod orphan {
    //! Stürzt Whisper Bird ab, läuft der llama-server weiter (kein Job-Objekt wie
    //! unter Windows). Seine PID liegt deshalb in einer Datei; beim nächsten Start
    //! wird ein übrig gebliebener Server beendet.
    use std::path::{Path, PathBuf};
    use std::process::Command;

    fn pid_file(bin_dir: &Path) -> PathBuf {
        bin_dir.join("llama-server.pid")
    }

    pub fn remember(bin_dir: &Path, pid: u32) {
        let _ = std::fs::write(pid_file(bin_dir), pid.to_string());
    }

    pub fn forget(bin_dir: &Path) {
        let _ = std::fs::remove_file(pid_file(bin_dir));
    }

    pub fn kill_stale(bin_dir: &Path) {
        let Some(pid) = std::fs::read_to_string(pid_file(bin_dir)).ok().and_then(|s| s.trim().parse::<u32>().ok()) else {
            return;
        };
        forget(bin_dir);
        // Nur beenden, wenn die PID noch zu einem llama-server gehört.
        let name = Command::new("ps")
            .args(["-p", &pid.to_string(), "-o", "comm="])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default();
        if name.ends_with("llama-server") {
            log::info!("Beende übrig gebliebenen llama-server (PID {pid})");
            let _ = Command::new("kill").arg(pid.to_string()).status();
        }
    }
}
