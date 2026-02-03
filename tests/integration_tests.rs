use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::path::PathBuf;
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

        // Set git user config for commits
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

        // Disable GPG signing for tests
        StdCommand::new("git")
            .args(["config", "commit.gpgsign", "false"])
            .current_dir(dir.path())
            .output()
            .expect("Failed to disable gpg signing");

        // Create initial commit
        StdCommand::new("git")
            .args(["commit", "--allow-empty", "-m", "init", "-q"])
            .current_dir(dir.path())
            .output()
            .expect("Failed to create initial commit");

        // Add remote (pointing to self for testing)
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

        // Set global config to use "main" as base branch (since origin/main doesn't exist in test repos)
        let config_file = config_dir.path().join("wt").join("config");
        fs::create_dir_all(config_file.parent().unwrap()).unwrap();
        fs::write(&config_file, "_default=main\n").expect("Failed to write config");

        TestRepo { dir, config_dir }
    }

    fn path(&self) -> PathBuf {
        self.dir.path().to_path_buf()
    }

    fn worktrees_path(&self) -> PathBuf {
        self.dir.path().join(".worktrees")
    }

    fn wt(&self) -> Command {
        let mut cmd = Command::cargo_bin("wt").expect("Failed to find wt binary");
        cmd.current_dir(self.path());
        cmd.env("XDG_CONFIG_HOME", self.config_dir.path());
        cmd
    }
}

// ============================================================================
// Basic Worktree Operations (test_basic.sh)
// ============================================================================

#[test]
fn test_create_worktree() {
    let repo = TestRepo::new();

    repo.wt()
        .args(["create", "test1"])
        .assert()
        .success()
        .stderr(predicate::str::contains("Created worktree"));

    assert!(repo.worktrees_path().join("test1").exists());
    assert!(repo.worktrees_path().join("test1").join(".git").is_file());
}

