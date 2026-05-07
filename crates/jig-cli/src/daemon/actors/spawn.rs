//! Spawn actor — polls for spawnable issues, creates parent integration
//! branches, and launches workers in a background thread.

use std::path::Path;
use std::sync::Mutex;

use crate::context::{self, RepoConfig};
use jig_core::agents;
use jig_core::git::{Branch, Repo};
use jig_core::issues::issue::{IssueFilter, IssueStatus};
use jig_core::issues::{Issue, IssueProvider};

type Worker = crate::worker::Worker;

use super::{Actor, TickContext};

pub struct SpawnRequest {
    pub ctx: TickContext,
}

#[derive(Default)]
pub struct SpawnActor {
    spawning_workers: Mutex<Vec<String>>,
}

impl SpawnActor {
    pub fn spawning_workers(&self) -> Vec<String> {
        self.spawning_workers.lock().unwrap().clone()
    }
}

impl Actor for SpawnActor {
    type Request = SpawnRequest;
    type Response = ();

    const NAME: &'static str = "jig-spawn";
    const QUEUE_SIZE: usize = 1;

    fn handle(&self, req: SpawnRequest) {
        let mut spawning = Vec::new();
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

            let provider = match cfg.issue_provider(&global) {
                Ok(p) => p,
                Err(e) => {
                    tracing::debug!(repo = %repo_name, error = %e, "failed to create issue provider");
                    continue;
                }
            };

            // -- Parent integration branches --
            let base = cfg.base_branch(&global);
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

            let max_workers = cfg.repo.spawn.max_concurrent_workers;
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

                let mux = jig_core::mux::TmuxMux::for_repo(&repo_name);
                match spawn_worker_for_issue(&repo_root, &issue, &worker_name, &cfg, &provider, &mux) {
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
    cfg: &RepoConfig,
    provider: &IssueProvider,
    mux: &dyn jig_core::mux::Mux,
) -> std::result::Result<Worker, String> {
    let worktree_path = context::worktree_path(repo_root, worker_name);

    if worktree_path.exists() {
        tracing::debug!(worker = %worker_name, "worktree already exists, skipping");
        return Ok(Worker::from_branch(repo_root, worker_name.into()));
    }

    let parent = issue.parent().and_then(|r| provider.get(r).ok().flatten());

    let base = match &parent {
        Some(p) => Branch::new(format!("origin/{}", p.branch())),
        None => context::resolve_base_branch_for(repo_root)
            .unwrap_or_else(|_| Branch::new(context::DEFAULT_BASE_BRANCH)),
    };

    let repo = Repo::open(repo_root).map_err(|e| e.to_string())?;
    let branch = issue.branch().clone();

    let agent = agents::Agent::from_config(
        &cfg.repo.agent.agent_type,
        Some(&cfg.repo.agent.model),
        &cfg.repo.agent.disallowed_tools,
    )
    .unwrap_or_else(|| agents::Agent::from_config("claude", None, &[]).unwrap());

    let task = issue.to_prompt(provider);

    let copy_files: Vec<std::path::PathBuf> =
        cfg.repo.worktree.copy.iter().map(std::path::PathBuf::from).collect();
    let on_create = cfg.repo.worktree.on_create.as_ref().map(|cmd| {
        let mut c = std::process::Command::new("sh");
        c.args(["-c", cmd]);
        c
    });

    let worker = Worker::spawn(
        &repo,
        &branch,
        &base,
        &agent,
        task,
        true,
        Some(issue.id().clone()),
        &copy_files,
        on_create,
        mux,
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
