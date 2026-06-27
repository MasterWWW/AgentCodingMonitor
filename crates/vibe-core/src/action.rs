use crate::event::redact_title;
use crate::types::{NormalizedEvent, VibeSource};
use chrono::{Duration, Utc};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use uuid::Uuid;
use vibe_protocol::{
    binary_actions, clipboard_for_choice, ActionKind, ActionPromptData, CancelledAction,
    PendingAction, ResolvedAction, WatchStreamEvent,
};

const ACTION_TTL_SECS: i64 = 300;

#[derive(Debug, Clone)]
pub enum ActionExecutorEvent {
    Resolved(ResolvedAction),
}

#[derive(Debug, Clone)]
struct StoredAction {
    action: PendingAction,
    resolved: bool,
}

#[derive(Clone)]
pub struct ActionStore {
    inner: Arc<RwLock<HashMap<Uuid, StoredAction>>>,
    session_index: Arc<RwLock<HashMap<String, Uuid>>>,
    tx: broadcast::Sender<WatchStreamEvent>,
    executor_tx: broadcast::Sender<ActionExecutorEvent>,
}

impl ActionStore {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(64);
        let (executor_tx, _) = broadcast::channel(16);
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
            session_index: Arc::new(RwLock::new(HashMap::new())),
            tx,
            executor_tx,
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<WatchStreamEvent> {
        self.tx.subscribe()
    }

    pub fn subscribe_executor(&self) -> broadcast::Receiver<ActionExecutorEvent> {
        self.executor_tx.subscribe()
    }

    /// 由调用方在 `waiting_user` 且手表伴侣启用时触发。
    pub async fn maybe_create_from_event(&self, ev: &NormalizedEvent) {
        let session_id = ev
            .session_id
            .clone()
            .unwrap_or_else(|| "default".to_string());
        let session_key = format!("{}:{}", source_slug(ev.source), session_id);

        {
            let index = self.session_index.read().await;
            if let Some(existing_id) = index.get(&session_key) {
                let actions = self.inner.read().await;
                if let Some(stored) = actions.get(existing_id) {
                    if !stored.resolved && stored.action.prompt.expires_at > Utc::now() {
                        return;
                    }
                }
            }
        }

        let title = ev
            .task_title
            .clone()
            .filter(|t| !t.is_empty())
            .or_else(|| ev.tool_name.clone())
            .map(|t| redact_title(&t))
            .unwrap_or_else(|| "Agent 等待你的确认".to_string());

        let body = ev
            .tool_name
            .clone()
            .map(|t| redact_title(&t))
            .unwrap_or_default();

        let now = Utc::now();
        let action = PendingAction {
            id: Uuid::new_v4(),
            created_at: now,
            prompt: ActionPromptData {
                source: source_slug(ev.source),
                session_id: session_id.clone(),
                phase: "waiting_user".to_string(),
                title,
                body,
                actions: binary_actions(),
                expires_at: now + Duration::seconds(ACTION_TTL_SECS),
                kind: Some(ActionKind::BinaryChoice),
            },
        };

        self.insert_action(session_key, action).await;
    }

    async fn insert_action(&self, session_key: String, action: PendingAction) {
        let id = action.id;
        {
            let mut actions = self.inner.write().await;
            actions.insert(
                id,
                StoredAction {
                    action: action.clone(),
                    resolved: false,
                },
            );
        }
        {
            let mut index = self.session_index.write().await;
            index.insert(session_key, id);
        }
        let _ = self.tx.send(WatchStreamEvent::ActionCreated { data: action });
    }

    pub async fn pending(&self) -> Vec<PendingAction> {
        self.purge_expired().await;
        let actions = self.inner.read().await;
        actions
            .values()
            .filter(|s| !s.resolved && s.action.prompt.expires_at > Utc::now())
            .map(|s| s.action.clone())
            .collect()
    }

