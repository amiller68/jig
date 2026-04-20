//! Type-safe tmux wrappers for session and window management.
//!
//! `TmuxSession` and `TmuxWindow` own the relevant identifiers and expose
//! operations directly — find/ensure a session, then talk to its windows.

use std::io;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::time::Duration;

#[derive(Debug, thiserror::Error)]
pub enum TmuxError {
    #[error("tmux {command} failed: {stderr}")]
    CommandFailed { command: String, stderr: String },

    #[error("tmux session not found: {0}")]
    SessionNotFound(String),

    #[error("tmux {command} timed out after {seconds}s")]
    Timeout { command: String, seconds: u64 },

    #[error("tmux exec failed: {0}")]
    ExecFailed(String),

    #[error(transparent)]
    Io(#[from] std::io::Error),
}

type Result<T> = std::result::Result<T, TmuxError>;

const TMUX_TIMEOUT: Duration = Duration::from_secs(5);

fn run_tmux(args: &[&str], timeout: Duration) -> Result<Output> {
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
                    return Err(TmuxError::Timeout {
                        command: args.first().unwrap_or(&"unknown").to_string(),
                        seconds: timeout.as_secs(),
                    });
                }
                std::thread::sleep(poll_interval);
            }
        }
    }
}

const DEFAULT_SESSION_PREFIX: &str = "jig-";

/// A named tmux session.
#[derive(Debug, Clone)]
pub struct TmuxSession(String);

impl TmuxSession {
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    /// Create a session for a repo using the default "jig-" prefix.
    pub fn for_repo(repo_name: &str) -> Self {
        Self(format!("{}{}", DEFAULT_SESSION_PREFIX, repo_name))
    }

    /// Create a session for a repo using a custom prefix.
    pub fn for_repo_with_prefix(prefix: &str, repo_name: &str) -> Self {
        Self(format!("{}{}", prefix, repo_name))
    }

    pub fn name(&self) -> &str {
        &self.0
    }

    pub fn exists(&self) -> bool {
        run_tmux(&["has-session", "-t", &self.0], TMUX_TIMEOUT)
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// Create the session if it doesn't exist.
    /// Sets Ctrl-a prefix (avoids collision with outer Ctrl-b) and enables mouse.
    pub fn ensure(&self) -> Result<()> {
        if !self.exists() {
            let output = run_tmux(&["new-session", "-d", "-s", &self.0], TMUX_TIMEOUT)?;
            if !output.status.success() {
                return Err(TmuxError::CommandFailed {
                    command: "new-session".to_string(),
                    stderr: String::from_utf8_lossy(&output.stderr).into(),
                });
            }
            let _ = run_tmux(
                &["set-option", "-t", &self.0, "prefix", "C-a"],
                TMUX_TIMEOUT,
            );
            let _ = run_tmux(&["set-option", "-t", &self.0, "mouse", "on"], TMUX_TIMEOUT);
        }
        Ok(())
    }

    pub fn window(&self, name: impl Into<String>) -> TmuxWindow {
        TmuxWindow {
            session: self.0.clone(),
            window: name.into(),
        }
    }

    pub fn windows(&self) -> Result<Vec<TmuxWindow>> {
        if !self.exists() {
            return Ok(Vec::new());
        }
        let output = run_tmux(
            &["list-windows", "-t", &self.0, "-F", "#{window_name}"],
            TMUX_TIMEOUT,
        )?;
        let text = String::from_utf8_lossy(&output.stdout);
        Ok(text.lines().map(|name| self.window(name)).collect())
    }

    pub fn window_names(&self) -> Result<Vec<String>> {
        if !self.exists() {
            return Ok(Vec::new());
        }
        let output = run_tmux(
            &["list-windows", "-t", &self.0, "-F", "#{window_name}"],
            TMUX_TIMEOUT,
        )?;
        let text = String::from_utf8_lossy(&output.stdout);
        Ok(text.lines().map(|s| s.to_string()).collect())
    }

    pub fn kill(&self) -> Result<()> {
        if !self.exists() {
            return Ok(());
        }
        let output = run_tmux(&["kill-session", "-t", &self.0], TMUX_TIMEOUT)?;
        if !output.status.success() {
            return Err(TmuxError::CommandFailed {
                command: "kill-session".to_string(),
                stderr: String::from_utf8_lossy(&output.stderr).into(),
            });
        }
        Ok(())
    }

    /// On Unix, replaces the current process via `execvp`.
    /// On non-Unix, spawns tmux as a child process and waits.
    #[cfg(unix)]
    pub fn attach(&self) -> Result<()> {
        if !self.exists() {
            return Err(TmuxError::SessionNotFound(self.0.clone()));
        }
        exec_tmux(&["attach", "-t", &self.0])
    }

    #[cfg(not(unix))]
    pub fn attach(&self) -> Result<()> {
        if !self.exists() {
            return Err(TmuxError::SessionNotFound(self.0.clone()));
        }
        let status = std::process::Command::new("tmux")
            .args(["attach", "-t", &self.0])
            .status()?;
        if !status.success() {
            return Err(TmuxError::CommandFailed {
                command: "attach".to_string(),
                stderr: "non-zero exit".to_string(),
            });
        }
        Ok(())
    }
}

/// A window within a tmux session.
#[derive(Debug, Clone)]
pub struct TmuxWindow {
    session: String,
    window: String,
}

impl TmuxWindow {
    pub fn new(session: impl Into<String>, window: impl Into<String>) -> Self {
        Self {
            session: session.into(),
            window: window.into(),
        }
    }

