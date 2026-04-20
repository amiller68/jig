//! Claude Code agent backend.

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
        Some(r#"{
  "permissions": {
    "allow": [],
    "deny": []
  }
}
"#)
    }
    fn health_command(&self) -> &[&str] {
        &["claude", "--version"]
    }

    fn spawn_command(&self, context: Option<&str>, disallowed_tools: &[String]) -> String {
        let mut cmd = COMMAND.to_string();

        if let Some(ctx) = context {
            let escaped = ctx.replace('\'', "'\\''");
            cmd = format!("{cmd} '{escaped}'");
        }

        cmd.push_str(" --dangerously-skip-permissions");

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

    fn resume_command(&self) -> String {
        format!("{COMMAND} -c --dangerously-skip-permissions")
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
}

#[cfg(test)]
mod tests {
    use super::super::Agent;

    fn agent() -> Agent {
        Agent::from_kind(super::AgentKind::Claude)
    }

    #[test]
    fn spawn_no_context() {
        let cmd = agent().spawn_command(None, &[]);
        assert_eq!(
            cmd,
            "claude --dangerously-skip-permissions \
             --disallowedTools \"Bash(gh pr create:*),Bash(gh pr merge:*)\""
        );
    }

    #[test]
    fn spawn_with_context() {
        let cmd = agent().spawn_command(Some("hello world"), &[]);
        assert_eq!(
            cmd,
            "claude 'hello world' --dangerously-skip-permissions \
             --disallowedTools \"Bash(gh pr create:*),Bash(gh pr merge:*)\""
        );
    }

    #[test]
    fn spawn_escapes_quotes() {
        let cmd = agent().spawn_command(Some("it's a test"), &[]);
        assert_eq!(
            cmd,
            "claude 'it'\\''s a test' --dangerously-skip-permissions \
             --disallowedTools \"Bash(gh pr create:*),Bash(gh pr merge:*)\""
        );
    }

    #[test]
    fn spawn_extra_disallowed() {
        let extra = vec!["Bash(rm -rf:*)".to_string()];
        let cmd = agent().spawn_command(Some("do work"), &extra);
        assert_eq!(
            cmd,
            "claude 'do work' --dangerously-skip-permissions \
             --disallowedTools \"Bash(gh pr create:*),Bash(gh pr merge:*),Bash(rm -rf:*)\""
        );
    }

    #[test]
    fn spawn_deduplicates() {
        let extra = vec!["Bash(gh pr create:*)".to_string()];
        let cmd = agent().spawn_command(Some("work"), &extra);
        assert_eq!(
            cmd,
            "claude 'work' --dangerously-skip-permissions \
             --disallowedTools \"Bash(gh pr create:*),Bash(gh pr merge:*)\""
        );
    }

    #[test]
    fn resume() {
        assert_eq!(
            agent().resume_command(),
            "claude -c --dangerously-skip-permissions"
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
