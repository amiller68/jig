//! Hooks command — manage hook integrations

use clap::{Args, Subcommand};

use jig_core::config::JigToml;

use crate::op::{NoOutput, Op, RepoCtx};
use crate::ui;

/// Manage hook integrations
#[derive(Args, Debug, Clone)]
pub struct Hooks {
    #[command(subcommand)]
    pub subcommand: HooksCommands,
}

#[derive(Subcommand, Debug, Clone)]
pub enum HooksCommands {
    /// Install jig git hooks and agent hooks into the current repo
    Init {
        /// Reinstall all hooks even if already installed
        #[arg(long, short)]
        force: bool,
    },
    /// Remove jig git hooks from the current repo
    Uninstall {
        /// Specific hook to uninstall (e.g. post-commit). Omit to uninstall all.
        hook: Option<String>,
    },
    /// Git post-commit handler (called by hook wrapper)
    #[command(hide = true)]
    PostCommit {
        /// Extra args passed by git (ignored).
        #[arg(trailing_var_arg = true, hide = true)]
        _args: Vec<String>,
    },
    /// Git post-merge handler (called by hook wrapper)
    #[command(hide = true)]
    PostMerge {
        /// Extra args passed by git (e.g. is-squash-merge flag, ignored).
        #[arg(trailing_var_arg = true, hide = true)]
        _args: Vec<String>,
    },
    /// Git commit-msg handler (called by hook wrapper)
    #[command(hide = true)]
    CommitMsg {
        /// Path to the commit message file (passed by git as $1).
        file: String,
        /// Extra args passed by git (ignored).
        #[arg(trailing_var_arg = true, hide = true)]
        _args: Vec<String>,
    },
    /// Git pre-commit handler (called by hook wrapper)
    #[command(hide = true)]
    PreCommit {
        /// Extra args passed by git (ignored).
        #[arg(trailing_var_arg = true, hide = true)]
        _args: Vec<String>,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum HooksError {
    #[error(transparent)]
    Core(#[from] jig_core::Error),
}

impl Op for Hooks {
    type Error = HooksError;
    type Output = NoOutput;

    fn run(&self, ctx: &RepoCtx) -> Result<Self::Output, Self::Error> {
        match &self.subcommand {
            HooksCommands::Init { force } => {
                let cfg = ctx.config()?;
                let repo_path = &cfg.repo_root;

                // Install git hooks
                eprintln!("Installing git hooks...");
                eprintln!();

                let result = jig_core::hooks::init_hooks(repo_path, *force)?;

                for r in &result.results {
                    match r {
                        jig_core::hooks::install::HookResult::Installed(name) => {
                            eprintln!("{} {}: installed", ui::SYM_OK, name);
                        }
                        jig_core::hooks::install::HookResult::AlreadyInstalled(name) => {
                            eprintln!("{} {}: already installed", ui::SYM_OK, name);
                        }
                        jig_core::hooks::install::HookResult::BackedUpAndInstalled {
                            hook,
                            backup: _,
                        } => {
                            eprintln!(
                                "{} {}: installed (backed up existing hook)",
                                ui::SYM_OK,
                                hook
                            );
                        }
                    }
                }

                let any_backed_up = result.results.iter().any(|r| {
                    matches!(
                        r,
                        jig_core::hooks::install::HookResult::BackedUpAndInstalled { .. }
                    )
                });

                if any_backed_up {
                    eprintln!();
                    eprintln!("Your existing hooks have been moved to .git/hooks/*.user");
                }

                // Install agent-specific hooks based on config
                let jig_toml = JigToml::load(repo_path)?.unwrap_or_default();
                if let Some(agent) = jig_core::agents::Agent::from_name(&jig_toml.agent.agent_type)
                {
                    eprintln!();
                    ui::progress(&format!("Installing {} agent hooks...", agent.name()));
                    match agent.install_hooks() {
                        Ok(result) => {
                            for name in &result.installed {
                                eprintln!("  {} {}: installed", ui::SYM_OK, name);
                            }
                            for name in &result.skipped {
                                eprintln!("  {} {}: up to date", ui::SYM_OK, name);
                            }
                        }
                        Err(e) => {
                            ui::warning(&format!("Agent hooks: {}", e));
                        }
                    }
                }

                eprintln!();
                ui::success("Hooks installed");

                Ok(NoOutput)
            }
            HooksCommands::Uninstall { hook } => {
                let cfg = ctx.config()?;
                let repo_path = &cfg.repo_root;

                eprintln!("Uninstalling jig hooks...");
                eprintln!();

                let result = jig_core::hooks::uninstall_hooks(repo_path, hook.as_deref())?;

                for outcome in &result.outcomes {
                    match outcome {
                        jig_core::hooks::uninstall::UninstallOutcome::Removed(name) => {
                            ui::success(&format!("{}: removed (no previous hook)", name));
                        }
                        jig_core::hooks::uninstall::UninstallOutcome::RestoredBackup {
                            hook,
                            backup: _,
                        } => {
                            ui::success(&format!("{}: removed, restored from backup", hook));
                        }
                        jig_core::hooks::uninstall::UninstallOutcome::RestoredUser(name) => {
                            ui::success(&format!("{}: removed, restored original hook", name));
                        }
                    }
                }

                if result.outcomes.is_empty() {
                    eprintln!("No jig hooks installed.");
                } else {
                    eprintln!();
                    eprintln!("All jig hooks uninstalled.");
                }

                Ok(NoOutput)
            }
            HooksCommands::PostCommit { .. } => {
                let cfg = ctx.config()?;
                jig_core::hooks::handle_post_commit(&cfg.repo_root)?;
                Ok(NoOutput)
            }
            HooksCommands::PostMerge { .. } => {
                let cfg = ctx.config()?;
                jig_core::hooks::handle_post_merge(&cfg.repo_root)?;
                Ok(NoOutput)
            }
            HooksCommands::CommitMsg { file, .. } => {
                let cfg = ctx.config()?;
                jig_core::hooks::handle_commit_msg(&cfg.repo_root, file)?;
                Ok(NoOutput)
            }
            HooksCommands::PreCommit { .. } => {
                let cfg = ctx.config()?;
                jig_core::hooks::handle_pre_commit(&cfg.repo_root)?;
                Ok(NoOutput)
            }
        }
    }
}
