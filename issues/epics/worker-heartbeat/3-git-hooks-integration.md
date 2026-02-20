# Git Hooks Integration

**Status:** Planned  
**Priority:** High  
**Category:** Features  
**Epic:** issues/epics/worker-heartbeat/index.md  
**Depends-On:** issues/epics/worker-heartbeat/2-nudge-system.md, issues/epics/git-hooks/index.md

## Objective

Integrate health system with git hooks to update worker metrics automatically on commit/merge events.

## Background

Git hooks (from git-hooks epic) call `jig hooks <name>`. These handlers need to:
- Update worker health state (commit count, timestamp)
- Reset nudge counts (worker made progress)
- Optionally trigger health checks

## Design

### Hook Handler Updates

Add health state updates to existing hook handlers:

**post-commit handler:**
```rust
pub fn handle_post_commit(repo_path: &Path) -> Result<()> {
    // Existing: collect git metrics
    let metrics = git::collect_metrics(repo_path)?;
    
    // NEW: Update health state
    let health_path = repo_path.join(".worktrees/.jig-health.json");
    if let Ok(mut health) = HealthState::load(&health_path) {
        let branch = git::current_branch(repo_path)?;
        
        if let Some(worker) = health.workers.get_mut(&branch) {
            // Update metrics
            worker.last_commit_at = metrics.last_commit_at;
            worker.commit_count = metrics.commit_count;
            
            // Reset idle nudge count (made progress)
            worker.reset_nudge("idle");
        }
        
        health.save(&health_path)?;
    }
    
    // Optionally trigger health check
    let config = Config::load(repo_path)?;
    if config.health.check_on_commit {
        spawn_async_health_check(repo_path)?;
    }
    
    Ok(())
}
```

**post-merge handler:**
```rust
pub fn handle_post_merge(repo_path: &Path) -> Result<()> {
    // NEW: Update health state
    let health_path = repo_path.join(".worktrees/.jig-health.json");
    if let Ok(mut health) = HealthState::load(&health_path) {
        let branch = git::current_branch(repo_path)?;
        
        if let Some(worker) = health.workers.get_mut(&branch) {
            // Reset conflict nudge count (merge succeeded)
            worker.reset_nudge("conflict");
        }
        
        health.save(&health_path)?;
    }
    
    // Trigger health check (merge is important)
    let config = Config::load(repo_path)?;
    if config.health.check_on_merge {
        spawn_async_health_check(repo_path)?;
    }
    
    Ok(())
}
```

### Async Health Check

Don't block git operations:

```rust
use std::process::Command;

pub fn spawn_async_health_check(repo_path: &Path) -> Result<()> {
    let path = repo_path.to_path_buf();
    
    std::thread::spawn(move || {
        // Run health check in background
        let _ = Command::new("jig")
            .arg("health")
            .current_dir(&path)
            .output();
    });
    
    Ok(())
}
```

### Configuration

```toml
[health]
# Run health check after commit (can be slow, default off)
checkOnCommit = false

# Run health check after merge (recommended)
checkOnMerge = true
```

## Implementation

**Files:**
- `crates/jig-core/src/hooks/handlers.rs` - update existing handlers
- `crates/jig-core/src/health/async.rs` - async health check spawn

**Integration points:**
1. Git hooks call handlers (already implemented in git-hooks epic)
2. Handlers update health state (this ticket)
3. Health check reads updated state (already implemented in earlier tickets)

## Acceptance Criteria

- [ ] `post-commit` handler updates worker metrics
- [ ] `post-commit` handler resets idle nudge count
- [ ] `post-merge` handler resets conflict nudge count
- [ ] Async health check spawns without blocking git
- [ ] Config flags `checkOnCommit` and `checkOnMerge` work
- [ ] Handlers are fast (<100ms)
- [ ] Works even if health state doesn't exist yet (graceful)

## Testing

```rust
#[test]
fn test_post_commit_updates_health() {
    let repo = TestRepo::new();
    
    // Create health state with worker
    let mut health = HealthState::new();
    health.add_worker("features/test", now() - 3600);
    health.save(&repo.health_path()).unwrap();
    
    // Make a commit
    repo.commit("test commit");
    
    // Run hook handler
    handle_post_commit(&repo.path()).unwrap();
    
    // Verify metrics updated
    let health = HealthState::load(&repo.health_path()).unwrap();
    let worker = health.workers.get("features/test").unwrap();
    assert_eq!(worker.commit_count, 1);
    assert!(worker.last_commit_at > 0);
}

#[test]
fn test_post_commit_resets_idle_nudge() {
    let repo = TestRepo::new();
    
    // Create worker with idle nudge
    let mut health = HealthState::new();
    health.add_worker("features/test", now() - 3600);
    let worker = health.workers.get_mut("features/test").unwrap();
    worker.increment_nudge("idle");
    health.save(&repo.health_path()).unwrap();
    
    // Make commit
    repo.commit("test");
    handle_post_commit(&repo.path()).unwrap();
    
    // Verify nudge reset
    let health = HealthState::load(&repo.health_path()).unwrap();
    let worker = health.workers.get("features/test").unwrap();
    assert_eq!(worker.get_nudge_count("idle"), 0);
}

#[test]
fn test_post_merge_resets_conflict() {
    let repo = TestRepo::new();
    
    // Create worker with conflict nudge
    let mut health = HealthState::new();
    health.add_worker("features/test", now());
    health.workers.get_mut("features/test").unwrap().increment_nudge("conflict");
    health.save(&repo.health_path()).unwrap();
    
    // Trigger post-merge
    handle_post_merge(&repo.path()).unwrap();
    
    // Verify nudge reset
    let health = HealthState::load(&repo.health_path()).unwrap();
    assert_eq!(health.workers.get("features/test").unwrap().get_nudge_count("conflict"), 0);
}
```

## Next Steps

After this ticket:
- Move to ticket 4 (watch mode)
- Watch mode will run periodic health checks
