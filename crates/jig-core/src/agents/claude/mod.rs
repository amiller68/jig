//! Claude Code agent backend.

use std::fmt;
use std::path::Path;
use std::str::FromStr;

use super::{AgentBackend, AgentKind, HookType, InstallResult, DEFAULT_DISALLOWED_TOOLS};

const COMMAND: &str = "claude";

/// Models supported by Claude Code's `--model` flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Model {
    Sonnet,
    Opus,
    Haiku,
}

impl Model {
    pub const ALL: &[Model] = &[Self::Sonnet, Self::Opus, Self::Haiku];
    pub const DEFAULT: Model = Model::Sonnet;

    pub fn as_cli_arg(&self) -> &str {
        match self {
            Self::Sonnet => "sonnet",
            Self::Opus => "opus",
            Self::Haiku => "haiku",
        }
    }
}

impl fmt::Display for Model {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_cli_arg())
    }
}

impl FromStr for Model {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "sonnet" => Ok(Self::Sonnet),
            "opus" => Ok(Self::Opus),
            "haiku" => Ok(Self::Haiku),
            _ => Err(format!("unknown claude model: {s} (expected sonnet, opus, or haiku)")),
        }
    }
}

pub struct ClaudeCode;

impl AgentBackend for ClaudeCode {
    fn kind(&self) -> AgentKind { AgentKind::Claude }
    fn command(&self) -> &str { COMMAND }
    fn project_file(&self) -> &Path { Path::new("CLAUDE.md") }
    fn skills_dir(&self) -> &Path { Path::new(".claude/skills") }
    fn skill_file(&self) -> &Path { Path::new("SKILL.md") }
    fn settings_file(&self) -> Option<&Path> { Some(Path::new(".claude/settings.json")) }
    fn settings_content(&self) -> Option<&str> { Some(include_str!("settings.json")) }

    fn hook_event_name(&self, hook: HookType) -> Option<&str> {
        match hook {
            HookType::ToolUseEnd => Some("PostToolUse"),
            HookType::Notification => Some("Notification"),
            HookType::Stop => Some("Stop"),
        }
    }

    fn validate_model(&self, model: &str) -> bool {
        model.parse::<Model>().is_ok()
    }

    fn default_model(&self) -> &str {
        Model::DEFAULT.as_cli_arg()
    }

