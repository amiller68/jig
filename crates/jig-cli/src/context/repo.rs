//! Repository configuration — jig.toml + jig.local.toml overlay.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

use super::{JIG_LOCAL_TOML, JIG_TOML};
use jig_core::error::Result;

/// JigToml configuration from jig.toml
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct JigToml {
    #[serde(default)]
    pub worktree: WorktreeConfig,
    #[serde(default)]
    pub spawn: SpawnConfig,
    #[serde(default)]
    pub agent: AgentConfig,
    #[serde(default)]
    pub issues: IssuesConfig,
    #[serde(default)]
    pub triage: TriageConfig,
    #[serde(skip)]
    pub has_local_overlay: bool,
    #[serde(skip)]
    pub base_keys: Vec<String>,
    #[serde(skip)]
    pub local_keys: Vec<String>,
}

/// Issue tracking configuration in jig.toml
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct IssuesConfig {
    pub linear: Option<LinearIssuesConfig>,
    pub auto_spawn_labels: Option<Vec<String>>,
    pub auto_complete_on_merge: bool,
}

/// Linear issue provider configuration in jig.toml.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinearIssuesConfig {
    pub profile: String,
    #[serde(default)]
    pub team: Option<String>,
    #[serde(default)]
    pub projects: Vec<String>,
    #[serde(default)]
    pub assignee: Option<String>,
    #[serde(default)]
    pub labels: Vec<String>,
}

/// Worktree configuration in jig.toml
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct WorktreeConfig {
    pub base: Option<String>,
    pub on_create: Option<String>,
    pub copy: Vec<String>,
}

/// Spawn configuration in jig.toml (per-repo).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SpawnConfig {
    pub max_concurrent_workers: usize,
}

impl Default for SpawnConfig {
    fn default() -> Self {
        Self {
            max_concurrent_workers: 3,
        }
    }
}

/// Agent configuration in jig.toml
#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct AgentConfig {
    #[serde(rename = "type")]
    pub agent_type: String,
    pub model: String,
    pub disallowed_tools: Vec<String>,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            agent_type: "claude".to_string(),
            model: "sonnet".to_string(),
            disallowed_tools: Vec::new(),
        }
    }
}

/// Per-repo triage configuration in jig.toml `[triage]`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TriageConfig {
    pub enabled: bool,
    pub model: String,
    pub timeout_seconds: i64,
}

impl Default for TriageConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            model: "sonnet".to_string(),
            timeout_seconds: 600,
        }
    }
}

impl JigToml {
    pub fn load(repo_root: &Path) -> Result<Option<Self>> {
        let toml_path = repo_root.join(JIG_TOML);
        if !toml_path.exists() {
            return Ok(None);
        }

        let content = fs::read_to_string(&toml_path)?;
        let local_path = repo_root.join(JIG_LOCAL_TOML);

        let base_value: toml::Value = toml::from_str(&content)?;
        let base_keys: Vec<String> = base_value
            .as_table()
            .map(|t| t.keys().cloned().collect())
            .unwrap_or_default();

        if local_path.exists() {
            let local_content = fs::read_to_string(&local_path)?;
            let local_value: toml::Value = toml::from_str(&local_content)?;
            let local_keys: Vec<String> = local_value
                .as_table()
                .map(|t| t.keys().cloned().collect())
                .unwrap_or_default();
            let mut merged = base_value;
            deep_merge(&mut merged, local_value);
            let mut config: JigToml = merged.try_into()?;
            config.has_local_overlay = true;
            config.base_keys = base_keys;
            config.local_keys = local_keys;
            Ok(Some(config))
        } else {
            let mut config: JigToml = base_value.try_into()?;
            config.base_keys = base_keys;
            Ok(Some(config))
        }
    }

    pub fn source_label(&self, section: &str) -> String {
        let in_base = self.base_keys.iter().any(|k| k == section);
        let in_local = self.local_keys.iter().any(|k| k == section);
        match (in_base, in_local) {
            (true, true) => format!("{} + {}", JIG_TOML, JIG_LOCAL_TOML),
            (true, false) => JIG_TOML.to_string(),
            (false, true) => JIG_LOCAL_TOML.to_string(),
            (false, false) => "default".to_string(),
        }
    }

    pub fn exists(repo_root: &Path) -> bool {
        repo_root.join(JIG_TOML).exists()
    }
}

