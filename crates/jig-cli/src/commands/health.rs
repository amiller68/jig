//! Health command - validate system dependencies and repo setup

use anyhow::{bail, Result};
use colored::Colorize;

use jig_core::config::{Config, JigToml};
use jig_core::git;
use jig_core::terminal;

const EXPECTED_SKILLS: &[&str] = &["jig", "check", "draft", "issues", "review"];

pub fn run() -> Result<()> {
    let version = env!("CARGO_PKG_VERSION");
    let mut all_passed = true;

    // Header
    eprintln!("jig v{}", version);

    // Section 1: System
    eprintln!();
    eprintln!("{}", "System".bold());
    let deps = terminal::check_dependencies();
    for dep in &deps {
        if dep.available {
            eprintln!("  {} {}", "✓".green(), dep.name);
        } else {
            eprintln!("  {} {}", "✗".red(), dep.name);
            all_passed = false;
        }
    }

    // Section 2: Repository
    eprintln!();
    let repo_root = match git::get_base_repo() {
        Ok(root) => {
            let repo_name = root
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "unknown".to_string());
            eprintln!("{}", format!("Repository: {}", repo_name).bold());
            Some(root)
        }
        Err(_) => {
            eprintln!("{}", "Repository:".bold());
            eprintln!("  {} Not in a git repository", "✗".red());
            all_passed = false;
            None
        }
    };

    if let Some(ref root) = repo_root {
        // jig.toml
        if JigToml::exists(root) {
            eprintln!("  {} jig.toml", "✓".green());
        } else {
            eprintln!("  {} jig.toml {}", "✗".red(), "(not found)".dimmed());
            all_passed = false;
        }

        // Base branch
        let config = Config::load()?;
        let branch = config.get_base_branch(root);
        eprintln!("  {} Base branch: {}", "✓".green(), branch);

        // .worktrees directory
        let worktrees_dir = root.join(".worktrees");
        if worktrees_dir.is_dir() {
            eprintln!("  {} .worktrees directory", "✓".green());
        } else {
            eprintln!(
                "  {} .worktrees directory {}",
                "✗".red(),
                "(not found)".dimmed()
            );
            all_passed = false;
        }
    }

    // Section 3: Agent scaffolding
    eprintln!();
    eprintln!("{}", "Agent: claude-code".bold());

    if let Some(ref root) = repo_root {
        // CLAUDE.md
        if root.join("CLAUDE.md").is_file() {
            eprintln!("  {} CLAUDE.md", "✓".green());
        } else {
            eprintln!("  {} CLAUDE.md {}", "✗".red(), "(not found)".dimmed());
            all_passed = false;
        }

        // .claude/settings.json
        if root.join(".claude").join("settings.json").is_file() {
            eprintln!("  {} .claude/settings.json", "✓".green());
        } else {
            eprintln!(
                "  {} .claude/settings.json {}",
                "✗".red(),
                "(not found)".dimmed()
            );
            all_passed = false;
        }

        // Skills
        eprintln!("  Skills (.claude/skills/):");
        let skills_dir = root.join(".claude").join("skills");
        for skill in EXPECTED_SKILLS {
            let skill_path = skills_dir.join(skill).join("SKILL.md");
            if skill_path.is_file() {
                eprintln!("    {} {}", "✓".green(), skill);
            } else {
                eprintln!("    {} {}", "✗".red(), skill);
                all_passed = false;
            }
        }
    } else {
        eprintln!("  {} Skipped (no repository)", "✗".red());
        all_passed = false;
    }

    // Footer
    eprintln!();
    if all_passed {
        eprintln!("All checks passed.");
        Ok(())
    } else {
        eprintln!("Run '{}' to set up this repository.", "jig init".bold());
        bail!("health check failed")
    }
}
