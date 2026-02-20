# Native GitHub Integration

**Status:** Planned  
**Priority:** High  
**Category:** Features  
**Depends-On:** issues/features/global-commands.md, issues/features/worker-heartbeat-system.md

## Objective

Add native GitHub integration to `jig` for automatic detection and handling of PR-related issues: merge conflicts, failing CI, review comments, non-conventional commits. Replace shell-outs to `gh` CLI with robust API client and structured configuration.

## Architecture

### GitHub Client: octorust

**Why octorust:**
- Native Rust, no subprocess overhead
- Type-safe API, compile-time validation
- Built-in rate limiting and retry logic
- Async support for batch operations

**Initialization:**
```rust
use octorust::{Client, auth::Credentials};

fn github_client(config: &RepoConfig) -> Result<Client> {
    let token = config.github.token
        .or_else(|| env::var("GITHUB_TOKEN").ok())
        .or_else(|| gh_cli_token())  // fallback to `gh auth token`
        .ok_or(Error::NoGitHubToken)?;
    
    let creds = Credentials::Token(token);
    Ok(Client::new("jig", creds)?)
}
```

### PR State Cache

**File: `.worktrees/.jig-github-cache.json`**

```json
{
  "prs": {
    "features/auth": {
      "number": 42,
      "state": "OPEN",
      "mergeable": "MERGEABLE",
      "ci_status": "SUCCESS",
      "review_decision": "APPROVED",
      "last_checked": 1708363200
    }
  },
  "cache_ttl": 300  # seconds
}
```

Avoid redundant API calls during health checks.

### Nudge State Integration

Extends `.worktrees/.jig-health.json`:

```json
{
  "workers": {
    "features/auth": {
      "nudges": {
        "ci_failure": 1,
        "conflict": 0,
        "review": 2,
        "bad_commits": 0
      }
    }
  }
}
```

## Configuration

**Per-repo in `jig.toml`:**

```toml
[github]
# Owner/repo (auto-detected from git remote if omitted)
owner = "amiller68"
repo = "jig"

# Auth token (fallback: GITHUB_TOKEN env, gh CLI)
token = "ghp_..."

# Auto-cleanup merged PRs
autoCleanupMerged = true

# Max age for closed PRs before cleanup (hours)
closedPrCleanupAfter = 24

# Require conventional commits
requireConventionalCommits = true

# CI nudge behavior
nudgeOnCiFailure = true
includeErrorLogs = true
errorLogMaxLines = 50

# Conflict nudge behavior
nudgeOnConflict = true

# Review nudge behavior
nudgeOnReview = true
includeInlineComments = true

[conventionalCommits]
# Allowed types
types = ["feat", "fix", "docs", "style", "refactor", "perf", "test", "chore", "ci"]

# Require scope (e.g., feat(auth):)
requireScope = false

# Allowed scopes (empty = any)
scopes = []

# Allow breaking changes (!)
allowBreaking = true
```

**Global fallback in `~/.config/jig/config`:**
```
github.token=ghp_...
github.autoCleanupMerged=true
conventionalCommits.types=feat,fix,docs,refactor,test,chore
```

## Commands

```bash
# Check PR status for worker
jig pr status <worker>

# List open PRs (current repo)
jig pr list

# List PRs across all registered repos
jig pr list -g

# Cleanup merged PRs
jig pr cleanup --merged
jig pr cleanup --merged -g  # all repos

# Force-sync PR state (bypass cache)
jig pr sync
jig pr sync -g

# Check conventional commits
jig pr check-commits <worker>
```

## Detection & Nudges

### CI Failure

**Detection:**
```rust
async fn check_ci_status(client: &Client, owner: &str, repo: &str, pr_num: u64) 
    -> Result<Vec<FailedCheck>> 
{
    let checks = client.checks()
        .list_for_ref(owner, repo, &format!("pull/{}/head", pr_num))
        .await?;
    
    checks.check_runs
        .into_iter()
        .filter(|c| c.conclusion == Some("FAILURE"))
        .map(|c| FailedCheck {
            name: c.name,
            conclusion: c.conclusion,
            details_url: c.details_url,
        })
        .collect()
}
```

**Nudge message:**
```
CI is failing on PR #42. Fix these issues:

• Lint & Format: clippy error at line 59
• Tests: 2 failing tests in auth module

Error log (last 50 lines):
error: unneeded `return` statement
  --> src/main.rs:59:17
   |
59 |                 return Ok(());
   |                 ^^^^^^^^^^^^^
   |
   = help: remove `return`

Fix the issues, commit using conventional commits (fix: ...), push, then call /review.
```

### Merge Conflicts

**Detection:**
```rust
async fn check_mergeable(client: &Client, owner: &str, repo: &str, pr_num: u64) 
    -> Result<bool> 
{
    let pr = client.pulls().get(owner, repo, pr_num).await?;
    Ok(pr.mergeable == Some(true))
}
```

