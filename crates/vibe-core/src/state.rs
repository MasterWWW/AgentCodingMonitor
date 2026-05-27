use crate::paths::state_file;
use crate::types::{StatusSnapshot, VibePhase, VibeSource};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum HudPresentation {
    #[default]
    Float,
    MenuBar,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PersistedState {
    #[serde(default)]
    pub lite_mode: Option<bool>,
    #[serde(default)]
    pub default_source: Option<VibeSource>,
    #[serde(default)]
    pub presentation: Option<HudPresentation>,
}

/// Default lite mode: on for macOS (transcript fallback), off elsewhere.
pub fn default_lite_mode() -> bool {
    cfg!(target_os = "macos")
}

pub fn default_default_source() -> VibeSource {
    VibeSource::Cursor
}

pub fn load_lite_mode() -> bool {
    load_state()
        .lite_mode
        .unwrap_or_else(default_lite_mode)
}

pub fn load_default_source() -> VibeSource {
    load_state()
        .default_source
        .unwrap_or_else(default_default_source)
}

pub fn write_lite_mode(enabled: bool) -> anyhow::Result<()> {
    let mut s = load_state();
    s.lite_mode = Some(enabled);
    write_state(&s)
}

pub fn write_default_source(source: VibeSource) -> anyhow::Result<()> {
    let mut s = load_state();
    s.default_source = Some(source);
    write_state(&s)
}

pub fn default_presentation() -> HudPresentation {
    if cfg!(target_os = "macos") {
        HudPresentation::Float
    } else {
        HudPresentation::MenuBar
    }
}

pub fn load_presentation() -> HudPresentation {
    load_state()
        .presentation
        .unwrap_or_else(default_presentation)
}

pub fn write_presentation(mode: HudPresentation) -> anyhow::Result<()> {
    let mut s = load_state();
    s.presentation = Some(mode);
    write_state(&s)
}

/// HUD display: latest in-progress session, else latest session by activity, else health `last_seen`, else default.
pub fn pick_display_source(snap: &StatusSnapshot, default: VibeSource) -> VibeSource {
    if let Some(source) = newest_session_matching(snap, |p| {
        matches!(p, VibePhase::Active | VibePhase::WaitingUser)
    }) {
        return source;
    }
    if let Some(source) = newest_session_matching(snap, |_| true) {
        return source;
    }
    if let Some(source) = newest_source_by_health(snap) {
        return source;
    }
    default
}

fn newest_session_matching(
    snap: &StatusSnapshot,
    pred: impl Fn(VibePhase) -> bool,
) -> Option<VibeSource> {
    snap.sessions
        .iter()
        .filter(|s| pred(s.phase))
        .max_by_key(|s| s.last_activity_at)
        .map(|s| s.source)
}

fn newest_source_by_health(snap: &StatusSnapshot) -> Option<VibeSource> {
    let mut best: Option<(VibeSource, chrono::DateTime<chrono::Utc>)> = None;
    for source in VibeSource::all() {
        let Some(health) = snap.sources.get(&source) else {
            continue;
        };
        let Some(last) = health.last_seen else {
            continue;
        };
        if best.as_ref().is_none_or(|(_, t)| last > *t) {
            best = Some((source, last));
        }
    }
    best.map(|(s, _)| s)
}

fn load_state() -> PersistedState {
    let path = match state_file() {
        Ok(p) => p,
        Err(_) => return PersistedState::default(),
    };
    let data = match std::fs::read_to_string(&path) {
        Ok(d) => d,
        Err(_) => return PersistedState::default(),
    };
    serde_json::from_str(&data).unwrap_or_default()
}

fn write_state(state: &PersistedState) -> anyhow::Result<()> {
    let path = state_file()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let pretty = serde_json::to_string_pretty(state)?;
    std::fs::write(path, pretty)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn pick_prefers_most_recent_in_progress() {
        let now = Utc::now();
        let snap = StatusSnapshot {
            daemon_ok: true,
            port: 1,
            lite_mode: false,
            sources: Default::default(),
            sessions: vec![
                crate::types::Session {
                    source: VibeSource::Cursor,
                    session_id: "c".into(),
                    cwd: None,
                    task_title: None,
                    last_tool: None,
                    last_activity_at: now - chrono::Duration::seconds(60),
                    phase: VibePhase::Active,
                },
                crate::types::Session {
                    source: VibeSource::Codex,
                    session_id: "x".into(),
                    cwd: None,
                    task_title: None,
                    last_tool: None,
                    last_activity_at: now,
                    phase: VibePhase::WaitingUser,
                },
            ],
        };
        assert_eq!(
            pick_display_source(&snap, VibeSource::Cursor),
            VibeSource::Codex
        );
    }

    #[test]
    fn pick_recent_stopped_over_default() {
        let now = Utc::now();
        let snap = StatusSnapshot {
            daemon_ok: true,
            port: 1,
            lite_mode: false,
            sources: Default::default(),
            sessions: vec![crate::types::Session {
                source: VibeSource::Cursor,
                session_id: "c".into(),
                cwd: None,
                task_title: None,
                last_tool: None,
                last_activity_at: now,
                phase: VibePhase::Stopped,
            }],
        };
        assert_eq!(
            pick_display_source(&snap, VibeSource::ClaudeCode),
            VibeSource::Cursor
        );
    }

    #[test]
    fn pick_falls_back_to_default_when_no_activity() {
        let snap = StatusSnapshot {
            daemon_ok: true,
            port: 1,
            lite_mode: false,
            sources: Default::default(),
            sessions: vec![],
        };
        assert_eq!(
            pick_display_source(&snap, VibeSource::ClaudeCode),
            VibeSource::ClaudeCode
        );
    }
}
