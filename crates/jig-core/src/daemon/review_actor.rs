//! Review actor — runs ephemeral Claude Code sessions to review worker code.
//!
//! Follows the actor pattern: a background thread with flume channels that
//! receives `ReviewRequest`s, runs an AI review, and returns `ReviewComplete`s.

use std::process::{Command, Stdio};

use crate::agents;
use crate::review;

use super::messages::{ReviewComplete, ReviewRequest};

/// Spawn the review actor thread. Returns immediately.
///
/// The actor blocks on `rx.recv()` waiting for review requests, runs each
/// review via an ephemeral agent session, and sends `ReviewComplete` back.
pub fn spawn(
    rx: flume::Receiver<ReviewRequest>,
    tx: flume::Sender<ReviewComplete>,
) -> std::thread::JoinHandle<()> {
    std::thread::Builder::new()
        .name("jig-review".into())
        .spawn(move || {
            while let Ok(req) = rx.recv() {
                let result = run_review(&req);
                if tx.send(result).is_err() {
                    break;
                }
            }
        })
        .expect("failed to spawn review actor thread")
}

/// Run a single review cycle for a worker.
fn run_review(req: &ReviewRequest) -> ReviewComplete {
    match run_review_inner(req) {
        Ok(()) => ReviewComplete {
            worker_key: req.worker_key.clone(),
            error: None,
        },
        Err(msg) => {
            tracing::warn!(worker = %req.worker_key, "review failed: {}", msg);
            ReviewComplete {
                worker_key: req.worker_key.clone(),
                error: Some(msg),
            }
        }
    }
}

fn run_review_inner(req: &ReviewRequest) -> Result<(), String> {
    let worktree_path = &req.worktree_path;
    let base_branch = &req.base_branch;

    if !worktree_path.exists() {
        return Err(format!("worktree not found: {}", worktree_path.display()));
    }

    // Get branch name from git
    let branch_output = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(worktree_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("failed to get branch: {}", e))?;
    let branch = String::from_utf8_lossy(&branch_output.stdout)
        .trim()
        .to_string();

    // Compute diff
    let diff_output = Command::new("git")
        .args(["diff", &format!("{}...HEAD", base_branch)])
        .current_dir(worktree_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("failed to compute diff: {}", e))?;
    let diff = String::from_utf8_lossy(&diff_output.stdout).to_string();

    if diff.trim().is_empty() {
        return Err("no diff to review".to_string());
    }

    // Read review history
    let history_files = review::review_history(worktree_path);
    let mut history_text = String::new();
    for file in &history_files {
        if let Ok(content) = std::fs::read_to_string(file) {
            if let Some(name) = file.file_name() {
                history_text.push_str(&format!("### {}\n", name.to_string_lossy()));
            }
            history_text.push_str(&content);
            history_text.push('\n');
        }
    }

    // Count reviews before execution
    let count_before = review::review_count(worktree_path);

    // Build the review prompt
    let prior_reviews = if history_text.is_empty() {
        "(none)".to_string()
    } else {
        history_text
    };

    let prompt = format!(
        "You are a code reviewer for a jig-managed worker. \
         Review the changes on branch {branch} against {base_branch}.\n\
         \n\
         ## Diff\n\
         {diff}\n\
         \n\
         ## Prior Reviews\n\
         {prior_reviews}\n\
         \n\
         ## Project Conventions\n\
         Read CLAUDE.md and docs/PATTERNS.md in the worktree for project-specific conventions.\n\
         \n\
         ## Instructions\n\
         1. Review for: correctness, conventions, error handling, security, test coverage, documentation\n\
         2. For each category, assign [PASS], [WARN], or [FAIL] with specific file:line references\n\
         3. If a prior finding was addressed or reasonably disputed in a response, do not re-raise it\n\
         4. End your review's Summary section with VERDICT: approve or VERDICT: changes_requested\n\
         5. Submit by piping your review to: jig review submit\n\
         6. If jig review submit reports a format error, fix your output and retry",
    );

    // Build the ephemeral command
    let agent = agents::Agent::from_name("claude").ok_or("claude agent not found")?;
    let cmd = agent.ephemeral_command(
        &prompt,
        &[
            "Read",
            "Grep",
            "Glob",
            "Bash(jig review submit:*)",
            "Bash(git diff:*)",
            "Bash(git log:*)",
        ],
    );

    tracing::info!(worker = %req.worker_key, "running review agent");

    // Execute in worktree directory
    let output = Command::new("sh")
        .arg("-c")
        .arg(&cmd)
        .current_dir(worktree_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("failed to execute review agent: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "review agent exited with {}: {}",
            output.status,
            stderr.chars().take(500).collect::<String>()
        ));
    }

    // Check if a new review file appeared
    let count_after = review::review_count(worktree_path);
    if count_after <= count_before {
        return Err("review agent ran but did not submit a review".to_string());
    }

    tracing::info!(
        worker = %req.worker_key,
        review_number = count_after,
        "review submitted successfully"
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn review_request_fields() {
        let req = ReviewRequest {
            worker_key: "myrepo/feature-auth".to_string(),
            worktree_path: PathBuf::from("/tmp/myrepo/.jig/feature-auth"),
            base_branch: "origin/main".to_string(),
        };
        assert_eq!(req.worker_key, "myrepo/feature-auth");
    }

    #[test]
    fn review_complete_success() {
        let complete = ReviewComplete {
            worker_key: "myrepo/feature-auth".to_string(),
            error: None,
        };
        assert!(complete.error.is_none());
    }

    #[test]
    fn review_complete_error() {
        let complete = ReviewComplete {
            worker_key: "myrepo/feature-auth".to_string(),
            error: Some("no diff to review".to_string()),
        };
        assert_eq!(complete.error.as_deref(), Some("no diff to review"));
    }

    #[test]
    fn run_review_missing_worktree() {
        let req = ReviewRequest {
            worker_key: "myrepo/nonexistent".to_string(),
            worktree_path: PathBuf::from("/tmp/nonexistent-worktree-path"),
            base_branch: "origin/main".to_string(),
        };
        let result = run_review(&req);
        assert!(result.error.is_some());
        assert!(result.error.unwrap().contains("worktree not found"));
    }
}
