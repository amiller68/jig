//! Type-safe tmux client for worker control operations.
//!
//! Wraps tmux CLI commands in a structured API for:
//! - Session/window lifecycle management
//! - Sending input to workers (literal text, special keys)
//! - Nudging idle/stuck workers

use std::io;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::time::Duration;

use crate::error::{Error, Result};

/// Default timeout for tmux subprocess calls.
const TMUX_TIMEOUT: Duration = Duration::from_secs(5);

/// Run a tmux command with a timeout. Spawns the process, polls `try_wait()`
/// every 50ms, and kills + reaps the child if the timeout elapses.
fn run_tmux(args: &[&str], timeout: Duration) -> io::Result<Output> {
    let mut child = Command::new("tmux")
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let poll_interval = Duration::from_millis(50);
    let start = std::time::Instant::now();

    loop {
        match child.try_wait()? {
            Some(status) => {
                let stdout = child.stdout.take().map_or_else(Vec::new, |mut s| {
                    let mut buf = Vec::new();
                    io::Read::read_to_end(&mut s, &mut buf).unwrap_or(0);
                    buf
                });
                let stderr = child.stderr.take().map_or_else(Vec::new, |mut s| {
                    let mut buf = Vec::new();
                    io::Read::read_to_end(&mut s, &mut buf).unwrap_or(0);
                    buf
                });
                return Ok(Output {
                    status,
                    stdout,
                    stderr,
                });
            }
            None => {
                if start.elapsed() >= timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        format!("tmux {:?} timed out after {:?}", args, timeout),
                    ));
                }
                std::thread::sleep(poll_interval);
            }
        }
    }
}

/// Check if a pane command string represents a running (non-shell) process.
///
/// Returns `false` for known shells, version-like strings (e.g. "2.1.72"
/// which tmux can report as `pane_current_command`), and `None`.
pub fn is_running_command(cmd: Option<&str>) -> bool {
    let cmd = match cmd {
        Some(c) if !c.is_empty() => c,
        _ => return false,
    };

    // Known shells / tmux itself
    if matches!(cmd, "bash" | "zsh" | "fish" | "sh" | "tmux") {
        return false;
    }

    // Version-like strings: all digits and dots, with at least one dot
    // (e.g. "2.1.72" which tmux reports for some panes)
    if cmd.contains('.') && cmd.chars().all(|c| c.is_ascii_digit() || c == '.') {
        return false;
    }

    true
}

/// A tmux target: `session:window`.
#[derive(Debug, Clone)]
pub struct TmuxTarget {
    pub session: String,
    pub window: String,
}

impl TmuxTarget {
    pub fn new(session: impl Into<String>, window: impl Into<String>) -> Self {
        Self {
            session: session.into(),
            window: window.into(),
        }
    }

    fn target_str(&self) -> String {
        format!("{}:{}", self.session, self.window)
    }
}

/// Type-safe tmux client.
///
/// All operations shell out to the `tmux` binary with a timeout.
pub struct TmuxClient;

impl TmuxClient {
    pub fn new() -> Self {
        Self
    }

    // ── Session operations ──

