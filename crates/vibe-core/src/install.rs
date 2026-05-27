use crate::paths::{
    self, claude_settings_path, codex_config_path, codex_hooks_path, cursor_hooks_path,
    ensure_parent, hook_binary_path, hook_command_for_install,
};
#[cfg(windows)]
use crate::paths::hook_cmd_path;
use crate::store::SessionStore;
use crate::types::{DoctorReport, InstallHooksResult, VibeSource};
use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

const VIBE_MARKER: &str = "vibe-monitor";

pub fn install_hooks(hook_binary_src: Option<&Path>, search_hints: &[PathBuf]) -> InstallHooksResult {
    let mut messages = Vec::new();
    match install_hook_binary(hook_binary_src, search_hints) {
        Ok(p) => messages.push(format!("Installed hook binary at {p}")),
        Err(e) => {
            return InstallHooksResult {
                ok: false,
                hook_path: String::new(),
                messages: vec![format!("Failed to install hook binary: {e}")],
            };
        }
    }

    let cmd = match hook_command_for_install() {
        Ok(c) => c,
        Err(e) => {
            return InstallHooksResult {
                ok: false,
                hook_path: String::new(),
                messages: vec![format!("Hook command path: {e}")],
            };
        }
    };

    let mut hooks_ok = true;

    if let Err(e) = install_cursor_hooks(&cmd) {
        hooks_ok = false;
        messages.push(format!("Cursor hooks: failed ({e})"));
    } else {
        messages.push("Cursor hooks: installed".to_string());
    }

    if let Err(e) = install_claude_hooks(&cmd) {
        hooks_ok = false;
        messages.push(format!("Claude Code hooks: failed ({e})"));
    } else {
        messages.push("Claude Code hooks: installed".to_string());
    }

    if let Err(e) = install_codex_hooks(&cmd) {
        hooks_ok = false;
        messages.push(format!("Codex hooks: failed ({e})"));
    } else {
        messages.push("Codex hooks: installed".to_string());
    }

    if let Err(e) = ensure_codex_feature() {
        messages.push(format!("Codex config: {e}"));
    } else {
        messages.push("Codex config: codex_hooks feature enabled".to_string());
    }

    let hook_path = hook_binary_path()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();

    messages.push(
        "请完全退出并重新打开 Cursor、Claude Code、Codex 后 hook 才会生效".to_string(),
    );

    InstallHooksResult {
        ok: hooks_ok,
        hook_path,
        messages,
    }
}

/// Sync in-memory hook_installed flags from on-disk hook configs.
pub async fn sync_hook_health_from_disk(store: &SessionStore) {
    let cursor = hook_config_has_vibe(&cursor_hooks_path());
    let claude = claude_has_vibe();
    let codex = hook_config_has_vibe(&codex_hooks_path());
    store
        .mark_hook_installed(VibeSource::Cursor, cursor)
        .await;
    store
        .mark_hook_installed(VibeSource::ClaudeCode, claude)
        .await;
    store.mark_hook_installed(VibeSource::Codex, codex).await;
}

fn install_hook_binary(src: Option<&Path>, search_hints: &[PathBuf]) -> Result<String> {
    let dest = hook_binary_path()?;
    ensure_parent(&dest)?;

    let discovered = paths::discover_hook_binary(search_hints);
    let resolved = src
        .filter(|p| p.is_file())
        .or(discovered.as_deref())
        .with_context(|| {
            format!(
                "vibe-hook binary not found. From the repo root run:\n  cargo build -p vibe-hook\nThen click 修复 or 启用监听 again."
            )
        })?;

    std::fs::copy(&resolved, &dest)?;

    #[cfg(windows)]
    write_hook_cmd(&dest)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&dest)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&dest, perms)?;
    }

    Ok(dest.to_string_lossy().into_owned())
}

#[cfg(windows)]
fn write_hook_cmd(exe: &Path) -> Result<()> {
    let cmd_path = hook_cmd_path()?;
    ensure_parent(&cmd_path)?;
    let content = format!(
        "@echo off\r\n\"{}\" %*\r\n",
        exe.to_string_lossy().replace('"', "\"\"")
    );
    std::fs::write(&cmd_path, content)?;
    Ok(())
}

/// Shell-safe hook command: paths on macOS often contain spaces (`Application Support`).
fn hook_command(cmd: &str, source_flag: &str) -> String {
    let escaped = cmd.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\" --source {source_flag}")
}

