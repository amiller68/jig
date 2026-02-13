//! Agent adapters for different AI coding assistants
//!
//! Each adapter knows how to lay out files for a specific agent (Claude Code, Cursor, etc.)

/// Agent type enum for compile-time safe matching
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentType {
    Claude,
    // Cursor,  // Future
}

/// Agent adapter containing all agent-specific configuration
#[derive(Debug, Clone)]
pub struct AgentAdapter {
    /// Agent type for pattern matching
    pub agent_type: AgentType,
    /// Agent name (e.g., "claude", "cursor")
    pub name: &'static str,
    /// Command to invoke the agent
    pub command: &'static str,
    /// Directory for skills (relative to repo root)
    pub skills_dir: &'static str,
    /// Skill file name (e.g., "SKILL.md", "rule.mdc")
    pub skill_file: &'static str,
    /// Settings file path (relative to repo root), if any
    pub settings_file: Option<&'static str>,
    /// Project context file (e.g., "CLAUDE.md", ".cursorrules")
    pub project_file: &'static str,
    /// Flag to run in auto mode
    pub auto_flag: &'static str,
}

/// Claude Code adapter
pub const CLAUDE_CODE: AgentAdapter = AgentAdapter {
    agent_type: AgentType::Claude,
    name: "claude",
    command: "claude",
    skills_dir: ".claude/skills",
    skill_file: "SKILL.md",
    settings_file: Some(".claude/settings.json"),
    project_file: "CLAUDE.md",
    auto_flag: "--dangerously-skip-permissions",
};

// Future adapters:
// pub const CURSOR: AgentAdapter = AgentAdapter {
//     name: "cursor",
//     command: "cursor",
//     skills_dir: ".cursor/rules",
//     skill_file: "rule.mdc",
//     settings_file: None,
//     project_file: ".cursorrules",
//     auto_flag: "",
// };

/// Get an adapter by name
pub fn get_adapter(name: &str) -> Option<&'static AgentAdapter> {
    match name {
        "claude" => Some(&CLAUDE_CODE),
        // "cursor" => Some(&CURSOR),
        _ => None,
    }
}

/// Get list of supported agent names
pub fn supported_agents() -> &'static [&'static str] {
    &["claude"]
}

/// Build the spawn command for an agent
pub fn build_spawn_command(adapter: &AgentAdapter, context: Option<&str>, auto: bool) -> String {
    let mut cmd = adapter.command.to_string();

    if let Some(ctx) = context {
        // Escape single quotes in context
        let escaped = ctx.replace('\'', "'\\''");
        cmd = format!("{} '{}'", cmd, escaped);
    }

    if auto && !adapter.auto_flag.is_empty() {
        cmd.push(' ');
        cmd.push_str(adapter.auto_flag);
    }

    cmd
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_adapter_claude() {
        let adapter = get_adapter("claude").unwrap();
        assert_eq!(adapter.name, "claude");
        assert_eq!(adapter.command, "claude");
        assert_eq!(adapter.skills_dir, ".claude/skills");
        assert_eq!(adapter.skill_file, "SKILL.md");
        assert_eq!(adapter.project_file, "CLAUDE.md");
    }

    #[test]
    fn test_get_adapter_unknown() {
        assert!(get_adapter("unknown").is_none());
    }

    #[test]
    fn test_build_spawn_command_simple() {
        let adapter = &CLAUDE_CODE;
        let cmd = build_spawn_command(adapter, None, false);
        assert_eq!(cmd, "claude");
    }

    #[test]
    fn test_build_spawn_command_with_context() {
        let adapter = &CLAUDE_CODE;
        let cmd = build_spawn_command(adapter, Some("hello world"), false);
        assert_eq!(cmd, "claude 'hello world'");
    }

    #[test]
    fn test_build_spawn_command_with_auto() {
        let adapter = &CLAUDE_CODE;
        let cmd = build_spawn_command(adapter, None, true);
        assert_eq!(cmd, "claude --dangerously-skip-permissions");
    }

    #[test]
    fn test_build_spawn_command_full() {
        let adapter = &CLAUDE_CODE;
        let cmd = build_spawn_command(adapter, Some("fix bug"), true);
        assert_eq!(cmd, "claude 'fix bug' --dangerously-skip-permissions");
    }

    #[test]
    fn test_build_spawn_command_escapes_quotes() {
        let adapter = &CLAUDE_CODE;
        let cmd = build_spawn_command(adapter, Some("it's a test"), false);
        assert_eq!(cmd, "claude 'it'\\''s a test'");
    }
}
