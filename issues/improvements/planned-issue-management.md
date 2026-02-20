# Planned Issue Management

**Status:** Planned  
**Priority:** Medium  
**Category:** Improvements  
**Depends-On:** issues/features/global-commands.md

## Objective

Improve how jig discovers, filters, prioritizes, and spawns planned issues. Add batch operations, dependency tracking, and priority-based auto-spawning across repos.

## Architecture

### Issue Frontmatter Schema

**Standard fields (all issues):**
```markdown
# Title

**Status:** Planned | In Progress | In Review | Done | Blocked  
**Priority:** Urgent | High | Medium | Low  
**Category:** Features | Improvements | Bugs | Chores | Documentation  
**Depends-On:** issues/path/to/dependency.md (optional, can be cross-repo)  
**Assigned-To:** worker-name (auto-populated by jig)  
**Estimated-Hours:** 4 (optional)  
**Labels:** auth, security (optional, comma-separated)

## Objective
[What needs to be done]

## Acceptance Criteria
- [ ] Thing 1
- [ ] Thing 2
```

### Issue Discovery

**Scan algorithm:**
```rust
fn discover_issues(repo_path: &Path, config: &IssueConfig) -> Result<Vec<Issue>> {
    let issues_dir = repo_path.join(&config.directory);
    
    WalkDir::new(issues_dir)
        .into_iter()
        .filter_entry(|e| {
            // Skip templates and hidden dirs
            !e.path().starts_with("_templates") && 
            !e.path().starts_with(".")
        })
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path().extension() == Some(OsStr::new("md")) &&
            e.path().file_name() != Some(OsStr::new("README.md"))
        })
        .map(|entry| parse_issue(entry.path()))
        .collect()
}
```

### Dependency Graph

**Cross-repo dependencies:**
```rust
struct IssueDependency {
    issue_path: PathBuf,      // Local issue path
    depends_on: Vec<String>,  // Can be local or remote
    blocked: bool,            // Are dependencies satisfied?
}

fn resolve_dependency(dep: &str, repos: &[RepoState]) -> Result<DependencyStatus> {
    // Local dependency: "issues/features/auth.md"
    if !dep.contains("://") {
        return check_local_dependency(dep);
    }
    
    // Cross-repo: "repo:jax-fs:issues/features/encryption.md"
    if dep.starts_with("repo:") {
        let parts: Vec<&str> = dep.split(':').collect();
        let repo_name = parts[1];
        let issue_path = parts[2];
        
        return check_cross_repo_dependency(repos, repo_name, issue_path);
    }
    
    Err(Error::InvalidDependency(dep.to_string()))
}
```

### Priority Queue

**Scoring algorithm:**
```rust
fn calculate_spawn_priority(issue: &Issue, config: &IssueConfig) -> u32 {
    let mut score = 0;
    
    // Priority weight
    score += match issue.priority {
        Priority::Urgent => 1000,
        Priority::High => 500,
        Priority::Medium => 100,
        Priority::Low => 10,
    };
    
    // Age weight (older = higher priority)
    let age_days = (now() - issue.created_at).num_days();
    score += age_days as u32 * 5;
    
    // Dependency weight (no blockers = higher priority)
    if !issue.has_blockers() {
        score += 200;
    }
    
    // Estimated hours weight (quick wins first)
    if let Some(hours) = issue.estimated_hours {
        if hours <= 2 {
            score += 100;
        }
    }
    
    score
}
```

## Configuration

**Per-repo in `jig.toml`:**

```toml
[issues]
# Issue directory (relative to repo root)
directory = "issues"

# Auto-spawn priorities (on jig health)
autoSpawnPriorities = ["Urgent", "High"]

# Max concurrent workers (0 = unlimited)
maxConcurrentWorkers = 5

# Max workers per priority level
[issues.maxPerPriority]
urgent = 3
high = 5
medium = 3
low = 1

# Require issue file for all workers
requireIssueFile = true

# Template directory
templateDirectory = "issues/_templates"

# Auto-assign workers when spawned
autoAssign = true

# Check dependencies before spawning
checkDependencies = true

# Age weight for priority scoring
ageWeightDays = 5
```

**Global in `~/.config/jig/config`:**
```
issues.maxConcurrentWorkers=5
issues.autoSpawnPriorities=Urgent,High
issues.requireIssueFile=true
```

## Commands