    fn spawn(&self, prompt: &str, model: &str, disallowed_tools: &[String]) -> String {
        let escaped = prompt.replace('\'', "'\\''");
        let mut cmd = format!(
            "{COMMAND} '{escaped}' --dangerously-skip-permissions --model {model}"
        );

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

    fn resume(&self, prompt: &str, model: &str, disallowed_tools: &[String]) -> String {
        let escaped = prompt.replace('\'', "'\\''");
        let mut cmd = format!(
            "{COMMAND} -c '{escaped}' --dangerously-skip-permissions --model {model}"
        );

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

    fn once(&self, prompt: &str, model: &str, allowed_tools: &[&str]) -> Vec<String> {
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

        argv.push(prompt.to_string());
        argv
    }

    fn health(&self) -> crate::error::Result<String> {
        let output = std::process::Command::new(COMMAND)
            .arg("--version")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output()
            .map_err(|e| crate::Error::Custom(format!("failed to run `claude --version`: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(crate::Error::Custom(format!(
                "claude --version failed: {}",
                stderr.trim()
            )));
        }

        let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Ok(version)
    }

    fn install(&self, hooks: &[(&str, &str)]) -> crate::error::Result<InstallResult> {
        let home =
            dirs::home_dir().ok_or_else(|| crate::Error::Custom("no home directory".into()))?;
        let hooks_dir = home.join(".claude").join("hooks");
        let settings_path = home.join(".claude").join("settings.json");

        std::fs::create_dir_all(&hooks_dir)?;

        let mut result = InstallResult::default();

        for (name, content) in hooks {
            let path = hooks_dir.join(name);
            let existed = path.exists();
            std::fs::write(&path, content)?;
            result.executables.push(path);
            if existed {
                result.skipped.push(name.to_string());
            } else {
                result.installed.push(name.to_string());
            }
        }

        // Register in ~/.claude/settings.json
        let mut settings: serde_json::Value = if settings_path.exists() {
            let content = std::fs::read_to_string(&settings_path)?;
            serde_json::from_str(&content).unwrap_or_else(|_| serde_json::json!({}))
        } else {
            serde_json::json!({})
        };

        let hooks_obj = settings
            .as_object_mut()
            .ok_or_else(|| crate::Error::Custom("settings.json is not an object".into()))?
            .entry("hooks")
            .or_insert_with(|| serde_json::json!({}));

        let hooks_map = hooks_obj
            .as_object_mut()
            .ok_or_else(|| crate::Error::Custom("hooks is not an object".into()))?;

        let mut modified = false;

        for (event_name, _) in hooks {
            let script_path = hooks_dir.join(event_name);
            let script_path_str = script_path.to_string_lossy().to_string();

            let event_hooks = hooks_map
                .entry(*event_name)
                .or_insert_with(|| serde_json::json!([]));

            let entries = match event_hooks.as_array_mut() {
                Some(arr) => arr,
                None => continue,
            };

            let has_jig_hook = entries.iter().any(|entry| {
                entry
                    .get("hooks")
                    .and_then(|h| h.as_array())
                    .map(|hooks| {
                        hooks.iter().any(|h| {
                            h.get("command")
                                .and_then(|c| c.as_str())
                                .map(|c| c.contains("jig"))
                                .unwrap_or(false)
                        })
                    })
                    .unwrap_or(false)
            });

            if !has_jig_hook {
                entries.push(serde_json::json!({
                    "hooks": [{
                        "type": "command",
                        "command": script_path_str
                    }]
                }));
                modified = true;
            }
        }

        if modified {
            if let Some(parent) = settings_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let content = serde_json::to_string_pretty(&settings)
                .map_err(|e| crate::Error::Custom(format!("failed to serialize settings: {e}")))?;
            std::fs::write(&settings_path, content)?;
        }

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Agent, HookType};
    use super::Model;
    use crate::prompt::Prompt;

    fn agent() -> Agent {
        Agent::from_config("claude", None, &[]).unwrap()
    }

    #[test]
    fn spawn_default_model() {
        let cmd = agent().spawn(Prompt::new("hello world")).unwrap();
        assert_eq!(
            cmd,
            "claude 'hello world' --dangerously-skip-permissions --model sonnet \
             --disallowedTools \"Bash(gh pr create:*),Bash(gh pr merge:*)\""
        );
    }

    #[test]
    fn spawn_with_opus() {
        let cmd = Agent::from_config("claude", Some("opus"), &[]).unwrap()
            .spawn(Prompt::new("do work")).unwrap();
        assert!(cmd.contains("--model opus"));
    }

    #[test]
    fn spawn_escapes_quotes() {
        let cmd = agent().spawn(Prompt::new("it's a test")).unwrap();
        assert_eq!(
            cmd,
            "claude 'it'\\''s a test' --dangerously-skip-permissions --model sonnet \
             --disallowedTools \"Bash(gh pr create:*),Bash(gh pr merge:*)\""
        );
    }

    #[test]
    fn spawn_extra_disallowed() {
        let cmd = Agent::from_config("claude", None, &["Bash(rm -rf:*)".into()]).unwrap()
            .spawn(Prompt::new("do work"))
            .unwrap();
        assert_eq!(
            cmd,
            "claude 'do work' --dangerously-skip-permissions --model sonnet \
             --disallowedTools \"Bash(gh pr create:*),Bash(gh pr merge:*),Bash(rm -rf:*)\""
        );
    }

    #[test]
    fn spawn_deduplicates() {
        let cmd = Agent::from_config("claude", None, &["Bash(gh pr create:*)".into()]).unwrap()
            .spawn(Prompt::new("work"))
            .unwrap();
        assert_eq!(
            cmd,
            "claude 'work' --dangerously-skip-permissions --model sonnet \
             --disallowedTools \"Bash(gh pr create:*),Bash(gh pr merge:*)\""
        );
    }

    #[test]
    fn resume() {
        let cmd = agent().resume(Prompt::new("continue working")).unwrap();
        assert!(cmd.contains("-c 'continue working'"));
        assert!(cmd.contains("--dangerously-skip-permissions"));
        assert!(cmd.contains("--model sonnet"));
        assert!(cmd.contains("--disallowedTools"));
    }

    #[test]
    fn once_with_tools() {
        let argv = agent()
            .once(Prompt::new("review this"), &["Read", "Glob"])
            .unwrap();
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
                "Read,Glob",
                "review this"
            ]
        );
    }

    #[test]
    fn once_no_tools() {
        let argv = agent()
            .once(Prompt::new("hello"), &[])
            .unwrap();
        assert_eq!(
            argv,
            vec![
                "claude",
                "--print",
                "--no-session-persistence",
                "--dangerously-skip-permissions",
                "--model",
                "sonnet",
                "hello"
            ]
        );
    }

    #[test]
    fn from_config_validates_model() {
        assert!(Agent::from_config("claude", Some("sonnet"), &[]).is_some());
        assert!(Agent::from_config("claude", Some("opus"), &[]).is_some());
        assert!(Agent::from_config("claude", Some("haiku"), &[]).is_some());
        assert!(Agent::from_config("claude", Some("gpt-4"), &[]).is_none());
        assert!(Agent::from_config("unknown", Some("sonnet"), &[]).is_none());
    }

    #[test]
    fn from_config_default_model() {
        let a = Agent::from_config("claude", None, &[]).unwrap();
        assert_eq!(a.model(), "sonnet");
    }

    #[test]
    fn model_stored_on_agent() {
        let a = Agent::from_config("claude", Some("opus"), &[]).unwrap();
        assert_eq!(a.model(), "opus");
    }

    #[test]
    fn claude_model_enum() {
        assert_eq!(Model::Sonnet.to_string(), "sonnet");
        assert_eq!(Model::Opus.to_string(), "opus");
        assert_eq!("haiku".parse::<Model>().unwrap(), Model::Haiku);
        assert!("gpt-4".parse::<Model>().is_err());
    }

    #[test]
    fn hook_scripts_are_non_empty() {
        for ht in HookType::ALL {
            let script = ht.script();
            assert!(!script.is_empty(), "{ht:?} script is empty");
            assert!(script.starts_with("#!/bin/bash"), "{ht:?} missing shebang");
        }
    }
}
