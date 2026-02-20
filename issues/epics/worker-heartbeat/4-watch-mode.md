# Watch Mode

**Status:** Planned  
**Priority:** Medium  
**Category:** Features  
**Epic:** issues/epics/worker-heartbeat/index.md  
**Depends-On:** issues/epics/worker-heartbeat/3-git-hooks-integration.md

## Objective

Implement `jig health --watch` that runs periodic health checks on an interval (default 15 minutes).

## Background

For continuous monitoring, need watch mode that:
- Runs health checks every N minutes
- Works with `-g` flag for all registered repos
- Handles SIGINT gracefully (Ctrl+C)
- Sleeps between checks

## Design

### Watch Loop

```rust
use std::time::Duration;
use std::thread;
use signal_hook::consts::SIGINT;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub fn watch(
    interval_minutes: u64,
    global: bool
) -> Result<()> {
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    
    // Handle SIGINT
    ctrlc::set_handler(move || {
        eprintln!("\n{} Stopping health watch...", "⏹".yellow());
        r.store(false, Ordering::SeqCst);
    })?;
    
    eprintln!(
        "{} Health watch started (checking every {} minutes)",
        "▶".green(),
        interval_minutes
    );
    eprintln!("Press Ctrl+C to stop\n");
    
    while running.load(Ordering::SeqCst) {
        // Run health check
        match run_health_check(global) {
            Ok(alerts) => {
                if !alerts.is_empty() {
                    eprintln!("\n{} {} alerts:", "⚠".yellow(), alerts.len());
                    for alert in alerts {
                        eprintln!("  {}", alert);
                    }
                }
            },
            Err(e) => {
                eprintln!("{} Health check failed: {}", "✗".red(), e);
            }
        }
        
        // Sleep until next check
        let duration = Duration::from_secs(interval_minutes * 60);
        for _ in 0..(interval_minutes * 60) {
            if !running.load(Ordering::SeqCst) {
                break;
            }
            thread::sleep(Duration::from_secs(1));
        }
    }
    
    eprintln!("{} Health watch stopped", "✓".green());
    Ok(())
}
```

### Health Check Runner

```rust
pub fn run_health_check(global: bool) -> Result<Vec<Alert>> {
    let timestamp = chrono::Local::now().format("%H:%M:%S");
    eprintln!("[{}] Running health check...", timestamp);
    
    let mut all_alerts = Vec::new();
    
    if global {
        let repos = load_repo_registry()?;
        for repo in repos {
            let alerts = check_repo(&repo.path)?;
            all_alerts.extend(alerts);
        }
    } else {
        let repo_path = std::env::current_dir()?;
        let alerts = check_repo(&repo_path)?;
        all_alerts.extend(alerts);
    }
    
    Ok(all_alerts)
}

pub fn check_repo(repo_path: &Path) -> Result<Vec<Alert>> {
    let config = Config::load(repo_path)?;
    let detector = WorkerDetector::from_config(&config.health)?;
    let health_path = repo_path.join(".worktrees/.jig-health.json");
    let mut health = HealthState::load(&health_path)?;
    
    let repo_name = repo_path.file_name()
        .unwrap()
        .to_string_lossy()
        .to_string();
    
    let mut alerts = Vec::new();
    
    for (worker_name, worker_health) in health.workers.iter_mut() {
        if let Some(alert) = check_and_nudge_worker(
            &repo_name,
            worker_name,
            worker_health,
            &config.health,
            &detector
        )? {
            alerts.push(alert);
        }
    }
    
    health.save(&health_path)?;
    
    Ok(alerts)
}
```

### Configuration

```toml
[health]
# Watch interval in minutes (0 = disabled)
watchInterval = 15
```

## Implementation

**Files:**
- `crates/jig-cli/src/commands/health.rs` - add watch mode
- `crates/jig-core/src/health/watch.rs` - watch loop

**Dependencies:**
```toml
signal-hook = "0.3"
ctrlc = "3.4"
```

**CLI integration:**
```rust
#[derive(Args, Debug, Clone)]
pub struct Health {
    /// Run health check continuously on an interval
    #[arg(long)]
    watch: bool,
    
    /// Check all registered repos (requires global-commands epic)
    #[arg(short = 'g', long)]
    global: bool,
    
    /// Override watch interval in minutes
    #[arg(long)]
    interval: Option<u64>,
}

impl Op for Health {
    fn execute(&self, ctx: &OpContext) -> Result<()> {
        if self.watch {
            let interval = self.interval.unwrap_or_else(|| {
                ctx.config.health.watch_interval
            });
            
            watch::watch(interval, self.global)?;
        } else {
            let alerts = run_health_check(self.global)?;
            
            if alerts.is_empty() {
                eprintln!("{} All workers healthy", "✓".green());
            } else {
                for alert in alerts {
                    eprintln!("{} {}", "⚠".yellow(), alert);
                }
            }
        }
        
        Ok(())
    }
}
```

## Acceptance Criteria

- [ ] `jig health --watch` runs periodic checks
- [ ] `jig health --watch -g` checks all registered repos
- [ ] `jig health --watch --interval 5` overrides config
- [ ] Ctrl+C stops watch gracefully
- [ ] Timestamp shown for each check
- [ ] Alerts printed when workers need attention
- [ ] Sleep in 1-second increments (responsive to Ctrl+C)
- [ ] Config `watchInterval` sets default interval

## Testing

```rust
#[test]
fn test_run_health_check_single_repo() {
    let repo = TestRepo::new();
    
    // Create idle worker
    let mut health = HealthState::new();
    health.add_worker("test", now() - 4 * 3600);  // 4h old, no commits
    health.save(&repo.health_path()).unwrap();
    
    // Run health check
    let alerts = run_health_check(false).unwrap();
    
    // Should have nudged
    let health = HealthState::load(&repo.health_path()).unwrap();
    assert_eq!(health.workers.get("test").unwrap().get_nudge_count("idle"), 1);
}

// Watch mode tested manually:
// $ jig health --watch --interval 1
// [15:30:00] Running health check...
// [wait 1 minute]
// [15:31:00] Running health check...
// ^C
// ⏹ Stopping health watch...
// ✓ Health watch stopped
```

## Output Example

```
$ jig health --watch

▶ Health watch started (checking every 15 minutes)
Press Ctrl+C to stop

[10:00:00] Running health check...

[10:15:00] Running health check...
⚠ 2 alerts:
  Worker features/auth idle for 4h (nudge 2/3)
  Worker bugs/fix-123 stuck at prompt (auto-approved)

[10:30:00] Running health check...

^C
⏹ Stopping health watch...
✓ Health watch stopped
```

## Next Steps

After this ticket:
- Worker heartbeat epic is COMPLETE!
- All health checks can now run automatically
- Ready for GitHub integration epic to add PR-specific nudges