fn hook_entry(cmd: &str, event: &str, source_flag: &str) -> Value {
    json!({
        "command": hook_command(cmd, source_flag),
        "metadata": { "source": VIBE_MARKER, "event": event }
    })
}

fn install_cursor_hooks(cmd: &str) -> Result<()> {
    let path = cursor_hooks_path();
    ensure_parent(&path)?;
    let mut root = read_json_file(&path).unwrap_or(json!({ "version": 1, "hooks": {} }));
    if root.get("version").is_none() {
        root["version"] = json!(1);
    }
    let hooks = root
        .as_object_mut()
        .and_then(|o| o.get_mut("hooks"))
        .and_then(|h| h.as_object_mut())
        .context("invalid cursor hooks.json")?;

    for event in [
        "sessionStart",
        "beforeSubmitPrompt",
        "preToolUse",
        "postToolUse",
        "stop",
    ] {
        upsert_hook_list(hooks, event, hook_entry(cmd, event, "cursor"));
    }
    write_json_file(&path, &root)?;
    Ok(())
}

fn install_claude_hooks(cmd: &str) -> Result<()> {
    let path = claude_settings_path();
    ensure_parent(&path)?;
    let mut root = read_json_file(&path).unwrap_or(json!({}));
    let hooks = root
        .as_object_mut()
        .and_then(|o| {
            if !o.contains_key("hooks") {
                o.insert("hooks".to_string(), json!({}));
            }
            o.get_mut("hooks")
        })
        .and_then(|h| h.as_object_mut())
        .context("invalid claude settings.json")?;

    for event in [
        "SessionStart",
        "UserPromptSubmit",
        "PreToolUse",
        "PostToolUse",
        "Stop",
    ] {
        upsert_claude_hook(hooks, event, cmd, event);
    }
    write_json_file(&path, &root)?;
    Ok(())
}

fn upsert_claude_hook(hooks: &mut serde_json::Map<String, Value>, event: &str, cmd: &str, tag: &str) {
    let command = hook_command(cmd, "claude");
    let entry = json!([{
        "matcher": "*",
        "hooks": [{
            "type": "command",
            "command": command,
            "metadata": { "source": VIBE_MARKER, "event": tag }
        }]
    }]);
    let list = hooks.entry(event.to_string()).or_insert_with(|| json!([]));
    if let Some(arr) = list.as_array_mut() {
        arr.retain(|item| !claude_item_is_vibe(item));
        arr.push(entry[0].clone());
    } else {
        *list = entry;
    }
}

fn claude_item_is_vibe(item: &Value) -> bool {
    item.get("hooks")
        .and_then(|h| h.as_array())
        .map(|arr| {
            arr.iter().any(|x| {
                x.get("metadata")
                    .and_then(|m| m.get("source"))
                    .and_then(|s| s.as_str())
                    == Some(VIBE_MARKER)
            })
        })
        .unwrap_or(false)
}

fn install_codex_hooks(cmd: &str) -> Result<()> {
    let path = codex_hooks_path();
    ensure_parent(&path)?;
    let mut root = read_json_file(&path).unwrap_or(json!({ "hooks": {} }));
    let hooks = root
        .as_object_mut()
        .and_then(|o| {
            if !o.contains_key("hooks") {
                o.insert("hooks".to_string(), json!({}));
            }
            o.get_mut("hooks")
        })
        .and_then(|h| h.as_object_mut())
        .context("invalid codex hooks.json")?;

    for event in [
        "SessionStart",
        "UserPromptSubmit",
        "PreToolUse",
        "PostToolUse",
        "Stop",
    ] {
        upsert_codex_hook(hooks, event, cmd, event);
    }
    write_json_file(&path, &root)?;
    Ok(())
}

fn upsert_codex_hook(hooks: &mut serde_json::Map<String, Value>, event: &str, cmd: &str, tag: &str) {
    upsert_claude_hook(hooks, event, cmd, tag);
    // Re-use structure; fix command for codex
    if let Some(arr) = hooks.get_mut(event).and_then(|v| v.as_array_mut()) {
        for item in arr.iter_mut() {
            if claude_item_is_vibe(item) {
                if let Some(hooks_inner) = item.get_mut("hooks").and_then(|h| h.as_array_mut()) {
                    for h in hooks_inner.iter_mut() {
                        if h.get("metadata")
                            .and_then(|m| m.get("source"))
                            .and_then(|s| s.as_str())
                            == Some(VIBE_MARKER)
                        {
                            h["command"] = json!(hook_command(cmd, "codex"));
                        }
                    }
                }
            }
        }
    }
}