```bash
# List all planned issues (current repo)
jig issues list
jig issues list --status Planned

# List issues across all registered repos
jig issues list -g

# Filter by priority
jig issues list --priority High
jig issues list --priority Urgent,High

# Filter by category
jig issues list --category Features,Bugs

# Filter by labels
jig issues list --labels auth,security

# Show only unblocked issues (dependencies satisfied)
jig issues list --unblocked

# Show blocked issues (with dependencies)
jig issues list --blocked

# Show dependency tree
jig issues tree features/auth.md

# Show next issue in queue (highest priority, unblocked)
jig issues next

# Spawn next issue
jig issues spawn-next

# Spawn next N issues
jig issues spawn-next --count 3

# Spawn specific issue
jig issues spawn features/auth

# Batch spawn (interactive selection)
jig issues spawn --batch
jig issues spawn --batch --priority High  # pre-filter
jig issues spawn --batch -g  # across all repos

# Create new issue from template
jig issues create --template feature
jig issues create --template bug --category Bugs --priority High

# Update issue status
jig issues status features/auth --status "In Progress"

# Mark issue as blocked
jig issues block features/ui --depends-on features/auth.md

# Mark issue complete
jig issues complete features/auth

# Show issue stats
jig issues stats
jig issues stats -g
```

## Batch Spawning

**Interactive flow:**
```
$ jig issues spawn --batch --priority High

Select issues to spawn (↑↓ to move, space to select, enter to confirm):

 [ ] features/github-integration       High      Native GitHub API integration
 [x] features/worker-heartbeat         High      Built-in health checks
 [ ] improvements/activity-metrics     Medium    Track worker activity
 [x] bugs/auto-spawn-blocked           Medium    Fix permission prompt blocking

2 selected. Continue? [y/N] y

Spawning features/worker-heartbeat... ✓
Spawning bugs/auto-spawn-blocked... ✓

2 workers spawned. Run 'jig ps' to see status.
```

**Implementation:**
```rust
use inquire::MultiSelect;

fn spawn_batch(issues: Vec<Issue>) -> Result<Vec<String>> {
    let options: Vec<String> = issues.iter()
        .map(|i| format!("{:<40} {:<10} {}", i.name, i.priority, i.title))
        .collect();
    
    let selected = MultiSelect::new("Select issues to spawn:", options)
        .prompt()?;
    
    let mut spawned = Vec::new();
    
    for idx in selected {
        let issue = &issues[idx];
        spawn_worker(&issue.name, &issue.content)?;
        spawned.push(issue.name.clone());
    }
    
    Ok(spawned)
}
```

## Issue Stats

**Per-repo:**
```
$ jig issues stats

ISSUE STATISTICS (jig):

By Status:
  Planned:     12
  In Progress:  4
  In Review:    2
  Done:        45
  Blocked:      3

By Priority:
  Urgent:  1
  High:    5
  Medium: 10
  Low:     6

By Category:
  Features:     8
  Improvements: 4
  Bugs:         2
  Chores:       3

Average completion time: 4.2 days
Oldest planned issue: 28 days (features/legacy-api-removal)
```

**Global:**
```
$ jig issues stats -g

ISSUE STATISTICS (ALL REPOS):

Repositories: 3 (jig, jax-fs, sites)

By Status:
  Planned:     28 (jig: 12, jax-fs: 14, sites: 2)
  In Progress: 11 (jig: 4, jax-fs: 6, sites: 1)
  In Review:    5 (jig: 2, jax-fs: 3, sites: 0)
  Done:       143 (jig: 45, jax-fs: 89, sites: 9)
  Blocked:      7 (jig: 3, jax-fs: 4, sites: 0)

Top priorities:
  • jax-fs: features/streaming-encryption (Urgent)
  • jig: features/worker-heartbeat (High)
  • jig: features/github-integration (High)
```

## Auto-Spawn Logic

**On `jig health` run:**
```rust
fn auto_spawn_issues(
    issues: Vec<Issue>,
    config: &IssueConfig,
    current_workers: usize
) -> Result<Vec<String>> {
    let mut spawned = Vec::new();
    
    // Filter to auto-spawn priorities
    let candidates: Vec<_> = issues.into_iter()
        .filter(|i| config.auto_spawn_priorities.contains(&i.priority))
        .filter(|i| !i.has_blockers())  // Only unblocked issues
        .collect();
    
    // Sort by priority score
    let mut sorted = candidates;
    sorted.sort_by_key(|i| std::cmp::Reverse(calculate_spawn_priority(i, config)));
    
    // Spawn up to max concurrent workers
    let max_workers = config.max_concurrent_workers;
    let slots_available = if max_workers == 0 {
        usize::MAX
    } else {
        max_workers.saturating_sub(current_workers)
    };
    
    for issue in sorted.iter().take(slots_available) {
        spawn_worker(&issue.name, &issue.content)?;
        spawned.push(issue.name.clone());
    }
    
    Ok(spawned)
}
```

