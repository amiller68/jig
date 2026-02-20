# Tmux Integration Library

**Status:** Planned  
**Priority:** Medium  
**Category:** Improvements

## Objective

Build a robust, type-safe Rust library for tmux integration: scraping pane output, sending input, managing sessions/windows, and detecting worker state.

## Background

Jig needs tmux for:
- Scraping worker output (stuck prompt detection)
- Sending nudge messages to workers
- Detecting if worker is at shell prompt
- Managing tmux sessions/windows lifecycle

Current approach uses shell commands. Need native Rust library with:
- Type-safe API
- Error handling
- Output parsing
- Cross-platform support

## Architecture

### Core Types

```rust
pub mod tmux {
    use std::process::{Command, Output};
    use std::path::{Path, PathBuf};
    
    #[derive(Debug, Clone)]
    pub struct TmuxTarget {
        pub session: String,
        pub window: Option<String>,
        pub pane: Option<u32>,
    }
    
    impl TmuxTarget {
        pub fn new(session: impl Into<String>) -> Self {
            Self {
                session: session.into(),
                window: None,
                pane: None,
            }
        }
        
        pub fn window(mut self, window: impl Into<String>) -> Self {
            self.window = Some(window.into());
            self
        }
        
        pub fn pane(mut self, pane: u32) -> Self {
            self.pane = Some(pane);
            self
        }
        
        pub fn to_string(&self) -> String {
            let mut target = self.session.clone();
            if let Some(window) = &self.window {
                target.push(':');
                target.push_str(window);
            }
            if let Some(pane) = self.pane {
                target.push('.');
                target.push_str(&pane.to_string());
            }
            target
        }
    }
    
    #[derive(Debug)]
    pub struct TmuxSession {
        pub name: String,
        pub created: i64,
        pub attached: bool,
    }
    
    #[derive(Debug)]
    pub struct TmuxWindow {
        pub index: u32,
        pub name: String,
        pub active: bool,
    }
    
    #[derive(Debug)]
    pub struct TmuxPane {
        pub id: String,
        pub index: u32,
        pub active: bool,
        pub width: u32,
        pub height: u32,
    }
}
```

### Client

```rust
pub struct TmuxClient {
    socket: Option<PathBuf>,
}

impl TmuxClient {
    pub fn new() -> Self {
        Self { socket: None }
    }
    
    pub fn with_socket(socket: impl Into<PathBuf>) -> Self {
        Self { socket: Some(socket.into()) }
    }
    
    fn command(&self) -> Command {
        let mut cmd = Command::new("tmux");
        if let Some(socket) = &self.socket {
            cmd.arg("-S").arg(socket);
        }
        cmd
    }
    
    /// List all sessions
    pub fn list_sessions(&self) -> Result<Vec<TmuxSession>> {
        let output = self.command()
            .args(&["list-sessions", "-F", "#{session_name}\t#{session_created}\t#{session_attached}"])
            .output()?;
        
        if !output.status.success() {
            return Err(Error::TmuxCommand(String::from_utf8_lossy(&output.stderr).to_string()));
        }
        
        let stdout = String::from_utf8(output.stdout)?;
        stdout.lines()
            .map(|line| {
                let parts: Vec<&str> = line.split('\t').collect();
                Ok(TmuxSession {
                    name: parts[0].to_string(),
                    created: parts[1].parse()?,
                    attached: parts[2] == "1",
                })
            })
            .collect()
    }
    
    /// List windows in session
    pub fn list_windows(&self, session: &str) -> Result<Vec<TmuxWindow>> {
        let output = self.command()
            .args(&["list-windows", "-t", session, "-F", "#{window_index}\t#{window_name}\t#{window_active}"])
            .output()?;
        
        if !output.status.success() {
            return Err(Error::TmuxCommand(String::from_utf8_lossy(&output.stderr).to_string()));
        }
        
        let stdout = String::from_utf8(output.stdout)?;
        stdout.lines()
            .map(|line| {
                let parts: Vec<&str> = line.split('\t').collect();
                Ok(TmuxWindow {
                    index: parts[0].parse()?,
                    name: parts[1].to_string(),
                    active: parts[2] == "1",
                })
            })
            .collect()
    }
    
    /// Capture pane output
    pub fn capture_pane(&self, target: &TmuxTarget, start_line: Option<i32>) -> Result<String> {
        let mut cmd = self.command();
        cmd.args(&["capture-pane", "-p", "-t", &target.to_string()]);
        
        if let Some(start) = start_line {
            cmd.args(&["-S", &start.to_string()]);
        }
        
        let output = cmd.output()?;
        
        if !output.status.success() {
            return Err(Error::TmuxCommand(String::from_utf8_lossy(&output.stderr).to_string()));
        }
        
        Ok(String::from_utf8(output.stdout)?)
    }
    
    /// Send keys to pane (literal mode, safer for text)
    pub fn send_keys_literal(&self, target: &TmuxTarget, text: &str) -> Result<()> {
        let output = self.command()
            .args(&["send-keys", "-t", &target.to_string(), "-l", text])
            .output()?;
        
        if !output.status.success() {
            return Err(Error::TmuxCommand(String::from_utf8_lossy(&output.stderr).to_string()));
        }
        
        Ok(())
    }
    
    /// Send keys (can include special keys like Enter)
    pub fn send_keys(&self, target: &TmuxTarget, keys: &[&str]) -> Result<()> {
        let mut cmd = self.command();
        cmd.args(&["send-keys", "-t", &target.to_string()]);
        cmd.args(keys);
        
        let output = cmd.output()?;
        
        if !output.status.success() {
            return Err(Error::TmuxCommand(String::from_utf8_lossy(&output.stderr).to_string()));
        }
        
        Ok(())
    }
    
    /// Check if session exists
    pub fn has_session(&self, session: &str) -> bool {
        self.command()
            .args(&["has-session", "-t", session])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
    
    /// Kill window
    pub fn kill_window(&self, target: &TmuxTarget) -> Result<()> {
        let output = self.command()
            .args(&["kill-window", "-t", &target.to_string()])
            .output()?;
        
        if !output.status.success() {
            return Err(Error::TmuxCommand(String::from_utf8_lossy(&output.stderr).to_string()));
        }
        
        Ok(())
    }
}
```

