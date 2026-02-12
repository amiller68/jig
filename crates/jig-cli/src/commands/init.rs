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
const DOCS_INDEX_TEMPLATE: &str = include_str!("../../../../templates/docs/index.md");
const ISSUE_TRACKING_TEMPLATE: &str = include_str!("../../../../templates/docs/issue-tracking.md");

// Skills
const SKILL_CHECK: &str = include_str!("../../../../templates/skills/check/SKILL.md");
const SKILL_DRAFT: &str = include_str!("../../../../templates/skills/draft/SKILL.md");
const SKILL_ISSUES: &str = include_str!("../../../../templates/skills/issues/SKILL.md");
const SKILL_REVIEW: &str = include_str!("../../../../templates/skills/review/SKILL.md");
const SKILL_SPAWN: &str = include_str!("../../../../templates/skills/jig/SKILL.md");

const SETTINGS_JSON: &str = r#"{
  "permissions": {
    "allow": [
      "Bash(git *)",
      "Bash(gh *)",
      "Bash(cargo *)",
      "Bash(npm *)",
      "Bash(pnpm *)",
      "Bash(yarn *)",
      "Bash(make *)",
      "Bash(go *)",
      "Bash(python *)",
      "Bash(pytest *)",
      "Bash(uv *)",
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
    "deny": [
      "Bash(rm -rf /)",
      "Bash(rm -rf ~)",
      "Bash(sudo *)",
      "Bash(git push --force *)"
    ]
  }
}
"#;

const AUDIT_PROMPT: &str = r#"Audit this codebase and populate the project documentation.

## Tasks

1. **Explore the codebase** - Identify languages, frameworks, build system, test runner, package manager, and project structure.

2. **Update CLAUDE.md** - Replace placeholder sections with project-specific content:
   - Project overview (what it does, key concepts)
   - Key files and their purposes
   - Development commands (build, test, lint, format)
   - Testing instructions

3. **Update docs/index.md** - Write agent instructions specific to this project:
   - Code style conventions
   - Testing requirements
   - Documentation standards
   - Project-specific context

4. **Commit the changes** - Create a single commit with message "docs: populate project documentation via audit"

Be thorough but concise. Focus on information that helps developers and AI agents be productive in this codebase."#;

pub fn run(force: bool, backup: bool, audit: bool) -> Result<()> {
    let repo = git::get_base_repo()?;

    // Check if already initialized
    if config::has_jig_toml()? && !force {
        return Err(Error::AlreadyInitialized.into());
    }

    eprintln!("{} Initializing jig in {}", "→".cyan(), repo.display());

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

    // Write jig.toml
    write_file(&repo.join("jig.toml"), JIG_TOML_CONTENT, force, backup)?;

    // Write CLAUDE.md
    write_file(&repo.join("CLAUDE.md"), CLAUDE_MD_TEMPLATE, force, backup)?;

    // Write docs files
    write_file(
        &repo.join("docs/index.md"),
        DOCS_INDEX_TEMPLATE,
        force,
        backup,
    )?;
    write_file(
        &repo.join("docs/issue-tracking.md"),
        ISSUE_TRACKING_TEMPLATE,
        force,
        backup,
    )?;

    // Write .claude/settings.json
    write_file(
        &repo.join(".claude/settings.json"),
        SETTINGS_JSON,
        force,
        backup,
    )?;

    // Write skills
    write_file(
        &repo.join(".claude/skills/check/SKILL.md"),
        SKILL_CHECK,
        force,
        backup,
    )?;
    write_file(
        &repo.join(".claude/skills/draft/SKILL.md"),
        SKILL_DRAFT,
        force,
        backup,
    )?;
    write_file(
        &repo.join(".claude/skills/issues/SKILL.md"),
        SKILL_ISSUES,
        force,
        backup,
    )?;
    write_file(
        &repo.join(".claude/skills/review/SKILL.md"),
        SKILL_REVIEW,
        force,
        backup,
    )?;
    write_file(
        &repo.join(".claude/skills/spawn/SKILL.md"),
        SKILL_SPAWN,
        force,
        backup,
    )?;

    eprintln!();
    eprintln!("{} Initialization complete", "✓".green().bold());

    if audit {
        use jig_core::terminal::command_exists;

        eprintln!();
        eprintln!("{} Launching Claude to audit documentation...", "→".cyan());

        // Check claude is available
        if !command_exists("claude") {
            return Err(Error::MissingDependency("claude".to_string()).into());
        }

        // Use exec to replace current process with Claude for full terminal control
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            use std::process::Command;

            let err = Command::new("claude")
                .arg(AUDIT_PROMPT)
                .current_dir(&repo)
                .exec();

            // exec only returns if there was an error
            return Err(anyhow::anyhow!("Failed to exec claude: {}", err));
        }

        #[cfg(not(unix))]
        {
            use std::process::Command;

            let status = Command::new("claude")
                .arg(AUDIT_PROMPT)
                .current_dir(&repo)
                .status()?;

            if !status.success() {
                eprintln!("{} Claude exited with non-zero status", "!".yellow());
            }
        }
    }

    Ok(())
}

fn write_file(path: &Path, content: &str, force: bool, backup: bool) -> Result<()> {
    let name = path.file_name().unwrap().to_string_lossy();

    if path.exists() {
        if backup {
            let backup_path = path.with_extension("bak");
            fs::copy(path, &backup_path)?;
            eprintln!("  {} Backed up {}", "→".dimmed(), name);
        }

        if !force {
            eprintln!("  {} Skipped {} (exists)", "→".dimmed(), name);
            return Ok(());
        }
    }

    fs::write(path, content)?;
    eprintln!("  {} Created {}", "✓".green(), name);
    Ok(())
}
