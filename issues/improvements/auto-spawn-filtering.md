# Auto-spawn Filtering

**Status:** Planned
**Priority:** High
**Category:** Improvements

## Objective

Make the daemon's auto-spawn smarter. Today it spawns any issue with `auto=true` + `status=Planned`. It should respect priority thresholds, target labels, and assignee filters — all configurable in `jig.toml`.

## Current State

**What works:**
- `issue_actor.rs` calls `provider.list_spawnable()` which returns `auto=true` + `status=Planned`
- Respects `max_concurrent_workers` budget
- Skips issues that already have a worker

**What's missing:**
- No priority threshold (spawns Low issues just as eagerly as Urgent)
- No label/tag targeting (can't say "only spawn issues tagged `backend`")
- No assignee filter (can't say "only spawn unassigned issues")
- `list_spawnable()` takes no filter arguments — it's all-or-nothing

## Design

### Config (`jig.toml`)

```toml
[spawn]
autoSpawn = true
maxConcurrentWorkers = 5

# NEW: auto-spawn filters
[spawn.filter]
# Only auto-spawn issues at or above this priority (default: none, spawn all)
minPriority = "High"

# Only auto-spawn issues with ALL of these labels (Linear labels, file provider tags)
labels = ["backend", "jig-auto"]

# Only auto-spawn unassigned issues (default: true)
unassignedOnly = true
```

### SpawnConfig changes

```rust
// in config.rs
pub struct SpawnFilter {
    /// Minimum priority for auto-spawn (None = no threshold)
    pub min_priority: Option<IssuePriority>,
    /// Required labels (all must match)
    pub labels: Vec<String>,
    /// Only spawn unassigned issues
    pub unassigned_only: bool,
}

pub struct SpawnConfig {
    pub auto_spawn: bool,
    pub max_concurrent_workers: usize,
    pub auto_spawn_interval: u64,
    pub filter: SpawnFilter,  // NEW
}
```

### Issue type changes

The `Issue` struct needs an `assignee` field:

```rust
pub struct Issue {
    // ... existing fields ...
    pub labels: Vec<String>,     // from labels-and-tags.md
    pub assignee: Option<String>, // NEW
}
```

### Provider changes

Both providers need to populate `assignee`:
- **Linear**: fetch `assignee { name }` in GraphQL query
- **File**: parse `**Assigned-To:**` frontmatter field

### Issue actor changes

`issue_actor.rs` `process_request()` should apply `SpawnFilter` after `list_spawnable()`:

```rust
fn apply_spawn_filter(issues: Vec<Issue>, filter: &SpawnFilter) -> Vec<Issue> {
    issues.into_iter()
        .filter(|i| {
            // Priority threshold
            if let Some(ref min) = filter.min_priority {
                match &i.priority {
                    Some(p) if p <= min => {},  // at or above threshold (Urgent < High < Med < Low)
                    Some(_) => return false,     // below threshold
                    None => return false,        // no priority set
                }
            }
            true
        })
        .filter(|i| {
            // Label matching (all required labels must be present)
            filter.labels.iter().all(|required| {
                i.labels.iter().any(|l| l.eq_ignore_ascii_case(required))
            })
        })
        .filter(|i| {
            // Assignee filter
            if filter.unassigned_only {
                i.assignee.is_none()
            } else {
                true
            }
        })
        .collect()
}
```

### IssueRequest changes

The `IssueRequest` message needs to carry the filter:

```rust
pub struct IssueRequest {
    pub repo_root: PathBuf,
    pub existing_workers: Vec<String>,
    pub max_concurrent_workers: usize,
    pub spawn_filter: SpawnFilter,  // NEW
}
```

## Acceptance Criteria

- [ ] `SpawnFilter` struct in config with `min_priority`, `labels`, `unassigned_only`
- [ ] `jig.toml` `[spawn.filter]` section parsed
- [ ] Issue actor applies filter before returning spawnable issues
- [ ] Linear provider fetches assignee
- [ ] File provider parses `**Assigned-To:**`
- [ ] Priority threshold: `minPriority = "High"` skips Medium and Low
- [ ] Label filter: `labels = ["backend"]` only spawns issues with that label
- [ ] Assignee filter: `unassignedOnly = true` skips assigned issues
- [ ] Existing behavior unchanged when no filter configured (backwards compatible)

## Depends On

- [Labels and tags](./labels-and-tags.md) (for label filtering to work)