    pub async fn respond(
        &self,
        id: Uuid,
        choice: String,
        from: vibe_protocol::ResponseOrigin,
    ) -> Result<ResolvedAction, ActionError> {
        self.purge_expired().await;
        let mut actions = self.inner.write().await;
        let Some(stored) = actions.get_mut(&id) else {
            return Err(ActionError::NotFound);
        };
        if stored.resolved {
            return Err(ActionError::AlreadyResolved);
        }
        if stored.action.prompt.expires_at <= Utc::now() {
            return Err(ActionError::Expired);
        }
        let valid = stored
            .action
            .prompt
            .actions
            .iter()
            .any(|a| a.id == choice);
        if !valid {
            return Err(ActionError::InvalidChoice);
        }

        stored.resolved = true;
        let clipboard_text = clipboard_for_choice(&choice).map(str::to_string);
        let resolved = ResolvedAction {
            id,
            choice: choice.clone(),
            clipboard_text: clipboard_text.clone(),
            title: stored.action.prompt.title.clone(),
            source: stored.action.prompt.source.clone(),
        };
        drop(actions);

        let _ = self.tx.send(WatchStreamEvent::ActionResolved {
            data: resolved.clone(),
        });
        let _ = self
            .executor_tx
            .send(ActionExecutorEvent::Resolved(resolved.clone()));

        let _ = from;
        Ok(resolved)
    }

    pub async fn tick(&self) {
        let expired = self.purge_expired().await;
        for id in expired {
            let _ = self.tx.send(WatchStreamEvent::ActionCancelled {
                data: CancelledAction {
                    id,
                    reason: "expired".to_string(),
                },
            });
        }
    }

    async fn purge_expired(&self) -> Vec<Uuid> {
        let now = Utc::now();
        let mut expired = Vec::new();
        let mut actions = self.inner.write().await;
        for (id, stored) in actions.iter_mut() {
            if !stored.resolved && stored.action.prompt.expires_at <= now {
                stored.resolved = true;
                expired.push(*id);
            }
        }
        expired
    }
}

fn source_slug(source: VibeSource) -> String {
    match source {
        VibeSource::Cursor => "cursor".to_string(),
        VibeSource::ClaudeCode => "claude_code".to_string(),
        VibeSource::Codex => "codex".to_string(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionError {
    NotFound,
    AlreadyResolved,
    Expired,
    InvalidChoice,
}

impl ActionError {
    pub fn status_code(&self) -> axum::http::StatusCode {
        use axum::http::StatusCode;
        match self {
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::AlreadyResolved => StatusCode::CONFLICT,
            Self::Expired => StatusCode::GONE,
            Self::InvalidChoice => StatusCode::BAD_REQUEST,
        }
    }

    pub fn message(&self) -> &'static str {
        match self {
            Self::NotFound => "action_not_found",
            Self::AlreadyResolved => "action_already_resolved",
            Self::Expired => "action_expired",
            Self::InvalidChoice => "invalid_choice",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::normalize_raw;
    use serde_json::json;

    #[tokio::test]
    async fn creates_action_on_waiting_user_event() {
        let store = ActionStore::new();
        let raw = json!({
            "hook_event_name": "afterAgentResponse",
            "session_id": "sess-1",
            "tool_name": "Shell"
        });
        let ev = normalize_raw(VibeSource::Cursor, &raw);
        store.maybe_create_from_event(&ev).await;
        let pending = store.pending().await;
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].prompt.actions.len(), 2);
    }

    #[tokio::test]
    async fn respond_sets_clipboard_hint() {
        let store = ActionStore::new();
        let raw = json!({
            "hook_event_name": "afterAgentResponse",
            "session_id": "sess-2"
        });
        let ev = normalize_raw(VibeSource::Cursor, &raw);
        store.maybe_create_from_event(&ev).await;
        let id = store.pending().await[0].id;
        let resolved = store
            .respond(id, "approve".to_string(), vibe_protocol::ResponseOrigin::Watch)
            .await
            .unwrap();
        assert_eq!(resolved.clipboard_text.as_deref(), Some("y"));
    }
}
