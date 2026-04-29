//! Health command - validate system dependencies and repo setup

use clap::Args;

use crate::config::JigToml;
use jig_core::mux;

use crate::cli::op::{NoOutput, Op, RepoCtx};
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

    fn run(&self, ctx: &RepoCtx) -> Result<Self::Output, Self::Error> {
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
        let deps = mux::check_dependencies();
        for dep in &deps {
            if dep.available {
                check_ok(&dep.name);
            } else {
                check_fail(&dep.name, None);
                all_passed = false;
            }
        }

        // Section 2: Repository — use Option to handle non-repo gracefully
        eprintln!();
        let cfg = ctx.config.as_ref();

        match cfg {
            Some(cfg) => {
                let repo_name = cfg
                    .repo_root
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "unknown".to_string());
                ui::header(&format!("Repository: {}", repo_name));

                // jig.toml
                if JigToml::exists(&cfg.repo_root) {
                    check_ok("jig.toml");
                } else {
                    check_fail("jig.toml", Some("(not found)"));
                    all_passed = false;
                }

                // Base branch
                let branch = cfg.base_branch();
                eprintln!("  {} Base branch: {}", ui::SYM_OK, branch);

                // Jig worktrees directory
                if cfg.worktrees_path.is_dir() {
                    check_ok(&format!("{} directory", crate::config::JIG_DIR));
                } else {
                    check_fail(
                        &format!("{} directory", crate::config::JIG_DIR),
                        Some("(not found)"),
                    );
                    all_passed = false;
                }

                // Section 3: Agent scaffolding
                eprintln!();
                ui::header("Agent: claude-code");

                // CLAUDE.md
                if cfg.repo_root.join("CLAUDE.md").is_file() {
                    check_ok("CLAUDE.md");
                } else {
                    check_fail("CLAUDE.md", Some("(not found)"));
                    all_passed = false;
                }

                // .claude/settings.json
                if cfg
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
                let skills_dir = cfg.repo_root.join(".claude").join("skills");
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
