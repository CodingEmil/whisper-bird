//! Verwaltet einen lokalen llama-server (llama.cpp, Vulkan-Build) als
//! Kindprozess: herunterladen, starten, Gesundheitscheck, beenden.

use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub const LLAMA_TAG: &str = "b11600";
pub const LLAMA_ZIP_SHA256: &str = "a66cf6f15e1839a6245a6f54ac130ff5f9475e2b6383f38b77823a9d268ab888";
pub const LLAMA_ZIP_SIZE: u64 = 32_126_995;

pub fn zip_url() -> String {
    format!(
        "https://github.com/ggml-org/llama.cpp/releases/download/{LLAMA_TAG}/llama-{LLAMA_TAG}-bin-win-vulkan-x64.zip"
    )
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
        self.bin_dir.join(LLAMA_TAG).join("llama-server.exe")
    }

    pub fn is_installed(&self) -> bool {
        self.exe_path().exists()
    }

    /// Entpackt das heruntergeladene Release-Zip.
    pub fn install_from_zip(&self, zip_path: &Path) -> anyhow::Result<()> {
        let target = self.bin_dir.join(LLAMA_TAG);
        std::fs::create_dir_all(&target)?;
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
        let _ = std::fs::remove_file(zip_path);
        if !self.exe_path().exists() {
            anyhow::bail!("llama-server.exe fehlt im Archiv");
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
