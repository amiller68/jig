//! Spawn actor — polls for spawnable issues, creates parent integration
//! branches, and launches workers in a background thread.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use crate::config::{self, Config};
use crate::worker::SPAWN_PREAMBLE;
use jig_core::agents;
use jig_core::git::{Branch, Repo};
use jig_core::issues::issue::{IssueFilter, IssueStatus};
use jig_core::issues::{Issue, IssueProvider};

type Worker = crate::worker::Worker<jig_core::mux::tmux::TmuxWindow>;

use super::Actor;

pub struct SpawnRequest {
    pub repos: Vec<Repo>,
}

#[derive(Default)]
pub struct SpawnActor {
    spawning_workers: Mutex<Vec<String>>,
    first_poll_done: AtomicBool,
}

impl SpawnActor {
    pub fn spawning_workers(&self) -> Vec<String> {
        self.spawning_workers.lock().unwrap().clone()
    }

    pub fn should_first_poll(&self) -> bool {
        !self.first_poll_done.load(Ordering::Relaxed)
    }

    pub fn mark_first_poll_done(&self) {
        self.first_poll_done.store(true, Ordering::Relaxed);
    }
}

impl Actor for SpawnActor {
    type Request = SpawnRequest;
    type Response = ();

    const NAME: &'static str = "jig-spawn";
    const QUEUE_SIZE: usize = 1;

    fn handle(&self, req: SpawnRequest) {
        let mut spawning = Vec::new();

        for repo in &req.repos {
            let repo_root = repo.clone_path();
            let repo_name = repo_root
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();

            let cfg = match Config::from_path(&repo_root) {
                Ok(cfg) => cfg,
                Err(e) => {
                    tracing::debug!(repo = %repo_name, error = %e, "failed to load config");
                    continue;
                }
            };

            let provider = match cfg.issue_provider() {
                Ok(p) => p,
                Err(e) => {
                    tracing::debug!(repo = %repo_name, error = %e, "failed to create issue provider");
                    continue;
                }
            };

            // -- Parent integration branches --
            let base: Branch = cfg.base_branch().as_str().into();
            let parent_candidates: Vec<_> = [IssueStatus::Planned, IssueStatus::InProgress]
                .into_iter()
                .flat_map(|status| {
                    provider
                        .list(&IssueFilter {
                            status: Some(status),
                            ..Default::default()
                        })
                        .unwrap_or_default()
                })
                .filter(|i| !i.children().is_empty())
                .collect();

            for issue in parent_candidates {
                let branch = issue.branch().clone();

                if !repo.remote_branch_exists(&branch) {
                    match repo.create_and_push_branch(&branch, &base) {
                        Ok(()) => {
                            tracing::info!(
                                repo = %repo_name, issue = %issue.id(), branch = %branch,
                                "created parent integration branch"
                            );
                        }
                        Err(e) => {
                            tracing::warn!(
                                repo = %repo_name, issue = %issue.id(), branch = %branch,
                                "failed to create parent integration branch: {}", e
                            );
                            continue;
                        }
                    }
                }

                if *issue.status() == IssueStatus::Planned {
                    if let Err(e) = provider.update_status(issue.id(), &IssueStatus::InProgress) {
                        tracing::warn!(
                            repo = %repo_name, issue = %issue.id(),
                            "failed to update parent status: {}", e
                        );
                    } else {
                        tracing::info!(
                            repo = %repo_name, issue = %issue.id(),
                            "flipped parent issue to InProgress"
                        );
                    }
                }
            }

            // -- Worker budget --
            let existing_branches: Vec<Branch> = repo
                .list_worktrees()
                .unwrap_or_default()
                .iter()
                .filter_map(|wt| wt.branch().ok())
                .collect();

            let max_workers = cfg
                .repo
                .spawn
                .resolve_max_concurrent_workers(&cfg.global.spawn);
            let budget = max_workers.saturating_sub(existing_branches.len());

            if budget == 0 {
                tracing::debug!(
                    repo = %repo_name,
                    active = existing_branches.len(),
                    max = max_workers,
                    "repo at worker capacity, skipping"
                );
                continue;
            }

            // -- Auto-spawn --
            let Some(labels) = &cfg.repo.issues.auto_spawn_labels else {
                continue;
            };

            let planned = provider
                .list(&IssueFilter {
                    status: Some(IssueStatus::Planned),
                    ..Default::default()
                })
                .unwrap_or_default();

            let mut repo_spawned = 0;
            for issue in planned {
                if repo_spawned >= budget {
                    break;
                }
                if !issue.children().is_empty() {
                    continue;
                }
                if !(labels.is_empty() || issue.auto(labels)) {
                    continue;
                }
                if !provider.may_spawn(issue.id()) {
                    continue;
                }
                if let Some(parent_ref) = issue.parent() {
                    let ready = match provider.get(parent_ref) {
                        Ok(Some(parent)) => {
                            *parent.status() == IssueStatus::InProgress
                                && repo.remote_branch_exists(parent.branch())
                        }
                        _ => false,
                    };
                    if !ready {
                        continue;
                    }
                }
                if existing_branches.iter().any(|b| b == issue.branch()) {
                    continue;
                }

                let worker_name = issue.branch().to_string();
                spawning.push(worker_name.clone());

                match spawn_worker_for_issue(&repo_root, &issue, &worker_name, &cfg, &provider) {
                    Ok(_worker) => {
                        tracing::info!(worker = %worker_name, "auto-spawned worker");
                    }
                    Err(msg) => {
                        tracing::warn!(worker = %worker_name, "auto-spawn failed: {}", msg);
                    }
                }
                repo_spawned += 1;
            }
        }

        *self.spawning_workers.lock().unwrap() = spawning;
    }
}

fn spawn_worker_for_issue(
    repo_root: &Path,
    issue: &Issue,
    worker_name: &str,
    cfg: &Config,
    provider: &IssueProvider,
) -> std::result::Result<Worker, String> {
    let worktree_path = config::worktree_path(repo_root, worker_name);

    if worktree_path.exists() {
        tracing::debug!(worker = %worker_name, "worktree already exists, skipping");
        return Ok(Worker::from_branch(repo_root, worker_name.into()));
    }

    let parent = issue.parent().and_then(|r| provider.get(r).ok().flatten());

    let base_branch = match &parent {
        Some(p) => format!("origin/{}", p.branch()),
        None => Config::resolve_base_branch_for(repo_root)
            .unwrap_or_else(|_| config::DEFAULT_BASE_BRANCH.to_string()),
    };

    let repo = Repo::open(repo_root).map_err(|e| e.to_string())?;
    let branch = issue.branch().clone();
    let base = Branch::new(&base_branch);

    let agent = agents::Agent::from_name(&cfg.repo.agent.agent_type)
        .unwrap_or_else(|| agents::Agent::from_kind(agents::AgentKind::Claude))
        .with_disallowed_tools(cfg.repo.agent.disallowed_tools.clone());

    let prompt = issue
        .to_prompt(SPAWN_PREAMBLE, provider)
        .var_num("max_nudges", cfg.global.health.max_nudges);

    let worker = Worker::spawn(
        &repo,
        &branch,
        &base,
        &agent,
        prompt,
        true,
        Some(issue.id().clone()),
    )
    .map_err(|e| e.to_string())?;

    if let Err(e) = provider.update_status(issue.id(), &IssueStatus::InProgress) {
        tracing::warn!(
            issue = %issue.id(),
            error = %e,
            "failed to update issue status to InProgress"
        );
    }

    Ok(worker)
}
