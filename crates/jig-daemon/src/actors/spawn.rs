//! Spawn actor — creates worktrees and launches agents in a background thread.

use std::path::{Path, PathBuf};

use jig_core::agents;
use jig_core::config::{self, Config};
use jig_core::git::{Branch, Repo};
use jig_core::issues::{Issue, IssueStatus, ProviderKind};
use jig_core::worker::{Worker, SPAWN_PREAMBLE};

use crate::actors::Actor;

#[derive(Debug, Clone)]
pub struct SpawnableIssue {
    pub repo_root: PathBuf,
    pub issue: Issue,
    pub worker_name: String,
    pub provider_kind: ProviderKind,
}

pub struct SpawnRequest {
    pub issues: Vec<SpawnableIssue>,
}

pub struct SpawnResult {
    pub worker_name: String,
    pub repo_name: String,
    pub issue_id: Option<String>,
    pub error: Option<String>,
}

pub struct SpawnComplete {
    pub results: Vec<SpawnResult>,
}

pub struct SpawnActor {
    tx: flume::Sender<SpawnRequest>,
    rx: flume::Receiver<SpawnComplete>,
    pending: bool,
    spawning_workers: Vec<String>,
}

impl Actor for SpawnActor {
    type Request = SpawnRequest;
    type Response = SpawnComplete;

    const NAME: &'static str = "jig-spawn";
    const QUEUE_SIZE: usize = 1;

    fn handle(req: SpawnRequest) -> SpawnComplete {
        let mut results = Vec::new();

        for issue in &req.issues {
            let result = spawn_worker_for_issue(&issue.repo_root, &issue.issue, &issue.worker_name);
            let worker_name = issue.worker_name.clone();
            let repo_name = issue
                .repo_root
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            let issue_id = Some(issue.issue.id().to_string());
            match result {
                Ok(()) => {
                    tracing::info!(worker = %worker_name, "auto-spawned worker");
                    results.push(SpawnResult {
                        worker_name,
                        repo_name,
                        issue_id,
                        error: None,
                    });
                }
                Err(msg) => {
                    tracing::warn!(worker = %worker_name, "auto-spawn failed: {}", msg);
                    results.push(SpawnResult {
                        worker_name,
                        repo_name,
                        issue_id,
                        error: Some(msg),
                    });
                }
            }
        }

        SpawnComplete { results }
    }

    fn send(&mut self, req: SpawnRequest) -> bool {
        if self.pending {
            return false;
        }
        self.spawning_workers = req.issues.iter().map(|i| i.worker_name.clone()).collect();
        if self.tx.try_send(req).is_ok() {
            self.pending = true;
            true
        } else {
            self.spawning_workers.clear();
            false
        }
    }

    fn drain(&mut self) -> Vec<SpawnComplete> {
        match self.rx.try_recv() {
            Ok(result) => {
                self.pending = false;
                self.spawning_workers.clear();
                vec![result]
            }
            Err(_) => vec![],
        }
    }

    fn from_channels(
        tx: flume::Sender<SpawnRequest>,
        rx: flume::Receiver<SpawnComplete>,
    ) -> Self {
        Self {
            tx,
            rx,
            pending: false,
            spawning_workers: Vec::new(),
        }
    }
}

impl SpawnActor {
    pub fn is_pending(&self) -> bool {
        self.pending
    }

    pub fn spawning_workers(&self) -> &[String] {
        &self.spawning_workers
    }
}

pub(crate) fn spawn_worker_for_issue(
    repo_root: &Path,
    issue: &Issue,
    worker_name: &str,
) -> std::result::Result<(), String> {
    let worktree_path = config::worktree_path(repo_root, worker_name);

    if worktree_path.exists() {
        tracing::debug!(worker = %worker_name, "worktree already exists, skipping");
        return Ok(());
    }

    let cfg = Config::from_path(repo_root).map_err(|e| e.to_string())?;
    let provider = cfg.issue_provider().map_err(|e| e.to_string())?;

    let parent = issue
        .parent()
        .and_then(|r| provider.get(r).ok().flatten());

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
        .to_prompt(SPAWN_PREAMBLE, &provider)
        .var_num("max_nudges", cfg.global.health.max_nudges);

    let _worker = Worker::spawn(
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

    Ok(())
}
