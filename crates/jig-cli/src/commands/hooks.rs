//! Hooks command — manage hook integrations

use clap::{Args, Subcommand};
use colored::Colorize;

use crate::op::{NoOutput, Op, OpContext};

/// Manage hook integrations
#[derive(Args, Debug, Clone)]
pub struct Hooks {
    #[command(subcommand)]
    pub subcommand: HooksCommands,
}

#[derive(Subcommand, Debug, Clone)]
pub enum HooksCommands {
    /// Install Claude Code hooks to ~/.claude/hooks/
    InstallClaude,
    /// Install jig git hooks into the current repo
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
    PostCommit,
    /// Git post-merge handler (called by hook wrapper)
    #[command(hide = true)]
    PostMerge,
    /// Git pre-commit handler (called by hook wrapper)
    #[command(hide = true)]
    PreCommit,
}

#[derive(Debug, thiserror::Error)]
pub enum HooksError {
    #[error(transparent)]
    Core(#[from] jig_core::Error),
}

impl Op for Hooks {
    type Error = HooksError;
    type Output = NoOutput;

    fn execute(&self, ctx: &OpContext) -> Result<Self::Output, Self::Error> {
        match &self.subcommand {
            HooksCommands::InstallClaude => {
                let result = jig_core::hooks::install_claude_hooks()?;

                for name in &result.installed {
                    eprintln!("{} Installed {}", "✓".green(), name);
                }
                for name in &result.skipped {
                    eprintln!("{} Skipped {} (already exists)", "!".yellow(), name);
                }

                if result.installed.is_empty() && !result.skipped.is_empty() {
                    eprintln!("All hooks already installed.");
                }

                Ok(NoOutput)
            }
            HooksCommands::Init { force } => {
                let repo = ctx.repo()?;
                let repo_path = &repo.repo_root;

                eprintln!("Installing git hooks...");
                eprintln!();

                let result = jig_core::hooks::init_hooks(repo_path, *force)?;

                for r in &result.results {
                    match r {
                        jig_core::hooks::install::HookResult::Installed(name) => {
                            eprintln!("{} {}: installed", "✓".green(), name);
                        }
                        jig_core::hooks::install::HookResult::AlreadyInstalled(name) => {
                            eprintln!("{} {}: already installed", "✓".green(), name);
                        }
                        jig_core::hooks::install::HookResult::BackedUpAndInstalled {
                            hook,
                            backup: _,
                        } => {
                            eprintln!(
                                "{} {}: installed (backed up existing hook)",
                                "✓".green(),
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

                eprintln!();
                if any_backed_up {
                    eprintln!("Your existing hooks have been moved to .git/hooks/*.user");
                }

                Ok(NoOutput)
            }
            HooksCommands::Uninstall { hook } => {
                let repo = ctx.repo()?;
                let repo_path = &repo.repo_root;

                eprintln!("Uninstalling jig hooks...");
                eprintln!();

                let result = jig_core::hooks::uninstall_hooks(repo_path, hook.as_deref())?;

                for outcome in &result.outcomes {
                    match outcome {
                        jig_core::hooks::uninstall::UninstallOutcome::Removed(name) => {
                            eprintln!("{} {}: removed (no previous hook)", "✓".green(), name);
                        }
                        jig_core::hooks::uninstall::UninstallOutcome::RestoredBackup {
                            hook,
                            backup: _,
                        } => {
                            eprintln!("{} {}: removed, restored from backup", "✓".green(), hook);
                        }
                        jig_core::hooks::uninstall::UninstallOutcome::RestoredUser(name) => {
                            eprintln!("{} {}: removed, restored original hook", "✓".green(), name);
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
            HooksCommands::PostCommit => {
                let repo = ctx.repo()?;
                jig_core::hooks::handle_post_commit(&repo.repo_root)?;
                Ok(NoOutput)
            }
            HooksCommands::PostMerge => {
                let repo = ctx.repo()?;
                jig_core::hooks::handle_post_merge(&repo.repo_root)?;
                Ok(NoOutput)
            }
            HooksCommands::PreCommit => {
                let repo = ctx.repo()?;
                jig_core::hooks::handle_pre_commit(&repo.repo_root)?;
                Ok(NoOutput)
            }
        }
    }
}
