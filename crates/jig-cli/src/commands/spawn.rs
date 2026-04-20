//! Spawn command - create worktree and launch Claude in tmux

use clap::Args;

use jig_core::git::Branch;
use jig_core::Worker;
use jig_core::{config, terminal, Error};

use crate::op::{NoOutput, Op, RepoCtx};
use crate::ui;

/// Create worktree and launch Claude in tmux
#[derive(Args, Debug, Clone)]
pub struct Spawn {
    /// Worktree name (derived from --issue if omitted)
    pub name: Option<String>,

    /// Task context/description
    #[arg(long, short)]
    pub context: Option<String>,

    /// Issue ID or branch name to work on (e.g. "AUT-5044" or "feature/aut-5044-refactor-foo")
    #[arg(long, short = 'I')]
    pub issue: Option<String>,

    /// Base branch to create worktree from (overrides jig.toml default)
    #[arg(long, short = 'b')]
    pub base: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum SpawnError {
    #[error(transparent)]
    Core(#[from] Error),
    #[error(transparent)]
    Git(#[from] jig_core::GitError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl Op for Spawn {
    type Error = SpawnError;
    type Output = NoOutput;

    fn run(&self, ctx: &RepoCtx) -> Result<Self::Output, Self::Error> {
        let repo = ctx.repo()?;

        // Check for tmux
        if !terminal::command_exists("tmux") {
            return Err(Error::MissingDependency("tmux".to_string()).into());
        }

        // Check for claude
        if !terminal::command_exists("claude") {
            return Err(Error::MissingDependency("claude".to_string()).into());
        }

        let issue = if let Some(ref issue_ref) = self.issue {
            let provider = repo.issue_provider()?;
            Some(
                provider
                    .get(issue_ref)?
                    .ok_or_else(|| Error::Custom(format!("issue not found: {}", issue_ref)))?,
            )
        } else {
            None
        };

        // Resolve the worktree name: explicit > derived from issue > error
        let name = if let Some(ref explicit) = self.name {
            explicit.clone()
        } else if let Some(ref issue) = issue {
            issue
                .branch_name
                .as_deref()
                .map(|b| Branch::new(b).to_string())
                .unwrap_or_else(|| issue.id.to_lowercase())
        } else {
            return Err(Error::Custom(
                "worktree name required: provide a name argument or use --issue".into(),
            )
            .into());
        };

        let worktree_path = repo.worktrees_path.join(&name);

        // Create worktree if needed using Worker::create
        let wt = if !worktree_path.exists() {
            // Resolve base branch: explicit --base > parent branch > repo default
            let parent_base = issue
                .as_ref()
                .and_then(|i| i.parent.as_ref())
                .and_then(|p| p.branch_name.as_deref())
                .map(|b| format!("origin/{}", b));
            let base = self
                .base
                .as_deref()
                .or(parent_base.as_deref())
                .unwrap_or(&repo.base_branch);
            let copy_files = config::get_copy_files(&repo.repo_root)?;
            let on_create_hook = config::get_on_create_hook(&repo.repo_root)?;

            let git_repo = jig_core::Repo::open(&repo.repo_root)?;
            let branch = Branch::new(&name);
            let base_branch = Branch::new(base);
            let wt = Worker::create(
                &git_repo,
                &branch,
                &base_branch,
                on_create_hook.as_deref(),
                &copy_files,
                false,
            )?;

            ui::success(&format!(
                "Created worktree '{}' from '{}'",
                ui::highlight(&name),
                ui::highlight(base)
            ));

            wt
        } else {
            Worker::open(&repo.repo_root, &repo.worktrees_path, &name)?
        };

        // Track issue ID before consuming the issue
        let issue_id_for_status = issue.as_ref().map(|i| i.id.clone());
        let issue_context = issue.map(|i| i.body);

        // Build effective context: --context takes precedence, issue body as fallback
        let effective_context = match (&self.context, &issue_context) {
            (Some(ctx), _) => Some(ctx.clone()),
            (None, Some(body)) => Some(body.clone()),
            (None, None) => None,
        };

        // Register and launch using Worktree methods
        wt.register(self.issue.as_deref())?;
        wt.launch(effective_context.as_deref())?;

        // Update issue status to InProgress to prevent duplicate spawning
        if let Some(ref issue_id) = issue_id_for_status {
            jig_core::spawn::update_issue_status(&repo.repo_root, issue_id);
        }

        ui::success(&format!(
            "Launched Claude in tmux window '{}'",
            ui::highlight(&name)
        ));

        eprintln!();
        eprintln!(
            "  Use '{}' to attach",
            ui::highlight(&format!("jig attach {}", name))
        );
        eprintln!("  Use '{}' to check status", ui::highlight("jig ps"));

        Ok(NoOutput)
    }
}
