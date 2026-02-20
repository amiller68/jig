# Health State Storage

**Status:** In Review  
**Priority:** High  
**Category:** Features  
**Epic:** issues/epics/worker-heartbeat/index.md

## Objective

Implement health state storage that tracks worker metrics and nudge counts in `.worktrees/.jig-health.json`.

## Background

Need persistent storage for:
- Worker start time, last commit, commit count
- Last file modification time
- Nudge counts per type (idle, stuck, ci_failure, etc.)
- Health score

## Design

### State File Schema

`.worktrees/.jig-health.json`:
```json
{
  "version": "1",
  "max_nudges": 3,
  "workers": {
    "features/auth": {
      "started_at": 1708358400,
      "last_commit_at": 1708362000,
      "commit_count": 3,
      "last_file_mod_at": 1708363200,
      "nudges": {
        "idle": 2,
        "ci_failure": 0,
        "conflict": 0
      }
    }
  }
}
```

### Data Structures

```rust
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthState {
    pub version: String,
    pub max_nudges: u32,
    pub workers: HashMap<String, WorkerHealth>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerHealth {
    pub started_at: i64,
    pub last_commit_at: i64,
    pub commit_count: u32,
    pub last_file_mod_at: i64,
    #[serde(default)]
    pub nudges: HashMap<String, u32>,
}

impl HealthState {
    pub fn new() -> Self {
        Self {
            version: "1".to_string(),
            max_nudges: 3,
            workers: HashMap::new(),
        }
    }
    
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::new());
        }
        let content = std::fs::read_to_string(path)?;
        Ok(serde_json::from_str(&content)?)
    }
    
    pub fn save(&self, path: &Path) -> Result<()> {
        std::fs::create_dir_all(path.parent().unwrap())?;
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }
    
    pub fn add_worker(&mut self, name: &str, started_at: i64) {
        let health = WorkerHealth {
            started_at,
            last_commit_at: 0,
            commit_count: 0,
            last_file_mod_at: 0,
            nudges: HashMap::new(),
        };
        self.workers.insert(name.to_string(), health);
    }
    
    pub fn remove_worker(&mut self, name: &str) -> Option<WorkerHealth> {
        self.workers.remove(name)
    }
}

impl WorkerHealth {
    pub fn increment_nudge(&mut self, nudge_type: &str) {
        *self.nudges.entry(nudge_type.to_string()).or_insert(0) += 1;
    }
    
    pub fn reset_nudge(&mut self, nudge_type: &str) {
        self.nudges.remove(nudge_type);
    }
    
    pub fn get_nudge_count(&self, nudge_type: &str) -> u32 {
        *self.nudges.get(nudge_type).unwrap_or(&0)
    }
    
    pub fn age_hours(&self) -> u64 {
        let now = chrono::Utc::now().timestamp();
        ((now - self.started_at) / 3600) as u64
    }
    
    pub fn hours_since_commit(&self) -> u64 {
        if self.last_commit_at == 0 {
            return self.age_hours();
        }
        let now = chrono::Utc::now().timestamp();
        ((now - self.last_commit_at) / 3600) as u64
    }
}
```

## Implementation

**Files:**
- `crates/jig-core/src/health/state.rs` - state structures
- `crates/jig-core/src/health/mod.rs` - module exports

**Dependencies in `Cargo.toml`:**
```toml
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
chrono = "0.4"
```

## Acceptance Criteria

- [ ] `HealthState` struct with serde support
- [ ] Load from `.worktrees/.jig-health.json` (create if missing)
- [ ] Save with pretty-printed JSON
- [ ] Add/remove workers
- [ ] Increment/reset/get nudge counts per type
- [ ] Calculate worker age and time since commit
- [ ] Thread-safe (use mutex if needed for concurrent access)

## Testing

```rust
#[test]
fn test_health_state_new() {
    let state = HealthState::new();
    assert_eq!(state.version, "1");
    assert_eq!(state.max_nudges, 3);
}

#[test]
fn test_add_remove_worker() {
    let mut state = HealthState::new();
    state.add_worker("test", 1000);
    assert!(state.workers.contains_key("test"));
    
    state.remove_worker("test");
    assert!(!state.workers.contains_key("test"));
}

#[test]
fn test_nudge_counts() {
    let mut state = HealthState::new();
    state.add_worker("test", 1000);
    let worker = state.workers.get_mut("test").unwrap();
    
    worker.increment_nudge("idle");
    assert_eq!(worker.get_nudge_count("idle"), 1);
    
    worker.increment_nudge("idle");
    assert_eq!(worker.get_nudge_count("idle"), 2);
    
    worker.reset_nudge("idle");
    assert_eq!(worker.get_nudge_count("idle"), 0);
}

#[test]
fn test_save_load_roundtrip() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join(".worktrees/.jig-health.json");
    
    let mut state = HealthState::new();
    state.add_worker("test", 1000);
    state.save(&path).unwrap();
    
    let loaded = HealthState::load(&path).unwrap();
    assert!(loaded.workers.contains_key("test"));
}
```

## Next Steps

After this ticket:
- Move to ticket 1 (tmux detection)
- Detection will read this state to check worker activity