    /// Check if a tmux session exists.
    pub fn has_session(&self, session: &str) -> bool {
        run_tmux(&["has-session", "-t", session], TMUX_TIMEOUT)
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// Create a session if it doesn't exist.
    pub fn ensure_session(&self, session: &str) -> Result<()> {
        if !self.has_session(session) {
            let output = run_tmux(&["new-session", "-d", "-s", session], TMUX_TIMEOUT)?;
            if !output.status.success() {
                return Err(Error::Custom(format!(
                    "tmux new-session failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                )));
            }
        }
        Ok(())
    }

    // ── Window operations ──

    /// Check if a window exists in a session.
    pub fn has_window(&self, target: &TmuxTarget) -> bool {
        if !self.has_session(&target.session) {
            return false;
        }
        let output = run_tmux(
            &[
                "list-windows",
                "-t",
                &target.session,
                "-F",
                "#{window_name}",
            ],
            TMUX_TIMEOUT,
        );

        match output {
            Ok(o) => {
                let windows = String::from_utf8_lossy(&o.stdout);
                windows.lines().any(|w| w == target.window)
            }
            Err(_) => false,
        }
    }

    /// Create a new window in a session.
    pub fn create_window(&self, target: &TmuxTarget, dir: &Path) -> Result<()> {
        self.ensure_session(&target.session)?;
        let dir_str = dir.to_string_lossy();
        let output = run_tmux(
            &[
                "new-window",
                "-t",
                &target.session,
                "-n",
                &target.window,
                "-c",
                &dir_str,
            ],
            TMUX_TIMEOUT,
        )?;
        if !output.status.success() {
            return Err(Error::Custom(format!(
                "tmux new-window failed: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }
        Ok(())
    }

    /// Kill a window.
    pub fn kill_window(&self, target: &TmuxTarget) -> Result<()> {
        if !self.has_window(target) {
            return Ok(());
        }
        let target_str = target.target_str();
        let output = run_tmux(&["kill-window", "-t", &target_str], TMUX_TIMEOUT)?;
        if !output.status.success() {
            return Err(Error::Custom(format!(
                "tmux kill-window failed: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }
        Ok(())
    }

    /// Kill an entire session.
    pub fn kill_session(&self, session: &str) -> Result<()> {
        if !self.has_session(session) {
            return Ok(());
        }
        let output = run_tmux(&["kill-session", "-t", session], TMUX_TIMEOUT)?;
        if !output.status.success() {
            return Err(Error::Custom(format!(
                "tmux kill-session failed: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }
        Ok(())
    }

    /// List window names in a session.
    pub fn list_windows(&self, session: &str) -> Result<Vec<String>> {
        if !self.has_session(session) {
            return Ok(Vec::new());
        }
        let output = run_tmux(
            &["list-windows", "-t", session, "-F", "#{window_name}"],
            TMUX_TIMEOUT,
        )?;
        let text = String::from_utf8_lossy(&output.stdout);
        Ok(text.lines().map(|s| s.to_string()).collect())
    }

    // ── Input operations ──

    /// Send keys to a window (tmux interprets special keys like "Enter", "C-c").
    pub fn send_keys(&self, target: &TmuxTarget, keys: &[&str]) -> Result<()> {
        let target_str = target.target_str();
        let mut args = vec!["send-keys", "-t", &target_str];
        args.extend(keys);
        let output = run_tmux(&args, TMUX_TIMEOUT)?;
        if !output.status.success() {
            return Err(Error::Custom(format!(
                "tmux send-keys failed: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }
        Ok(())
    }

    /// Send literal text (no special key interpretation).
    pub fn send_keys_literal(&self, target: &TmuxTarget, text: &str) -> Result<()> {
        let target_str = target.target_str();
        let output = run_tmux(&["send-keys", "-t", &target_str, "-l", text], TMUX_TIMEOUT)?;
        if !output.status.success() {
            return Err(Error::Custom(format!(
                "tmux send-keys -l failed: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }
        Ok(())
    }

    // ── Nudge operations ──

    /// Send a nudge message to a worker: literal text + Enter.
    /// Collapses newlines to spaces so the message is sent as a single input
    /// (multiline literal text causes premature submission in some TUIs).
    pub fn send_message(&self, target: &TmuxTarget, message: &str) -> Result<()> {
        let single_line = collapse_to_single_line(message);
        self.send_keys_literal(target, &single_line)?;
        std::thread::sleep(std::time::Duration::from_millis(100));
        self.send_keys(target, &["Enter"])?;
        Ok(())
    }

    /// Auto-approve a stuck prompt by sending "1" + Enter.
    pub fn auto_approve(&self, target: &TmuxTarget) -> Result<()> {
        self.send_keys(target, &["1", "Enter"])?;
        Ok(())
    }

    /// Send Ctrl+C to interrupt.
    pub fn interrupt(&self, target: &TmuxTarget) -> Result<()> {
        self.send_keys(target, &["C-c"])?;
        Ok(())
    }

    // ── Pane inspection ──

    /// Get the current command running in a pane.
    pub fn pane_command(&self, target: &TmuxTarget) -> Option<String> {
        if !self.has_window(target) {
            return None;
        }
        let target_str = target.target_str();
        let output = run_tmux(
            &[
                "list-panes",
                "-t",
                &target_str,
                "-F",
                "#{pane_current_command}",
            ],
            TMUX_TIMEOUT,
        )
        .ok()?;
        let cmd = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if cmd.is_empty() {
            None
        } else {
            Some(cmd)
        }
    }

    /// Check if the pane is running a command (not at a shell prompt).
    pub fn pane_is_running(&self, target: &TmuxTarget) -> bool {
        is_running_command(self.pane_command(target).as_deref())
    }
}

/// Collapse a multiline message into a single line for tmux input.
fn collapse_to_single_line(message: &str) -> String {
    message
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

impl Default for TmuxClient {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_str_format() {
        let target = TmuxTarget::new("jig-repo", "feat/auth");
        assert_eq!(target.target_str(), "jig-repo:feat/auth");
    }

    #[test]
    fn has_session_nonexistent() {
        let client = TmuxClient::new();
        assert!(!client.has_session("jig-nonexistent-test-session-xyz"));
    }

    #[test]
    fn has_window_nonexistent() {
        let client = TmuxClient::new();
        let target = TmuxTarget::new("jig-nonexistent-test-session-xyz", "window");
        assert!(!client.has_window(&target));
    }

    #[test]
    fn list_windows_no_session() {
        let client = TmuxClient::new();
        let windows = client
            .list_windows("jig-nonexistent-test-session-xyz")
            .unwrap();
        assert!(windows.is_empty());
    }

    #[test]
    fn pane_command_nonexistent() {
        let client = TmuxClient::new();
        let target = TmuxTarget::new("jig-nonexistent-test-session-xyz", "window");
        assert!(client.pane_command(&target).is_none());
    }

    #[test]
    fn collapse_multiline_message() {
        let msg = "STATUS CHECK: You've been idle (nudge 1/3).\n\nYou have uncommitted changes but no PR yet.\nWhat's blocking you?\n";
        let result = collapse_to_single_line(msg);
        assert_eq!(
            result,
            "STATUS CHECK: You've been idle (nudge 1/3). You have uncommitted changes but no PR yet. What's blocking you?"
        );
    }

    #[test]
    fn collapse_single_line_unchanged() {
        let msg = "simple message";
        assert_eq!(collapse_to_single_line(msg), "simple message");
    }

    #[test]
    fn collapse_strips_blank_lines_and_whitespace() {
        let msg = "  line one  \n\n\n  line two  \n";
        assert_eq!(collapse_to_single_line(msg), "line one line two");
    }

    // ── is_running_command tests ──

    #[test]
    fn is_running_command_shells() {
        assert!(!is_running_command(Some("bash")));
        assert!(!is_running_command(Some("zsh")));
        assert!(!is_running_command(Some("fish")));
        assert!(!is_running_command(Some("sh")));
        assert!(!is_running_command(Some("tmux")));
    }

    #[test]
    fn is_running_command_version_strings() {
        assert!(!is_running_command(Some("2.1.72")));
        assert!(!is_running_command(Some("3.4")));
        assert!(!is_running_command(Some("1.0.0")));
    }

    #[test]
    fn is_running_command_real_commands() {
        assert!(is_running_command(Some("claude")));
        assert!(is_running_command(Some("node")));
        assert!(is_running_command(Some("python3")));
        assert!(is_running_command(Some("cargo")));
    }

    #[test]
    fn is_running_command_none_and_empty() {
        assert!(!is_running_command(None));
        assert!(!is_running_command(Some("")));
    }
}
