//! Repository configuration — jig.toml + jig.local.toml overlay.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

use super::global::HealthConfig;
use super::{JIG_LOCAL_TOML, JIG_TOML};
use jig_core::error::Result;
use jig_core::git::conventional::CommitType;

/// Conventional commits validation configuration in jig.toml `[commits]`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ConventionalCommitsConfig {
    /// Allowed commit types (as strings in TOML, parsed to CommitType).
    pub types: Vec<String>,
    /// Require a scope.
    pub require_scope: bool,
    /// Allowed scopes (empty = any).
    pub scopes: Vec<String>,
    /// Allow breaking changes.
    pub allow_breaking: bool,
    /// Max subject line length.
    pub max_subject_length: usize,
    /// Require lowercase first char in subject.
    pub require_lowercase: bool,
}

impl Default for ConventionalCommitsConfig {
    fn default() -> Self {
        Self {
            types: CommitType::ALL
                .iter()
                .map(|t| t.as_str().to_string())
                .collect(),
            require_scope: false,
            scopes: vec![],
            allow_breaking: true,
            max_subject_length: 72,
            require_lowercase: true,
        }
    }
}

impl ConventionalCommitsConfig {
    pub fn to_validation_config(&self) -> jig_core::git::conventional::ValidationConfig {
        let allowed_types: Vec<CommitType> =
            self.types.iter().filter_map(|s| s.parse().ok()).collect();

        jig_core::git::conventional::ValidationConfig {
            allowed_types,
            require_scope: self.require_scope,
            allowed_scopes: self.scopes.clone(),
            allow_breaking: self.allow_breaking,
            max_subject_length: self.max_subject_length,
            require_lowercase: self.require_lowercase,
        }
    }
}

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
    pub health: RepoHealthConfig,
    #[serde(default)]
    pub commits: ConventionalCommitsConfig,
    #[serde(default)]
    pub triage: TriageConfig,
    /// Whether a jig.local.toml overlay was merged into this config.
    #[serde(skip)]
    pub has_local_overlay: bool,
    /// Raw top-level keys present in jig.toml (for provenance attribution).
    #[serde(skip)]
    pub base_keys: Vec<String>,
    /// Raw top-level keys present in jig.local.toml (for provenance attribution).
    #[serde(skip)]
    pub local_keys: Vec<String>,
}

/// Per-repo health/nudge configuration in jig.toml `[health]`.
///
/// All fields are optional — when absent, the global config is used.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct RepoHealthConfig {
    /// Override global silence threshold (seconds before a worker is "stalled").
    pub silence_threshold_seconds: Option<u64>,
    /// Override global max nudges before escalation.
    pub max_nudges: Option<u32>,
    /// Per-nudge-type overrides.
    #[serde(default)]
    pub nudge: NudgeTypeConfigs,
}

/// Per-nudge-type configuration overrides.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct NudgeTypeConfigs {
    pub idle: Option<NudgeTypeConfig>,
    pub stalled: Option<NudgeTypeConfig>,
    pub ci: Option<NudgeTypeConfig>,
    pub review: Option<NudgeTypeConfig>,
    pub conflict: Option<NudgeTypeConfig>,
    pub bad_commits: Option<NudgeTypeConfig>,
}

/// Configuration for a single nudge type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NudgeTypeConfig {
    /// Max nudges of this type before escalation.
    pub max: Option<u32>,
    /// Minimum seconds between nudges of this type.
    pub cooldown_seconds: Option<u64>,
}

/// Resolved nudge parameters for a specific nudge type.
#[derive(Debug, Clone, Copy)]
pub struct ResolvedNudgeConfig {
    pub max: u32,
    pub cooldown_seconds: u64,
}

impl RepoHealthConfig {
    /// Resolve the effective silence threshold: jig.toml > global config.
    pub fn resolve_silence_threshold(&self, global: &HealthConfig) -> u64 {
        self.silence_threshold_seconds
            .unwrap_or(global.silence_threshold_seconds)
    }

    /// Resolve the effective max nudges: jig.toml > global config.
    pub fn resolve_max_nudges(&self, global: &HealthConfig) -> u32 {
        self.max_nudges.unwrap_or(global.max_nudges)
    }

    /// Resolve (max, cooldown) for a specific nudge type.
    ///
    /// Resolution: `[health.nudge.<type>]` > `[health]` > global config > defaults.
    pub fn resolve_for_nudge_type(
        &self,
        nudge_type_key: &str,
        global: &HealthConfig,
    ) -> ResolvedNudgeConfig {
        let base_max = self.resolve_max_nudges(global);
        let base_cooldown = self.resolve_silence_threshold(global);

        let type_config = match nudge_type_key {
            "idle" => &self.nudge.idle,
            "stuck" | "stalled" => &self.nudge.stalled,
            "ci" => &self.nudge.ci,
            "review" => &self.nudge.review,
            "conflict" => &self.nudge.conflict,
            "bad_commits" => &self.nudge.bad_commits,
            _ => &None,
        };

        let max = type_config.as_ref().and_then(|c| c.max).unwrap_or(base_max);
        let cooldown_seconds = type_config
            .as_ref()
            .and_then(|c| c.cooldown_seconds)
            .unwrap_or(base_cooldown);

        ResolvedNudgeConfig {
            max,
            cooldown_seconds,
        }
    }
}

