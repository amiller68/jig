# Hook Handlers

**Status:** Planned  
**Priority:** High  
**Category:** Features  
**Epic:** issues/epics/git-hooks/index.md  
**Depends-On:** issues/epics/git-hooks/2-idempotent-init.md

## Objective

Implement the `jig hooks <name>` handlers that perform actual git hook logic: updating worker metrics and triggering health checks.

## Background

Hook wrappers call `jig hooks <name>` which needs to:
- Update worker health state (commit count, timestamp, etc.)
- Reset nudge counts (worker made progress)
- Optionally trigger health checks
- Exit quickly (<100ms) to not slow down git operations

## Design

### Handler Commands

```bash
jig hooks post-commit    # Called by .git/hooks/post-commit wrapper
jig hooks post-merge     # Called by .git/hooks/post-merge wrapper
jig hooks pre-commit     # Called by .git/hooks/pre-commit wrapper
```

### Handler Logic

**post-commit:**
```rust
pub fn handle_post_commit(repo_path: &Path) -> Result<()> {
    // Collect git metrics
    let metrics = git::collect_metrics(repo_path)?;
    
    // Update health state
    let state_path = repo_path.join(".worktrees/.jig-health.json");
    if let Ok(mut health) = HealthState::load(&state_path) {
        // Find current worker (current branch)
        let branch = git::current_branch(repo_path)?;
        if let Some(worker) = health.workers.get_mut(&branch) {
            worker.last_commit_at = metrics.last_commit_at;
            worker.commit_count = metrics.commit_count;
            worker.reset_nudge_count("idle");  // Made progress
        }
        health.save(&state_path)?;
    }
    
    // Optionally trigger health check
    let config = Config::load(repo_path)?;
    if config.health.check_on_commit {
        // Async spawn health check (don't block)
        spawn_health_check(repo_path)?;
    }
    
    Ok(())
}
```

**post-merge:**
```rust
pub fn handle_post_merge(repo_path: &Path) -> Result<()> {
    // Update health state
    let state_path = repo_path.join(".worktrees/.jig-health.json");
    if let Ok(mut health) = HealthState::load(&state_path) {
        let branch = git::current_branch(repo_path)?;
        if let Some(worker) = health.workers.get_mut(&branch) {
            worker.reset_nudge_count("conflict");  // Merge succeeded
        }
        health.save(&state_path)?;
    }
    
    // Trigger health check (merge is important event)
    let config = Config::load(repo_path)?;
    if config.health.check_on_merge {
        spawn_health_check(repo_path)?;
    }
    
    Ok(())
}
```

**pre-commit:**
```rust
pub fn handle_pre_commit(repo_path: &Path, _msg_file: &Path) -> Result<()> {
    let config = Config::load(repo_path)?;
    
    // Validate conventional commits if enabled
    if config.github.require_conventional_commits {
        // Read commit message from file
        let msg = std::fs::read_to_string(_msg_file)?;
        
        // Validate format
        if let Err(e) = validate_commit_message(&msg, &config.conventional_commits) {
            eprintln!("❌ Invalid commit message:\n{}", e);
            eprintln!("\nExamples:");
            eprintln!("  feat(auth): add OAuth2 support");
            eprintln!("  fix(ui): resolve button alignment");
            eprintln!("  docs: update README");
            return Err(e.into());
        }
    }
    
    Ok(())
}
```

### Git Metrics Collection

```rust
pub fn collect_metrics(repo_path: &Path) -> Result<GitMetrics> {
    let repo = git2::Repository::open(repo_path)?;
    
    // Last commit timestamp
    let head = repo.head()?.peel_to_commit()?;
    let last_commit_at = head.time().seconds();
    
    // Commit count on current branch
    let mut revwalk = repo.revwalk()?;
    revwalk.push_head()?;
    revwalk.hide_ref("refs/remotes/origin/main")?;
    let commit_count = revwalk.count();
    
    Ok(GitMetrics {
        last_commit_at,
        commit_count,
    })
}
```

## Implementation

**Files:**
- `crates/jig-cli/src/commands/hooks.rs` - hooks subcommand
- `crates/jig-core/src/hooks/handlers.rs` - handler implementations
- `crates/jig-core/Cargo.toml` - add `git2` dependency

**CLI integration:**
```rust
#[derive(Subcommand, Debug, Clone)]
pub enum HooksCommand {
    PostCommit,
    PostMerge,
    PreCommit,
}

impl Op for HooksCommand {
    type Error = HooksError;
    type Output = ();
    
    fn execute(&self, ctx: &OpContext) -> Result<Self::Output, Self::Error> {
        let repo_path = ctx.repo_path()?;
        
        match self {
            HooksCommand::PostCommit => {
                handlers::handle_post_commit(&repo_path)?;
            },
            HooksCommand::PostMerge => {
                handlers::handle_post_merge(&repo_path)?;
            },
            HooksCommand::PreCommit => {
                let msg_file = repo_path.join(".git/COMMIT_EDITMSG");
                handlers::handle_pre_commit(&repo_path, &msg_file)?;
            },
        }
        
        Ok(())
    }
}
```

## Acceptance Criteria

- [ ] `jig hooks post-commit` updates worker metrics
- [ ] `jig hooks post-commit` resets idle nudge count
- [ ] `jig hooks post-merge` resets conflict nudge count
- [ ] `jig hooks pre-commit` validates commit messages (if enabled)
- [ ] Handlers exit quickly (<100ms)
- [ ] Handlers never fail (except pre-commit validation)
- [ ] Health checks spawned asynchronously (don't block git)
- [ ] Works even if health state doesn't exist yet

## Testing

```rust
#[test]
fn test_post_commit_updates_metrics() {
    let repo = TestRepo::new();
    
    // Create health state with worker
    let mut health = HealthState::new();
    health.add_worker("features/test", 0);
    health.save(&repo.path().join(".worktrees/.jig-health.json")).unwrap();
    
    // Make a commit
    repo.commit("test");
    
    // Run hook handler
    handle_post_commit(&repo.path()).unwrap();
    
    // Load state and verify
    let health = HealthState::load(&repo.path().join(".worktrees/.jig-health.json")).unwrap();
    let worker = health.workers.get("features/test").unwrap();
    assert_eq!(worker.commit_count, 1);
    assert!(worker.last_commit_at > 0);
}

#[test]
fn test_pre_commit_validates_message() {
    let repo = TestRepo::new();
    let msg_file = repo.path().join(".git/COMMIT_EDITMSG");
    
    // Invalid message
    std::fs::write(&msg_file, "bad commit").unwrap();
    assert!(handle_pre_commit(&repo.path(), &msg_file).is_err());
    
    // Valid message
    std::fs::write(&msg_file, "feat: add feature").unwrap();
    assert!(handle_pre_commit(&repo.path(), &msg_file).is_ok());
}
```

## Next Steps

After this ticket:
- Move to ticket 4 (uninstall & rollback)
- Uninstall will remove hooks and restore backups
- Epic will be complete!
