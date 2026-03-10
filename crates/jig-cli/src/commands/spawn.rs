//! Spawn command - create worktree and launch Claude in tmux

use clap::Args;

use jig_core::global::GlobalConfig;
use jig_core::worktree::Worktree;
use jig_core::{config, issues, terminal, Error, JigToml};

use crate::op::{NoOutput, Op, RepoCtx};
use crate::ui;

/// Create worktree and launch Claude in tmux
#[derive(Args, Debug, Clone)]
pub struct Spawn {
    /// Worktree name
    pub name: String,

    /// Task context/description
    #[arg(long, short)]
    pub context: Option<String>,

    /// Issue ID to work on (e.g. "features/smart-context-injection")
    #[arg(long, short = 'I')]
    pub issue: Option<String>,

    /// Base branch to create worktree from (overrides jig.toml default)
    #[arg(long, short = 'b')]
    pub base: Option<String>,

    /// Auto-start Claude with full prompt
    #[arg(long)]
    pub auto: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum SpawnError {
    #[error(transparent)]
    Core(#[from] Error),
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

        let worktree_path = repo.worktrees_dir.join(&self.name);

        // Create worktree if needed using Worktree::create
        let wt = if !worktree_path.exists() {
            let base = self.base.as_deref().unwrap_or(&repo.base_branch);
            let copy_files = config::get_copy_files(&repo.repo_root)?;
            let on_create_hook = config::get_on_create_hook(&repo.repo_root)?;

            let wt = Worktree::create(
                &repo.repo_root,
                &repo.worktrees_dir,
                &repo.git_common_dir,
                &self.name,
                None,
                base,
                on_create_hook.as_deref(),
                &copy_files,
                false,
            )?;

            ui::success(&format!(
                "Created worktree '{}' from '{}'",
                ui::highlight(&self.name),
                ui::highlight(base)
            ));

            wt
        } else {
            Worktree::open(&repo.repo_root, &repo.worktrees_dir, &self.name)?
        };

        // Resolve issue if provided
        let jig_toml = JigToml::load(&repo.repo_root)?.unwrap_or_default();
        let issue_ref = self.issue.as_deref();
        let issue_context = if let Some(id) = issue_ref {
            let global_config = GlobalConfig::load().unwrap_or_default();
            let provider = issues::make_provider(&repo.repo_root, &jig_toml, &global_config)?;
            let issue = provider
                .get(id)?
                .ok_or_else(|| Error::Custom(format!("issue not found: {}", id)))?;
            Some(issue.body)
        } else {
            None
        };

        // Build effective context: --context takes precedence, issue body as fallback
        let effective_context = match (&self.context, &issue_context) {
            (Some(ctx), _) => Some(ctx.clone()),
            (None, Some(body)) => Some(body.clone()),
            (None, None) => None,
        };

        // Determine if auto mode should be used
        let use_auto = if self.auto { true } else { jig_toml.spawn.auto };

        // Register and launch using Worktree methods
        wt.register(effective_context.as_deref(), issue_ref)?;
        wt.launch(effective_context.as_deref(), use_auto)?;

        ui::success(&format!(
            "Launched Claude in tmux window '{}'",
            ui::highlight(&self.name)
        ));

        if use_auto {
            ui::detail("Auto mode enabled");
        }

        eprintln!();
        eprintln!(
            "  Use '{}' to attach",
            ui::highlight(&format!("jig attach {}", self.name))
        );
        eprintln!("  Use '{}' to check status", ui::highlight("jig ps"));

        Ok(NoOutput)
    }
}