/// Issue tracking configuration in jig.toml
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssuesConfig {
    /// Provider type.
    #[serde(default = "default_issues_provider")]
    pub provider: jig_core::issues::ProviderKind,
    /// Directory containing issue files (relative to repo root).
    #[serde(default = "default_issues_directory")]
    pub directory: String,
    /// Linear-specific configuration (required when provider = "linear").
    #[serde(default)]
    pub linear: Option<LinearIssuesConfig>,
    /// Labels required for auto-spawn.
    #[serde(default)]
    pub auto_spawn_labels: Option<Vec<String>>,
    /// Automatically mark linked issues as Complete when a worker's PR merges.
    #[serde(default)]
    pub auto_complete_on_merge: bool,
}

/// Linear issue provider configuration in jig.toml.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinearIssuesConfig {
    /// Name of the profile in global config to use for API key.
    pub profile: String,
    /// Linear team key (e.g. "ENG"). Optional — falls back to profile default.
    #[serde(default)]
    pub team: Option<String>,
    /// Optional list of allowed project names to filter by.
    #[serde(default)]
    pub projects: Vec<String>,
    /// Optional assignee filter. "me" resolves to the API key owner.
    #[serde(default)]
    pub assignee: Option<String>,
    /// Optional label filter. Issues must carry all listed labels.
    #[serde(default)]
    pub labels: Vec<String>,
}

fn default_issues_provider() -> jig_core::issues::ProviderKind {
    jig_core::issues::ProviderKind::Linear
}

fn default_issues_directory() -> String {
    "issues".to_string()
}

impl Default for IssuesConfig {
    fn default() -> Self {
        Self {
            provider: jig_core::issues::ProviderKind::Linear,
            directory: default_issues_directory(),
            linear: None,
            auto_spawn_labels: None,
            auto_complete_on_merge: false,
        }
    }
}

/// Worktree configuration in jig.toml
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct WorktreeConfig {
    /// Base branch for new worktrees (overrides global config)
    #[serde(default)]
    pub base: Option<String>,
    /// Shell command to run after worktree creation
    #[serde(default)]
    pub on_create: Option<String>,
    /// Gitignored files to copy to new worktrees (e.g., [".env", ".env.local"])
    #[serde(default)]
    pub copy: Vec<String>,
}

/// Spawn configuration in jig.toml (per-repo, committed).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SpawnConfig {
    /// Override global max_concurrent_workers for this repo.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_concurrent_workers: Option<usize>,
    /// Override global auto_spawn_interval for this repo.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_spawn_interval: Option<u64>,
}

impl SpawnConfig {
    /// Resolve max_concurrent_workers: jig.toml override > global config default.
    pub fn resolve_max_concurrent_workers(
        &self,
        global: &super::global::GlobalSpawnConfig,
    ) -> usize {
        self.max_concurrent_workers
            .unwrap_or(global.max_concurrent_workers)
    }

    /// Resolve auto_spawn_interval: jig.toml override > global config default.
    pub fn resolve_auto_spawn_interval(&self, global: &super::global::GlobalSpawnConfig) -> u64 {
        self.auto_spawn_interval
            .unwrap_or(global.auto_spawn_interval)
    }
}

/// Agent configuration in jig.toml
#[derive(Debug, Serialize, Deserialize)]
pub struct AgentConfig {
    /// Agent type (e.g., "claude-code", "cursor")
    #[serde(rename = "type", default = "default_agent_type")]
    pub agent_type: String,
    /// Tools to disallow for spawned workers (passed as `--disallowedTools`).
    #[serde(default)]
    pub disallowed_tools: Vec<String>,
}

fn default_agent_type() -> String {
    "claude".to_string()
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            agent_type: default_agent_type(),
            disallowed_tools: Vec::new(),
        }
    }
}

/// Per-repo triage configuration in jig.toml `[triage]`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriageConfig {
    /// Whether triage auto-spawn is enabled for this repo.
    #[serde(default)]
    pub enabled: bool,
    /// Model for triage agents (default "sonnet").
    #[serde(default = "default_triage_model")]
    pub model: String,
    /// Max time in seconds for a triage worker before it's considered stuck.
    #[serde(default = "default_triage_timeout")]
    pub timeout_seconds: i64,
}