### Worker State Detection

```rust
pub mod detection {
    use super::*;
    use regex::Regex;
    
    pub struct WorkerDetector {
        prompt_patterns: Vec<Regex>,
        stuck_patterns: Vec<Regex>,
    }
    
    impl WorkerDetector {
        pub fn new() -> Self {
            Self {
                prompt_patterns: vec![
                    Regex::new(r"❯\s*$").unwrap(),
                    Regex::new(r"\$\s*$").unwrap(),
                    Regex::new(r"#\s*$").unwrap(),
                ],
                stuck_patterns: vec![
                    Regex::new(r"Would you like to proceed").unwrap(),
                    Regex::new(r"ctrl-g to edit").unwrap(),
                    Regex::new(r"❯.*\d+\.\s+Yes.*\d+\.\s+Yes").unwrap(),
                ],
            }
        }
        
        pub fn with_custom_patterns(
            prompt_patterns: Vec<String>,
            stuck_patterns: Vec<String>
        ) -> Result<Self> {
            Ok(Self {
                prompt_patterns: prompt_patterns.into_iter()
                    .map(|p| Regex::new(&p))
                    .collect::<Result<Vec<_>, _>>()?,
                stuck_patterns: stuck_patterns.into_iter()
                    .map(|p| Regex::new(&p))
                    .collect::<Result<Vec<_>, _>>()?,
            })
        }
        
        /// Check if worker is at shell prompt
        pub fn is_at_prompt(&self, output: &str) -> bool {
            let last_lines: String = output.lines().rev().take(3).collect::<Vec<_>>().join("\n");
            self.prompt_patterns.iter().any(|re| re.is_match(&last_lines))
        }
        
        /// Check if worker is stuck at interactive prompt
        pub fn is_stuck(&self, output: &str) -> bool {
            self.stuck_patterns.iter().any(|re| re.is_match(output))
        }
        
        /// Detect worker state
        pub fn detect_state(&self, output: &str) -> WorkerState {
            if self.is_stuck(output) {
                WorkerState::Stuck
            } else if self.is_at_prompt(output) {
                WorkerState::AtPrompt
            } else {
                WorkerState::Working
            }
        }
    }
    
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub enum WorkerState {
        Working,     // Not at prompt, actively doing something
        AtPrompt,    // At shell prompt, idle
        Stuck,       // At interactive prompt, waiting for input
    }
}
```

### Nudging Helper

```rust
pub mod nudge {
    use super::*;
    use std::thread;
    use std::time::Duration;
    
    pub struct NudgeClient<'a> {
        tmux: &'a TmuxClient,
    }
    
    impl<'a> NudgeClient<'a> {
        pub fn new(tmux: &'a TmuxClient) -> Self {
            Self { tmux }
        }
        
        /// Send a nudge message to worker
        /// Handles special case for interactive CLIs (Claude Code, etc.)
        pub fn send_message(&self, target: &TmuxTarget, message: &str) -> Result<()> {
            // Send message as literal text
            self.tmux.send_keys_literal(target, message)?;
            
            // Small delay before Enter (prevents paste interpretation)
            thread::sleep(Duration::from_millis(100));
            
            // Send Enter
            self.tmux.send_keys(target, &["Enter"])?;
            
            Ok(())
        }
        
        /// Auto-approve stuck prompt by sending "1" + Enter
        pub fn auto_approve(&self, target: &TmuxTarget) -> Result<()> {
            self.tmux.send_keys(target, &["1", "Enter"])?;
            Ok(())
        }
        
        /// Send Ctrl+C to interrupt
        pub fn interrupt(&self, target: &TmuxTarget) -> Result<()> {
            self.tmux.send_keys(target, &["C-c"])?;
            Ok(())
        }
    }
}
```

## Usage Examples

### Basic Usage

