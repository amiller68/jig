//! Spawn command - create worktree and launch Claude in tmux

use clap::Args;

use jig_core::global::GlobalConfig;
use jig_core::issues::naming::{derive_worker_name, extract_linear_identifier};
use jig_core::worktree::Worktree;
use jig_core::{config, issues, terminal, Error, JigToml};

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

        // Resolve issue early so we can derive the name if needed
        let jig_toml = JigToml::load(&repo.repo_root)?.unwrap_or_default();
        let issue_ref = self.issue.as_deref();

        // Resolve the issue ID — if the input is a branch-format string,
        // extract the Linear identifier for the API lookup.
        let resolved_issue_id = issue_ref
            .and_then(|raw| extract_linear_identifier(raw).or_else(|| Some(raw.to_string())));

        let issue = if let Some(ref id) = resolved_issue_id {
            let global_config = GlobalConfig::load().unwrap_or_default();
            let provider = issues::make_provider(&repo.repo_root, &jig_toml, &global_config)?;
            Some(
                provider
                    .get(id)?
                    .ok_or_else(|| Error::Custom(format!("issue not found: {}", id)))?,
            )
        } else {
            None
        };

        // Resolve the worktree name: explicit > derived from issue > error
        let name = if let Some(ref explicit) = self.name {
            explicit.clone()
        } else if let Some(ref issue) = issue {
            derive_worker_name(&issue.id, issue.branch_name.as_deref())
        } else {
            return Err(Error::Custom(
                "worktree name required: provide a name argument or use --issue".into(),
            )
            .into());
        };

        let worktree_path = repo.worktrees_dir.join(&name);

        // Create worktree if needed using Worktree::create
        let wt = if !worktree_path.exists() {
            let base = self.base.as_deref().unwrap_or(&repo.base_branch);
            let copy_files = config::get_copy_files(&repo.repo_root)?;
            let on_create_hook = config::get_on_create_hook(&repo.repo_root)?;

            let wt = Worktree::create(
                &repo.repo_root,
                &repo.worktrees_dir,
                &repo.git_common_dir,
                &name,
                None,
                base,
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
            Worktree::open(&repo.repo_root, &repo.worktrees_dir, &name)?
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
        wt.register(effective_context.as_deref(), issue_ref)?;
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
