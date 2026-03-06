//! List worktrees command

use std::path::Path;

use clap::Args;
use colored::Colorize;
use comfy_table::{presets, Attribute, Cell, CellAlignment, Color, ContentArrangement, Table};

use jig_core::git;

use crate::op::{GlobalCtx, Op, RepoCtx};

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

        if self.plain {
            let out = worktrees
                .iter()
                .map(|w| format!("{w}\n"))
                .collect::<String>();
            return Ok(ListOutput(out));
        }

        let table = build_worktree_table(&worktrees, &repo.worktrees_dir, &repo.base_branch);
        eprintln!("{table}");
        Ok(ListOutput(String::new()))
    }

    fn run_global(&self, ctx: &GlobalCtx) -> Result<Self::Output, Self::Error> {
        if self.all {
            return self.list_all_git_worktrees();
        }

        if self.plain {
            return self.run_global_plain(ctx);
        }

        self.run_global_table(ctx)
    }
}

impl List {
    fn list_all_git_worktrees(&self) -> Result<ListOutput, ListError> {
        let worktrees = git::list_all_worktrees()?;
        for (path, branch) in &worktrees {
            let branch_display = if branch.is_empty() {
                "(detached)".dimmed().to_string()
            } else {
                branch.cyan().to_string()
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
            out.push_str(&format!("{}:\n", repo_name.bold()));
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
            eprintln!("{}", repo_name.bold());
            let table = build_worktree_table(&worktrees, &repo.worktrees_dir, &repo.base_branch);
            eprintln!("{table}");
        }
        Ok(ListOutput(String::new()))
    }
}

fn build_worktree_table(names: &[String], worktrees_dir: &Path, base_branch: &str) -> Table {
    let mut table = Table::new();
    table
        .load_preset(presets::NOTHING)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(vec![
            Cell::new("NAME").add_attribute(Attribute::Bold),
            Cell::new("BRANCH").add_attribute(Attribute::Bold),
            Cell::new("COMMITS").add_attribute(Attribute::Bold),
        ]);

    for name in names {
        let wt_path = worktrees_dir.join(name);

        let branch = git::get_worktree_branch(&wt_path).unwrap_or_else(|_| "?".to_string());

        // Only show branch if it differs from worktree name
        let (branch_display, branch_color) = if branch == *name {
            ("-".to_string(), Color::DarkGrey)
        } else if branch == base_branch {
            (crate::ui::truncate(&branch, 40), Color::DarkGrey)
        } else {
            (crate::ui::truncate(&branch, 40), Color::Cyan)
        };

        let commits_ahead = git::get_commits_ahead(&wt_path, base_branch)
            .map(|c| c.len())
            .unwrap_or(0);
        let dirty = git::has_uncommitted_changes(&wt_path).unwrap_or(false);

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