#[test]
fn test_list_worktrees() {
    let repo = TestRepo::new();

    // Create a worktree first
    repo.wt().args(["create", "test1"]).assert().success();

    // List should show it
    repo.wt()
        .args(["list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("test1"));
}

#[test]
fn test_create_with_o_outputs_cd() {
    let repo = TestRepo::new();

    repo.wt()
        .args(["create", "test2", "-o"])
        .assert()
        .success()
        .stdout(predicate::str::contains("cd "));
}

#[test]
fn test_open_outputs_cd() {
    let repo = TestRepo::new();

    repo.wt().args(["create", "test1"]).assert().success();

    repo.wt()
        .args(["open", "test1"])
        .assert()
        .success()
        .stdout(predicate::str::contains("cd "))
        .stdout(predicate::str::contains("test1"));
}

#[test]
fn test_remove_worktree() {
    let repo = TestRepo::new();

    repo.wt().args(["create", "test1"]).assert().success();
    assert!(repo.worktrees_path().join("test1").exists());

    repo.wt()
        .args(["remove", "test1"])
        .assert()
        .success()
        .stderr(predicate::str::contains("Removed worktree"));

    assert!(!repo.worktrees_path().join("test1").exists());
}

#[test]
fn test_list_after_remove() {
    let repo = TestRepo::new();

    repo.wt().args(["create", "test1"]).assert().success();
    repo.wt().args(["remove", "test1"]).assert().success();

    repo.wt()
        .args(["list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("test1").not());
}

#[test]
fn test_create_error_no_color_in_stdout() {
    let repo = TestRepo::new();

    repo.wt().args(["create", "errortest"]).assert().success();

    // Try to create same one with -o
    let output = repo
        .wt()
        .args(["create", "errortest", "-o"])
        .output()
        .expect("Failed to run command");

    // stdout should not contain ANSI escape codes
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.contains('\x1b'),
        "stdout contains color codes: {}",
        stdout
    );
}

// ============================================================================
// Nested Path Tests (test_nested.sh)
// ============================================================================

#[test]
fn test_nested_path_create() {
    let repo = TestRepo::new();

    repo.wt()
        .args(["create", "feature/test/nested"])
        .assert()
        .success();

    assert!(repo
        .worktrees_path()
        .join("feature/test/nested")
        .exists());
}

#[test]
fn test_list_shows_nested() {
    let repo = TestRepo::new();

    repo.wt()
        .args(["create", "feature/test/nested"])
        .assert()
        .success();

    repo.wt()
        .args(["list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("feature/test/nested"));
}

#[test]
fn test_glob_remove() {
    let repo = TestRepo::new();

    repo.wt().args(["create", "regex-test1"]).assert().success();
    repo.wt().args(["create", "regex-test2"]).assert().success();

    repo.wt()
        .args(["remove", "regex-test*"])
        .assert()
        .success();

    repo.wt()
        .args(["list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("regex-test").not());
}

// ============================================================================
// Config Tests (test_config.sh)
// ============================================================================

#[test]
fn test_config_default() {
    let repo = TestRepo::new();

    // Test setup sets _default=main, so we expect "main" (not "origin/main")
    repo.wt()
        .args(["config"])
        .assert()
        .success()
        .stderr(predicate::str::contains("main"));
}

#[test]
fn test_config_set_repo_base() {
    let repo = TestRepo::new();

    repo.wt()
        .args(["config", "base", "origin/develop"])
        .assert()
        .success();

    repo.wt()
        .args(["config", "base"])
        .assert()
        .success()
        .stdout(predicate::str::contains("origin/develop"));
}

#[test]
fn test_config_unset_repo_base() {
    let repo = TestRepo::new();

    repo.wt()
        .args(["config", "base", "origin/develop"])
        .assert()
        .success();

    repo.wt()
        .args(["config", "base", "--unset"])
        .assert()
        .success();

    repo.wt()
        .args(["config", "base"])
        .assert()
        .success()
        .stderr(predicate::str::contains("No config set"));
}

#[test]
fn test_config_set_global() {
    let repo = TestRepo::new();

    repo.wt()
        .args(["config", "base", "--global", "origin/master"])
        .assert()
        .success();

    repo.wt()
        .args(["config", "base", "--global"])
        .assert()
        .success()
        .stdout(predicate::str::contains("origin/master"));
}

#[test]
fn test_config_global_fallback() {
    let repo = TestRepo::new();

    repo.wt()
        .args(["config", "base", "--global", "origin/master"])
        .assert()
        .success();

    repo.wt()
        .args(["config"])
        .assert()
        .success()
        .stderr(predicate::str::contains("origin/master"));
}

#[test]
fn test_config_repo_over_global() {
    let repo = TestRepo::new();

    repo.wt()
        .args(["config", "base", "--global", "origin/master"])
        .assert()
        .success();

    repo.wt()
        .args(["config", "base", "origin/feature"])
        .assert()
        .success();

    repo.wt()
        .args(["config"])
        .assert()
        .success()
        .stderr(predicate::str::contains("origin/feature"));
}

#[test]
fn test_config_list() {
    let repo = TestRepo::new();

    repo.wt()
        .args(["config", "base", "--global", "origin/master"])
        .assert()
        .success();

    repo.wt()
        .args(["config", "base", "origin/feature"])
        .assert()
        .success();

    repo.wt()
        .args(["config", "--list"])
        .assert()
        .success()
        .stderr(predicate::str::contains("[global]"));
}

// ============================================================================
// Hook Tests (test_hooks.sh)
// ============================================================================

#[test]
fn test_on_create_hook_set_get() {
    let repo = TestRepo::new();

    repo.wt()
        .args(["config", "on-create", "echo hello"])
        .assert()
        .success();

    repo.wt()
        .args(["config", "on-create"])
        .assert()
        .success()
        .stdout(predicate::str::contains("echo hello"));
}

#[test]
fn test_on_create_hook_shows_in_config() {
    let repo = TestRepo::new();

    repo.wt()
        .args(["config", "on-create", "echo hello"])
        .assert()
        .success();

    repo.wt()
        .args(["config"])
        .assert()
        .success()
        .stderr(predicate::str::contains("On-create hook"))
        .stderr(predicate::str::contains("echo hello"));
}

#[test]
fn test_on_create_hook_runs() {
    let repo = TestRepo::new();

    repo.wt()
        .args(["config", "on-create", "touch hook_ran.txt"])
        .assert()
        .success();

    repo.wt()
        .args(["create", "hook-test"])
        .assert()
        .success();

    assert!(repo
        .worktrees_path()
        .join("hook-test")
        .join("hook_ran.txt")
        .exists());
}

#[test]
fn test_no_hooks_skips_hook() {
    let repo = TestRepo::new();

    repo.wt()
        .args(["config", "on-create", "touch hook_ran.txt"])
        .assert()
        .success();

    repo.wt()
        .args(["create", "no-hook-test", "--no-hooks"])
        .assert()
        .success();

    assert!(!repo
        .worktrees_path()
        .join("no-hook-test")
        .join("hook_ran.txt")
        .exists());
}

#[test]
fn test_on_create_hook_unset() {
    let repo = TestRepo::new();

    repo.wt()
        .args(["config", "on-create", "echo hello"])
        .assert()
        .success();

    repo.wt()
        .args(["config", "on-create", "--unset"])
        .assert()
        .success();

    repo.wt()
        .args(["config", "on-create"])
        .assert()
        .success()
        .stderr(predicate::str::contains("No on-create hook"));
}

#[test]
fn test_hook_failure_still_creates_worktree() {
    let repo = TestRepo::new();

    repo.wt()
        .args(["config", "on-create", "exit 1"])
        .assert()
        .success();

    // The command may fail, but worktree should still exist
    let _ = repo.wt().args(["create", "fail-hook-test"]).output();

    assert!(repo.worktrees_path().join("fail-hook-test").exists());
}

// ============================================================================
// Exit Tests (test_exit.sh)
// ============================================================================

#[test]
fn test_exit_from_base_errors() {
    let repo = TestRepo::new();

    repo.wt()
        .args(["exit"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Not in a worktree"));
}

#[test]
fn test_exit_from_worktree_removes_only_that_worktree() {
    let repo = TestRepo::new();

    repo.wt().args(["create", "exit-test1"]).assert().success();
    repo.wt().args(["create", "exit-test2"]).assert().success();

    // Run exit from within the worktree
    let wt_path = repo.worktrees_path().join("exit-test1");
    let mut cmd = Command::cargo_bin("wt").expect("Failed to find wt binary");
    cmd.current_dir(&wt_path);
    cmd.env("XDG_CONFIG_HOME", repo.config_dir.path());
    cmd.args(["exit"])
        .assert()
        .success()
        .stdout(predicate::str::contains("cd "))
        .stderr(predicate::str::contains("Exited worktree"));

    // exit-test1 should be gone
    assert!(!repo.worktrees_path().join("exit-test1").exists());
    // exit-test2 should still exist
    assert!(repo.worktrees_path().join("exit-test2").exists());
}

#[test]
fn test_exit_preserves_dirty_worktree() {
    let repo = TestRepo::new();

    repo.wt()
        .args(["create", "exit-dirty-test"])
        .assert()
        .success();

    // Create uncommitted changes
    let wt_path = repo.worktrees_path().join("exit-dirty-test");
    fs::write(wt_path.join("testfile.txt"), "uncommitted change").unwrap();
    StdCommand::new("git")
        .args(["add", "testfile.txt"])
        .current_dir(&wt_path)
        .output()
        .expect("Failed to stage file");

    // Try to exit without --force (should fail)
    let mut cmd = Command::cargo_bin("wt").expect("Failed to find wt binary");
    cmd.current_dir(&wt_path);
    cmd.env("XDG_CONFIG_HOME", repo.config_dir.path());
    cmd.args(["exit"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("uncommitted changes"));

    // Worktree should still exist
    assert!(repo.worktrees_path().join("exit-dirty-test").exists());
}

#[test]
fn test_exit_force_removes_dirty_worktree() {
    let repo = TestRepo::new();

    repo.wt()
        .args(["create", "exit-force-test"])
        .assert()
        .success();

    // Create uncommitted changes
    let wt_path = repo.worktrees_path().join("exit-force-test");
    fs::write(wt_path.join("testfile.txt"), "uncommitted change").unwrap();
    StdCommand::new("git")
        .args(["add", "testfile.txt"])
        .current_dir(&wt_path)
        .output()
        .expect("Failed to stage file");

    // Exit with --force
    let mut cmd = Command::cargo_bin("wt").expect("Failed to find wt binary");
    cmd.current_dir(&wt_path);
    cmd.env("XDG_CONFIG_HOME", repo.config_dir.path());
    cmd.args(["exit", "--force"]).assert().success();

    // Worktree should be gone
    assert!(!repo.worktrees_path().join("exit-force-test").exists());
}

#[test]
fn test_exit_from_nested_worktree() {
    let repo = TestRepo::new();

    repo.wt()
        .args(["create", "feature/nested/test"])
        .assert()
        .success();

    let wt_path = repo.worktrees_path().join("feature/nested/test");
    let mut cmd = Command::cargo_bin("wt").expect("Failed to find wt binary");
    cmd.current_dir(&wt_path);
    cmd.env("XDG_CONFIG_HOME", repo.config_dir.path());
    cmd.args(["exit"]).assert().success();

    assert!(!repo
        .worktrees_path()
        .join("feature/nested/test")
        .exists());
}

// ============================================================================
// Spawn Tests (test_spawn.sh)
// ============================================================================

#[test]
fn test_spawn_requires_name() {
    let repo = TestRepo::new();

    repo.wt()
        .args(["spawn"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("required"));
}

#[test]
fn test_ps_with_no_sessions() {
    let repo = TestRepo::new();

    repo.wt()
        .args(["ps"])
        .assert()
        .success()
        .stderr(predicate::str::contains("No spawned sessions"));
}

#[test]
fn test_review_requires_name() {
    let repo = TestRepo::new();

    repo.wt()
        .args(["review"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("required"));
}

#[test]
fn test_merge_requires_name() {
    let repo = TestRepo::new();

    repo.wt()
        .args(["merge"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("required"));
}

#[test]
fn test_kill_requires_name() {
    let repo = TestRepo::new();

    repo.wt()
        .args(["kill"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("required"));
}

#[test]
fn test_review_nonexistent_worktree() {
    let repo = TestRepo::new();

    repo.wt()
        .args(["review", "FAKE-123"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("does not exist"));
}

#[test]
fn test_merge_nonexistent_worktree() {
    let repo = TestRepo::new();

    repo.wt()
        .args(["merge", "FAKE-123"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("does not exist"));
}

#[test]
fn test_help_includes_spawn_commands() {
    let repo = TestRepo::new();

    repo.wt()
        .args(["--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("spawn"))
        .stdout(predicate::str::contains("ps"))
        .stdout(predicate::str::contains("attach"))
        .stdout(predicate::str::contains("review"));
}

// ============================================================================
// Open All Tests (test_open_all.sh)
// ============================================================================

#[test]
fn test_health_command() {
    let repo = TestRepo::new();

    repo.wt()
        .args(["health"])
        .assert()
        .success()
        .stderr(predicate::str::contains("Terminal:"))
        .stderr(predicate::str::contains("Dependencies:"));
}

#[test]
fn test_open_all_no_worktrees() {
    let repo = TestRepo::new();

    repo.wt()
        .args(["open", "--all"])
        .assert()
        .success()
        .stderr(predicate::str::contains("No worktrees"));
}

#[test]
fn test_open_all_no_cd_output() {
    let repo = TestRepo::new();

    repo.wt().args(["create", "openall-test1"]).assert().success();
    repo.wt().args(["create", "openall-test2"]).assert().success();

    // Set a terminal that we know won't have tab support
    let output = repo
        .wt()
        .env("TERM_PROGRAM", "test-runner")
        .args(["open", "--all"])
        .output()
        .expect("Failed to run command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    // stdout should not contain cd (tabs are opened directly or warning shown)
    assert!(
        !stdout.contains("cd ") || stdout.is_empty(),
        "stdout should not contain cd command: {}",
        stdout
    );
}

// ============================================================================
// Init Tests (test_spawn.sh)
// ============================================================================

#[test]
fn test_init_command_exists() {
    let repo = TestRepo::new();

    // Init should work or fail with known errors
    let result = repo.wt().args(["init"]).output().expect("Failed to run");

    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(
        stderr.contains("Initializing") || stderr.contains("Already initialized"),
        "Unexpected output: {}",
        stderr
    );
}

#[test]
fn test_init_errors_without_force() {
    let repo = TestRepo::new();

    // Create wt.toml to simulate already-initialized repo
    fs::write(repo.path().join("wt.toml"), "[spawn]\nauto = true\n").unwrap();

    repo.wt()
        .args(["init"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Already initialized"));
}

#[test]
fn test_init_creates_expected_files() {
    let repo = TestRepo::new();

    // Remove any existing files
    let _ = fs::remove_dir_all(repo.path().join("docs"));
    let _ = fs::remove_dir_all(repo.path().join("issues"));
    let _ = fs::remove_file(repo.path().join("CLAUDE.md"));
    let _ = fs::remove_file(repo.path().join("wt.toml"));
    let _ = fs::remove_dir_all(repo.path().join(".claude"));

    repo.wt().args(["init"]).assert().success();

    assert!(repo.path().join("docs").is_dir());
    assert!(repo.path().join("issues").is_dir());
    assert!(repo.path().join("wt.toml").is_file());
    assert!(repo.path().join(".claude").is_dir());
}

#[test]
fn test_wt_toml_no_agents_section() {
    let repo = TestRepo::new();

    let _ = fs::remove_file(repo.path().join("wt.toml"));
    repo.wt().args(["init"]).assert().success();

    let content = fs::read_to_string(repo.path().join("wt.toml")).unwrap();
    assert!(
        !content.contains("[agents]"),
        "wt.toml should not contain [agents] section"
    );
}

// ============================================================================
// Version and Which Tests
// ============================================================================

#[test]
fn test_version() {
    let repo = TestRepo::new();

    repo.wt()
        .args(["version"])
        .assert()
        .success()
        .stderr(predicate::str::contains("wt"));
}

#[test]
fn test_which() {
    let repo = TestRepo::new();

    repo.wt()
        .args(["which"])
        .assert()
        .success()
        .stdout(predicate::str::contains("wt"));
}
