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
    /// Flags for ephemeral (one-shot, non-interactive) execution mode.
    /// Empty string means ephemeral mode is unsupported for this adapter.
    pub ephemeral_flags: &'static str,
    /// Flag to continue the most recent session in the current cwd.
    /// Empty string means session continuation is unsupported.
    pub continue_flag: &'static str,
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
    ephemeral_flags: "--print --no-session-persistence --dangerously-skip-permissions",
    continue_flag: "-c",
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
//     ephemeral_flags: "",
//     continue_flag: "",
// };

impl AgentAdapter {
    /// Returns true if this adapter supports ephemeral (one-shot) execution mode.
    pub fn supports_ephemeral(&self) -> bool {
        !self.ephemeral_flags.is_empty()
    }

    /// Returns true if this adapter supports continuing a prior session.
    pub fn supports_continue(&self) -> bool {
        !self.continue_flag.is_empty()
    }

    /// Build a command string for ephemeral (one-shot, non-interactive) execution.
    ///
    /// The command includes the adapter's ephemeral flags, optional allowed-tools,
    /// and the prompt as a single-quoted shell argument.
    pub fn build_ephemeral_command(&self, prompt: &str, allowed_tools: &[&str]) -> String {
        let mut cmd = format!("{} {}", self.command, self.ephemeral_flags);

        if !allowed_tools.is_empty() {
            let tools = allowed_tools.join(",");
            cmd = format!("{} --allowed-tools \"{}\"", cmd, tools);
        }

        // Escape single quotes in prompt
        let escaped = prompt.replace('\'', "'\\''");
        format!("{} '{}'", cmd, escaped)
    }
}

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

/// Build a triage command for ephemeral execution with `--model` and
/// `--allowed-tools` restrictions. The prompt is supplied on stdin by
/// redirecting from `prompt_file` — Claude Code has no file-based prompt
/// flag, and stdin redirection avoids any shell-escaping pitfalls with
/// long markdown prompts.
pub fn build_triage_command(
    adapter: &AgentAdapter,
    prompt_file: &std::path::Path,
    model: &str,
    allowed_tools: &[&str],
) -> String {
    let mut cmd = format!("{} {}", adapter.command, adapter.ephemeral_flags);
    cmd = format!("{} --model {}", cmd, model);
    if !allowed_tools.is_empty() {
        let tools = allowed_tools.join(",");
        cmd = format!("{} --allowed-tools \"{}\"", cmd, tools);
    }
    cmd = format!("{} < {}", cmd, prompt_file.display());
    cmd
}

/// Build a triage command as an argv vector for direct subprocess execution.
///
/// Unlike [`build_triage_command`] which returns a shell string for tmux,
/// this returns a `Vec<String>` suitable for `std::process::Command`. The
/// prompt is read from `prompt_file` and piped to stdin by the caller.
pub fn build_triage_argv(
    adapter: &AgentAdapter,
    model: &str,
    allowed_tools: &[&str],
) -> Vec<String> {
    let mut argv: Vec<String> = vec![adapter.command.to_string()];

    // Add ephemeral flags as individual arguments
    for flag in adapter.ephemeral_flags.split_whitespace() {
        argv.push(flag.to_string());
    }

    argv.push("--model".to_string());
    argv.push(model.to_string());

    if !allowed_tools.is_empty() {
        argv.push("--allowed-tools".to_string());
        argv.push(allowed_tools.join(","));
    }

    argv
}

/// Build a resume command that continues the most recent session in the current cwd.
///
/// Uses the adapter's `continue_flag` (e.g. `-c`) instead of a prompt argument,
/// so the agent picks up its prior session transcript rather than starting fresh.
pub fn build_resume_command(adapter: &AgentAdapter) -> String {
    let mut cmd = adapter.command.to_string();
    if !adapter.continue_flag.is_empty() {
        cmd.push(' ');
        cmd.push_str(adapter.continue_flag);
    }
    if !adapter.auto_flag.is_empty() {
        cmd.push(' ');
        cmd.push_str(adapter.auto_flag);
    }
    cmd
}

