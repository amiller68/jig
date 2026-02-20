//! Worker state detection via pattern matching on tmux output

use regex::Regex;

use crate::config::HealthConfig;
use crate::error::Result;

/// Detected state of a worker
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerState {
    /// Actively doing something (not at any prompt)
    Working,
    /// At a shell prompt, waiting for input
    Idle,
    /// At an interactive prompt, needs user approval
    Stuck,
}

/// Detects worker state by matching patterns against tmux pane output
pub struct WorkerDetector {
    prompt_patterns: Vec<Regex>,
    stuck_patterns: Vec<Regex>,
}

impl WorkerDetector {
    /// Create a detector with default patterns for Claude Code
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

    /// Create a detector from config patterns
    pub fn from_config(config: &HealthConfig) -> Result<Self> {
        let prompt_patterns = config
            .prompt_patterns
            .iter()
            .map(|p| Regex::new(p))
            .collect::<std::result::Result<Vec<_>, _>>()?;

        let stuck_patterns = config
            .stuck_patterns
            .iter()
            .map(|p| Regex::new(p))
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(Self {
            prompt_patterns,
            stuck_patterns,
        })
    }

    /// Check if the output ends at a shell prompt (worker is idle)
    pub fn is_at_prompt(&self, output: &str) -> bool {
        let last_lines: Vec<&str> = output.lines().rev().take(3).collect();
        last_lines
            .iter()
            .any(|line| self.prompt_patterns.iter().any(|re| re.is_match(line)))
    }

    /// Check if the output contains an interactive prompt (worker is stuck)
    pub fn is_stuck(&self, output: &str) -> bool {
        self.stuck_patterns.iter().any(|re| re.is_match(output))
    }

    /// Detect the overall worker state from pane output.
    /// Priority: Stuck > Idle > Working
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

impl Default for WorkerDetector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::HealthConfig;

    #[test]
    fn test_detect_at_prompt_zsh() {
        let detector = WorkerDetector::new();
        let output = "some output\nmore output\n❯ ";
        assert!(detector.is_at_prompt(output));
    }

    #[test]
    fn test_detect_at_prompt_bash() {
        let detector = WorkerDetector::new();
        let output = "some output\nuser@host:~$ ";
        assert!(detector.is_at_prompt(output));
    }

    #[test]
    fn test_detect_at_prompt_root() {
        let detector = WorkerDetector::new();
        let output = "some output\nroot@host:~# ";
        assert!(detector.is_at_prompt(output));
    }

    #[test]
    fn test_detect_not_at_prompt() {
        let detector = WorkerDetector::new();
        let output = "Compiling...\nProcessing files...";
        assert!(!detector.is_at_prompt(output));
    }

    #[test]
    fn test_detect_stuck_proceed() {
        let detector = WorkerDetector::new();
        let output = "Would you like to proceed? (y/n)";
        assert!(detector.is_stuck(output));
    }

    #[test]
    fn test_detect_stuck_ctrl_g() {
        let detector = WorkerDetector::new();
        let output = "Some text\nctrl-g to edit\nmore text";
        assert!(detector.is_stuck(output));
    }

    #[test]
    fn test_detect_not_stuck() {
        let detector = WorkerDetector::new();
        let output = "Compiling...\nProcessing files...";
        assert!(!detector.is_stuck(output));
    }

    #[test]
    fn test_detect_state_working() {
        let detector = WorkerDetector::new();
        let output = "Compiling...\nProcessing files...";
        assert_eq!(detector.detect_state(output), WorkerState::Working);
    }

    #[test]
    fn test_detect_state_idle() {
        let detector = WorkerDetector::new();
        let output = "Done.\n❯ ";
        assert_eq!(detector.detect_state(output), WorkerState::Idle);
    }

    #[test]
    fn test_detect_state_stuck() {
        let detector = WorkerDetector::new();
        let output = "Would you like to proceed? (y/n)";
        assert_eq!(detector.detect_state(output), WorkerState::Stuck);
    }

    #[test]
    fn test_stuck_takes_priority_over_idle() {
        let detector = WorkerDetector::new();
        // Output that matches both stuck and idle
        let output = "Would you like to proceed?\n❯ ";
        assert_eq!(detector.detect_state(output), WorkerState::Stuck);
    }

    #[test]
    fn test_empty_output_is_working() {
        let detector = WorkerDetector::new();
        assert_eq!(detector.detect_state(""), WorkerState::Working);
    }

    #[test]
    fn test_custom_patterns() {
        let config = HealthConfig {
            prompt_patterns: vec![r"custom>\s*$".to_string()],
            stuck_patterns: vec![r"Approve\?".to_string()],
        };
        let detector = WorkerDetector::from_config(&config).unwrap();

        assert!(detector.is_at_prompt("custom> "));
        assert!(!detector.is_at_prompt("❯ "));
        assert!(detector.is_stuck("Approve?"));
        assert!(!detector.is_stuck("Would you like to proceed"));
    }

    #[test]
    fn test_invalid_regex_returns_error() {
        let config = HealthConfig {
            prompt_patterns: vec![r"[invalid".to_string()],
            stuck_patterns: vec![],
        };
        assert!(WorkerDetector::from_config(&config).is_err());
    }

    #[test]
    fn test_default_matches_new() {
        let default = WorkerDetector::default();
        let output = "❯ ";
        assert!(default.is_at_prompt(output));
    }
}