fn deep_merge(base: &mut toml::Value, overlay: toml::Value) {
    match (base.is_table(), overlay) {
        (true, toml::Value::Table(overlay_table)) => {
            let base_table = base.as_table_mut().unwrap();
            for (key, value) in overlay_table {
                if let Some(existing) = base_table.get_mut(&key) {
                    deep_merge(existing, value);
                } else {
                    base_table.insert(key, value);
                }
            }
        }
        (_, overlay) => {
            *base = overlay;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_jig_toml_with_assignee() {
        let toml_str = r#"
[issues.linear]
profile = "work"
team = "ENG"
projects = ["Backend"]
assignee = "alice@co.com"
labels = ["auto"]
"#;
        let config: JigToml = toml::from_str(toml_str).unwrap();
        let linear = config.issues.linear.unwrap();
        assert_eq!(linear.profile, "work");
        assert_eq!(linear.team.as_deref(), Some("ENG"));
        assert_eq!(linear.projects, vec!["Backend"]);
        assert_eq!(linear.assignee.as_deref(), Some("alice@co.com"));
        assert_eq!(linear.labels, vec!["auto"]);
    }

    #[test]
    fn parse_jig_toml_linear_minimal() {
        let toml_str = r#"
[issues.linear]
profile = "work"
"#;
        let config: JigToml = toml::from_str(toml_str).unwrap();
        let linear = config.issues.linear.unwrap();
        assert_eq!(linear.profile, "work");
        assert!(linear.team.is_none());
        assert!(linear.projects.is_empty());
        assert!(linear.assignee.is_none());
        assert!(linear.labels.is_empty());
    }

    #[test]
    fn spawn_config_defaults() {
        let config = SpawnConfig::default();
        assert_eq!(config.max_concurrent_workers, 3);
    }

    #[test]
    fn spawn_config_from_toml() {
        let toml_str = r#"
[spawn]
max_concurrent_workers = 5
"#;
        let config: JigToml = toml::from_str(toml_str).unwrap();
        assert_eq!(config.spawn.max_concurrent_workers, 5);
    }

    #[test]
    fn unknown_sections_ignored() {
        let toml_str = r#"
[worktree]
base = "origin/main"

[health]
max_nudges = 5
"#;
        let config: JigToml = toml::from_str(toml_str).unwrap();
        assert_eq!(config.worktree.base.as_deref(), Some("origin/main"));
    }

    #[test]
    fn deep_merge_tables() {
        let mut base: toml::Value = toml::from_str(
            r#"
            [agent]
            type = "claude"
            "#,
        )
        .unwrap();
        let overlay: toml::Value = toml::from_str(
            r#"
            [worktree]
            base = "origin/main"
            "#,
        )
        .unwrap();
        deep_merge(&mut base, overlay);
        let tbl = base.as_table().unwrap();
        assert!(tbl.contains_key("agent"));
        assert!(tbl.contains_key("worktree"));
    }

    #[test]
    fn deep_merge_scalar_wins() {
        let mut base: toml::Value = toml::from_str("[agent]\ntype = \"claude\"\n").unwrap();
        let overlay: toml::Value = toml::from_str("[agent]\ntype = \"cursor\"\n").unwrap();
        deep_merge(&mut base, overlay);
        assert_eq!(base["agent"]["type"].as_str(), Some("cursor"));
    }

    #[test]
    fn load_with_local_overlay() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(JIG_TOML), "[worktree]\nbase = \"origin/main\"\n").unwrap();
        fs::write(
            dir.path().join(JIG_LOCAL_TOML),
            "[issues]\nauto_spawn_labels = []\n",
        )
        .unwrap();
        let config = JigToml::load(dir.path()).unwrap().unwrap();
        assert_eq!(config.worktree.base.as_deref(), Some("origin/main"));
        assert_eq!(config.issues.auto_spawn_labels, Some(vec![]));
        assert!(config.has_local_overlay);
    }

    #[test]
    fn local_only_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join(JIG_LOCAL_TOML),
            "[issues]\nauto_spawn_labels = []\n",
        )
        .unwrap();
        assert!(JigToml::load(dir.path()).unwrap().is_none());
    }

    #[test]
    fn triage_config_defaults() {
        let config = TriageConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.model, "sonnet");
        assert_eq!(config.timeout_seconds, 600);
    }
}