fn upsert_hook_list(hooks: &mut serde_json::Map<String, Value>, event: &str, entry: Value) {
    let list = hooks
        .entry(event.to_string())
        .or_insert_with(|| json!([]));
    if let Some(arr) = list.as_array_mut() {
        arr.retain(|item| !is_vibe_entry(item));
        arr.push(entry);
    } else {
        *list = json!([entry]);
    }
}

fn is_vibe_entry(item: &Value) -> bool {
    item.get("metadata")
        .and_then(|m| m.get("source"))
        .and_then(|s| s.as_str())
        == Some(VIBE_MARKER)
}

fn read_json_file(path: &Path) -> Option<Value> {
    let data = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&data).ok()
}

fn write_json_file(path: &Path, value: &Value) -> Result<()> {
    ensure_parent(path)?;
    let pretty = serde_json::to_string_pretty(value)?;
    std::fs::write(path, pretty)?;
    Ok(())
}

fn ensure_codex_feature() -> Result<()> {
    let path = codex_config_path();
    ensure_parent(&path)?;
    let content = if path.exists() {
        std::fs::read_to_string(&path)?
    } else {
        String::new()
    };
    if content.contains("codex_hooks") {
        return Ok(());
    }
    let mut new = content;
    if !new.ends_with('\n') && !new.is_empty() {
        new.push('\n');
    }
    new.push_str("\n[features]\ncodex_hooks = true\n");
    std::fs::write(&path, new)?;
    Ok(())
}

pub async fn doctor(hook_binary_src: Option<&Path>) -> DoctorReport {
    let mut messages = Vec::new();
    let port = paths::read_port().unwrap_or(17392);
    let hook_binary_installed = hook_binary_path()
        .map(|p| p.exists())
        .unwrap_or(false);
    let cursor_hook = hook_config_has_vibe(&cursor_hooks_path());
    let claude_hook = claude_has_vibe();
    let codex_hook = hook_config_has_vibe(&codex_hooks_path());
    let codex_hooks_feature = codex_feature_enabled();

    if !hook_binary_installed {
        messages.push(
            "vibe-hook binary not installed in the app data directory (see README)".to_string(),
        );
    }
    if !cursor_hook {
        messages.push("Cursor hooks not configured".to_string());
    }
    if !claude_hook {
        messages.push("Claude Code hooks not configured".to_string());
    }
    if !codex_hook {
        messages.push("Codex hooks not configured".to_string());
    }
    if codex_hooks_feature == Some(false) {
        messages.push("Codex codex_hooks feature not enabled in config.toml".to_string());
    }
    if cfg!(windows) {
        messages.push("Codex hooks may be limited on Windows".to_string());
    }
    let _ = hook_binary_src;

    DoctorReport {
        daemon_ok: true,
        port,
        hook_binary_installed,
        cursor_hook,
        claude_hook,
        codex_hook,
        codex_hooks_feature,
        lite_mode: false,
        messages,
    }
}

fn hook_config_has_vibe(path: &Path) -> bool {
    let Some(v) = read_json_file(path) else {
        return false;
    };
    let s = v.to_string();
    s.contains(VIBE_MARKER)
}

fn claude_has_vibe() -> bool {
    hook_config_has_vibe(&claude_settings_path())
}

fn codex_feature_enabled() -> Option<bool> {
    let path = codex_config_path();
    let content = std::fs::read_to_string(path).ok()?;
    if content.contains("codex_hooks") && content.contains("true") {
        Some(true)
    } else if content.contains("codex_hooks") {
        Some(false)
    } else {
        Some(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hook_command_quotes_paths_with_spaces() {
        let cmd =
            "/Users/me/Library/Application Support/com.VibeMonitor.vibe-monitor/bin/vibe-hook";
        let out = hook_command(cmd, "cursor");
        assert_eq!(
            out,
            "\"/Users/me/Library/Application Support/com.VibeMonitor.vibe-monitor/bin/vibe-hook\" --source cursor"
        );
    }
}
