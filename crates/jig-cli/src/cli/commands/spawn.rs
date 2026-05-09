//! Spawn command - create worktree and launch Claude in tmux

use clap::Args;

use crate::context;
use crate::worker::Worker;
use jig_core::agents;
use jig_core::git::Branch;
use crate::context::ContextError;
use crate::terminal;
use jig_core::mux::TmuxMux;

use crate::cli::op::{NoOutput, Op};
use crate::context::Context;
use crate::cli::ui;

/// Create worktree and launch Claude in tmux
#[derive(Args, Debug, Clone)]
pub struct Spawn {
    /// Branch name (derived from --issue if omitted)
    pub branch: Option<String>,

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
    Context(#[from] ContextError),
    #[error(transparent)]
    Worker(#[from] crate::worker::WorkerError),
    #[error(transparent)]
    Git(#[from] jig_core::GitError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Linear(#[from] jig_core::issues::providers::linear::client::LinearError),
    #[error("{0}")]
    Usage(String),
}

impl Op for Spawn {
    type Error = SpawnError;
    type Output = NoOutput;

    fn run(&self) -> Result<Self::Output, Self::Error> {
        let cfg = Context::from_cwd()?;
        let repo = cfg.repo()?;

        if terminal::which("tmux").is_none() {
            return Err(SpawnError::Usage("missing dependency: tmux".into()));
        }
        if terminal::which("claude").is_none() {
            return Err(SpawnError::Usage("missing dependency: claude".into()));
        }

        let issue = if let Some(ref issue_ref) = self.issue {
            let provider = repo.issue_provider(&cfg.config)?;
            Some(
                provider
                    .get(issue_ref)?
                    .ok_or_else(|| SpawnError::Usage(format!("issue not found: {}", issue_ref)))?,
            )
        } else {
            None
        };

        let branch_name = if let Some(ref explicit) = self.branch {
            explicit.clone()
        } else if let Some(ref issue) = issue {
            issue.branch().to_string()
        } else {
            return Err(SpawnError::Usage(
                "branch name required: provide a name argument or use --issue".into(),
            ));
        };

        let worktree_path = repo.worktrees_path.join(&branch_name);
        if worktree_path.exists() {
            return Err(SpawnError::Usage(format!(
                "Worktree '{}' already exists — use `jig resume` or `jig attach`",
                branch_name
            )));
        }

        // Resolve base branch
        let parent_issue = issue
            .as_ref()
            .and_then(|i| i.parent())
            .and_then(|parent_ref| {
                let provider = repo.issue_provider(&cfg.config).ok()?;
                provider.get(parent_ref).ok().flatten()
            });
        let base_branch = if let Some(b) = &self.base {
            Branch::new(b)
        } else if let Some(p) = &parent_issue {
            Branch::new(format!("origin/{}", p.branch()))
        } else {
            repo.base_branch(&cfg.config)
        };

        // Track issue ID before consuming the issue
        let issue_id_for_status = issue.as_ref().map(|i| i.id().clone());
        let issue_context = issue.map(|i| i.body().to_string());

        // Build effective context: --context takes precedence, issue body as fallback
        let effective_context = match (&self.context, &issue_context) {
            (Some(ctx), _) => Some(ctx.clone()),
            (None, Some(body)) => Some(body.clone()),
            (None, None) => None,
        };

        let jig_config = context::JigToml::load(&repo.repo_root)?.unwrap_or_default();
        let agent = agents::Agent::from_config(
            &jig_config.agent.agent_type,
            Some(&jig_config.agent.model),
            &jig_config.agent.disallowed_tools,
        )
        .unwrap_or_else(|| agents::Agent::from_config("claude", None, &[]).unwrap());

        let git_repo = jig_core::Repo::open(&repo.repo_root)?;
        let branch = Branch::new(&branch_name);

        let task_context = effective_context.as_deref().unwrap_or(
            "No specific task provided. Check CLAUDE.md and the issue tracker for context.",
        );
        let prompt = crate::prompts::spawn_task_raw(task_context);

        let copy_files: Vec<std::path::PathBuf> =
            jig_config.worktree.copy.iter().map(std::path::PathBuf::from).collect();
        let on_create = jig_config.worktree.on_create.as_ref().map(|cmd| {
            let mut c = std::process::Command::new("sh");
            c.args(["-c", cmd]);
            c
        });

        let issue_ref = self.issue.as_deref().map(jig_core::IssueRef::new);
        let repo_name = repo.repo_root
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".to_string());
        let mux = TmuxMux::for_repo(&repo_name);
        let _worker = Worker::spawn(
            &git_repo,
            &branch,
            &base_branch,
            &agent,
            prompt,
            false,
            issue_ref,
            &copy_files,
            on_create,
            &mux,
        )?;

        if let Some(ref issue_id) = issue_id_for_status {
            if let Ok(provider) = repo.issue_provider(&cfg.config) {
                let _ = provider.update_status(issue_id, &jig_core::IssueStatus::InProgress);
            }
        }

        ui::success(&format!(
            "Launched Claude in tmux window '{}'",
            ui::highlight(&branch)
        ));

        eprintln!();
        eprintln!(
            "  Use '{}' to attach",
            ui::highlight(&format!("jig attach {}", branch))
        );
        eprintln!("  Use '{}' to check status", ui::highlight("jig ps"));

        Ok(NoOutput)
    }
}
