//! Tmux session management
//!
//! Handles tmux session and window operations for worker management.

use std::path::Path;
use std::process::Command;

use crate::error::{Error, Result};

/// Check if a tmux session exists
pub fn session_exists(session: &str) -> bool {
    Command::new("tmux")
        .args(["has-session", "-t", session])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Create a tmux session if it doesn't exist
pub fn ensure_session(session: &str) -> Result<()> {
    if !session_exists(session) {
        Command::new("tmux")
            .args(["new-session", "-d", "-s", session])
            .output()?;
    }
    Ok(())
}

/// Check if a window exists in a session
pub fn window_exists(session: &str, window: &str) -> bool {
    if !session_exists(session) {
        return false;
    }

    let output = Command::new("tmux")
        .args(["list-windows", "-t", session, "-F", "#{window_name}"])
        .output();

    match output {
        Ok(o) => {
            let windows = String::from_utf8_lossy(&o.stdout);
            windows.lines().any(|w| w == window)
        }
        Err(_) => false,
    }
}

/// Create a new window in a session
pub fn create_window(session: &str, window: &str, dir: &Path) -> Result<()> {
    ensure_session(session)?;

    Command::new("tmux")
        .args([
            "new-window",
            "-t",
            session,
            "-n",
            window,
            "-c",
            &dir.to_string_lossy(),
        ])
        .output()?;

    Ok(())
}

/// Send keys to a window
pub fn send_keys(session: &str, window: &str, keys: &str) -> Result<()> {
    Command::new("tmux")
        .args([
            "send-keys",
            "-t",
            &format!("{}:{}", session, window),
            keys,
            "Enter",
        ])
        .output()?;

    Ok(())
}

/// Kill a window
pub fn kill_window(session: &str, window: &str) -> Result<()> {
    if !window_exists(session, window) {
        return Ok(());
    }

    Command::new("tmux")
        .args(["kill-window", "-t", &format!("{}:{}", session, window)])
        .output()?;

    Ok(())
}

/// Select a window
pub fn select_window(session: &str, window: &str) -> Result<()> {
    Command::new("tmux")
        .args(["select-window", "-t", &format!("{}:{}", session, window)])
        .output()?;

    Ok(())
}

/// Attach to a session (replaces current process on Unix)
#[cfg(unix)]
pub fn attach(session: &str) -> Result<()> {
    if !session_exists(session) {
        return Err(Error::TmuxSessionNotFound(session.to_string()));
    }

    use std::ffi::CString;

    let cmd = CString::new("tmux").unwrap();
    let args: Vec<CString> = ["tmux", "attach", "-t", session]
        .iter()
        .map(|a| CString::new(*a).unwrap())
        .collect();
    let args: Vec<&std::ffi::CStr> = args.iter().map(|a| a.as_c_str()).collect();

    let err = nix::unistd::execvp(&cmd, &args);
    Err(Error::Custom(format!("Failed to attach: {:?}", err)))
}

#[cfg(not(unix))]
pub fn attach(session: &str) -> Result<()> {
    if !session_exists(session) {
        return Err(Error::TmuxSessionNotFound(session.to_string()));
    }

    // On non-Unix, just run tmux attach as a subprocess
    let status = Command::new("tmux")
        .args(["attach", "-t", session])
        .status()?;

    if !status.success() {
        return Err(Error::Custom("Failed to attach to tmux session".to_string()));
    }

    Ok(())
}

/// Check if a pane is running a command (not at shell prompt)
pub fn pane_is_running(session: &str, window: &str) -> bool {
    if !window_exists(session, window) {
        return false;
    }

    let output = Command::new("tmux")
        .args([
            "list-panes",
            "-t",
            &format!("{}:{}", session, window),
            "-F",
            "#{pane_current_command}",
        ])
        .output();

    match output {
        Ok(o) => {
            let cmd = String::from_utf8_lossy(&o.stdout).trim().to_string();
            // If running bash/zsh/fish/sh, the command has exited
            !matches!(cmd.as_str(), "bash" | "zsh" | "fish" | "sh")
        }
        Err(_) => false,
    }
}

/// Get the current command running in a pane
pub fn get_pane_command(session: &str, window: &str) -> Option<String> {
    if !window_exists(session, window) {
        return None;
    }

    let output = Command::new("tmux")
        .args([
            "list-panes",
            "-t",
            &format!("{}:{}", session, window),
            "-F",
            "#{pane_current_command}",
        ])
        .output()
        .ok()?;

    let cmd = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if cmd.is_empty() {
        None
    } else {
        Some(cmd)
    }
}

/// List all windows in a session
pub fn list_windows(session: &str) -> Result<Vec<String>> {
    if !session_exists(session) {
        return Ok(Vec::new());
    }

    let output = Command::new("tmux")
        .args(["list-windows", "-t", session, "-F", "#{window_name}"])
        .output()?;

    let text = String::from_utf8_lossy(&output.stdout);
    Ok(text.lines().map(|s| s.to_string()).collect())
}
