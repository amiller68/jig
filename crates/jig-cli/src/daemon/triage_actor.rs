//! Triage actor — runs ephemeral triage agents as direct subprocesses.
//!
//! Unlike normal workers (which run in tmux windows), triage workers are
//! one-shot read-only agents that don't need interactive sessions. This actor
//! receives `TriageRequest`s, spawns each triage as a `std::process::Command`
//! subprocess, and returns `TriageComplete`s with per-issue results.

use std::path::Path;

use jig_core::agents;
use jig_core::config::{self, Config};
use jig_core::issues::Issue;

use super::messages::{TriageComplete, TriageIssue, TriageRequest, TriageResult};

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

/// Spawn the triage actor thread. Returns immediately.
pub fn spawn(
    rx: flume::Receiver<TriageRequest>,
    tx: flume::Sender<TriageComplete>,
) -> std::thread::JoinHandle<()> {
    std::thread::Builder::new()
        .name("jig-triage".into())
        .spawn(move || {
            while let Ok(req) = rx.recv() {
                let mut results = Vec::new();

                for issue in &req.issues {
                    let result = run_single(issue);
                    results.push(result);
                }

                if tx.send(TriageComplete { results }).is_err() {
                    break;
                }
            }
        })
        .expect("failed to spawn triage actor thread")
}

/// Run a single triage subprocess and produce a result.
fn run_single(issue: &TriageIssue) -> TriageResult {
    let repo_name = issue
        .repo_root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

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
            TriageResult {
                worker_name: issue.worker_name.clone(),
                repo_name,
                issue_id: issue.issue.id().to_string(),
                error: None,
            }
        }
        Err(msg) => {
            tracing::warn!(
                worker = %issue.worker_name,
                issue = %issue.issue.id(),
                "triage subprocess failed: {}", msg
            );
            TriageResult {
                worker_name: issue.worker_name.clone(),
                repo_name,
                issue_id: issue.issue.id().to_string(),
                error: Some(msg),
            }
        }
    }
}

/// Render the triage prompt for an issue.
fn render_triage_prompt(repo_root: &Path, issue: &Issue) -> jig_core::error::Result<String> {
    let repo_name = repo_root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    let cfg = Config::from_path(repo_root)?;
    let provider = cfg.issue_provider()?;

    issue
        .to_prompt(TRIAGE_PROMPT, &provider)
        .var("repo_name", repo_name)
        .render()
}

/// Run a triage agent as a direct subprocess (blocking).
pub(crate) fn run_triage_subprocess(
    repo_root: &Path,
    issue: &Issue,
) -> std::result::Result<(), String> {
    let prompt = render_triage_prompt(repo_root, issue).map_err(|e| e.to_string())?;

    let jig_toml = config::JigToml::load(repo_root)
        .map_err(|e| e.to_string())?
        .unwrap_or_default();
    let model = &jig_toml.triage.model;
    let agent = agents::Agent::from_name(&jig_toml.agent.agent_type)
        .unwrap_or_else(|| agents::Agent::from_kind(agents::AgentKind::Claude));

    let argv = agent.triage_argv(model, TRIAGE_ALLOWED_TOOLS);

    let (cmd, args) = argv.split_first().ok_or("empty triage argv")?;

    let output = std::process::Command::new(cmd)
        .args(args)
        .current_dir(repo_root)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            if let Some(ref mut stdin) = child.stdin {
                let _ = stdin.write_all(prompt.as_bytes());
            }
            drop(child.stdin.take());
            child.wait_with_output()
        })
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
    use jig_core::git::Branch;
    use jig_core::issues::{IssuePriority, IssueStatus, ProviderKind};
    use std::path::PathBuf;

    fn make_triage_issue(id: &str, worker: &str) -> TriageIssue {
        TriageIssue {
            repo_root: PathBuf::from("/tmp/nonexistent-repo"),
            issue: Issue::new(
                id,
                "Test issue",
                IssueStatus::Triage,
                IssuePriority::Medium,
                Branch::new(id.to_lowercase()),
                "Test body",
            ),
            worker_name: worker.to_string(),
            provider_kind: ProviderKind::Linear,
        }
    }

    #[test]
    fn run_single_missing_repo() {
        let issue = make_triage_issue("JIG-99", "triage-jig-99");
        let result = run_single(&issue);
        assert!(result.error.is_some());
        assert_eq!(result.issue_id, "JIG-99");
        assert_eq!(result.worker_name, "triage-jig-99");
    }

    #[test]
    fn triage_result_success_fields() {
        let result = TriageResult {
            worker_name: "triage-jig-38".to_string(),
            repo_name: "my-repo".to_string(),
            issue_id: "JIG-38".to_string(),
            error: None,
        };
        assert!(result.error.is_none());
        assert_eq!(result.worker_name, "triage-jig-38");
    }

    #[test]
    fn triage_result_error_fields() {
        let result = TriageResult {
            worker_name: "triage-jig-38".to_string(),
            repo_name: "my-repo".to_string(),
            issue_id: "JIG-38".to_string(),
            error: Some("triage agent exited with code 1".to_string()),
        };
        assert!(result.error.is_some());
    }
}
