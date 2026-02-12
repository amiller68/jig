//! Init command - initialize repository for jig

use anyhow::Result;
use colored::Colorize;
use std::fs;
use std::path::Path;

use jig_core::{config, git, Error};

// Embed templates at compile time from the templates/ directory
const JIG_TOML_CONTENT: &str = r#"[spawn]
auto = true
"#;

const CLAUDE_MD_TEMPLATE: &str = include_str!("../../../../templates/CLAUDE.md");

// Docs templates
const DOCS_INDEX: &str = include_str!("../../../../templates/docs/index.md");
const DOCS_PATTERNS: &str = include_str!("../../../../templates/docs/PATTERNS.md");
const DOCS_CONTRIBUTING: &str = include_str!("../../../../templates/docs/CONTRIBUTING.md");
const DOCS_SUCCESS_CRITERIA: &str = include_str!("../../../../templates/docs/SUCCESS_CRITERIA.md");
const DOCS_PROJECT_LAYOUT: &str = include_str!("../../../../templates/docs/PROJECT_LAYOUT.md");

// Issues templates
const ISSUES_README: &str = include_str!("../../../../templates/issues/README.md");
const ISSUES_TEMPLATE: &str = include_str!("../../../../templates/issues/_template.md");

// Skills
const SKILL_CHECK: &str = include_str!("../../../../templates/skills/check/SKILL.md");
const SKILL_DRAFT: &str = include_str!("../../../../templates/skills/draft/SKILL.md");
const SKILL_ISSUES: &str = include_str!("../../../../templates/skills/issues/SKILL.md");
const SKILL_REVIEW: &str = include_str!("../../../../templates/skills/review/SKILL.md");
const SKILL_SPAWN: &str = include_str!("../../../../templates/skills/spawn/SKILL.md");

const AUDIT_PROMPT: &str = r#"
Audit this codebase and populate the skeleton documentation files with project-specific content.

## Files to populate

1. **CLAUDE.md** — Fill in:
   - One-line project description
   - Quick Reference commands (build, test, lint, run)
   - Project structure overview
   - Constraints specific to this project
   - Do Not rules specific to this project

2. **docs/index.md** — Fill in:
   - Quick Start section with actual commands
   - Any project-specific agent guidelines

3. **docs/PATTERNS.md** — Document:
   - Error handling patterns used in the codebase
   - Module/file organization conventions
   - Naming conventions
   - Output conventions (stderr/stdout usage)
   - Testing patterns

4. **docs/SUCCESS_CRITERIA.md** — Fill in:
   - Actual build command
   - Actual test command
   - Actual lint command
   - Actual format check command

5. **docs/PROJECT_LAYOUT.md** — Document:
   - Actual directory structure with descriptions
   - Key files and their purposes
   - Entry points

6. **docs/CONTRIBUTING.md** — Fill in:
   - Setup instructions
   - Commit message conventions used
   - Any project-specific contribution rules

7. **Skills** — Review each skill in .claude/skills/ and update if needed:
   - /check — Update with project-specific check commands
   - /review — Ensure review criteria match project conventions

Remove HTML comment placeholders as you fill in actual content. Commit when done.
"#;

const SETTINGS_JSON: &str = r#"{
  "$schema": "https://claude.ai/schemas/claude-settings.json",
  "permissions": {
    "allow": [
      "Bash(git status)",
      "Bash(git log:*)",
      "Bash(git diff:*)",
      "Bash(git branch:*)",
      "Bash(git add:*)",
      "Bash(git commit:*)",
      "Bash(git push:*)",
      "Bash(git pull:*)",
      "Bash(git checkout:*)",
      "Bash(git switch:*)",
      "Bash(git fetch:*)",
      "Bash(git stash:*)",
      "Bash(gh pr view:*)",
      "Bash(gh pr list:*)",
      "Bash(gh pr create:*)",
      "Bash(gh pr checkout:*)",
      "Bash(gh issue:*)",
      "Bash(gh repo view:*)",
      "Bash(cargo *)",
      "Bash(npm *)",
      "Bash(pnpm *)",
      "Bash(yarn *)",
      "Bash(bun *)",
      "Bash(make *)",
      "Bash(go *)",
      "Bash(python *)",
      "Bash(python3 *)",
      "Bash(pytest *)",
      "Bash(uv *)",
      "Bash(pip *)",
      "Bash(bundle *)",
      "Bash(rake *)",
      "Bash(jig *)",
      "Bash(tmux *)",
      "Bash(ls *)",
      "Bash(pwd)",
      "Bash(which *)",
      "Bash(cat *)",
      "Bash(head *)",
      "Bash(tail *)",
      "Bash(wc *)",
      "Bash(find *)",
      "Bash(grep *)",
      "Bash(./test.sh *)"
    ],
    "ask": [
      "Bash(git merge:*)",
      "Bash(git rebase:*)",
      "Bash(git reset:*)",
      "Bash(gh pr merge:*)"
    ],
    "deny": [
      "Bash(rm -rf /)",
      "Bash(rm -rf ~)",
      "Bash(rm -rf .)",
      "Bash(sudo *)",
      "Bash(git push --force *)",
      "Bash(git push -f *)",
      "Bash(git reset --hard *)",
      "Bash(cat .env*)",
      "Bash(cat */.env*)",
      "Bash(cat *.pem)",
      "Bash(cat *.key)",
      "Bash(cat *credentials*)"
    ]
  }
}
"#;

