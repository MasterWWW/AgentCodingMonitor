use crate::paths::state_file;
use crate::types::{Session, StatusSnapshot, VibePhase, VibeSource};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Max gap between duplicate hook reports for the same upstream `session_id`.
const NEAR_DUPLICATE_SECS: i64 = 2;

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
    if let Some(source) = newest_session_matching(snap, default, |p| {
        matches!(p, VibePhase::Active | VibePhase::WaitingUser)
    }) {
        return source;
    }
    if let Some(source) = newest_session_matching(snap, default, |_| true) {
        return source;
    }
    if let Some(source) = newest_source_by_health(snap) {
        return source;
    }
    default
}

fn source_display_priority(source: VibeSource, default: VibeSource) -> u8 {
    if source == default {
        return 0;
    }
    match source {
        VibeSource::Cursor => 1,
        VibeSource::ClaudeCode => 2,
        VibeSource::Codex => 3,
    }
}

fn is_near_duplicate_group(sessions: &[&Session]) -> bool {
    if sessions.len() < 2 {
        return false;
    }
    let mut min = sessions[0].last_activity_at;
    let mut max = sessions[0].last_activity_at;
    for s in sessions.iter().skip(1) {
        min = min.min(s.last_activity_at);
        max = max.max(s.last_activity_at);
    }
    (max - min).num_seconds() <= NEAR_DUPLICATE_SECS
}

fn pick_source_from_session_group(sessions: &[&Session], default: VibeSource) -> VibeSource {
    debug_assert!(!sessions.is_empty());
    if is_near_duplicate_group(sessions) {
        sessions
            .iter()
            .min_by_key(|s| {
                (
                    source_display_priority(s.source, default),
                    std::cmp::Reverse(s.last_activity_at),
                )
            })
            .map(|s| s.source)
            .unwrap()
    } else {
        sessions
            .iter()
            .max_by_key(|s| s.last_activity_at)
            .map(|s| s.source)
            .unwrap()
    }
}

fn newest_session_matching(
    snap: &StatusSnapshot,
    default: VibeSource,
    pred: impl Fn(VibePhase) -> bool,
) -> Option<VibeSource> {
    let candidates: Vec<&Session> = snap.sessions.iter().filter(|s| pred(s.phase)).collect();
    if candidates.is_empty() {
        return None;
    }

    let mut groups: HashMap<&str, Vec<&Session>> = HashMap::new();
    for s in candidates {
        groups
            .entry(s.session_id.as_str())
            .or_default()
            .push(s);
    }

    let mut best: Option<(VibeSource, DateTime<Utc>)> = None;
    for sessions in groups.values() {
        let source = pick_source_from_session_group(sessions, default);
        let at = sessions
            .iter()
            .map(|s| s.last_activity_at)
            .max()
            .unwrap();
        if best.as_ref().is_none_or(|(_, t)| at > *t) {
            best = Some((source, at));
        }
    }
    best.map(|(s, _)| s)
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

    #[test]
    fn pick_duplicate_session_id_prefers_cursor_over_claude() {
        let now = Utc::now();
        let snap = StatusSnapshot {
            daemon_ok: true,
            port: 1,
            lite_mode: false,
            sources: Default::default(),
            sessions: vec![
                crate::types::Session {
                    source: VibeSource::Cursor,
                    session_id: "shared".into(),
                    cwd: None,
                    task_title: Some("same task".into()),
                    last_tool: Some("Read".into()),
                    last_activity_at: now,
                    phase: VibePhase::Active,
                },
                crate::types::Session {
                    source: VibeSource::ClaudeCode,
                    session_id: "shared".into(),
                    cwd: None,
                    task_title: Some("same task".into()),
                    last_tool: Some("Read".into()),
                    last_activity_at: now + chrono::Duration::microseconds(50),
                    phase: VibePhase::Active,
                },
            ],
        };
        assert_eq!(
            pick_display_source(&snap, VibeSource::Cursor),
            VibeSource::Cursor
        );
    }

    #[test]
    fn pick_duplicate_session_id_prefers_default_when_set() {
        let now = Utc::now();
        let snap = StatusSnapshot {
            daemon_ok: true,
            port: 1,
            lite_mode: false,
            sources: Default::default(),
            sessions: vec![
                crate::types::Session {
                    source: VibeSource::Cursor,
                    session_id: "shared".into(),
                    cwd: None,
                    task_title: None,
                    last_tool: None,
                    last_activity_at: now,
                    phase: VibePhase::Active,
                },
                crate::types::Session {
                    source: VibeSource::ClaudeCode,
                    session_id: "shared".into(),
                    cwd: None,
                    task_title: None,
                    last_tool: None,
                    last_activity_at: now + chrono::Duration::milliseconds(10),
                    phase: VibePhase::Active,
                },
            ],
        };
        assert_eq!(
            pick_display_source(&snap, VibeSource::ClaudeCode),
            VibeSource::ClaudeCode
        );
    }
}
