//! Agent adapters for different AI coding assistants.
//!
//! Public API is the [`Agent`] struct — a concrete handle wrapping a
//! trait-object backend. Call [`Agent::from_kind`] or [`Agent::from_name`].

mod claude;

use std::fmt;
use std::path::Path;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

pub use claude::ClaudeCode;

/// Tools that are always blocked for spawned workers.
pub const DEFAULT_DISALLOWED_TOOLS: &[&str] = &["Bash(gh pr create:*)", "Bash(gh pr merge:*)"];

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

/// Backend trait — one impl per agent. Not public; callers use [`Agent`].
pub(crate) trait AgentBackend: Send + Sync {
    fn kind(&self) -> AgentKind;
    fn command(&self) -> &str;
    fn project_file(&self) -> &Path;
    fn skills_dir(&self) -> &Path;
    fn skill_file(&self) -> &Path;
    fn settings_file(&self) -> Option<&Path>;
    fn settings_content(&self) -> Option<&str>;
    fn health_command(&self) -> &[&str];

    fn spawn_command(&self, context: Option<&str>, disallowed_tools: &[String]) -> String;
    fn resume_command(&self) -> String;
    fn ephemeral_command(&self, prompt: &str, allowed_tools: &[&str]) -> String;
    fn triage_argv(&self, model: &str, allowed_tools: &[&str]) -> Vec<String>;
}

/// Concrete agent handle.
pub struct Agent {
    inner: Box<dyn AgentBackend>,
}

impl Agent {
    pub fn from_kind(kind: AgentKind) -> Self {
        match kind {
            AgentKind::Claude => Self {
                inner: Box::new(ClaudeCode),
            },
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        name.parse::<AgentKind>().ok().map(Self::from_kind)
    }

    pub fn kind(&self) -> AgentKind {
        self.inner.kind()
    }
    pub fn name(&self) -> String {
        self.kind().to_string()
    }
    pub fn command(&self) -> &str {
        self.inner.command()
    }
    pub fn project_file(&self) -> &Path {
        self.inner.project_file()
    }
    pub fn skills_dir(&self) -> &Path {
        self.inner.skills_dir()
    }
    pub fn skill_file(&self) -> &Path {
        self.inner.skill_file()
    }
    pub fn settings_file(&self) -> Option<&Path> {
        self.inner.settings_file()
    }
    pub fn settings_content(&self) -> Option<&str> {
        self.inner.settings_content()
    }
    pub fn health_command(&self) -> &[&str] {
        self.inner.health_command()
    }

    pub fn spawn_command(&self, context: Option<&str>, disallowed_tools: &[String]) -> String {
        self.inner.spawn_command(context, disallowed_tools)
    }
    pub fn resume_command(&self) -> String {
        self.inner.resume_command()
    }
    pub fn ephemeral_command(&self, prompt: &str, allowed_tools: &[&str]) -> String {
        self.inner.ephemeral_command(prompt, allowed_tools)
    }
    pub fn triage_argv(&self, model: &str, allowed_tools: &[&str]) -> Vec<String> {
        self.inner.triage_argv(model, allowed_tools)
    }
}
