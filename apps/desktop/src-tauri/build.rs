use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    copy_vibe_hook_sidecar();
    tauri_build::build();
}

/// Copy workspace `vibe-hook` into `src-tauri/binaries/` for Tauri external bin (release builds).
fn copy_vibe_hook_sidecar() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace = manifest_dir.join("../../..");
    let profile = env::var("PROFILE").unwrap_or_else(|_| "debug".into());
    let name = if cfg!(windows) {
        "vibe-hook.exe"
    } else {
        "vibe-hook"
    };

    let built = workspace
        .join("target")
        .join(&profile)
        .join(name);
    if !built.is_file() {
        println!(
            "cargo:warning=vibe-hook not found at {} — run `cargo build -p vibe-hook` from repo root before install-hooks",
            built.display()
        );
        return;
    }

    let target = env::var("TARGET").unwrap_or_default();
    let out_dir = manifest_dir.join("binaries");
    let _ = fs::create_dir_all(&out_dir);

    if target.is_empty() {
        let flat = out_dir.join(name);
        let _ = fs::copy(&built, &flat);
        println!("cargo:rerun-if-changed={}", built.display());
        return;
    }

    // Tauri externalBin 期望 `vibe-hook-{target}{ext}`，Windows 必须保留 `.exe`，
    // 否则 `tauri build` 在 Windows 上会报 `resource path '...\vibe-hook-x86_64-pc-windows-msvc.exe' doesn't exist`。
    let sidecar_name = if target.contains("windows") {
        format!("vibe-hook-{target}.exe")
    } else {
        format!("vibe-hook-{target}")
    };
    let sidecar = out_dir.join(sidecar_name);
    if fs::copy(&built, &sidecar).is_ok() {
        println!("cargo:rerun-if-changed={}", built.display());
        let _ = fs::copy(&built, out_dir.join(name));
    }
}
