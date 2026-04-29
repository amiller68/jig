//! Commit command — conventional commit validation and examples.

use std::io::{self, Read as _};

use clap::{Args, Subcommand};

use crate::config::JigToml;
use jig_core::git::conventional::ValidationConfig;

use crate::cli::op::{NoOutput, Op, RepoCtx};
use crate::cli::ui;

/// Validate and work with conventional commits
#[derive(Args, Debug, Clone)]
pub struct Commit {
    #[command(subcommand)]
    pub command: CommitCommand,
}

#[derive(Subcommand, Debug, Clone)]
pub enum CommitCommand {
    /// Validate commit messages against conventional commits spec
    Validate {
        /// Commit ref to validate (default: HEAD)
        #[arg(default_value = "HEAD")]
        rev: String,

        /// Read commit message from stdin
        #[arg(long)]
        stdin: bool,

        /// Read commit message from a file
        #[arg(long, value_name = "PATH")]
        file: Option<String>,
    },

    /// Show conventional commit examples
    Examples,
}

#[derive(Debug, thiserror::Error)]
pub enum CommitError {
    #[error(transparent)]
    Core(#[from] jig_core::Error),
    #[error("{0}")]
    Usage(String),
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
    #[error("git error: {0}")]
    Git2(#[from] git2::Error),
    #[error("{0}")]
    Validation(String),
}

impl Op for Commit {
    type Error = CommitError;
    type Output = NoOutput;

    fn run(&self, ctx: &RepoCtx) -> Result<Self::Output, Self::Error> {
        match &self.command {
            CommitCommand::Validate { rev, stdin, file } => {
                run_validate(ctx, rev, *stdin, file.as_deref())
            }
            CommitCommand::Examples => {
                print_examples();
                Ok(NoOutput)
            }
        }
    }
}

fn run_validate(
    ctx: &RepoCtx,
    rev: &str,
    stdin: bool,
    file: Option<&str>,
) -> Result<NoOutput, CommitError> {
    let message = if stdin {
        let mut buf = String::new();
        io::stdin().read_to_string(&mut buf)?;
        buf
    } else if let Some(path) = file {
        std::fs::read_to_string(path)?
    } else {
        let cfg = ctx.config()?;
        let git_repo = git2::Repository::open(&cfg.repo_root)?;
        let obj = git_repo.revparse_single(rev)?;
        let commit = obj
            .peel_to_commit()
            .map_err(|_| CommitError::Usage(format!("'{}' is not a commit", rev)))?;
        commit
            .message()
            .ok_or_else(|| CommitError::Usage("commit message is not valid UTF-8".into()))?
            .to_string()
    };

    let config = if let Some(cfg) = ctx.config.as_ref() {
        let jig_toml = JigToml::load(&cfg.repo_root)?.unwrap_or_default();
        jig_toml.commits.to_validation_config()
    } else {
        ValidationConfig::default()
    };

    match config.parse_and_validate(&message) {
        Ok((msg, errors)) => {
            if errors.is_empty() {
                ui::success(&format!(
                    "valid conventional commit: {}{}{}",
                    msg.commit_type,
                    msg.scope
                        .as_ref()
                        .map(|s| format!("({})", s))
                        .unwrap_or_default(),
                    if msg.breaking { "!" } else { "" },
                ));
                Ok(NoOutput)
            } else {
                let summary: Vec<String> = errors.iter().map(|e| format!("{}", e)).collect();
                for s in &summary {
                    ui::failure(s);
                }
                Err(CommitError::Validation(format!(
                    "{} validation error(s)",
                    summary.len()
                )))
            }
        }
        Err(e) => {
            ui::failure(&format!("{}", e));
            Err(CommitError::Validation("invalid commit message".into()))
        }
    }
}

fn print_examples() {
    eprintln!(
        "\
{header}

{section_basic}
  feat: add user authentication
  fix: resolve login timeout
  docs: update README with examples
  refactor: simplify error handling

{section_scope}
  feat(auth): add OAuth2 support
  fix(ui): correct button alignment
  test(api): add integration tests

{section_breaking}
  feat!: remove legacy API
  feat(api)!: change response format

  Or with footer:
  feat(api): change response format

  BREAKING CHANGE: responses now use camelCase

{section_types}
  feat      new feature (minor version bump)
  fix       bug fix (patch version bump)
  docs      documentation only
  style     formatting, missing semicolons, etc.
  refactor  code change (no bug fix or feature)
  perf      performance improvement
  test      adding tests
  chore     maintenance tasks
  ci        CI configuration changes

{section_tips}
  Keep subject line under 72 characters
  Use present tense (\"add\" not \"added\")
  Start with lowercase (after type)
  No period at end of subject",
        header = ui::bold("Conventional Commit Examples"),
        section_basic = ui::bold("Basic:"),
        section_scope = ui::bold("With scope:"),
        section_breaking = ui::bold("Breaking changes:"),
        section_types = ui::bold("Types:"),
        section_tips = ui::bold("Tips:"),
    );
}
