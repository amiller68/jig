//! Triage actor — polls for triageable issues and runs ephemeral triage agents
//! as direct subprocesses. Owns the TriageTracker for dedup and stuck detection.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::context::{self, RepoConfig};
use jig_core::agents;
use jig_core::prompt::Prompt;
use jig_core::git::{Branch, Repo};
use jig_core::issues::issue::{IssueFilter, IssueStatus};
use jig_core::issues::Issue;

use super::{Actor, TickContext};

pub struct TriageRequest {
    pub ctx: TickContext,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriageEntry {
    pub worker_name: String,
    pub spawned_at: i64,
    pub issue_id: String,
    pub repo_name: String,
}

#[derive(Default)]
pub struct TriageActor {
    tracker: Mutex<HashMap<String, TriageEntry>>,
}

impl TriageActor {
    pub fn is_active(&self, issue_id: &str) -> bool {
        self.tracker.lock().unwrap().contains_key(issue_id)
    }

    pub fn active_entries(&self) -> Vec<TriageEntry> {
        self.tracker.lock().unwrap().values().cloned().collect()
    }

    pub fn remove(&self, issue_id: &str) {
        self.tracker.lock().unwrap().remove(issue_id);
    }

    fn register(&self, issue_id: String, entry: TriageEntry) {
        self.tracker.lock().unwrap().insert(issue_id, entry);
    }
}

struct TriageIssue {
    repo_root: PathBuf,
    issue: Issue,
    worker_name: String,
}

impl Actor for TriageActor {
    type Request = TriageRequest;
    type Response = ();

    const NAME: &'static str = "jig-triage";
    const QUEUE_SIZE: usize = 1;

