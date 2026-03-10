//! Daemon lifecycle log — records start/stop events to detect crashes.
//!
//! Stored as JSONL at `~/.config/jig/state/daemon.jsonl`.

use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::global::daemon_log_path;

/// A daemon lifecycle event.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DaemonEvent {
    Started { ts: i64, pid: u32 },
    Stopped { ts: i64, pid: u32, reason: String },
}

/// Append a lifecycle event to the daemon log.
pub fn log_event(event: &DaemonEvent) -> Result<()> {
    let path = daemon_log_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new().create(true).append(true).open(&path)?;
    let line = serde_json::to_string(event)?;
    writeln!(file, "{}", line)?;
    Ok(())
}

/// Read the last lifecycle event from the daemon log.
pub fn last_event() -> Result<Option<DaemonEvent>> {
    let path = daemon_log_path()?;
    if !path.exists() {
        return Ok(None);
    }
    let file = fs::File::open(&path)?;
    let reader = BufReader::new(file);
    let mut last_line = None;
    for line in reader.lines() {
        let line = line?;
        if !line.is_empty() {
            last_line = Some(line);
        }
    }
    match last_line {
        Some(line) => Ok(Some(serde_json::from_str(&line)?)),
        None => Ok(None),
    }
}

/// Check if the previous daemon run crashed (last event is not Stopped).
pub fn was_unclean_shutdown() -> bool {
    match last_event() {
        Ok(Some(DaemonEvent::Stopped { .. })) => false,
        Ok(None) => false, // No previous run
        _ => true,         // Started without Stopped, or read error
    }
}

/// Record daemon start.
pub fn record_start() -> Result<()> {
    let event = DaemonEvent::Started {
        ts: chrono::Utc::now().timestamp(),
        pid: std::process::id(),
    };
    log_event(&event)
}

/// Record daemon stop.
pub fn record_stop(reason: &str) -> Result<()> {
    let event = DaemonEvent::Stopped {
        ts: chrono::Utc::now().timestamp(),
        pid: std::process::id(),
        reason: reason.to_string(),
    };
    log_event(&event)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn started_stopped_roundtrip() {
        let started = DaemonEvent::Started { ts: 1000, pid: 42 };
        let json = serde_json::to_string(&started).unwrap();
        let parsed: DaemonEvent = serde_json::from_str(&json).unwrap();
        assert!(matches!(parsed, DaemonEvent::Started { ts: 1000, pid: 42 }));
    }

    #[test]
    fn stopped_roundtrip() {
        let stopped = DaemonEvent::Stopped {
            ts: 2000,
            pid: 42,
            reason: "signal".to_string(),
        };
        let json = serde_json::to_string(&stopped).unwrap();
        let parsed: DaemonEvent = serde_json::from_str(&json).unwrap();
        assert!(matches!(parsed, DaemonEvent::Stopped { ts: 2000, .. }));
    }
}
