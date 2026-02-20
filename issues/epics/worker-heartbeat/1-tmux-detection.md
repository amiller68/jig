# Tmux Detection

**Status:** In Review
**Priority:** High  
**Category:** Features  
**Epic:** issues/epics/worker-heartbeat/index.md  
**Depends-On:** issues/epics/worker-heartbeat/0-health-state-storage.md

## Objective

Implement tmux scraping to detect worker state: stuck at prompts, idle at shell, or actively working.

## Background

Workers run in tmux. Need to:
- Capture pane output
- Detect shell prompt patterns (idle)
- Detect interactive prompt patterns (stuck)
- Determine if worker is actively working

## Design

### Detection Module

```rust
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
    
    pub fn from_config(config: &HealthConfig) -> Result<Self> {
        Ok(Self {
            prompt_patterns: config.prompt_patterns.iter()
                .map(|p| Regex::new(p))
                .collect::<Result<Vec<_>, _>>()?,
            stuck_patterns: config.stuck_patterns.iter()
                .map(|p| Regex::new(p))
                .collect::<Result<Vec<_>, _>>()?,
        })
    }
    
    pub fn is_at_prompt(&self, output: &str) -> bool {
        let last_lines: String = output.lines()
            .rev()
            .take(3)
            .collect::<Vec<_>>()
            .join("\n");
        self.prompt_patterns.iter().any(|re| re.is_match(&last_lines))
    }
    
    pub fn is_stuck(&self, output: &str) -> bool {
        self.stuck_patterns.iter().any(|re| re.is_match(output))
    }
    
    pub fn detect_state(&self, output: &str) -> WorkerState {
        if self.is_stuck(output) {
            WorkerState::Stuck
        } else if self.is_at_prompt(output) {
            WorkerState::Idle
        } else {
            WorkerState::Working
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WorkerState {
    Working,  // Not at prompt, actively doing something
    Idle,     // At shell prompt, waiting for input
    Stuck,    // At interactive prompt, needs approval
}
```

### Tmux Integration

```rust
use std::process::Command;

pub fn capture_pane(session: &str, window: &str, lines: i32) -> Result<String> {
    let target = format!("{}:{}", session, window);
    
    let output = Command::new("tmux")
        .args(&["capture-pane", "-p", "-t", &target, "-S", &lines.to_string()])
        .output()?;
    
    if !output.status.success() {
        return Err(Error::TmuxCommand(
            String::from_utf8_lossy(&output.stderr).to_string()
        ));
    }
    
    Ok(String::from_utf8(output.stdout)?)
}

pub fn check_worker(repo_name: &str, worker_name: &str, detector: &WorkerDetector) 
    -> Result<WorkerState> 
{
    let output = capture_pane(
        &format!("jig-{}", repo_name),
        worker_name,
        -20  // Last 20 lines
    )?;
    
    Ok(detector.detect_state(&output))
}
```

### Configuration

In `jig.toml`:
```toml
[health]
# Prompt patterns (regex, worker is idle at shell)
promptPatterns = [
    "❯\\s*$",
    "\\$\\s*$",
]

# Stuck patterns (regex, worker needs approval)
stuckPatterns = [
    "Would you like to proceed",
    "ctrl-g to edit",
]
```

## Implementation

**Files:**
- `crates/jig-core/src/health/detector.rs` - detection logic
- `crates/jig-core/src/health/tmux.rs` - tmux capture utilities

**Dependencies:**
```toml
regex = "1.10"
```

## Acceptance Criteria

- [x] `WorkerDetector` with configurable patterns
- [x] `is_at_prompt()` detects shell prompts
- [x] `is_stuck()` detects interactive prompts
- [x] `detect_state()` returns Working/Idle/Stuck
- [x] `capture_pane()` scrapes tmux output
- [x] `check_worker()` combines capture + detection
- [x] Patterns loaded from `jig.toml` config
- [x] Default patterns work for Claude Code

## Testing

```rust
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

#[test]
fn test_detect_working() {
    let detector = WorkerDetector::new();
    let output = "Compiling...\nProcessing files...";
    assert!(!detector.is_at_prompt(output));
    assert!(!detector.is_stuck(output));
    assert_eq!(detector.detect_state(output), WorkerState::Working);
}

#[test]
fn test_custom_patterns() {
    let config = HealthConfig {
        prompt_patterns: vec!["custom>\\s*$".to_string()],
        stuck_patterns: vec!["Approve?".to_string()],
        ..Default::default()
    };
    let detector = WorkerDetector::from_config(&config).unwrap();
    
    assert!(detector.is_at_prompt("custom> "));
    assert!(detector.is_stuck("Approve?"));
}
```

## Next Steps

After this ticket:
- Move to ticket 2 (nudge system)
- Nudge system will use detection to decide when to nudge
