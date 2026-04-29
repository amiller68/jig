//! Spawn command - create worktree and launch Claude in tmux

use clap::Args;

use crate::config;
use crate::worker::TmuxWorker as Worker;
use jig_core::agents;
use jig_core::git::Branch;
use jig_core::{mux, Error, Prompt};

use crate::cli::op::{NoOutput, Op, RepoCtx};
use crate::cli::ui;

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
        let cfg = ctx.config()?;

        if !mux::command_exists("tmux") {
            return Err(Error::MissingDependency("tmux".to_string()).into());
        }
        if !mux::command_exists("claude") {
            return Err(Error::MissingDependency("claude".to_string()).into());
        }

        let issue = if let Some(ref issue_ref) = self.issue {
            let provider = cfg.issue_provider()?;
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
            issue.branch().to_string()
        } else {
            return Err(Error::Custom(
                "worktree name required: provide a name argument or use --issue".into(),
            )
            .into());
        };

        let worktree_path = cfg.worktrees_path.join(&name);
        if worktree_path.exists() {
            return Err(Error::Custom(format!(
                "Worktree '{}' already exists — use `jig resume` or `jig attach`",
                name
            ))
            .into());
        }

        // Resolve base branch
        let parent_issue = issue
            .as_ref()
            .and_then(|i| i.parent())
            .and_then(|parent_ref| {
                let provider = cfg.issue_provider().ok()?;
                provider.get(parent_ref).ok().flatten()
            });
        let parent_base = parent_issue
            .as_ref()
            .map(|p| format!("origin/{}", p.branch()));
        let base_branch_str = cfg.base_branch();
        let base = self
            .base
            .as_deref()
            .or(parent_base.as_deref())
            .unwrap_or(&base_branch_str);

        // Track issue ID before consuming the issue
        let issue_id_for_status = issue.as_ref().map(|i| i.id().clone());
        let issue_context = issue.map(|i| i.body().to_string());

        // Build effective context: --context takes precedence, issue body as fallback
        let effective_context = match (&self.context, &issue_context) {
            (Some(ctx), _) => Some(ctx.clone()),
            (None, Some(body)) => Some(body.clone()),
            (None, None) => None,
        };

        let global_config = crate::config::GlobalConfig::load()?;
        let jig_config = config::JigToml::load(&cfg.repo_root)?.unwrap_or_default();
        let agent = agents::Agent::from_name(&jig_config.agent.agent_type)
            .unwrap_or_else(|| agents::Agent::from_kind(agents::AgentKind::Claude))
            .with_disallowed_tools(jig_config.agent.disallowed_tools.clone());

        let git_repo = jig_core::Repo::open(&cfg.repo_root)?;
        let branch = Branch::new(&name);
        let base_branch = Branch::new(base);

        let prompt = Prompt::new(crate::worker::SPAWN_PREAMBLE)
            .var(
                "task_context",
                effective_context.as_deref().unwrap_or(
                    "No specific task provided. Check CLAUDE.md and the issue tracker for context.",
                ),
            )
            .var_num("max_nudges", global_config.health.max_nudges);

        let issue_ref = self.issue.as_deref().map(jig_core::IssueRef::new);
        let _worker = Worker::spawn(
            &git_repo,
            &branch,
            &base_branch,
            &agent,
            prompt,
            false,
            issue_ref,
        )?;

        if let Some(ref issue_id) = issue_id_for_status {
            if let Ok(provider) = cfg.issue_provider() {
                let _ = provider.update_status(issue_id, &jig_core::IssueStatus::InProgress);
            }
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