    pub fn session_name(&self) -> &str {
        &self.session
    }

    pub fn window_name(&self) -> &str {
        &self.window
    }

    fn target_str(&self) -> String {
        format!("{}:{}", self.session, self.window)
    }

    pub fn exists(&self) -> bool {
        let session = TmuxSession::new(&self.session);
        if !session.exists() {
            return false;
        }
        let output = run_tmux(
            &["list-windows", "-t", &self.session, "-F", "#{window_name}"],
            TMUX_TIMEOUT,
        );
        match output {
            Ok(o) => {
                let windows = String::from_utf8_lossy(&o.stdout);
                windows.lines().any(|w| w == self.window)
            }
            Err(_) => false,
        }
    }

    pub fn create(&self, dir: &Path) -> Result<()> {
        TmuxSession::new(&self.session).ensure()?;
        let dir_str = dir.to_string_lossy();
        let output = run_tmux(
            &[
                "new-window",
                "-t",
                &self.session,
                "-n",
                &self.window,
                "-c",
                &dir_str,
            ],
            TMUX_TIMEOUT,
        )?;
        if !output.status.success() {
            return Err(TmuxError::CommandFailed {
                command: "new-window".to_string(),
                stderr: String::from_utf8_lossy(&output.stderr).into(),
            });
        }
        Ok(())
    }

    pub fn kill(&self) -> Result<()> {
        if !self.exists() {
            return Ok(());
        }
        let target = self.target_str();
        let output = run_tmux(&["kill-window", "-t", &target], TMUX_TIMEOUT)?;
        if !output.status.success() {
            return Err(TmuxError::CommandFailed {
                command: "kill-window".to_string(),
                stderr: String::from_utf8_lossy(&output.stderr).into(),
            });
        }
        Ok(())
    }

    pub fn select(&self) -> Result<()> {
        let target = self.target_str();
        let output = run_tmux(&["select-window", "-t", &target], TMUX_TIMEOUT)?;
        if !output.status.success() {
            return Err(TmuxError::CommandFailed {
                command: "select-window".to_string(),
                stderr: String::from_utf8_lossy(&output.stderr).into(),
            });
        }
        Ok(())
    }

    /// Send keys (tmux interprets special keys like "Enter", "C-c").
    pub fn send_keys(&self, keys: &[&str]) -> Result<()> {
        let target = self.target_str();
        let mut args = vec!["send-keys", "-t", &target];
        args.extend(keys);
        let output = run_tmux(&args, TMUX_TIMEOUT)?;
        if !output.status.success() {
            return Err(TmuxError::CommandFailed {
                command: "send-keys".to_string(),
                stderr: String::from_utf8_lossy(&output.stderr).into(),
            });
        }
        Ok(())
    }

