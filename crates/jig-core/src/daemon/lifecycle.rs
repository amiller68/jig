//! Daemon lifecycle log — records Started/Stopped events in a JSONL file.
//!
//! Stored at `~/.config/jig/state/daemon.jsonl`.

use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;

use crate::error::Result;
use crate::global::paths::daemon_log_path;

/// A daemon lifecycle event.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event")]
pub enum DaemonEvent {
    Started { ts: i64, pid: u32 },
    Stopped { ts: i64, pid: u32, reason: String },
}

impl DaemonEvent {
    pub fn started() -> Self {
        Self::Started {
            ts: chrono::Utc::now().timestamp(),
            pid: std::process::id(),
        }
    }

    pub fn stopped(reason: &str) -> Self {
        Self::Stopped {
            ts: chrono::Utc::now().timestamp(),
            pid: std::process::id(),
            reason: reason.to_string(),
        }
    }
}

/// Append a lifecycle event to the daemon log.
pub fn append_event(event: &DaemonEvent) -> Result<()> {
    let path = daemon_log_path()?;
    append_event_to(&path, event)
}

/// Append a lifecycle event to a specific file path.
fn append_event_to(path: &std::path::Path, event: &DaemonEvent) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    let line = serde_json::to_string(event)?;
    writeln!(file, "{}", line)?;
    Ok(())
}

/// Read the last event from the daemon log.
pub fn last_event() -> Result<Option<DaemonEvent>> {
    let path = daemon_log_path()?;
    last_event_from(&path)
}

/// Read the last event from a specific file path.
fn last_event_from(path: &std::path::Path) -> Result<Option<DaemonEvent>> {
    if !path.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(path)?;
    let last_line = content.lines().rev().find(|l| !l.trim().is_empty());
    match last_line {
        Some(line) => {
            let event: DaemonEvent = serde_json::from_str(line)?;
            Ok(Some(event))
        }
        None => Ok(None),
    }
}

/// Check if the previous daemon run crashed (last event is not Stopped).
pub fn previous_run_crashed() -> Result<bool> {
    match last_event()? {
        Some(DaemonEvent::Started { .. }) => Ok(true),
        _ => Ok(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_events() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("daemon.jsonl");

        let started = DaemonEvent::started();
        append_event_to(&path, &started).unwrap();

        let last = last_event_from(&path).unwrap().unwrap();
        assert!(matches!(last, DaemonEvent::Started { .. }));

        let stopped = DaemonEvent::stopped("signal");
        append_event_to(&path, &stopped).unwrap();

        let last = last_event_from(&path).unwrap().unwrap();
        assert!(matches!(last, DaemonEvent::Stopped { .. }));
    }

    #[test]
    fn empty_log_not_crashed() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("daemon.jsonl");

        assert!(last_event_from(&path).unwrap().is_none());
    }
}
