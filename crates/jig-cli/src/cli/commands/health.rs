//! Health command - validate system dependencies and repo setup

use clap::Args;

use crate::context::JigToml;
use crate::terminal::Terminal;

use crate::cli::op::{NoOutput, Op};
use crate::context::Context;
use crate::cli::ui;

const EXPECTED_SKILLS: &[&str] = &["jig", "check", "draft", "issues", "review"];

/// Show terminal and dependency status
#[derive(Args, Debug, Clone)]
pub struct Health;

#[derive(Debug, thiserror::Error)]
pub enum HealthError {
    #[error("Health check failed")]
    CheckFailed,
    #[error(transparent)]
    Core(#[from] jig_core::Error),
}

impl Op for Health {
    type Error = HealthError;
    type Output = NoOutput;

    fn run(&self) -> Result<Self::Output, Self::Error> {
        let version = env!("CARGO_PKG_VERSION");
        let mut all_passed = true;

        let check_ok = |name: &str| {
            eprintln!("  {} {}", ui::SYM_OK, name);
        };
        let check_fail = |name: &str, note: Option<&str>| {
            if let Some(n) = note {
                eprintln!("  {} {} {}", ui::SYM_FAIL, name, ui::dim(n));
            } else {
                eprintln!("  {} {}", ui::SYM_FAIL, name);
            }
        };

        // Header
        eprintln!("jig v{}", version);

        // Section 1: System
        eprintln!();
        ui::header("System");
        for name in ["git", "tmux", "claude"] {
            if Terminal::which(name).is_some() {
                check_ok(name);
            } else {
                check_fail(name, None);
                all_passed = false;
            }
        }

        // Section 2: Repository — use Option to handle non-repo gracefully
        eprintln!();
        let cfg = Context::from_cwd().ok();
        let global = cfg.as_ref().map(|c| &c.config);

        match cfg.as_ref().and_then(|c| c.repo().ok()) {
            Some(repo) => {
                let repo_name = repo
                    .repo_root
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "unknown".to_string());
                ui::header(&format!("Repository: {}", repo_name));

                // jig.toml
                if JigToml::exists(&repo.repo_root) {
                    check_ok("jig.toml");
                } else {
                    check_fail("jig.toml", Some("(not found)"));
                    all_passed = false;
                }

                // Base branch
                let global_ref = global.cloned().unwrap_or_default();
                let branch = repo.base_branch(&global_ref);
                eprintln!("  {} Base branch: {}", ui::SYM_OK, branch);

                // Jig worktrees directory
                if repo.worktrees_path.is_dir() {
                    check_ok(&format!("{} directory", crate::context::JIG_DIR));
                } else {
                    check_fail(
                        &format!("{} directory", crate::context::JIG_DIR),
                        Some("(not found)"),
                    );
                    all_passed = false;
                }

                // Section 3: Agent scaffolding
                eprintln!();
                ui::header("Agent: claude-code");

                // CLAUDE.md
                if repo.repo_root.join("CLAUDE.md").is_file() {
                    check_ok("CLAUDE.md");
                } else {
                    check_fail("CLAUDE.md", Some("(not found)"));
                    all_passed = false;
                }

                // .claude/settings.json
                if repo
                    .repo_root
                    .join(".claude")
                    .join("settings.json")
                    .is_file()
                {
                    check_ok(".claude/settings.json");
                } else {
                    check_fail(".claude/settings.json", Some("(not found)"));
                    all_passed = false;
                }

                // Skills
                eprintln!("  Skills (.claude/skills/):");
                let skills_dir = repo.repo_root.join(".claude").join("skills");
                for skill in EXPECTED_SKILLS {
                    let skill_path = skills_dir.join(skill).join("SKILL.md");
                    if skill_path.is_file() {
                        eprintln!("    {} {}", ui::SYM_OK, skill);
                    } else {
                        eprintln!("    {} {}", ui::SYM_FAIL, skill);
                        all_passed = false;
                    }
                }
            }
            None => {
                ui::header("Repository:");
                eprintln!("  {} Not in a git repository", ui::SYM_FAIL);

                eprintln!();
                ui::header("Agent: claude-code");
                eprintln!("  {} Skipped (no repository)", ui::SYM_FAIL);
                all_passed = false;
            }
        }

        // Footer
        eprintln!();
        if all_passed {
            eprintln!("All checks passed.");
            Ok(NoOutput)
        } else {
            eprintln!(
                "Run '{}' to set up this repository.",
                ui::highlight("jig init")
            );
            Err(HealthError::CheckFailed)
        }
    }
}
