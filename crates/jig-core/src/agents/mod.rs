//! Agent adapters — the interface between jig and AI coding CLIs.
//!
//! An **agent** is a CLI tool that runs inside a terminal and produces code
//! autonomously. Claude Code, Codex, and OpenCode are all viable targets.
//! The only requirements are:
//!
//! 1. **Session-based interaction via a TUI.** The agent must run inside a
//!    terminal that a multiplexer (tmux, cmux, etc.) can manage. jig
//!    orchestrates agents by creating mux windows and sending keystrokes —
//!    the agent never knows jig exists.
//!
//! 2. **Three session modes:**
//!    - [`Agent::spawn`] — start a new persistent session with a prompt.
//!    - [`Agent::resume`] — continue an existing session (ideally by session
//!      id — today we re-launch with context, but agents that support session
//!      resumption should use it).
//!    - [`Agent::once`] — a non-persistent one-shot completion (e.g. triage).
//!
//! 3. **Idempotent installation** ([`Agent::install`]). Sets up everything
//!    the agent needs to work with jig in a given repo:
//!    - Event hooks that feed agent activity into jig's event log.
//!    - A project file (e.g. `CLAUDE.md`) with repo-level instructions.
//!    - Skills/commands the agent can invoke (e.g. `.claude/skills/`).
//!    - Agent-specific settings (e.g. `.claude/settings.json`).
//!
//!    All of this is dictated by the agent's own configuration format —
//!    jig just writes the files the agent expects.
//!
//! Everything else — the TUI, the model, the tool permissions — is the
//! agent's responsibility. jig treats it as a black box that accepts
//! keystrokes and emits events.
//!
//! # Usage
//!
//! ```no_run
//! use jig_core::agents::Agent;
//! use jig_core::Prompt;
//!
//! let agent = Agent::from_config("claude", Some("opus"), &[]).unwrap();
//!
//! // Generate a shell command to send to a mux window
//! let prompt = Prompt::new("Fix the auth bug in {{file}}")
//!     .var("file", "src/login.rs");
//! let cmd = agent.spawn(prompt).unwrap();
//! ```

pub mod claude;

use std::fmt;
use std::path::Path;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

pub use claude::ClaudeCode;

use crate::prompt::Prompt;

/// Tools that are always blocked for spawned workers.
pub const DEFAULT_DISALLOWED_TOOLS: &[&str] = &["Bash(gh pr create:*)", "Bash(gh pr merge:*)"];

/// Jig hook types — agent-agnostic events that jig needs to observe.
///
/// Each variant carries a shell script that writes the event to jig's
/// event log. Backends map these to their own event naming convention
/// (e.g. Claude Code calls [`ToolUseEnd`](HookType::ToolUseEnd) `"PostToolUse"`).
#[derive(Debug, Clone, Copy)]
pub enum HookType {
    ToolUseEnd,
    Notification,
    Stop,
}

impl HookType {
    pub const ALL: &[HookType] = &[Self::ToolUseEnd, Self::Notification, Self::Stop];

    /// The agent-agnostic shell script for this hook type.
    pub fn script(&self) -> &'static str {
        match self {
            Self::ToolUseEnd => include_str!("scripts/PostToolUse.sh"),
            Self::Notification => include_str!("scripts/Notification.sh"),
            Self::Stop => include_str!("scripts/Stop.sh"),
        }
    }
}

/// Known agent backends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentKind {
    Claude,
}

impl AgentKind {
    pub const ALL: &[AgentKind] = &[AgentKind::Claude];
}

impl fmt::Display for AgentKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Claude => f.write_str("claude"),
        }
    }
}

impl FromStr for AgentKind {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "claude" => Ok(Self::Claude),
            _ => Err(format!("unknown agent: {s}")),
        }
    }
}

/// Result of [`Agent::install`] — which hooks were created vs already present.
#[derive(Debug, Default)]
pub struct InstallResult {
    pub installed: Vec<String>,
    pub skipped: Vec<String>,
    /// Paths that should be made executable (scripts the backend wrote).
    pub executables: Vec<std::path::PathBuf>,
}

/// Backend trait — one impl per agent. Not public; callers use [`Agent`].
///
/// Accessor methods return static data (trait consts aren't dyn-compatible).
///
/// Each backend defines its own model enum and validates model strings
/// via [`validate_model`](Self::validate_model). The [`default_model`](Self::default_model)
/// method returns the fallback when no model is configured.
pub(crate) trait AgentBackend: Send + Sync {
    fn kind(&self) -> AgentKind;
    fn command(&self) -> &str;
    fn project_file(&self) -> &Path;
    fn skills_dir(&self) -> &Path;
    fn skill_file(&self) -> &Path;
    fn settings_file(&self) -> Option<&Path>;
    fn settings_content(&self) -> Option<&str>;

    /// Map a jig hook type to this agent's event name, or `None` if unsupported.
    fn hook_event_name(&self, hook: HookType) -> Option<&str>;

    /// Check if a model string is valid for this backend.
    fn validate_model(&self, model: &str) -> bool;

    /// The default model for this backend.
    fn default_model(&self) -> &str;

    fn spawn(&self, prompt: &str, model: &str, disallowed_tools: &[String]) -> String;
    fn resume(&self, prompt: &str, model: &str, disallowed_tools: &[String]) -> String;
    fn once(&self, prompt: &str, model: &str, allowed_tools: &[&str]) -> Vec<String>;

