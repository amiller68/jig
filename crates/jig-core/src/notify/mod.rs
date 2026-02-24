//! Notification system for human-facing alerts.
//!
//! Append-only JSONL queue at `~/.config/jig/state/notifications.jsonl`.

mod events;
mod queue;

pub use events::{Notification, NotificationEvent};
pub use queue::NotificationQueue;
