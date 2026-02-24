//! Notification system for human-facing alerts.
//!
//! Append-only JSONL queue at `~/.config/jig/state/notifications.jsonl`.

mod events;
mod hook;
mod queue;

pub use events::{Notification, NotificationEvent};
pub use hook::Notifier;
pub use queue::NotificationQueue;
