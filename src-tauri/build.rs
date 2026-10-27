fn main() {
    // transcribe-cpp-sys linkt vulkan-1.lib, gibt aber den Suchpfad des
    // Vulkan SDK nicht weiter – hier ergänzen.
    println!("cargo:rerun-if-env-changed=VULKAN_SDK");
    if let Some(lib) = vulkan_lib_dir() {
        println!("cargo:rustc-link-search=native={}", lib.display());
    }
    tauri_build::build()
}

fn vulkan_lib_dir() -> Option<std::path::PathBuf> {
    if let Some(sdk) = std::env::var_os("VULKAN_SDK") {
        let lib = std::path::Path::new(&sdk).join("Lib");
        if lib.join("vulkan-1.lib").exists() {
            return Some(lib);
        }
    }
    // Fallback: neueste Installation unter C:\VulkanSDK
    let mut versions: Vec<_> = std::fs::read_dir("C:\\VulkanSDK")
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.join("Lib").join("vulkan-1.lib").exists())
        .collect();
    versions.sort();
    versions.pop().map(|p| p.join("Lib"))
}
