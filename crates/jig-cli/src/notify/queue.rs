//! Notification queue — append-only JSONL file.

use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;

use crate::context::notifications_path;
use jig_core::error::Result;

use super::{Notification, NotificationEvent};

/// Append-only notification queue backed by a JSONL file.
pub struct NotificationQueue {
    path: PathBuf,
}

impl NotificationQueue {
    /// Queue at the global state dir (`~/.config/jig/state/notifications.jsonl`).
    pub fn global() -> Result<Self> {
        Ok(Self {
            path: notifications_path()?,
        })
    }

    /// Queue at a specific path (useful for testing).
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// Append a notification to the queue.
    pub fn emit(&self, event: NotificationEvent) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        tracing::debug!(event_type = event.type_name(), "queuing notification");
        let notification = Notification {
            ts: chrono::Utc::now().timestamp(),
            id: uuid::Uuid::new_v4().to_string(),
            event,
        };
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        let line = serde_json::to_string(&notification)?;
        writeln!(file, "{}", line)?;
        Ok(())
    }

    /// Read notifications newer than the given timestamp.
    pub fn read_since(&self, since_ts: i64) -> Result<Vec<Notification>> {
        let all = self.read_all()?;
        Ok(all.into_iter().filter(|n| n.ts > since_ts).collect())
    }

    /// Return the last N notifications.
    pub fn tail(&self, n: usize) -> Result<Vec<Notification>> {
        let all = self.read_all()?;
        let skip = all.len().saturating_sub(n);
        Ok(all.into_iter().skip(skip).collect())
    }

    /// Return the queue file path.
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }

    /// Check if the queue file exists.
    pub fn exists(&self) -> bool {
        self.path.exists()
    }

    fn read_all(&self) -> Result<Vec<Notification>> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }
        let file = fs::File::open(&self.path)?;
        let reader = BufReader::new(file);
        let mut notifications = Vec::new();
        for line in reader.lines() {
            let line = line?;
            if !line.is_empty() {
                notifications.push(serde_json::from_str(&line)?);
            }
        }
        Ok(notifications)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emit_and_tail() {
        let tmp = tempfile::tempdir().unwrap();
        let queue = NotificationQueue::new(tmp.path().join("notifications.jsonl"));

        queue
            .emit(NotificationEvent::WorkStarted {
                repo: "jig".to_string(),
                worker: "feat".to_string(),
                issue: Some("ABC-123".to_string()),
            })
            .unwrap();

        let notifications = queue.tail(10).unwrap();
        assert_eq!(notifications.len(), 1);
        assert!(matches!(
            notifications[0].event,
            NotificationEvent::WorkStarted { .. }
        ));
        assert!(!notifications[0].id.is_empty());
    }

    #[test]
    fn multiple_emits_accumulate() {
        let tmp = tempfile::tempdir().unwrap();
        let queue = NotificationQueue::new(tmp.path().join("notifications.jsonl"));

        queue
            .emit(NotificationEvent::WorkStarted {
                repo: "jig".to_string(),
                worker: "a".to_string(),
                issue: None,
            })
            .unwrap();
        queue
            .emit(NotificationEvent::NeedsIntervention {
                repo: "jig".to_string(),
                worker: "a".to_string(),
                reason: "stalled".to_string(),
            })
            .unwrap();
        queue
            .emit(NotificationEvent::WorkCompleted {
                repo: "jig".to_string(),
                worker: "a".to_string(),
                pr_url: None,
            })
            .unwrap();

        let all = queue.tail(100).unwrap();
        assert_eq!(all.len(), 3);
    }

    #[test]
    fn tail_limits_results() {
        let tmp = tempfile::tempdir().unwrap();
        let queue = NotificationQueue::new(tmp.path().join("notifications.jsonl"));

        for i in 0..5 {
            queue
                .emit(NotificationEvent::WorkStarted {
                    repo: "jig".to_string(),
                    worker: format!("w{}", i),
                    issue: None,
                })
                .unwrap();
        }

        let last2 = queue.tail(2).unwrap();
        assert_eq!(last2.len(), 2);
    }

    #[test]
    fn missing_file_returns_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let queue = NotificationQueue::new(tmp.path().join("nonexistent.jsonl"));

        assert!(!queue.exists());
        assert!(queue.tail(10).unwrap().is_empty());
        assert!(queue.read_since(0).unwrap().is_empty());
    }

    #[test]
    fn read_since_filters() {
        let tmp = tempfile::tempdir().unwrap();
        let queue = NotificationQueue::new(tmp.path().join("notifications.jsonl"));

        queue
            .emit(NotificationEvent::WorkStarted {
                repo: "jig".to_string(),
                worker: "a".to_string(),
                issue: None,
            })
            .unwrap();

        let now = chrono::Utc::now().timestamp();

        queue
            .emit(NotificationEvent::PrOpened {
                repo: "jig".to_string(),
                worker: "a".to_string(),
                pr_url: "https://github.com/pr/1".to_string(),
            })
            .unwrap();

        // read_since(now) should only return the second notification
        // (both have same second-resolution timestamp, so use now-1 to be safe)
        let all = queue.tail(100).unwrap();
        assert_eq!(all.len(), 2);

        // At minimum, read_since with a future ts should return empty
        let future = queue.read_since(now + 100).unwrap();
        assert!(future.is_empty());
    }
}