**Nudge message:**
```
PR #42 has merge conflicts with main. Resolve them:

1. Fetch latest main: git fetch origin
2. Rebase on main: git rebase origin/main
3. Git will stop at each conflict - resolve them manually
   • Edit conflicted files, remove conflict markers (<<<<<<, >>>>>>)
   • Test that the code still works
4. After resolving each file: git add <file>
5. Continue: git rebase --continue
6. Repeat steps 3-5 until all conflicts are resolved
7. Force push: git push --force-with-lease
8. Call /review when conflicts are resolved

If the rebase is too complex, call /review and ask for human help.
```

### Review Comments

**Detection:**
```rust
async fn check_review_comments(client: &Client, owner: &str, repo: &str, pr_num: u64) 
    -> Result<ReviewComments> 
{
    let reviews = client.pulls().list_reviews(owner, repo, pr_num).await?;
    let inline = client.pulls().list_review_comments(owner, repo, pr_num).await?;
    
    let changes_requested = reviews.iter()
        .any(|r| r.state == Some("CHANGES_REQUESTED"));
    
    Ok(ReviewComments {
        changes_requested,
        inline_count: inline.len(),
        inline: inline.into_iter()
            .map(|c| InlineComment {
                path: c.path,
                line: c.line,
                body: c.body,
            })
            .collect(),
        general: reviews.into_iter()
            .filter(|r| !r.body.is_empty())
            .map(|r| r.body)
            .collect(),
    })
}
```

**Nudge message:**
```
PR #42 has unresolved review comments. Address all feedback:

Inline comments:
• src/auth.rs:42: Consider using a more descriptive variable name
• src/main.rs:15: This error handling could be improved

General review:
> Overall looks good, but please add tests for the new auth flow.
> Also, can you document the token refresh logic?

Fix the issues, push updates, then call /review to notify reviewers.
```

### Conventional Commits

**Detection:**
```rust
fn validate_commit_message(msg: &str, config: &ConventionalCommitsConfig) 
    -> Result<(), String> 
{
    let pattern = if config.require_scope {
        r"^(feat|fix|docs|style|refactor|perf|test|chore|ci)(\(.+\))!?: .+"
    } else {
        r"^(feat|fix|docs|style|refactor|perf|test|chore|ci)(\(.+\))?!?: .+"
    };
    
    let re = Regex::new(pattern).unwrap();
    
    if !re.is_match(msg) {
        return Err(format!("Commit message '{}' doesn't follow conventional commits", msg));
    }
    
    // Validate type
    if let Some(caps) = re.captures(msg) {
        let commit_type = &caps[1];
        if !config.types.contains(&commit_type.to_string()) {
            return Err(format!("Commit type '{}' not in allowed list", commit_type));
        }
    }
    
    Ok(())
}
```

**Nudge message:**
```
PR #42 has commits that don't follow conventional commit format:

• a3f2d1c: added auth feature
• 8b4e9a2: fix bug

Fix with interactive rebase:
  git rebase -i origin/main

For each bad commit, change 'pick' to 'reword' and update the message to:
  <type>(<scope>): <description>

Types: feat|fix|docs|style|refactor|perf|test|chore|ci
Example: feat(auth): add OAuth2 support
Breaking changes: feat!: remove legacy API

Then force push: git push --force-with-lease
Finally, call /review to request a review.
```

## PR Lifecycle

### Auto-Cleanup Merged PRs

**On `jig health` run:**

```rust
async fn cleanup_merged_prs(
    client: &Client, 
    config: &RepoConfig,
    workers: &[WorkerState]
) -> Result<Vec<String>> {
    let mut cleaned = Vec::new();
    
    for worker in workers {
        if let Some(pr) = get_pr_for_branch(client, config, &worker.branch).await? {
            if pr.merged_at.is_some() {
                // Kill worker
                tmux_kill_window(&worker.tmux_target)?;
                
                // Remove worktree
                remove_worktree(&worker.path)?;
                
                // Reset nudge counts
                clear_all_nudges(&worker.name)?;
                
                cleaned.push(format!("{} (PR #{})", worker.name, pr.number));
            }
        }
    }
    
    Ok(cleaned)
}
```

### Handle Closed PRs

**On `jig health` run:**

```rust
async fn handle_closed_prs(
    client: &Client,
    config: &RepoConfig,
    workers: &[WorkerState]
) -> Result<Vec<Alert>> {
    let mut alerts = Vec::new();
    
    for worker in workers {
        if let Some(pr) = get_pr_for_branch(client, config, &worker.branch).await? {
            if pr.state == "CLOSED" && pr.merged_at.is_none() {
                // Human closed the PR (not merged)
                let age_hours = pr.closed_age_hours();
                
                if config.github.auto_cleanup_closed && age_hours > config.github.closed_pr_cleanup_after {
                    // Old closed PR, safe to cleanup
                    tmux_kill_window(&worker.tmux_target)?;
                    remove_worktree(&worker.path)?;
                    clear_all_nudges(&worker.name)?;
                } else {
                    // Alert human
                    alerts.push(Alert::PrClosed {
                        worker: worker.name.clone(),
                        pr_number: pr.number,
                        closed_at: pr.closed_at,
                    });
                }
            }
        }
    }
    
    Ok(alerts)
}
```

