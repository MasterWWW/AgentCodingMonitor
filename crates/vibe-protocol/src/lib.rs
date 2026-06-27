//! Cross-platform message types for watch companion (phone ↔ watch ↔ PC).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Envelope `type` field values.
pub mod envelope_type {
    pub const ACTION_PROMPT: &str = "action_prompt";
    pub const ACTION_RESPONSE: &str = "action_response";
    pub const ACTION_CANCELLED: &str = "action_cancelled";
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ActionStyle {
    Primary,
    Destructive,
    Default,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ActionButton {
    pub id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<ActionStyle>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ActionKind {
    BinaryChoice,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionPromptData {
    pub source: String,
    pub session_id: String,
    pub phase: String,
    pub title: String,
    pub body: String,
    pub actions: Vec<ActionButton>,
    pub expires_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<ActionKind>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingAction {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    #[serde(flatten)]
    pub prompt: ActionPromptData,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResponseOrigin {
    Watch,
    Phone,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionResponseBody {
    pub action_id: Uuid,
    pub choice: String,
    pub from: ResponseOrigin,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Envelope<T> {
    #[serde(rename = "type")]
    pub envelope_type: String,
    pub id: Uuid,
    pub ts: i64,
    pub data: T,
}

impl<T: Serialize> Envelope<T> {
    pub fn new(envelope_type: &str, data: T) -> Self {
        let now = Utc::now();
        Self {
            envelope_type: envelope_type.to_string(),
            id: Uuid::new_v4(),
            ts: now.timestamp(),
            data,
        }
    }
}

/// Default clipboard text for binary yes/no style choices.
pub fn clipboard_for_choice(choice: &str) -> Option<&'static str> {
    match choice {
        "approve" => Some("y"),
        "deny" => Some("n"),
        _ => None,
    }
}

/// Build standard allow/deny buttons for watch UI.
pub fn binary_actions() -> Vec<ActionButton> {
    vec![
        ActionButton {
            id: "approve".to_string(),
            label: "允许".to_string(),
            style: Some(ActionStyle::Primary),
        },
        ActionButton {
            id: "deny".to_string(),
            label: "拒绝".to_string(),
            style: Some(ActionStyle::Destructive),
        },
    ]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum WatchStreamEvent {
    Status { data: serde_json::Value },
    ActionCreated { data: PendingAction },
    ActionResolved {
        data: ResolvedAction,
    },
    ActionCancelled {
        data: CancelledAction,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedAction {
    pub id: Uuid,
    pub choice: String,
    pub clipboard_text: Option<String>,
    pub title: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelledAction {
    pub id: Uuid,
    pub reason: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipboard_mapping() {
        assert_eq!(clipboard_for_choice("approve"), Some("y"));
        assert_eq!(clipboard_for_choice("deny"), Some("n"));
        assert_eq!(clipboard_for_choice("other"), None);
    }
}
