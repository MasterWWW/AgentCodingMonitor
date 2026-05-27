use crate::event::extract_title_from_prompt;
use crate::paths;
use crate::store::SessionStore;
use crate::types::VibeSource;
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

/// (watch root, source) — every configured root is watched; paths map by prefix, not skipped.
type WatchRoot = (PathBuf, VibeSource);

pub fn spawn_lite_watcher(store: SessionStore) {
    std::thread::spawn(move || {
        let rt = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(rt) => rt,
            Err(e) => {
                tracing::warn!("lite watcher runtime: {e}");
                return;
            }
        };
        if let Err(e) = run_watcher(store, &rt) {
            tracing::warn!("lite watcher stopped: {e}");
        }
    });
}

fn run_watcher(store: SessionStore, rt: &tokio::runtime::Runtime) -> anyhow::Result<()> {
    let watch_roots = paths::lite_watch_roots();
    let (tx, rx) = mpsc::channel();
    let mut watcher = RecommendedWatcher::new(
        move |res| {
            if let Ok(ev) = res {
                let _ = tx.send(ev);
            }
        },
        Config::default(),
    )?;

    for (root, source) in &watch_roots {
        if root.exists() {
            if let Err(e) = watcher.watch(root, RecursiveMode::Recursive) {
                tracing::warn!("lite watch {} ({source:?}): {e}", root.display());
            } else {
                tracing::info!("lite watching {} for {:?}", root.display(), source);
            }
        } else {
            tracing::debug!("lite watch root missing: {}", root.display());
        }
    }

    loop {
        match rx.recv_timeout(Duration::from_millis(500)) {
            Ok(event) => {
                let lite = rt.block_on(store.get_lite_mode());
                if !lite {
                    continue;
                }
                for path in event.paths {
                    if path.extension().and_then(|s| s.to_str()) != Some("jsonl") {
                        continue;
                    }
                    let Some(source) = source_for_watched_path(&path, &watch_roots) else {
                        continue;
                    };
                    let (title, cwd) = parse_transcript_tail(&path);
                    rt.block_on(store.apply_lite_activity(source, cwd, title));
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    Ok(())
}

/// Map a changed file to its source from the watch root prefix (Cursor / Claude / Codex).
fn source_for_watched_path(path: &Path, roots: &[WatchRoot]) -> Option<VibeSource> {
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    for (root, source) in roots {
        let root_canon = root.canonicalize().unwrap_or_else(|_| root.clone());
        if canonical.starts_with(&root_canon) {
            return Some(*source);
        }
    }
    source_for_path_fallback(path)
}

/// Fallback for paths under known home dirs when canonicalize/prefix fails.
fn source_for_path_fallback(path: &Path) -> Option<VibeSource> {
    let s = path.to_string_lossy();
    if s.contains(".cursor/") || s.contains(".cursor\\") {
        return Some(VibeSource::Cursor);
    }
    if s.contains(".claude/") || s.contains(".claude\\") {
        return Some(VibeSource::ClaudeCode);
    }
    if s.contains(".codex/") || s.contains(".codex\\") {
        return Some(VibeSource::Codex);
    }
    None
}

fn parse_transcript_tail(path: &Path) -> (Option<String>, Option<String>) {
    let Ok(content) = std::fs::read_to_string(path) else {
        return (None, None);
    };
    let Some(last) = content.lines().filter(|l| !l.trim().is_empty()).last() else {
        return (None, None);
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(last) else {
        return (None, None);
    };

    let mut title = None;
    if v.get("role").and_then(|r| r.as_str()) == Some("user") {
        if let Some(text) = v
            .get("message")
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_array())
            .and_then(|a| a.first())
            .and_then(|x| x.get("text"))
            .and_then(|t| t.as_str())
        {
            title = Some(extract_title_from_prompt(text));
        }
    }
    if title.is_none() {
        if let Some(text) = v.get("text").and_then(|t| t.as_str()) {
            title = Some(extract_title_from_prompt(text));
        }
    }
    if title.is_none() {
        if let Some(text) = v.get("content").and_then(|t| t.as_str()) {
            title = Some(extract_title_from_prompt(text));
        }
    }

    let cwd = v
        .get("cwd")
        .or_else(|| v.get("workspace"))
        .and_then(|c| c.as_str())
        .map(|s| s.to_string());
    (title, cwd)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn maps_path_under_cursor_root() {
        let p = PathBuf::from("/home/u/.cursor/projects/foo/agent-transcripts/x.jsonl");
        assert_eq!(source_for_path_fallback(&p), Some(VibeSource::Cursor));
    }

    #[test]
    fn fallback_codex_path() {
        let p = PathBuf::from("/home/u/.codex/projects/sess/log.jsonl");
        assert_eq!(
            source_for_path_fallback(&p),
            Some(VibeSource::Codex)
        );
    }
}
