//! List worktrees command

use std::path::Path;

use clap::Args;
use comfy_table::{Cell, CellAlignment, Color};

use jig_core::events::{EventLog, WorkerState};
use jig_core::git::{self, Repo};
use jig_core::global::GlobalConfig;
use jig_core::worker::WorkerStatus;

use crate::op::{GlobalCtx, Op, RepoCtx};
use crate::ui;

/// List worktrees
#[derive(Args, Debug, Clone)]
pub struct List {
    /// Show all git worktrees (including base repo)
    #[arg(long)]
    pub all: bool,

    /// Plain output (bare names, no table)
    #[arg(short, long)]
    pub plain: bool,
}

/// Output containing worktree names
#[derive(Debug)]
pub struct ListOutput(String);

impl std::fmt::Display for ListOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ListError {
    #[error(transparent)]
    Core(#[from] jig_core::Error),
}

impl Op for List {
    type Error = ListError;
    type Output = ListOutput;

    fn run(&self, ctx: &RepoCtx) -> Result<Self::Output, Self::Error> {
        if self.all {
            return self.list_all_git_worktrees();
        }

        let repo = ctx.repo()?;
        let worktrees = git::list_worktree_names(&repo.worktrees_dir)?;
        if worktrees.is_empty() {
            eprintln!("No worktrees found");
        }

        if self.plain || ui::is_plain() {
            let out = worktrees
                .iter()
                .map(|w| format!("{w}\n"))
                .collect::<String>();
            return Ok(ListOutput(out));
        }

        let table = build_worktree_table(
            &worktrees,
            &repo.worktrees_dir,
            &repo.base_branch,
            &repo.repo_root,
        );
        eprintln!("{table}");
        Ok(ListOutput(String::new()))
    }

    fn run_global(&self, ctx: &GlobalCtx) -> Result<Self::Output, Self::Error> {
        if self.all {
            return self.list_all_git_worktrees();
        }

        if self.plain || ui::is_plain() {
            return self.run_global_plain(ctx);
        }

        self.run_global_table(ctx)
    }
}

impl List {
    fn list_all_git_worktrees(&self) -> Result<ListOutput, ListError> {
        let repo = Repo::discover()?;
        let worktrees = repo.list_all_worktrees()?;
        for (path, branch) in &worktrees {
            let branch_display = if branch.is_empty() {
                ui::dim("(detached)")
            } else {
                ui::highlight(branch)
            };
            eprintln!("{} {}", path.display(), branch_display);
        }
        Ok(ListOutput(String::new()))
    }

    fn run_global_plain(&self, ctx: &GlobalCtx) -> Result<ListOutput, ListError> {
        let mut out = String::new();
        let mut first = true;
        for repo in &ctx.repos {
            let worktrees = git::list_worktree_names(&repo.worktrees_dir)?;
            if worktrees.is_empty() {
                continue;
            }
            let repo_name = repo
                .repo_root
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "unknown".to_string());
            if !first {
                out.push('\n');
            }
            first = false;
            out.push_str(&format!("{}:\n", ui::bold(&repo_name)));
            for wt in &worktrees {
                out.push_str(&format!("  {wt}\n"));
            }
        }
        Ok(ListOutput(out))
    }

    fn run_global_table(&self, ctx: &GlobalCtx) -> Result<ListOutput, ListError> {
        let mut first = true;
        for repo in &ctx.repos {
            let worktrees = git::list_worktree_names(&repo.worktrees_dir)?;
            if worktrees.is_empty() {
                continue;
            }
            let repo_name = repo
                .repo_root
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "unknown".to_string());
            if !first {
                eprintln!();
            }
            first = false;
            ui::header(&repo_name);
            let table = build_worktree_table(
                &worktrees,
                &repo.worktrees_dir,
                &repo.base_branch,
                &repo.repo_root,
            );
            eprintln!("{table}");
        }
        Ok(ListOutput(String::new()))
    }
}

/// Get worker status from event log for a worktree.
fn worktree_event_status(repo_root: &Path, name: &str) -> Option<WorkerStatus> {
    let repo_name = repo_root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());
    let event_log = EventLog::for_worker(&repo_name, name).ok()?;
    let events = event_log.read_all().ok()?;
    if events.is_empty() {
        return None;
    }
    let config = GlobalConfig::load().ok()?.health;
    Some(WorkerState::reduce(&events, &config).status)
}

fn build_worktree_table(
    names: &[String],
    worktrees_dir: &Path,
    base_branch: &str,
    repo_root: &Path,
) -> comfy_table::Table {
    let mut table = ui::new_table(&["NAME", "BRANCH", "COMMITS"]);

    for name in names {
        let wt_path = worktrees_dir.join(name);

        // Check if worker is initializing or failed
        let worker_status = worktree_event_status(repo_root, name);

        let branch = Repo::worktree_branch(&wt_path).unwrap_or_else(|_| "?".to_string());

        // Show status hint for initializing/failed workers
        let (branch_display, branch_color) = match worker_status {
            Some(WorkerStatus::Initializing) => ("setting up...".to_string(), Color::Blue),
            Some(WorkerStatus::Failed) => ("setup failed".to_string(), Color::Red),
            _ => {
                // Only show branch if it differs from worktree name
                if branch == *name {
                    ("-".to_string(), Color::DarkGrey)
                } else if branch == base_branch {
                    (crate::ui::truncate(&branch, 40), Color::DarkGrey)
                } else {
                    (crate::ui::truncate(&branch, 40), Color::Cyan)
                }
            }
        };

        let commits_ahead = Repo::commits_ahead(&wt_path, base_branch)
            .map(|c| c.len())
            .unwrap_or(0);
        let dirty = Repo::has_uncommitted_changes(&wt_path).unwrap_or(false);

        let dirty_marker = if dirty { "*" } else { "" };
        let commits_str = if commits_ahead > 0 || dirty {
            format!("{commits_ahead}{dirty_marker}")
        } else {
            "-".to_string()
        };
        let commit_color = if dirty {
            Color::Yellow
        } else if commits_ahead > 0 {
            Color::White
        } else {
            Color::DarkGrey
        };

        table.add_row(vec![
            Cell::new(crate::ui::truncate(name, 48)).fg(Color::White),
            Cell::new(&branch_display).fg(branch_color),
            Cell::new(&commits_str)
                .fg(commit_color)
                .set_alignment(CellAlignment::Right),
        ]);
    }

    table
}