/// Build the spawn command for an agent (always appends auto_flag)
pub fn build_spawn_command(adapter: &AgentAdapter, context: Option<&str>) -> String {
    let mut cmd = adapter.command.to_string();

    if let Some(ctx) = context {
        // Escape single quotes in context
        let escaped = ctx.replace('\'', "'\\''");
        cmd = format!("{} '{}'", cmd, escaped);
    }

    if !adapter.auto_flag.is_empty() {
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
    fn test_build_spawn_command_no_context() {
        let adapter = &CLAUDE_CODE;
        let cmd = build_spawn_command(adapter, None);
        assert_eq!(cmd, "claude --dangerously-skip-permissions");
    }

    #[test]
    fn test_build_spawn_command_with_context() {
        let adapter = &CLAUDE_CODE;
        let cmd = build_spawn_command(adapter, Some("hello world"));
        assert_eq!(cmd, "claude 'hello world' --dangerously-skip-permissions");
    }

    #[test]
    fn test_build_spawn_command_escapes_quotes() {
        let adapter = &CLAUDE_CODE;
        let cmd = build_spawn_command(adapter, Some("it's a test"));
        assert_eq!(
            cmd,
            "claude 'it'\\''s a test' --dangerously-skip-permissions"
        );
    }

    #[test]
    fn test_supports_ephemeral_claude() {
        assert!(CLAUDE_CODE.supports_ephemeral());
    }

    #[test]
    fn test_supports_ephemeral_false_when_empty() {
        let adapter = AgentAdapter {
            agent_type: AgentType::Claude,
            name: "test",
            command: "test",
            skills_dir: "",
            skill_file: "",
            settings_file: None,
            project_file: "",
            auto_flag: "",
            ephemeral_flags: "",
            continue_flag: "",
        };
        assert!(!adapter.supports_ephemeral());
    }

    #[test]
    fn test_build_resume_command_claude() {
        let cmd = build_resume_command(&CLAUDE_CODE);
        assert_eq!(cmd, "claude -c --dangerously-skip-permissions");
    }

    #[test]
    fn test_build_resume_command_no_continue_flag() {
        let adapter = AgentAdapter {
            agent_type: AgentType::Claude,
            name: "test",
            command: "test-agent",
            skills_dir: "",
            skill_file: "",
            settings_file: None,
            project_file: "",
            auto_flag: "--auto",
            ephemeral_flags: "",
            continue_flag: "",
        };
        let cmd = build_resume_command(&adapter);
        assert_eq!(cmd, "test-agent --auto");
    }

    #[test]
    fn test_supports_continue() {
        assert!(CLAUDE_CODE.supports_continue());
        let adapter = AgentAdapter {
            agent_type: AgentType::Claude,
            name: "test",
            command: "test",
            skills_dir: "",
            skill_file: "",
            settings_file: None,
            project_file: "",
            auto_flag: "",
            ephemeral_flags: "",
            continue_flag: "",
        };
        assert!(!adapter.supports_continue());
    }

    #[test]
    fn test_build_ephemeral_command() {
        let cmd = CLAUDE_CODE
            .build_ephemeral_command("review this code", &["Read", "Grep", "Glob", "Bash(jig:*)"]);
        assert_eq!(
            cmd,
            "claude --print --no-session-persistence --dangerously-skip-permissions \
             --allowed-tools \"Read,Grep,Glob,Bash(jig:*)\" 'review this code'"
        );
    }

    #[test]
    fn test_build_ephemeral_command_escapes_quotes() {
        let cmd = CLAUDE_CODE.build_ephemeral_command("it's a test", &[]);
        assert_eq!(
            cmd,
            "claude --print --no-session-persistence --dangerously-skip-permissions \
             'it'\\''s a test'"
        );
    }

    #[test]
    fn test_build_ephemeral_command_empty_tools() {
        let cmd = CLAUDE_CODE.build_ephemeral_command("hello", &[]);
        assert_eq!(
            cmd,
            "claude --print --no-session-persistence --dangerously-skip-permissions 'hello'"
        );
    }

    #[test]
    fn test_build_triage_command() {
        let prompt_file = std::path::Path::new("/tmp/worktree/.jig/triage-prompt.md");
        let cmd = build_triage_command(
            &CLAUDE_CODE,
            prompt_file,
            "sonnet",
            &["Read", "Glob", "Grep", "Bash(jig *)", "mcp__linear*"],
        );
        assert_eq!(
            cmd,
            "claude --print --no-session-persistence --dangerously-skip-permissions \
             --model sonnet \
             --allowed-tools \"Read,Glob,Grep,Bash(jig *),mcp__linear*\" \
             < /tmp/worktree/.jig/triage-prompt.md"
        );
    }

    #[test]
    fn test_build_triage_argv() {
        let argv = build_triage_argv(
            &CLAUDE_CODE,
            "sonnet",
            &["Read", "Glob", "Grep", "Bash(jig *)", "mcp__linear*"],
        );
        assert_eq!(
            argv,
            vec![
                "claude",
                "--print",
                "--no-session-persistence",
                "--dangerously-skip-permissions",
                "--model",
                "sonnet",
                "--allowed-tools",
                "Read,Glob,Grep,Bash(jig *),mcp__linear*",
            ]
        );
    }

    #[test]
    fn test_build_triage_argv_no_tools() {
        let argv = build_triage_argv(&CLAUDE_CODE, "opus", &[]);
        assert_eq!(
            argv,
            vec![
                "claude",
                "--print",
                "--no-session-persistence",
                "--dangerously-skip-permissions",
                "--model",
                "opus",
            ]
        );
    }

    #[test]
    fn test_build_triage_command_custom_model() {
        let prompt_file = std::path::Path::new("/tmp/triage.md");
        let cmd = build_triage_command(&CLAUDE_CODE, prompt_file, "opus", &[]);
        assert_eq!(
            cmd,
            "claude --print --no-session-persistence --dangerously-skip-permissions \
             --model opus \
             < /tmp/triage.md"
        );
    }
}