## Implementation Phases

### Phase 1: Core Discovery
1. Issue parsing with frontmatter
2. Recursive directory scan
3. Filter by status/priority/category/labels
4. `jig issues list` command

### Phase 2: Dependency Tracking
1. Parse `Depends-On` field
2. Resolve local dependencies
3. Resolve cross-repo dependencies
4. `jig issues tree` visualization
5. Block spawning if dependencies unsatisfied

### Phase 3: Priority Queue
1. Priority scoring algorithm
2. `jig issues next` shows highest priority
3. `jig issues spawn-next` spawns next in queue
4. Auto-spawn on `jig health` (configurable priorities)

### Phase 4: Batch Operations
1. Interactive batch spawn (inquire crate)
2. Pre-filtering (priority, category, labels)
3. Global batch spawn (`-g` flag)

### Phase 5: Issue Management
1. `jig issues create` from templates
2. `jig issues status` update status
3. `jig issues block` add dependencies
4. `jig issues complete` mark done
5. `jig issues stats` show analytics

## Acceptance Criteria

### Discovery
- [ ] Scan `issues/` directory recursively
- [ ] Parse frontmatter (status, priority, category, etc.)
- [ ] Skip templates and hidden dirs
- [ ] Support subdirectories (features/, bugs/, etc.)

### Filtering
- [ ] Filter by status (Planned, In Progress, etc.)
- [ ] Filter by priority (Urgent, High, etc.)
- [ ] Filter by category (Features, Bugs, etc.)
- [ ] Filter by labels (comma-separated)
- [ ] Filter by blocked/unblocked (dependencies)

### Dependencies
- [ ] Parse `Depends-On` field (local and cross-repo)
- [ ] Resolve local dependencies (same repo)
- [ ] Resolve cross-repo dependencies (repo:name:path format)
- [ ] Block spawning if dependencies unsatisfied
- [ ] `jig issues tree` shows dependency graph

### Priority Queue
- [ ] Calculate spawn priority (priority + age + blockers + estimated hours)
- [ ] `jig issues next` shows next issue to spawn
- [ ] `jig issues spawn-next` spawns next issue
- [ ] Auto-spawn on `jig health` (configurable priorities)
- [ ] Respect max concurrent workers

### Batch Operations
- [ ] Interactive batch spawn with `inquire`
- [ ] Pre-filtering before selection
- [ ] Global batch spawn (`-g`)
- [ ] Confirmation before spawning

### Issue Management
- [ ] Create new issues from templates
- [ ] Update issue status
- [ ] Add/remove dependencies
- [ ] Mark issues complete
- [ ] Show issue statistics

### Configuration
- [ ] Per-repo settings in `jig.toml` `[issues]`
- [ ] Auto-spawn priorities configurable
- [ ] Max concurrent workers configurable
- [ ] Max per priority level configurable
- [ ] Global fallback in `~/.config/jig/config`

## Testing

```bash
# Create test issues with dependencies
mkdir -p test-repo/issues/features
cat > test-repo/issues/features/api.md << EOF
# API Client
**Status:** Planned
**Priority:** High
**Category:** Features

## Objective
Build API client library
EOF

cat > test-repo/issues/features/ui.md << EOF
# UI Dashboard
**Status:** Planned
**Priority:** High
**Category:** Features
**Depends-On:** issues/features/api.md

## Objective
Build UI dashboard using API client
EOF

# List issues
cd test-repo && jig issues list

# Show dependency tree
jig issues tree features/ui.md
# Output: features/ui.md → features/api.md

# Try to spawn blocked issue
jig issues spawn features/ui
# Error: Cannot spawn: blocked by unsatisfied dependencies

# Spawn dependency first
jig issues spawn features/api

# Mark dependency complete
jig issues complete features/api

# Now spawn UI issue
jig issues spawn features/ui  # ✓ succeeds
```

## Open Questions

1. Should issue age be tracked in frontmatter or git history? (Git history - created date)
2. Should we support issue templates with variables? (Yes, handlebars)
3. Should dependencies be transitive? (Yes, recursively check)
4. Should we support external issue trackers (Jira, Linear)? (Future, via MCP integrations)

## Related Issues

- issues/features/worker-heartbeat-system.md (auto-spawn on health checks)
- issues/features/smart-context-injection.md (issue content in spawn template)
- issues/features/global-commands.md (multi-repo operations)