fn default_triage_model() -> String {
    "sonnet".to_string()
}

fn default_triage_timeout() -> i64 {
    600
}

impl Default for TriageConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            model: default_triage_model(),
            timeout_seconds: default_triage_timeout(),
        }
    }
}

impl JigToml {
    /// Read jig.toml from a repository, with optional jig.local.toml deep-merge overlay.
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

    /// Return a source attribution label for a top-level TOML section key.
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

    /// Check if jig.toml exists (jig.local.toml alone is not sufficient)
    pub fn exists(repo_root: &Path) -> bool {
        repo_root.join(JIG_TOML).exists()
    }
}

/// Deep-merge `overlay` into `base`. Tables merge recursively; scalars and arrays
/// from the overlay replace the base value.
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
[issues]
provider = "linear"

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
[issues]
provider = "linear"

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
    fn parse_jig_toml_with_health() {
        let toml_str = r#"
[health]
silence_threshold_seconds = 600
max_nudges = 5

[health.nudge.idle]
max = 3
cooldown_seconds = 120

[health.nudge.ci]
max = 2
cooldown_seconds = 60
"#;
        let config: JigToml = toml::from_str(toml_str).unwrap();
        assert_eq!(config.health.silence_threshold_seconds, Some(600));
        assert_eq!(config.health.max_nudges, Some(5));
        assert_eq!(config.health.nudge.idle.as_ref().unwrap().max, Some(3));
        assert_eq!(
            config.health.nudge.idle.as_ref().unwrap().cooldown_seconds,
            Some(120)
        );
        assert_eq!(config.health.nudge.ci.as_ref().unwrap().max, Some(2));
        assert!(config.health.nudge.review.is_none());
    }

    #[test]
    fn parse_jig_toml_without_health() {
        let toml_str = r#"
[worktree]
base = "origin/main"
"#;
        let config: JigToml = toml::from_str(toml_str).unwrap();
        assert!(config.health.silence_threshold_seconds.is_none());
        assert!(config.health.max_nudges.is_none());
    }

    #[test]
    fn resolve_defaults_to_global() {
        let repo_health = RepoHealthConfig::default();
        let global = HealthConfig::default();

        let resolved = repo_health.resolve_for_nudge_type("idle", &global);
        assert_eq!(resolved.max, 3);
        assert_eq!(resolved.cooldown_seconds, 300);
    }

    #[test]
    fn resolve_repo_overrides_global() {
        let repo_health = RepoHealthConfig {
            silence_threshold_seconds: Some(600),
            max_nudges: Some(5),
            ..Default::default()
        };
        let global = HealthConfig::default();

        let resolved = repo_health.resolve_for_nudge_type("idle", &global);
        assert_eq!(resolved.max, 5);
        assert_eq!(resolved.cooldown_seconds, 600);
    }

    #[test]
    fn resolve_per_type_overrides_repo() {
        let repo_health = RepoHealthConfig {
            silence_threshold_seconds: Some(600),
            max_nudges: Some(5),
            nudge: NudgeTypeConfigs {
                ci: Some(NudgeTypeConfig {
                    max: Some(2),
                    cooldown_seconds: Some(60),
                }),
                ..Default::default()
            },
        };
        let global = HealthConfig::default();

        let ci = repo_health.resolve_for_nudge_type("ci", &global);
        assert_eq!(ci.max, 2);
        assert_eq!(ci.cooldown_seconds, 60);

        let idle = repo_health.resolve_for_nudge_type("idle", &global);
        assert_eq!(idle.max, 5);
        assert_eq!(idle.cooldown_seconds, 600);
    }

    #[test]
    fn resolve_partial_type_config() {
        let repo_health = RepoHealthConfig {
            max_nudges: Some(5),
            nudge: NudgeTypeConfigs {
                review: Some(NudgeTypeConfig {
                    max: Some(10),
                    cooldown_seconds: None,
                }),
                ..Default::default()
            },
            ..Default::default()
        };
        let global = HealthConfig::default();

        let review = repo_health.resolve_for_nudge_type("review", &global);
        assert_eq!(review.max, 10);
        assert_eq!(review.cooldown_seconds, 300);
    }

    #[test]
    fn test_deep_merge_tables_merge_recursively() {
        let mut base: toml::Value = toml::from_str(
            r#"
            [health]
            max_nudges = 3
            "#,
        )
        .unwrap();
        let overlay: toml::Value = toml::from_str(
            r#"
            [health]
            silence_threshold_seconds = 600
            "#,
        )
        .unwrap();
        deep_merge(&mut base, overlay);
        let tbl = base.as_table().unwrap();
        let health = tbl["health"].as_table().unwrap();
        assert_eq!(health["max_nudges"].as_integer(), Some(3));
        assert_eq!(health["silence_threshold_seconds"].as_integer(), Some(600));
    }

    #[test]
    fn test_deep_merge_scalar_wins() {
        let mut base: toml::Value = toml::from_str(
            r#"
            [agent]
            type = "claude"
            "#,
        )
        .unwrap();
        let overlay: toml::Value = toml::from_str(
            r#"
            [agent]
            type = "cursor"
            "#,
        )
        .unwrap();
        deep_merge(&mut base, overlay);
        let tbl = base.as_table().unwrap();
        assert_eq!(tbl["agent"]["type"].as_str(), Some("cursor"));
    }

    #[test]
    fn test_deep_merge_array_replaces() {
        let mut base: toml::Value = toml::from_str(
            r#"
            [worktree]
            copy = [".env"]
            "#,
        )
        .unwrap();
        let overlay: toml::Value = toml::from_str(
            r#"
            [worktree]
            copy = [".env", ".env.local"]
            "#,
        )
        .unwrap();
        deep_merge(&mut base, overlay);
        let arr = base["worktree"]["copy"].as_array().unwrap();
        assert_eq!(arr.len(), 2);
    }

    #[test]
    fn test_deep_merge_adds_new_keys() {
        let mut base: toml::Value = toml::from_str(
            r#"
            [worktree]
            base = "origin/main"
            "#,
        )
        .unwrap();
        let overlay: toml::Value = toml::from_str(
            r#"
            [issues]
            auto_spawn_labels = []
            "#,
        )
        .unwrap();
        deep_merge(&mut base, overlay);
        let tbl = base.as_table().unwrap();
        assert!(tbl.contains_key("worktree"));
        assert!(tbl.contains_key("issues"));
    }

    #[test]
    fn test_load_with_local_overlay() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join(JIG_TOML),
            r#"
