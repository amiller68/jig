//! Integration tests for issue lifecycle commands.

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::process::Command as StdCommand;
use tempfile::TempDir;

struct TestRepo {
    dir: TempDir,
    config_dir: TempDir,
}

impl TestRepo {
    fn new() -> Self {
        let dir = TempDir::new().expect("Failed to create temp dir");
        let config_dir = TempDir::new().expect("Failed to create config dir");

        // Initialize git repo
        StdCommand::new("git")
            .args(["init", "-q", "-b", "main"])
            .current_dir(dir.path())
            .output()
            .expect("Failed to init git repo");

        StdCommand::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(dir.path())
            .output()
            .expect("Failed to set git email");

        StdCommand::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(dir.path())
            .output()
            .expect("Failed to set git name");

        StdCommand::new("git")
            .args(["config", "commit.gpgsign", "false"])
            .current_dir(dir.path())
            .output()
            .expect("Failed to disable gpg signing");

        StdCommand::new("git")
            .args(["commit", "--allow-empty", "-m", "init", "-q"])
            .current_dir(dir.path())
            .output()
            .expect("Failed to create initial commit");

        let path_str = dir.path().to_string_lossy().to_string();
        StdCommand::new("git")
            .args(["remote", "add", "origin", &path_str])
            .current_dir(dir.path())
            .output()
            .expect("Failed to add remote");

        StdCommand::new("git")
            .args(["fetch", "-q", "origin"])
            .current_dir(dir.path())
            .output()
            .expect("Failed to fetch");

        let config_file = config_dir.path().join("jig").join("config");
        fs::create_dir_all(config_file.parent().unwrap()).unwrap();
        fs::write(&config_file, "_default=main\n").expect("Failed to write config");

        // Create issues directory with templates
        let issues_dir = dir.path().join("issues");
        let templates_dir = issues_dir.join("_templates");
        fs::create_dir_all(&templates_dir).unwrap();
        fs::write(
            templates_dir.join("standalone.md"),
            "# [Title]\n\n**Status:** Planned\n\n## Objective\n\nDescribe.\n",
        )
        .unwrap();

        // Create some existing issues
        let features_dir = issues_dir.join("features");
        fs::create_dir_all(&features_dir).unwrap();
        fs::write(
            features_dir.join("existing.md"),
            "# Existing Feature\n\n**Status:** Planned\n**Priority:** High\n",
        )
        .unwrap();

        let bugs_dir = issues_dir.join("bugs");
        fs::create_dir_all(&bugs_dir).unwrap();
        fs::write(
            bugs_dir.join("old-bug.md"),
            "# Old Bug\n\n**Status:** Complete\n**Priority:** Low\n",
        )
        .unwrap();

        TestRepo { dir, config_dir }
    }

    fn jig(&self) -> Command {
        let mut cmd = Command::cargo_bin("jig").expect("Failed to find jig binary");
        cmd.current_dir(self.dir.path());
        cmd.env("XDG_CONFIG_HOME", self.config_dir.path());
        cmd
    }
}

#[test]
fn create_basic() {
    let repo = TestRepo::new();

    repo.jig()
        .args(["issues", "create", "Add verbose flag"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Created issue: features/add-verbose-flag",
        ));

    assert!(repo
        .dir
        .path()
        .join("issues/features/add-verbose-flag.md")
        .exists());
}

#[test]
fn create_with_options() {
    let repo = TestRepo::new();

    repo.jig()
        .args([
            "issues",
            "create",
            "Fix crash on exit",
            "--priority",
            "high",
            "--category",
            "bugs",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Created issue: bugs/fix-crash-on-exit",
        ));

    let content =
        fs::read_to_string(repo.dir.path().join("issues/bugs/fix-crash-on-exit.md")).unwrap();
    assert!(content.contains("**Priority:** High"));
    assert!(content.contains("Fix crash on exit"));
}

#[test]
fn status_update() {
    let repo = TestRepo::new();

    repo.jig()
        .args([
            "issues",
            "status",
            "features/existing",
            "--status",
            "in-progress",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Updated features/existing -> In Progress",
        ));

    let content = fs::read_to_string(repo.dir.path().join("issues/features/existing.md")).unwrap();
    assert!(content.contains("**Status:** In Progress"));
}

#[test]
fn complete_issue() {
    let repo = TestRepo::new();

    repo.jig()
        .args(["issues", "complete", "features/existing"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Completed: features/existing"));

    let content = fs::read_to_string(repo.dir.path().join("issues/features/existing.md")).unwrap();
    assert!(content.contains("**Status:** Complete"));
}

#[test]
fn complete_with_delete() {
    let repo = TestRepo::new();

    let file_path = repo.dir.path().join("issues/features/existing.md");
    assert!(file_path.exists());

    repo.jig()
        .args(["issues", "complete", "features/existing", "--delete"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Completed and deleted: features/existing",
        ));

    assert!(!file_path.exists());
}

#[test]
fn stats() {
    let repo = TestRepo::new();

    repo.jig()
        .args(["issues", "stats"])
        .assert()
        .success()
        .stdout(predicate::str::contains("By Status:"))
        .stdout(predicate::str::contains("By Priority:"))
        .stdout(predicate::str::contains("Planned: 1"))
        .stdout(predicate::str::contains("Complete: 1"));
}

#[test]
fn list_still_works() {
    let repo = TestRepo::new();

    // No subcommand should still list issues
    repo.jig()
        .args(["issues"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Existing Feature"));
}

#[test]
fn detail_still_works() {
    let repo = TestRepo::new();

    repo.jig()
        .args(["issues", "features/existing"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Existing Feature"));
}
