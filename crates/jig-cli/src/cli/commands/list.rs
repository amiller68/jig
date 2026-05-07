//! List worktrees command

use std::path::Path;

use clap::Args;
use comfy_table::{Cell, CellAlignment, Color};

use crate::context::{Config, Context, RepoConfig};
use crate::worker::events::{self, WorkerState};
use crate::worker::WorkerStatus;
use jig_core::git::{Branch, Repo};

use crate::cli::op::Op;
use crate::cli::ui;

/// List worktrees
#[derive(Args, Debug, Clone)]
pub struct List {
    /// Show all git worktrees (including base repo)
    #[arg(long)]
    pub all: bool,

    /// Plain output (bare names, no table)
    #[arg(short, long)]
    pub plain: bool,

    /// Operate on all tracked repos
    #[arg(short = 'g', long)]
    pub global: bool,
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

    fn run(&self) -> Result<Self::Output, Self::Error> {
        if self.all {
            return self.list_all_git_worktrees();
        }

        if self.global {
            let cfg = Context::from_global()?;
            if self.plain || ui::is_plain() {
                return self.run_global_plain(&cfg.repos, &cfg.config);
            }
            return self.run_global_table(&cfg.repos, &cfg.config);
        }

        let cfg = Context::from_cwd()?;
        let repo = cfg.repo()?;
        let git_repo = Repo::open(&repo.repo_root)?;
        let worktrees = git_repo.list_worktrees()?;
        let names: Vec<String> = worktrees
            .iter()
            .map(|wt| wt.branch_name().to_string())
            .collect();
        if names.is_empty() {
            eprintln!("No worktrees found");
        }

        if self.plain || ui::is_plain() {
            let out = names.iter().map(|w| format!("{w}\n")).collect::<String>();
            return Ok(ListOutput(out));
        }

        let base_branch = repo.base_branch(&cfg.config);
        let table = build_worktree_table(&names, &repo.worktrees_path, &base_branch, &repo.repo_root);
        eprintln!("{table}");
        Ok(ListOutput(String::new()))
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

    fn run_global_plain(&self, repos: &[RepoConfig], _global: &Config) -> Result<ListOutput, ListError> {
        let mut out = String::new();
        let mut first = true;
        for cfg in repos {
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
                out.push_str(&format!("  {}\n", wt.branch_name()));
            }
        }
        Ok(ListOutput(out))
    }

    fn run_global_table(&self, repos: &[RepoConfig], global: &Config) -> Result<ListOutput, ListError> {
        let mut first = true;
        for cfg in repos {
            let git_repo = Repo::open(&cfg.repo_root)?;
            let wts = git_repo.list_worktrees()?;
            if wts.is_empty() {
                continue;
            }
            let worktrees: Vec<String> =
                wts.iter().map(|wt| wt.branch_name().to_string()).collect();
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
            let base_branch = cfg.base_branch(global);
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
    let event_log = events::event_log_for_worker(&repo_name, name).ok()?;
    if !event_log.exists() {
        return None;
    }
    let config = Config::load().ok()?;
    let mut state: WorkerState = event_log.reduce().ok()?;
    state.check_silence(&config);
    Some(state.status)
}

fn build_worktree_table(
    names: &[String],
    worktrees_path: &Path,
    base_branch: &Branch,
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
                } else if *base_branch == branch {
                    (crate::cli::ui::truncate(&branch, 40), Color::DarkGrey)
                } else {
                    (crate::cli::ui::truncate(&branch, 40), Color::Cyan)
                }
            }
        };

        let commits_ahead = Repo::open(&wt_path)
            .and_then(|r| r.commits_ahead(base_branch))
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
            Cell::new(crate::cli::ui::truncate(name, 48)).fg(Color::White),
            Cell::new(&branch_display).fg(branch_color),
            Cell::new(&commits_str)
                .fg(commit_color)
                .set_alignment(CellAlignment::Right),
        ]);
    }

    table
}
