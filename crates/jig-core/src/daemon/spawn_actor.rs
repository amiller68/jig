//! Spawn actor — creates worktrees and launches agents in a background thread.
//!
//! This keeps the on-create hook (which can be slow, e.g. `pnpm install`)
//! off the main tick thread so the UI stays responsive.

use crate::config::{self, JIG_DIR};
use crate::context::RepoContext;
use crate::git::Repo;
use crate::issues::ProviderKind;
use crate::worktree::Worktree;

use super::messages::{SpawnComplete, SpawnRequest, SpawnResult, SpawnableIssue};

/// Spawn the spawn actor thread. Returns immediately.
pub fn spawn(
    rx: flume::Receiver<SpawnRequest>,
    tx: flume::Sender<SpawnComplete>,
) -> std::thread::JoinHandle<()> {
    std::thread::Builder::new()
        .name("jig-spawn".into())
        .spawn(move || {
            while let Ok(req) = rx.recv() {
                let mut results = Vec::new();

                for issue in &req.issues {
                    let result = spawn_single(issue);
                    let worker_name = issue.worker_name.clone();
                    match result {
                        Ok(()) => {
                            tracing::info!(worker = %worker_name, "auto-spawned worker");
                            results.push(SpawnResult {
                                worker_name,
                                error: None,
                            });
                        }
                        Err(msg) => {
                            tracing::warn!(worker = %worker_name, "auto-spawn failed: {}", msg);
                            results.push(SpawnResult {
                                worker_name,
                                error: Some(msg),
                            });
                        }
                    }
                }

                if tx.send(SpawnComplete { results }).is_err() {
                    break;
                }
            }
        })
        .expect("failed to spawn spawn actor thread")
}

/// Spawn a single worker: create worktree, register as initializing, run hook, launch.
fn spawn_single(issue: &SpawnableIssue) -> std::result::Result<(), String> {
    let repo_root = &issue.repo_root;
    let worktrees_dir = repo_root.join(JIG_DIR);
    let worktree_path = config::worktree_path(repo_root, &issue.worker_name);

    if worktree_path.exists() {
        tracing::debug!(worker = %issue.worker_name, "worktree already exists, skipping");
        return Ok(());
    }

    let base_branch = RepoContext::resolve_base_branch_for(repo_root)
        .unwrap_or_else(|_| config::DEFAULT_BASE_BRANCH.to_string());

    let copy_files = config::get_copy_files(repo_root).map_err(|e| e.to_string())?;
    let on_create_hook = config::get_on_create_hook(repo_root).map_err(|e| e.to_string())?;
    let git_common_dir = Repo::open(repo_root)
        .map_err(|e| e.to_string())?
        .common_dir();

    // Create worktree WITHOUT running on-create hook — we handle it after registration
    let wt = Worktree::create(
        repo_root,
        &worktrees_dir,
        &git_common_dir,
        &issue.worker_name,
        None,
        &base_branch,
        None, // defer on-create hook
        &copy_files,
        true,
    )
    .map_err(|e| e.to_string())?;

    let context = build_context(issue);

    // Register as Initializing so jig ps/ls show the worker immediately
    wt.register_initializing(Some(&context), Some(&issue.issue_id))
        .map_err(|e| e.to_string())?;

    // Run on-create hook now that the worker is visible
    if let Some(hook) = on_create_hook.as_deref() {
        let success = config::run_on_create_hook(hook, &wt.path).map_err(|e| e.to_string())?;
        if !success {
            wt.emit_setup_failed("on-create hook failed");
            return Err("on-create hook failed".to_string());
        }
    }

    // Transition from Initializing → Spawned
    wt.emit_spawn_event();

    wt.launch(Some(&context)).map_err(|e| e.to_string())?;

    Ok(())
}

fn build_context(issue: &SpawnableIssue) -> String {
    let completion_instructions = match issue.provider_kind {
        ProviderKind::File => format!(
            "\n\nISSUE COMPLETION: This issue is tracked by the file provider. \
             After your PR is created, mark the issue as done by changing \
             `**Status:** Planned` to `**Status:** Complete` in the issue file \
             (`issues/{}.md`) and committing the change.",
            issue.issue_id
        ),
        ProviderKind::Linear => "\n\nISSUE COMPLETION: This issue is tracked by Linear. \
             Status sync is handled automatically — no manual status update is needed."
            .to_string(),
    };
    format!(
        "{}\n\n{}{}",
        issue.issue_title, issue.issue_body, completion_instructions
    )
}
