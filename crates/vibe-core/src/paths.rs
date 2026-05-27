use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

pub fn hook_file_name() -> &'static str {
    if cfg!(windows) {
        "vibe-hook.exe"
    } else {
        "vibe-hook"
    }
}

/// Search common locations for a built `vibe-hook` binary (dev + bundle + prior install).
pub fn discover_hook_binary(hints: &[PathBuf]) -> Option<PathBuf> {
    let name = hook_file_name();
    let mut candidates: Vec<PathBuf> = Vec::new();

    for h in hints {
        candidates.push(h.clone());
    }

    if let Ok(installed) = hook_binary_path() {
        candidates.push(installed);
    }

    if let Ok(cwd) = std::env::current_dir() {
        let mut dir: &Path = cwd.as_path();
        for _ in 0..8 {
            candidates.push(dir.join("target").join("debug").join(name));
            candidates.push(dir.join("target").join("release").join(name));
            match dir.parent() {
                Some(p) => dir = p,
                None => break,
            }
        }
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(mut dir) = exe.parent() {
            for _ in 0..6 {
                candidates.push(dir.join(name));
                candidates.push(dir.join("binaries").join(name));
                candidates.push(dir.join("../Resources/binaries").join(name));
                match dir.parent() {
                    Some(p) => dir = p,
                    None => break,
                }
            }
        }
    }

    for path in candidates {
        if path.is_file() {
            return Some(path);
        }
    }
    None
}

pub fn data_dir() -> Result<PathBuf> {
    let dir = directories::ProjectDirs::from("com", "VibeMonitor", "vibe-monitor")
        .context("could not resolve data directory")?
        .data_dir()
        .to_path_buf();
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

pub fn bin_dir() -> Result<PathBuf> {
    let dir = data_dir()?.join("bin");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

pub fn hook_binary_path() -> Result<PathBuf> {
    let name = if cfg!(windows) {
        "vibe-hook.exe"
    } else {
        "vibe-hook"
    };
    Ok(bin_dir()?.join(name))
}

pub fn hook_cmd_path() -> Result<PathBuf> {
    Ok(bin_dir()?.join("vibe-hook.cmd"))
}

pub fn port_file() -> Result<PathBuf> {
    Ok(data_dir()?.join("port"))
}

pub fn state_file() -> Result<PathBuf> {
    Ok(data_dir()?.join("state.json"))
}

pub fn first_run_marker() -> Result<PathBuf> {
    Ok(data_dir()?.join("first-run.done"))
}

pub fn cursor_hooks_path() -> PathBuf {
    directories::BaseDirs::new()
        .map(|b| b.home_dir().join(".cursor").join("hooks.json"))
        .unwrap_or_else(|| PathBuf::from(".cursor/hooks.json"))
}

pub fn claude_settings_path() -> PathBuf {
    directories::BaseDirs::new()
        .map(|b| b.home_dir().join(".claude").join("settings.json"))
        .unwrap_or_else(|| PathBuf::from(".claude/settings.json"))
}

pub fn codex_hooks_path() -> PathBuf {
    directories::BaseDirs::new()
        .map(|b| b.home_dir().join(".codex").join("hooks.json"))
        .unwrap_or_else(|| PathBuf::from(".codex/hooks.json"))
}

pub fn codex_config_path() -> PathBuf {
    directories::BaseDirs::new()
        .map(|b| b.home_dir().join(".codex").join("config.toml"))
        .unwrap_or_else(|| PathBuf::from(".codex/config.toml"))
}

pub fn cursor_transcripts_root() -> Option<PathBuf> {
    directories::BaseDirs::new().map(|b| b.home_dir().join(".cursor").join("projects"))
}

pub fn claude_transcripts_root() -> Option<PathBuf> {
    directories::BaseDirs::new().map(|b| b.home_dir().join(".claude").join("projects"))
}

pub fn codex_transcripts_root() -> Option<PathBuf> {
    directories::BaseDirs::new().map(|b| b.home_dir().join(".codex").join("projects"))
}

/// All lite-mode watch roots: Cursor, Claude Code, Codex — none skipped.
pub fn lite_watch_roots() -> Vec<(PathBuf, crate::types::VibeSource)> {
    use crate::types::VibeSource;
    let mut roots = Vec::new();
    if let Some(p) = cursor_transcripts_root() {
        roots.push((p, VibeSource::Cursor));
    }
    if let Some(p) = claude_transcripts_root() {
        roots.push((p, VibeSource::ClaudeCode));
    }
    if let Some(p) = codex_transcripts_root() {
        roots.push((p, VibeSource::Codex));
    }
    roots
}

pub fn write_port(port: u16) -> Result<()> {
    std::fs::write(port_file()?, port.to_string())?;
    Ok(())
}

pub fn read_port() -> Option<u16> {
    std::fs::read_to_string(port_file().ok()?).ok()?.trim().parse().ok()
}

pub fn hook_command_for_install() -> Result<String> {
    let bin = hook_binary_path()?;
    if cfg!(windows) {
        let cmd = hook_cmd_path()?;
        Ok(cmd.to_string_lossy().into_owned())
    } else {
        Ok(bin.to_string_lossy().into_owned())
    }
}

pub fn ensure_parent(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    Ok(())
}