```rust
use jig_tmux::{TmuxClient, TmuxTarget};

fn main() -> Result<()> {
    let tmux = TmuxClient::new();
    
    // List sessions
    let sessions = tmux.list_sessions()?;
    for session in sessions {
        println!("Session: {} (created: {})", session.name, session.created);
    }
    
    // List windows
    let windows = tmux.list_windows("jig-myrepo")?;
    for window in windows {
        println!("  Window {}: {}", window.index, window.name);
    }
    
    // Capture pane output
    let target = TmuxTarget::new("jig-myrepo").window("features/auth");
    let output = tmux.capture_pane(&target, Some(-20))?;
    println!("Last 20 lines:\n{}", output);
    
    Ok(())
}
```

### Worker Detection

```rust
use jig_tmux::{TmuxClient, TmuxTarget, detection::WorkerDetector};

fn check_worker_state(tmux: &TmuxClient, target: &TmuxTarget) -> Result<()> {
    let output = tmux.capture_pane(target, Some(-20))?;
    
    let detector = WorkerDetector::new();
    let state = detector.detect_state(&output);
    
    match state {
        WorkerState::Working => println!("Worker is actively working"),
        WorkerState::AtPrompt => println!("Worker is idle at prompt"),
        WorkerState::Stuck => println!("Worker is stuck at interactive prompt"),
    }
    
    Ok(())
}
```

### Nudging

```rust
use jig_tmux::{TmuxClient, TmuxTarget, nudge::NudgeClient};

fn nudge_idle_worker(target: &TmuxTarget) -> Result<()> {
    let tmux = TmuxClient::new();
    let nudger = NudgeClient::new(&tmux);
    
    let message = "STATUS CHECK: You've been idle. What's the current state?";
    nudger.send_message(target, message)?;
    
    Ok(())
}
```

## Configuration

**Per-repo in `jig.toml`:**

```toml
[tmux]
# Custom socket path (optional)
socket = "/tmp/my-tmux.sock"

# Prompt detection patterns (regex)
promptPatterns = [
    "❯\\s*$",
    "\\$\\s*$",
    "#\\s*$",
]

# Stuck detection patterns (regex)
stuckPatterns = [
    "Would you like to proceed",
    "ctrl-g to edit",
]

# Delay between text and Enter (ms)
messageDelayMs = 100
```

## Testing

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_tmux_target_to_string() {
        let target = TmuxTarget::new("session").window("window").pane(0);
        assert_eq!(target.to_string(), "session:window.0");
    }
    
    #[test]
    fn test_detect_at_prompt() {
        let detector = WorkerDetector::new();
        let output = "some output\nmore output\n❯ ";
        assert!(detector.is_at_prompt(output));
    }
    
    #[test]
    fn test_detect_stuck() {
        let detector = WorkerDetector::new();
        let output = "Would you like to proceed? (y/n)";
        assert!(detector.is_stuck(output));
    }
}
```

## Implementation Phases

### Phase 1: Core Client
1. Add dependencies (regex)
2. Implement TmuxClient
3. Basic session/window/pane operations
4. Error types

### Phase 2: Output Capture
1. Capture pane output
2. Parse tmux output formats
3. Handle special characters

### Phase 3: Input Sending
1. Send keys (literal mode)
2. Send keys (with special keys)
3. Delay handling for interactive CLIs

### Phase 4: Detection
1. Prompt pattern matching
2. Stuck pattern matching
3. Worker state detection
4. Configurable patterns

### Phase 5: Nudging
1. NudgeClient helper
2. Safe message sending
3. Auto-approval
4. Interrupt handling

## Acceptance Criteria

### Core Client
- [ ] Initialize with optional socket path
- [ ] List sessions with metadata
- [ ] List windows in session
- [ ] List panes in window
- [ ] Check if session exists

### Output Capture
- [ ] Capture full pane output
- [ ] Capture N lines from history
- [ ] Handle UTF-8 correctly
- [ ] Handle ANSI escape codes

### Input Sending
- [ ] Send literal text
- [ ] Send special keys (Enter, C-c, etc.)
- [ ] Delay between text and Enter
- [ ] No dropped characters

### Detection
- [ ] Detect shell prompt patterns
- [ ] Detect stuck patterns
- [ ] Configurable patterns via regex
- [ ] Per-repo pattern overrides

### Nudging
- [ ] Send message safely
- [ ] Auto-approve with "1"
- [ ] Interrupt with Ctrl+C
- [ ] No race conditions

## Testing

```bash
# Create test tmux session
tmux new-session -d -s test-jig

# Run tests
cargo test --package jig-tmux

# Verify in real tmux session
tmux send-keys -t test-jig "echo test" Enter
sleep 1

# Cleanup
tmux kill-session -t test-jig
```

## Open Questions

1. Should we use tmux control mode? (No, too complex for our needs)
2. Should we cache tmux output? (No, always fresh)
3. Should we support multiple tmux versions? (Yes, use feature detection)
4. Should we abstract beyond tmux (GNU screen support)? (No, tmux only)

## Related Issues

- issues/features/worker-heartbeat-system.md (uses detection)
- issues/features/github-integration.md (uses nudging)
- issues/improvements/worker-activity-metrics.md (uses output capture)
