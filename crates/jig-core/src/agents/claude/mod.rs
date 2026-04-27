//! Claude Code agent backend.

pub mod hooks;

use std::path::Path;

use super::{AgentBackend, AgentKind, DEFAULT_DISALLOWED_TOOLS};

const COMMAND: &str = "claude";

pub struct ClaudeCode;

impl AgentBackend for ClaudeCode {
    fn kind(&self) -> AgentKind {
        AgentKind::Claude
    }
    fn command(&self) -> &str {
        COMMAND
    }
    fn project_file(&self) -> &Path {
        Path::new("CLAUDE.md")
    }
    fn skills_dir(&self) -> &Path {
        Path::new(".claude/skills")
    }
    fn skill_file(&self) -> &Path {
        Path::new("SKILL.md")
    }
    fn settings_file(&self) -> Option<&Path> {
        Some(Path::new(".claude/settings.json"))
    }
    fn settings_content(&self) -> Option<&str> {
        Some(include_str!("settings.json"))
    }
    fn health_command(&self) -> &[&str] {
        &["claude", "--version"]
    }

    fn spawn_command(&self, context: &str, disallowed_tools: &[String]) -> String {
        let escaped = context.replace('\'', "'\\''");
        let mut cmd = format!("{COMMAND} '{escaped}' --dangerously-skip-permissions");

        let mut all_tools: Vec<&str> = DEFAULT_DISALLOWED_TOOLS.to_vec();
        for tool in disallowed_tools {
            if !all_tools.contains(&tool.as_str()) {
                all_tools.push(tool.as_str());
            }
        }
        if !all_tools.is_empty() {
            cmd = format!("{cmd} --disallowedTools \"{}\"", all_tools.join(","));
        }

        cmd
    }

    fn resume_command(&self, context: &str) -> String {
        let escaped = context.replace('\'', "'\\''");
        format!("{COMMAND} -c '{escaped}' --dangerously-skip-permissions")
    }

    fn ephemeral_command(&self, prompt: &str, allowed_tools: &[&str]) -> String {
        let mut cmd =
            format!("{COMMAND} --print --no-session-persistence --dangerously-skip-permissions");

        if !allowed_tools.is_empty() {
            cmd = format!("{cmd} --allowed-tools \"{}\"", allowed_tools.join(","));
        }

        let escaped = prompt.replace('\'', "'\\''");
        format!("{cmd} '{escaped}'")
    }

    fn triage_argv(&self, model: &str, allowed_tools: &[&str]) -> Vec<String> {
        let mut argv = vec![
            COMMAND.to_string(),
            "--print".to_string(),
            "--no-session-persistence".to_string(),
            "--dangerously-skip-permissions".to_string(),
            "--model".to_string(),
            model.to_string(),
        ];

        if !allowed_tools.is_empty() {
            argv.push("--allowed-tools".to_string());
            argv.push(allowed_tools.join(","));
        }

        argv
    }

    fn install_hooks(&self) -> crate::error::Result<super::AgentHookResult> {
        let result = hooks::install_claude_hooks()?;
        Ok(super::AgentHookResult {
            installed: result.installed,
            skipped: result.skipped,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::Agent;

    fn agent() -> Agent {
        Agent::from_kind(super::AgentKind::Claude)
    }

    #[test]
    fn spawn_with_context() {
        let cmd = agent().spawn_command("hello world");
        assert_eq!(
            cmd,
            "claude 'hello world' --dangerously-skip-permissions \
             --disallowedTools \"Bash(gh pr create:*),Bash(gh pr merge:*)\""
        );
    }

    #[test]
    fn spawn_escapes_quotes() {
        let cmd = agent().spawn_command("it's a test");
        assert_eq!(
            cmd,
            "claude 'it'\\''s a test' --dangerously-skip-permissions \
             --disallowedTools \"Bash(gh pr create:*),Bash(gh pr merge:*)\""
        );
    }

    #[test]
    fn spawn_extra_disallowed() {
        let cmd = agent()
            .with_disallowed_tools(vec!["Bash(rm -rf:*)".to_string()])
            .spawn_command("do work");
        assert_eq!(
            cmd,
            "claude 'do work' --dangerously-skip-permissions \
             --disallowedTools \"Bash(gh pr create:*),Bash(gh pr merge:*),Bash(rm -rf:*)\""
        );
    }

    #[test]
    fn spawn_deduplicates() {
        let cmd = agent()
            .with_disallowed_tools(vec!["Bash(gh pr create:*)".to_string()])
            .spawn_command("work");
        assert_eq!(
            cmd,
            "claude 'work' --dangerously-skip-permissions \
             --disallowedTools \"Bash(gh pr create:*),Bash(gh pr merge:*)\""
        );
    }

    #[test]
    fn resume() {
        assert_eq!(
            agent().resume_command("continue working"),
            "claude -c 'continue working' --dangerously-skip-permissions"
        );
    }

    #[test]
    fn ephemeral() {
        let cmd =
            agent().ephemeral_command("review this code", &["Read", "Grep", "Glob", "Bash(jig:*)"]);
        assert_eq!(
            cmd,
            "claude --print --no-session-persistence --dangerously-skip-permissions \
             --allowed-tools \"Read,Grep,Glob,Bash(jig:*)\" 'review this code'"
        );
    }

    #[test]
    fn ephemeral_no_tools() {
        let cmd = agent().ephemeral_command("hello", &[]);
        assert_eq!(
            cmd,
            "claude --print --no-session-persistence --dangerously-skip-permissions 'hello'"
        );
    }

    #[test]
    fn triage() {
        let argv = agent().triage_argv("sonnet", &["Read", "Glob"]);
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
                "Read,Glob"
            ]
        );
    }

    #[test]
    fn health() {
        assert_eq!(agent().health_command(), &["claude", "--version"]);
    }
}