    fn handle(&self, req: TriageRequest) {
        let now = chrono::Utc::now().timestamp();

        // Detect and remove stuck triages before polling new ones
        {
            let mut tracker = self.tracker.lock().unwrap();
            let stuck: Vec<String> = tracker
                .iter()
                .filter(|(_, entry)| {
                    let repo_timeout = req
                        .ctx
                        .repos
                        .iter()
                        .find(|e| {
                            e.path
                                .file_name()
                                .map(|n| n.to_string_lossy() == entry.repo_name)
                                .unwrap_or(false)
                        })
                        .and_then(|e| context::JigToml::load(&e.path).ok().flatten())
                        .map(|t| t.triage.timeout_seconds)
                        .unwrap_or(600);
                    now - entry.spawned_at > repo_timeout
                })
                .map(|(id, _)| id.clone())
                .collect();

            for id in &stuck {
                if let Some(entry) = tracker.remove(id) {
                    tracing::warn!(
                        issue = %entry.issue_id,
                        worker = %entry.worker_name,
                        "triage timed out"
                    );
                }
            }
        }

        // Poll for new triageable issues
        let global = &req.ctx.config;
        for entry in req.ctx.repos.iter() {
            let repo_root = entry.path.clone();
            let repo_name = repo_root
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();

            let repo = match Repo::open(&repo_root) {
                Ok(r) => r,
                Err(e) => {
                    tracing::debug!(repo = %repo_name, error = %e, "failed to open repo");
                    continue;
                }
            };

            let cfg = match RepoConfig::from_path(&repo_root) {
                Ok(cfg) => cfg,
                Err(e) => {
                    tracing::debug!(repo = %repo_name, error = %e, "failed to load config");
                    continue;
                }
            };

            if !cfg.repo.triage.enabled {
                continue;
            }

            let provider = match cfg.issue_provider(global) {
                Ok(p) => p,
                Err(e) => {
                    tracing::debug!(repo = %repo_name, error = %e, "failed to create issue provider");
                    continue;
                }
            };

            let existing_branches: Vec<Branch> = repo
                .list_worktrees()
                .unwrap_or_default()
                .iter()
                .filter_map(|wt| wt.branch().ok())
                .collect();

            let max_workers = cfg.repo.spawn.max_concurrent_workers;
            let budget = max_workers.saturating_sub(existing_branches.len());

            if budget == 0 {
                continue;
            }

            let triageable = match provider.list(&IssueFilter {
                status: Some(IssueStatus::Triage),
                ..Default::default()
            }) {
                Ok(issues) => issues,
                Err(_) => continue,
            };

            for issue in triageable.into_iter().take(budget) {
                let worker_name = format!("triage-{}", issue.id().to_lowercase());

                if self.is_active(issue.id()) {
                    continue;
                }

                self.register(
                    issue.id().to_string(),
                    TriageEntry {
                        worker_name: worker_name.clone(),
                        spawned_at: now,
                        issue_id: issue.id().to_string(),
                        repo_name: repo_name.clone(),
                    },
                );

                let ti = TriageIssue {
                    repo_root: repo_root.clone(),
                    issue,
                    worker_name,
                };

                run_single(&ti);
            }
        }
    }
}

const TRIAGE_PROMPT: &str = r#"You are triaging issue {{issue_id}}: {{issue_title}}

## Issue Description

{{issue_body}}

## Your Task

Investigate this issue in the codebase and produce a scoped analysis. Do NOT implement any changes -- you are read-only.

1. **Identify affected code** -- find the relevant files, functions, and modules
2. **Assess scope** -- is this a small fix, a medium refactor, or a large feature?
3. **Propose approach** -- outline what an implementing agent (or human) would need to do
4. **Flag risks** -- note any dependencies, breaking changes, or areas needing careful handling
5. **Suggest priority** -- based on severity and scope, suggest Urgent/High/Medium/Low

## Output

When you have completed your investigation, update the Linear issue with your findings using the jig CLI, then change the issue status to Backlog.

Run: `jig issues update {{issue_id}} --body "your investigation findings"`
Then: `jig issues status {{issue_id}} backlog`

Structure your findings as:

### Investigation
[Your findings about affected code, scope, and approach]

### Affected Files
- `path/to/file.rs` -- reason

### Proposed Approach
1. Step one
2. Step two

### Complexity
[Small | Medium | Large]

### Suggested Priority
[Urgent | High | Medium | Low]

### Risks
- [Any risks or concerns]
"#;

const TRIAGE_ALLOWED_TOOLS: &[&str] = &["Read", "Glob", "Grep", "Bash(jig *)"];

fn run_single(issue: &TriageIssue) {
    tracing::info!(
        worker = %issue.worker_name,
        issue = %issue.issue.id(),
        "running triage subprocess"
    );

    match run_triage_subprocess(&issue.repo_root, &issue.issue) {
        Ok(()) => {
            tracing::info!(
                worker = %issue.worker_name,
                issue = %issue.issue.id(),
                "triage subprocess completed successfully"
            );
        }
        Err(msg) => {
            tracing::warn!(
                worker = %issue.worker_name,
                issue = %issue.issue.id(),
                "triage subprocess failed: {}", msg
            );
        }
    }
}

fn render_triage_prompt(issue: &Issue) -> jig_core::error::Result<String> {
    Prompt::new(TRIAGE_PROMPT)
        .var("issue_id", issue.id().to_string())
        .var("issue_title", issue.title())
        .var("issue_body", issue.body())
        .render()
}

pub(crate) fn run_triage_subprocess(
    repo_root: &Path,
    issue: &Issue,
) -> std::result::Result<(), String> {
    let prompt = render_triage_prompt(issue).map_err(|e| e.to_string())?;

    let jig_toml = context::JigToml::load(repo_root)
        .map_err(|e| e.to_string())?
        .unwrap_or_default();
    let agent = agents::Agent::from_config(&jig_toml.agent.agent_type, Some(&jig_toml.triage.model), &[])
        .unwrap_or_else(|| agents::Agent::from_config("claude", None, &[]).unwrap());

    let argv = agent
        .once(jig_core::Prompt::new(&prompt), TRIAGE_ALLOWED_TOOLS)
        .map_err(|e| e.to_string())?;

    let (cmd, args) = argv.split_first().ok_or("empty triage argv")?;

    let output = std::process::Command::new(cmd)
        .args(args)
        .current_dir(repo_root)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .map_err(|e| format!("failed to execute triage agent: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "triage agent exited with {}: {}",
            output.status,
            stderr.chars().take(500).collect::<String>()
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracker_dedup() {
        let actor = TriageActor::default();
        actor.register(
            "JIG-1".to_string(),
            TriageEntry {
                worker_name: "triage-jig-1".to_string(),
                spawned_at: 1000,
                issue_id: "JIG-1".to_string(),
                repo_name: "repo".to_string(),
            },
        );
        assert!(actor.is_active("JIG-1"));
        assert!(!actor.is_active("JIG-2"));
        actor.remove("JIG-1");
        assert!(!actor.is_active("JIG-1"));
    }

    #[test]
    fn active_entries_returns_all() {
        let actor = TriageActor::default();
        actor.register(
            "JIG-1".to_string(),
            TriageEntry {
                worker_name: "triage-jig-1".to_string(),
                spawned_at: 100,
                issue_id: "JIG-1".to_string(),
                repo_name: "repo".to_string(),
            },
        );
        actor.register(
            "JIG-2".to_string(),
            TriageEntry {
                worker_name: "triage-jig-2".to_string(),
                spawned_at: 200,
                issue_id: "JIG-2".to_string(),
                repo_name: "repo".to_string(),
            },
        );
        assert_eq!(actor.active_entries().len(), 2);
    }
}