    /// Run the agent CLI's version/health command and return the version string.
    fn health(&self) -> crate::error::Result<String>;

    /// Install hooks into the agent's configuration. Receives resolved
    /// `(event_name, script_content)` pairs — the backend just needs to
    /// write them according to its own conventions.
    fn install(&self, hooks: &[(&str, &str)]) -> crate::error::Result<InstallResult>;
}

/// A handle to an AI coding agent.
///
/// Wraps a backend ([`AgentBackend`]) and a validated model. The model
/// is validated at construction time by the backend — you cannot create
/// an Agent with a model the backend doesn't support.
///
/// See the [module docs](self) for the full contract an agent must satisfy.
pub struct Agent {
    inner: Box<dyn AgentBackend>,
    model: String,
    disallowed_tools: Vec<String>,
}

impl Agent {
    /// Create an agent from config strings.
    ///
    /// `model` is optional — if `None`, uses the backend's default.
    /// `extra_disallowed_tools` are merged with [`DEFAULT_DISALLOWED_TOOLS`].
    ///
    /// Returns `None` if the kind is unknown or the model is not
    /// supported by that backend.
    pub fn from_config(
        kind: &str,
        model: Option<&str>,
        extra_disallowed_tools: &[String],
    ) -> Option<Self> {
        let k = kind.parse::<AgentKind>().ok()?;
        let inner: Box<dyn AgentBackend> = match k {
            AgentKind::Claude => Box::new(ClaudeCode),
        };
        let model_owned = model
            .unwrap_or_else(|| inner.default_model())
            .to_string();
        if !inner.validate_model(&model_owned) {
            return None;
        }
        let mut disallowed: Vec<String> = DEFAULT_DISALLOWED_TOOLS
            .iter()
            .map(|s| s.to_string())
            .collect();
        for tool in extra_disallowed_tools {
            if !disallowed.contains(tool) {
                disallowed.push(tool.clone());
            }
        }
        Some(Self {
            inner,
            model: model_owned,
            disallowed_tools: disallowed,
        })
    }

    pub fn kind(&self) -> AgentKind {
        self.inner.kind()
    }
    /// The validated model string.
    pub fn model(&self) -> &str {
        &self.model
    }
    pub fn name(&self) -> String {
        self.kind().to_string()
    }
    /// The CLI binary name (e.g. `"claude"`).
    pub fn command(&self) -> &str {
        self.inner.command()
    }
    /// Repo-level project file the agent reads (e.g. `CLAUDE.md`).
    pub fn project_file(&self) -> &Path {
        self.inner.project_file()
    }
    /// Directory for agent skills/commands.
    pub fn skills_dir(&self) -> &Path {
        self.inner.skills_dir()
    }
    /// Filename for individual skill definitions.
    pub fn skill_file(&self) -> &Path {
        self.inner.skill_file()
    }
    /// Agent-specific settings file, if any.
    pub fn settings_file(&self) -> Option<&Path> {
        self.inner.settings_file()
    }
    /// Default content for the settings file.
    pub fn settings_content(&self) -> Option<&str> {
        self.inner.settings_content()
    }

    /// Generate a shell command that starts a new agent session.
    ///
    /// The returned string is meant to be sent as keystrokes to a mux
    /// window — it is NOT meant to be parsed or exec'd directly.
    pub fn spawn(&self, prompt: Prompt) -> crate::error::Result<String> {
        let rendered = prompt.render()?;
        Ok(self.inner.spawn(&rendered, &self.model, &self.disallowed_tools))
    }

    /// Generate a shell command that continues an existing session.
    // TODO: accept an optional session id for agents that support resumption
    pub fn resume(&self, prompt: Prompt) -> crate::error::Result<String> {
        let rendered = prompt.render()?;
        Ok(self.inner.resume(&rendered, &self.model, &self.disallowed_tools))
    }

    /// Build argv for a non-persistent one-shot completion.
    ///
    /// Returns a `Vec<String>` suitable for `std::process::Command`.
    /// The prompt is included as the final positional argument.
    pub fn once(&self, prompt: Prompt, allowed_tools: &[&str]) -> crate::error::Result<Vec<String>> {
        let rendered = prompt.render()?;
        Ok(self.inner.once(&rendered, &self.model, allowed_tools))
    }

    /// Run the agent CLI's health check — validates it is installed and
    /// returns the version string.
    pub fn health(&self) -> crate::error::Result<String> {
        self.inner.health()
    }

    /// Idempotently install everything the agent needs to work with jig.
    ///
    /// Resolves jig's [`HookType`]s through the backend's naming convention,
    /// then hands the `(event_name, script)` pairs to the backend for
    /// installation according to the agent's own config format.
    /// Makes any returned executable paths executable.
    pub fn install(&self) -> crate::error::Result<InstallResult> {
        let hooks: Vec<(&str, &str)> = HookType::ALL
            .iter()
            .filter_map(|ht| self.inner.hook_event_name(*ht).map(|name| (name, ht.script())))
            .collect();
        let result = self.inner.install(&hooks)?;
        for path in &result.executables {
            make_executable(path)?;
        }
        Ok(result)
    }
}

#[cfg(unix)]
fn make_executable(path: &Path) -> crate::error::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(path)?.permissions();
    perms.set_mode(perms.mode() | 0o111);
    std::fs::set_permissions(path, perms)?;
    Ok(())
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> crate::error::Result<()> {
    Ok(())
}