## Integration with Heartbeat

**On `jig health` run:**

1. Check CI status for all open PRs
2. Check mergeable status (conflicts)
3. Check review comments
4. Validate commit messages
5. Cleanup merged/closed PRs
6. Nudge workers with issues (respecting nudge limits)

**Nudge priority:**
1. Closed PR (alert immediately, don't auto-nudge)
2. Merge conflicts (blocks merging)
3. CI failures (blocks merging)
4. Review comments (human waiting)
5. Conventional commits (nice-to-have)

## Implementation Phases

### Phase 0: Dependencies
- issues/features/global-commands.md
- issues/features/worker-heartbeat-system.md

### Phase 1: Core API
1. Add `octorust` dependency
2. GitHub client initialization (auth, config)
3. Basic PR fetching
4. Cache layer for PR state

### Phase 2: Detection
1. CI status checking
2. Mergeable status (conflicts)
3. Review comments parsing
4. Commit message validation
5. PR lifecycle (merged, closed)

### Phase 3: Nudging
1. Integrate with health system nudge state
2. CI failure nudges with error logs
3. Conflict resolution nudges
4. Review comment nudges with context
5. Conventional commit nudges with rebase instructions

### Phase 4: Lifecycle
1. Auto-cleanup merged PRs
2. Handle closed PRs gracefully
3. Alert on PR state changes

### Phase 5: Global Operations
1. Use GlobalContext for `-g` flag
2. `jig pr list -g` across all repos
3. Batch cleanup: `jig pr cleanup --merged -g`

## Acceptance Criteria

### Core
- [ ] octorust client initialized from config/env/gh CLI
- [ ] PR state cached in `.worktrees/.jig-github-cache.json`
- [ ] Cache TTL configurable, bypass with `jig pr sync`
- [ ] Auto-detect owner/repo from git remote
- [ ] Fallback to global token if per-repo not set

### Detection
- [ ] Detect CI failures with check names and details
- [ ] Fetch error logs from failed runs (truncate to configurable max)
- [ ] Detect merge conflicts via `mergeable` field
- [ ] Parse inline review comments (file, line, body)
- [ ] Parse general review comments
- [ ] Detect `CHANGES_REQUESTED` review decision
- [ ] Validate commits against conventional format
- [ ] Configurable allowed types and scopes

### Nudging
- [ ] CI failure nudges include check names + logs
- [ ] Conflict nudges include step-by-step rebase instructions
- [ ] Review nudges include inline + general comments
- [ ] Commit nudges include rebase instructions + format examples
- [ ] Nudge counts tracked per-type in health state
- [ ] Max nudges before escalation (configurable)

### Lifecycle
- [ ] Auto-cleanup merged PRs if enabled
- [ ] Handle closed PRs: alert or auto-cleanup after age threshold
- [ ] Remove worktree, kill tmux window, reset nudge counts

### Configuration
- [ ] Per-repo settings in `jig.toml` `[github]` section
- [ ] Conventional commit rules in `[conventionalCommits]`
- [ ] Global fallback in `~/.config/jig/config`
- [ ] Enable/disable individual nudge types

### Commands
- [ ] `jig pr status <worker>` shows PR details
- [ ] `jig pr list` lists open PRs (current repo)
- [ ] `jig pr list -g` lists across all registered repos
- [ ] `jig pr cleanup --merged` cleans up merged PRs
- [ ] `jig pr sync` bypasses cache, forces API fetch

## Testing

```bash
# Create PR with failing CI
jig spawn features/test
cd .worktrees/features/test
echo "bad code" >> src/main.rs
git commit -m "feat: add feature"
git push -u origin features/test
gh pr create --title "Test PR" --body "Test"

# Trigger health check
jig health

# Verify CI failure detected
cat .worktrees/.jig-github-cache.json | jq '.prs["features/test"].ci_status'

# Verify nudge sent
tmux capture-pane -p -t jig-myrepo:features/test | grep "CI is failing"

# Verify nudge count
cat .worktrees/.jig-health.json | jq '.workers["features/test"].nudges.ci_failure'
```

## Open Questions

1. Rate limiting: should we batch PR checks or check sequentially? (Batching with octorust's built-in retry)
2. Pre-commit hooks for conventional commits? (Yes, separate ticket)
3. Should nudges include links to CI runs? (Yes, extract from octorust check details)
4. Parallel API calls in `-g` mode? (Yes, use tokio::spawn for each repo)

## Related Issues

- issues/features/worker-heartbeat-system.md (nudge system integration)
- issues/features/smart-context-injection.md (nudge message templates)
- issues/improvements/worker-activity-metrics.md (PR status in metrics)