[worktree]
base = "origin/main"
"#,
        )
        .unwrap();
        fs::write(
            dir.path().join(JIG_LOCAL_TOML),
            r#"
[issues]
auto_spawn_labels = []
"#,
        )
        .unwrap();
        let config = JigToml::load(dir.path()).unwrap().unwrap();
        assert_eq!(config.worktree.base.as_deref(), Some("origin/main"));
        assert_eq!(config.issues.auto_spawn_labels, Some(vec![]));
        assert!(config.has_local_overlay);
    }

    #[test]
    fn test_local_overlay_scalar_wins() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join(JIG_TOML),
            r#"
[agent]
type = "claude"
"#,
        )
        .unwrap();
        fs::write(
            dir.path().join(JIG_LOCAL_TOML),
            r#"
[agent]
type = "cursor"
"#,
        )
        .unwrap();
        let config = JigToml::load(dir.path()).unwrap().unwrap();
        assert_eq!(config.agent.agent_type, "cursor");
    }

    #[test]
    fn test_local_overlay_deep_merge_tables() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join(JIG_TOML),
            r#"
[health]
max_nudges = 3
"#,
        )
        .unwrap();
        fs::write(
            dir.path().join(JIG_LOCAL_TOML),
            r#"
[health]
silence_threshold_seconds = 600
"#,
        )
        .unwrap();
        let config = JigToml::load(dir.path()).unwrap().unwrap();
        assert_eq!(config.health.max_nudges, Some(3));
        assert_eq!(config.health.silence_threshold_seconds, Some(600));
    }

    #[test]
    fn test_no_local_overlay() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join(JIG_TOML),
            r#"
[worktree]
base = "origin/main"
"#,
        )
        .unwrap();
        let config = JigToml::load(dir.path()).unwrap().unwrap();
        assert_eq!(config.worktree.base.as_deref(), Some("origin/main"));
        assert!(!config.has_local_overlay);
    }

    #[test]
    fn test_local_only_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join(JIG_LOCAL_TOML),
            r#"
[issues]
auto_spawn_labels = []
"#,
        )
        .unwrap();
        let result = JigToml::load(dir.path()).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_triage_config_defaults() {
        let config = TriageConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.model, "sonnet");
        assert_eq!(config.timeout_seconds, 600);
    }

    #[test]
    fn test_triage_config_from_toml() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join(JIG_TOML),
            r#"
[triage]
enabled = false
model = "opus"
timeout_seconds = 300
"#,
        )
        .unwrap();
        let toml = JigToml::load(dir.path()).unwrap().unwrap();
        assert!(!toml.triage.enabled);
        assert_eq!(toml.triage.model, "opus");
        assert_eq!(toml.triage.timeout_seconds, 300);
    }

    #[test]
    fn test_triage_config_absent_uses_defaults() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(JIG_TOML), "[worktree]\n").unwrap();
        let toml = JigToml::load(dir.path()).unwrap().unwrap();
        assert!(!toml.triage.enabled);
        assert_eq!(toml.triage.model, "sonnet");
        assert_eq!(toml.triage.timeout_seconds, 600);
    }
}
