//! Generic append-only JSONL event log.

use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;

use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::error::Result;

/// An event type that knows how to fold into a state.
///
/// Implement on your event enum, then call [`EventLog::reduce()`] to
/// replay the log into a fresh `State`.
pub trait Reducible {
    type State: Default;
    fn apply(state: &mut Self::State, event: &Self);
}

/// Append-only JSONL event log, generic over event type.
pub struct EventLog<E> {
    pub path: PathBuf,
    _phantom: std::marker::PhantomData<E>,
}

impl<E> EventLog<E> {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            _phantom: std::marker::PhantomData,
        }
    }

    pub fn exists(&self) -> bool {
        self.path.exists()
    }

    pub fn reset(&self) -> Result<()> {
        if self.path.exists() {
            fs::write(&self.path, "")?;
        }
        Ok(())
    }

    pub fn remove(&self) -> Result<()> {
        if self.path.exists() {
            fs::remove_file(&self.path)?;
        }
        if let Some(parent) = self.path.parent() {
            let _ = fs::remove_dir(parent);
        }
        Ok(())
    }
}

impl<E: Serialize> EventLog<E> {
    pub fn append(&self, event: &E) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        let line = serde_json::to_string(event)?;
        writeln!(file, "{}", line)?;
        Ok(())
    }
}

impl<E: DeserializeOwned + Reducible> EventLog<E> {
    /// Replay all events into the event's associated [`Reducible::State`].
    pub fn reduce(&self) -> Result<E::State> {
        let events = self.read_all()?;
        let mut state = E::State::default();
        for event in &events {
            E::apply(&mut state, event);
        }
        Ok(state)
    }
}

impl<E: DeserializeOwned> EventLog<E> {
    pub fn read_all(&self) -> Result<Vec<E>> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }
        let file = fs::File::open(&self.path)?;
        let reader = BufReader::new(file);
        let mut events = Vec::new();
        for line in reader.lines() {
            let line = line?;
            if !line.is_empty() {
                events.push(serde_json::from_str(&line)?);
            }
        }
        Ok(events)
    }

    pub fn last_event(&self) -> Result<Option<E>> {
        if !self.path.exists() {
            return Ok(None);
        }
        let file = fs::File::open(&self.path)?;
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    #[serde(tag = "event")]
    enum TestEvent {
        Started { ts: i64 },
        Stopped { ts: i64 },
    }

    #[test]
    fn append_and_read_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let log: EventLog<TestEvent> = EventLog::new(tmp.path().join("test.jsonl"));

        log.append(&TestEvent::Started { ts: 1000 }).unwrap();
        log.append(&TestEvent::Stopped { ts: 2000 }).unwrap();

        let events = log.read_all().unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0], TestEvent::Started { ts: 1000 });
        assert_eq!(events[1], TestEvent::Stopped { ts: 2000 });
    }

    #[test]
    fn last_event_returns_final() {
        let tmp = tempfile::tempdir().unwrap();
        let log: EventLog<TestEvent> = EventLog::new(tmp.path().join("test.jsonl"));

        log.append(&TestEvent::Started { ts: 1000 }).unwrap();
        log.append(&TestEvent::Stopped { ts: 2000 }).unwrap();

        let last = log.last_event().unwrap().unwrap();
        assert_eq!(last, TestEvent::Stopped { ts: 2000 });
    }

    #[test]
    fn missing_file_returns_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let log: EventLog<TestEvent> = EventLog::new(tmp.path().join("nope.jsonl"));

        assert!(!log.exists());
        assert!(log.read_all().unwrap().is_empty());
        assert!(log.last_event().unwrap().is_none());
    }

    #[test]
    fn reset_clears_events() {
        let tmp = tempfile::tempdir().unwrap();
        let log: EventLog<TestEvent> = EventLog::new(tmp.path().join("test.jsonl"));

        log.append(&TestEvent::Started { ts: 1000 }).unwrap();
        assert_eq!(log.read_all().unwrap().len(), 1);

        log.reset().unwrap();
        assert!(log.exists());
        assert!(log.read_all().unwrap().is_empty());
    }

    #[test]
    fn remove_deletes_file() {
        let tmp = tempfile::tempdir().unwrap();
        let subdir = tmp.path().join("sub");
        std::fs::create_dir_all(&subdir).unwrap();
        let log: EventLog<TestEvent> = EventLog::new(subdir.join("test.jsonl"));

        log.append(&TestEvent::Started { ts: 1000 }).unwrap();
        assert!(log.exists());

        log.remove().unwrap();
        assert!(!log.exists());
        assert!(!subdir.exists());
    }

    #[derive(Debug, Default, PartialEq)]
    struct Counter {
        started: u32,
        stopped: u32,
    }

    impl Reducible for TestEvent {
        type State = Counter;
        fn apply(state: &mut Counter, event: &TestEvent) {
            match event {
                TestEvent::Started { .. } => state.started += 1,
                TestEvent::Stopped { .. } => state.stopped += 1,
            }
        }
    }

    #[test]
    fn reduce_folds_events() {
        let tmp = tempfile::tempdir().unwrap();
        let log: EventLog<TestEvent> = EventLog::new(tmp.path().join("test.jsonl"));

        log.append(&TestEvent::Started { ts: 1 }).unwrap();
        log.append(&TestEvent::Stopped { ts: 2 }).unwrap();
        log.append(&TestEvent::Started { ts: 3 }).unwrap();

        let state = log.reduce().unwrap();
        assert_eq!(state, Counter { started: 2, stopped: 1 });
    }

    #[test]
    fn reduce_empty_log_returns_default() {
        let tmp = tempfile::tempdir().unwrap();
        let log: EventLog<TestEvent> = EventLog::new(tmp.path().join("nope.jsonl"));

        let state = log.reduce().unwrap();
        assert_eq!(state, Counter::default());
    }
}
