//! List worktrees command

use std::path::Path;

use clap::Args;
use comfy_table::{Cell, CellAlignment, Color};

use jig_core::git::{Branch, Repo};
use jig_core::worker::events::{EventLog, WorkerState};
use jig_core::worker::WorkerStatus;
use jig_core::GlobalConfig;

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
    #[error(transparent)]
    Git(#[from] jig_core::GitError),
}

impl Op for List {
    type Error = ListError;
    type Output = ListOutput;

    fn run(&self, ctx: &RepoCtx) -> Result<Self::Output, Self::Error> {
        if self.all {
            return self.list_all_git_worktrees();
        }

        let cfg = ctx.config()?;
        let git_repo = Repo::open(&cfg.repo_root)?;
        let worktrees = git_repo.list_worktrees()?;
        let names: Vec<String> = worktrees.iter().map(|wt| wt.branch_name().to_string()).collect();
        if names.is_empty() {
            eprintln!("No worktrees found");
        }

        if self.plain || ui::is_plain() {
            let out = names.iter().map(|w| format!("{w}\n")).collect::<String>();
            return Ok(ListOutput(out));
        }

        let base_branch = cfg.base_branch();
        let table = build_worktree_table(&names, &cfg.worktrees_path, &base_branch, &cfg.repo_root);
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
        let worktrees = repo.list_worktrees()?;
        for wt in &worktrees {
            let branch_display = match wt.branch() {
                Ok(b) => ui::highlight(&b),
                Err(_) => ui::dim("(detached)"),
            };
            eprintln!("{} {}", wt.path().display(), branch_display);
        }
        Ok(ListOutput(String::new()))
    }

    fn run_global_plain(&self, ctx: &GlobalCtx) -> Result<ListOutput, ListError> {
        let mut out = String::new();
        let mut first = true;
        for cfg in &ctx.configs {
            let git_repo = Repo::open(&cfg.repo_root)?;
            let worktrees = git_repo.list_worktrees()?;
            if worktrees.is_empty() {
                continue;
            }
            let repo_name = cfg
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
                out.push_str(&format!("  {}\n", wt.branch_name().to_string()));
            }
        }
        Ok(ListOutput(out))
    }

    fn run_global_table(&self, ctx: &GlobalCtx) -> Result<ListOutput, ListError> {
        let mut first = true;
        for cfg in &ctx.configs {
            let git_repo = Repo::open(&cfg.repo_root)?;
            let wts = git_repo.list_worktrees()?;
            if wts.is_empty() {
                continue;
            }
            let worktrees: Vec<String> = wts.iter().map(|wt| wt.branch_name().to_string()).collect();
            let repo_name = cfg
                .repo_root
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "unknown".to_string());
            if !first {
                eprintln!();
            }
            first = false;
            ui::header(&repo_name);
            let base_branch = cfg.base_branch();
            let table = build_worktree_table(
                &worktrees,
                &cfg.worktrees_path,
                &base_branch,
                &cfg.repo_root,
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
    worktrees_path: &Path,
    base_branch: &str,
    repo_root: &Path,
) -> comfy_table::Table {
    let mut table = ui::new_table(&["NAME", "BRANCH", "COMMITS"]);

    for name in names {
        let wt_path = worktrees_path.join(name);

        // Check if worker is initializing or failed
        let worker_status = worktree_event_status(repo_root, name);

        let branch = Repo::open(&wt_path)
            .and_then(|r| r.current_branch())
            .map(|b| b.to_string())
            .unwrap_or_else(|_| "?".to_string());

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

        let commits_ahead = Repo::open(&wt_path)
            .and_then(|r| r.commits_ahead(&Branch::new(base_branch)))
            .map(|c| c.len())
            .unwrap_or(0);
        let dirty = Repo::open(&wt_path)
            .and_then(|r| r.has_uncommitted_changes())
            .unwrap_or(false);

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
