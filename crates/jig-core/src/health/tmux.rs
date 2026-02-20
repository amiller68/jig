//! Tmux pane capture utilities for worker health detection

use std::process::Command;

use crate::error::{Error, Result};
use crate::health::detector::{WorkerDetector, WorkerState};

/// Capture the visible output of a tmux pane.
///
/// `session` and `window` identify the target pane (format: `session:window`).
/// `lines` controls how many lines of history to capture (negative = from end).
pub fn capture_pane(session: &str, window: &str, lines: i32) -> Result<String> {
    let target = format!("{}:{}", session, window);

    let output = Command::new("tmux")
        .args([
            "capture-pane",
            "-p",
            "-t",
            &target,
            "-S",
            &lines.to_string(),
        ])
        .output()?;

    if !output.status.success() {
        return Err(Error::TmuxCommand(
            String::from_utf8_lossy(&output.stderr).to_string(),
        ));
    }

    String::from_utf8(output.stdout)
        .map_err(|e| Error::TmuxCommand(format!("invalid UTF-8 in pane output: {}", e)))
}

/// Check a worker's state by capturing its tmux pane and running detection.
///
/// `session` is the tmux session name (e.g. `jig-myrepo`).
/// `window` is the worker's window name.
pub fn check_worker(session: &str, window: &str, detector: &WorkerDetector) -> Result<WorkerState> {
    let output = capture_pane(session, window, -20)?;
    Ok(detector.detect_state(&output))
}