pub fn run(force: bool, backup: bool, audit: bool) -> Result<()> {
    let repo = git::get_base_repo()?;

    // Check if already initialized
    if config::has_jig_toml()? && !force {
        return Err(Error::AlreadyInitialized.into());
    }

    eprintln!("{} Initializing jig in {}", "→".cyan(), repo.display());

    // Create backup directory if backup is enabled
    let backup_dir = repo.join(".backup");
    if backup {
        fs::create_dir_all(&backup_dir)?;
        eprintln!("  {} Created .backup/", "✓".green());
    }

    // Create directories
    let dirs = [
        "docs",
        "issues",
        ".claude/skills/check",
        ".claude/skills/draft",
        ".claude/skills/issues",
        ".claude/skills/review",
        ".claude/skills/spawn",
    ];
    for dir in dirs {
        let path = repo.join(dir);
        if !path.exists() {
            fs::create_dir_all(&path)?;
            eprintln!("  {} Created {}/", "✓".green(), dir);
        }
    }

    let backup_dir_opt = if backup {
        Some(backup_dir.as_path())
    } else {
        None
    };

    // Write jig.toml
    write_file(&repo, "jig.toml", JIG_TOML_CONTENT, force, backup_dir_opt)?;

    // Write CLAUDE.md
    write_file(&repo, "CLAUDE.md", CLAUDE_MD_TEMPLATE, force, backup_dir_opt)?;

    // Write docs files
    write_file(&repo, "docs/index.md", DOCS_INDEX, force, backup_dir_opt)?;
    write_file(&repo, "docs/PATTERNS.md", DOCS_PATTERNS, force, backup_dir_opt)?;
    write_file(&repo, "docs/CONTRIBUTING.md", DOCS_CONTRIBUTING, force, backup_dir_opt)?;
    write_file(&repo, "docs/SUCCESS_CRITERIA.md", DOCS_SUCCESS_CRITERIA, force, backup_dir_opt)?;
    write_file(&repo, "docs/PROJECT_LAYOUT.md", DOCS_PROJECT_LAYOUT, force, backup_dir_opt)?;

    // Write issues files
    write_file(&repo, "issues/README.md", ISSUES_README, force, backup_dir_opt)?;
    write_file(&repo, "issues/_template.md", ISSUES_TEMPLATE, force, backup_dir_opt)?;

    // Write .claude/settings.json
    write_file(&repo, ".claude/settings.json", SETTINGS_JSON, force, backup_dir_opt)?;

    // Write skills
    write_file(&repo, ".claude/skills/check/SKILL.md", SKILL_CHECK, force, backup_dir_opt)?;
    write_file(&repo, ".claude/skills/draft/SKILL.md", SKILL_DRAFT, force, backup_dir_opt)?;
    write_file(&repo, ".claude/skills/issues/SKILL.md", SKILL_ISSUES, force, backup_dir_opt)?;
    write_file(&repo, ".claude/skills/review/SKILL.md", SKILL_REVIEW, force, backup_dir_opt)?;
    write_file(&repo, ".claude/skills/spawn/SKILL.md", SKILL_SPAWN, force, backup_dir_opt)?;

    eprintln!();
    eprintln!("{} Initialization complete", "✓".green().bold());

    if audit {
        eprintln!();
        eprintln!(
            "{} Run this to audit and populate documentation:",
            "→".cyan()
        );
        eprintln!();
        eprintln!("  claude \"{}\"", AUDIT_PROMPT.trim());
    }

    Ok(())
}

fn write_file(
    repo: &Path,
    relative_path: &str,
    content: &str,
    force: bool,
    backup_dir: Option<&Path>,
) -> Result<()> {
    let path = repo.join(relative_path);

    if path.exists() {
        if let Some(backup_dir) = backup_dir {
            let backup_path = backup_dir.join(relative_path);
            if let Some(parent) = backup_path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(&path, &backup_path)?;
            eprintln!("  {} Backed up {}", "→".dimmed(), relative_path);
        }

        if !force {
            eprintln!("  {} Skipped {} (exists)", "→".dimmed(), relative_path);
            return Ok(());
        }
    }

    fs::write(&path, content)?;
    eprintln!("  {} Created {}", "✓".green(), relative_path);
    Ok(())
}
