//! Notification system for human-facing alerts.
//!
//! Append-only JSONL queue at `~/.config/jig/state/notifications.jsonl`.

mod events;
mod hook;
mod queue;

#[derive(Debug, thiserror::Error)]
pub enum NotifyError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Hook(String),
}

pub use events::{Notification, NotificationEvent};
pub use hook::Notifier;
pub use queue::NotificationQueue;