    /// Send literal text (no special key interpretation).
    pub fn send_keys_literal(&self, text: &str) -> Result<()> {
        let target = self.target_str();
        let output = run_tmux(&["send-keys", "-t", &target, "-l", text], TMUX_TIMEOUT)?;
        if !output.status.success() {
            return Err(TmuxError::CommandFailed {
                command: "send-keys -l".to_string(),
                stderr: String::from_utf8_lossy(&output.stderr).into(),
            });
        }
        Ok(())
    }

    /// Send a message: literal text + Enter.
    /// Collapses newlines to spaces (multiline text causes premature submission in some TUIs).
    pub fn send_message(&self, message: &str) -> Result<()> {
        let single_line = collapse_to_single_line(message);
        self.send_keys_literal(&single_line)?;
        std::thread::sleep(std::time::Duration::from_millis(100));
        self.send_keys(&["Enter"])?;
        Ok(())
    }

    pub fn interrupt(&self) -> Result<()> {
        self.send_keys(&["C-c"])
    }

    pub fn pane_command(&self) -> Option<String> {
        if !self.exists() {
            return None;
        }
        let target = self.target_str();
        let output = run_tmux(
            &["list-panes", "-t", &target, "-F", "#{pane_current_command}"],
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
    pub fn is_running(&self) -> bool {
        let cmd = match self.pane_command() {
            Some(c) if !c.is_empty() => c,
            _ => return false,
        };

        if super::KNOWN_SHELLS.contains(&cmd.as_str()) || cmd == "tmux" {
            return false;
        }

        if cmd.contains('.') && cmd.chars().all(|c| c.is_ascii_digit() || c == '.') {
            return false;
        }

        true
    }

    /// On Unix, replaces the current process via `execvp`.
    /// On non-Unix, spawns tmux as a child process and waits.
    #[cfg(unix)]
    pub fn attach(&self) -> Result<()> {
        let session = TmuxSession::new(&self.session);
        if !session.exists() {
            return Err(TmuxError::SessionNotFound(self.session.clone()));
        }
        let target = self.target_str();
        exec_tmux(&["attach", "-t", &target])
    }

    #[cfg(not(unix))]
    pub fn attach(&self) -> Result<()> {
        let session = TmuxSession::new(&self.session);
        if !session.exists() {
            return Err(TmuxError::SessionNotFound(self.session.clone()));
        }
        let target = self.target_str();
        let status = std::process::Command::new("tmux")
            .args(["attach", "-t", &target])
            .status()?;
        if !status.success() {
            return Err(TmuxError::CommandFailed {
                command: "attach".to_string(),
                stderr: "non-zero exit".to_string(),
            });
        }
        Ok(())
    }
}

#[cfg(unix)]
fn exec_tmux(args: &[&str]) -> Result<()> {
    use std::ffi::CString;

    let cmd = CString::new("tmux").unwrap();
    let mut argv: Vec<CString> = vec![CString::new("tmux").unwrap()];
    for arg in args {
        argv.push(CString::new(*arg).unwrap());
    }
    let argv_refs: Vec<&std::ffi::CStr> = argv.iter().map(|a| a.as_c_str()).collect();
    let err = nix::unistd::execvp(&cmd, &argv_refs);
    Err(TmuxError::ExecFailed(format!("{:?}", err)))
}

fn collapse_to_single_line(message: &str) -> String {
    message
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_target_str() {
        let w = TmuxWindow::new("jig-repo", "feat/auth");
        assert_eq!(w.target_str(), "jig-repo:feat/auth");
    }

    #[test]
    fn session_not_found() {
        let s = TmuxSession::new("jig-nonexistent-test-session-xyz");
        assert!(!s.exists());
    }

    #[test]
    fn window_not_found() {
        let w = TmuxWindow::new("jig-nonexistent-test-session-xyz", "window");
        assert!(!w.exists());
    }

    #[test]
    fn list_windows_no_session() {
        let s = TmuxSession::new("jig-nonexistent-test-session-xyz");
        assert!(s.window_names().unwrap().is_empty());
    }

    #[test]
    fn pane_command_nonexistent() {
        let w = TmuxWindow::new("jig-nonexistent-test-session-xyz", "window");
        assert!(w.pane_command().is_none());
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
}
