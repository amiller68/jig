# Smart Context Injection for Workers

**Status:** Planned  
**Priority:** Medium  
**Category:** Features  
**Depends-On:** issues/features/worker-heartbeat-system.md, issues/features/github-integration.md

## Objective

Standardize and templatize the context/prompts that workers receive when spawned, resumed, or nudged. Replace hardcoded prompt strings with a flexible template system that supports repo-specific customization and variable injection.

## Architecture

### Template Engine: Handlebars

**Why Handlebars:**
- Simple, well-tested syntax (`{{variable}}`)
- Conditionals and loops built-in
- Native Rust implementation (handlebars-rust)
- User-friendly for non-programmers

### Template Hierarchy

**Lookup order:**
1. **Repo-specific:** `<repo>/.jig/templates/<name>.hbs`
2. **Global user:** `~/.config/jig/templates/<name>.hbs`
3. **Built-in:** Embedded in jig binary (fallback)

**Example:** For `spawn-new-issue.hbs`, jig checks:
```
./jig/templates/spawn-new-issue.hbs   # repo override
~/.config/jig/templates/spawn-new-issue.hbs  # user default
[built-in]  # shipped with jig
```

### Template Variables

**Common variables (all templates):**
```rust
struct CommonContext {
    worker_name: String,      // e.g., "features/auth"
    issue_path: String,       // e.g., "issues/features/auth.md"
    repo: String,             // e.g., "jig"
    branch: String,           // e.g., "features/auth"
    base_branch: String,      // e.g., "origin/main"
    timestamp: String,        // ISO 8601
}
```

**Spawn-specific:**
```rust
struct SpawnContext {
    common: CommonContext,
    issue_content: String,    // Full issue markdown
    issue_title: String,      // First heading
    issue_priority: String,   // "High", "Medium", etc.
    issue_category: String,   // "Features", "Bugs", etc.
}
```

**Resume-specific:**
```rust
struct ResumeContext {
    common: CommonContext,
    has_changes: bool,        // Uncommitted changes exist
    commit_count: u32,        // Commits on branch
    status: String,           // "exited", "no-window"
    changed_files: Vec<String>, // List of modified files
}
```

**Nudge-specific:**
```rust
struct NudgeContext {
    common: CommonContext,
    nudge_type: String,       // "idle", "ci_failure", "conflict", etc.
    nudge_count: u32,         // How many times nudged
    max_nudges: u32,          // Escalation threshold
    
    // Type-specific fields (optional)
    ci_failures: Option<Vec<String>>,
    error_logs: Option<String>,
    review_comments: Option<Vec<Comment>>,
    conflict_files: Option<Vec<String>>,
    bad_commits: Option<Vec<Commit>>,
}
```

## Configuration

**Per-repo in `jig.toml`:**

```toml
[templates]
# Override specific templates (relative to repo root or absolute)
spawn = ".jig/templates/spawn.hbs"
resumeClean = ".jig/templates/resume-clean.hbs"
resumeDirty = ".jig/templates/resume-dirty.hbs"
nudgeIdle = ".jig/templates/nudge-idle.hbs"
nudgeStuck = ".jig/templates/nudge-stuck.hbs"
nudgeCI = ".jig/templates/nudge-ci.hbs"
nudgeConflict = ".jig/templates/nudge-conflict.hbs"
nudgeReview = ".jig/templates/nudge-review.hbs"
nudgeCommits = ".jig/templates/nudge-commits.hbs"

# Template variables (injected into all templates)
[templates.vars]
teamSlack = "#dev-team"
docsUrl = "https://docs.example.com/workflow"
supportEmail = "dev@example.com"
```

**Template access to custom vars:**
```handlebars
Need help? Reach out in {{team_slack}} or email {{support_email}}.
```

## Built-in Templates

### spawn-new-issue.hbs

```handlebars
{{issue_content}}

---
## Workflow Instructions

When you start work, update the issue status:
  sed -i 's/Status: Planned/Status: In Progress/' {{issue_path}}
  echo -e "\n## Progress Log\n\n### $(date +%Y-%m-%d) - Started\n- Beginning implementation" >> {{issue_path}}
  git add {{issue_path}} && git commit -m 'chore: mark {{worker_name}} in progress' && git push {{base_branch}}

As you work, document key decisions in the issue:
  echo "- [your decision or finding]" >> {{issue_path}}
  git add {{issue_path}} && git commit -m 'docs: update {{worker_name}} progress' && git push {{base_branch}}

When finished, you MUST:
1. Update issue with summary and set to In Review:
   echo -e "\n### $(date +%Y-%m-%d) - Ready for Review\n- Implementation complete\n- [summarize changes]" >> {{issue_path}}
   sed -i 's/Status: In Progress/Status: In Review/' {{issue_path}}
   git add {{issue_path}} && git commit -m 'chore: mark {{worker_name}} in review' && git push {{base_branch}}

2. Commit using CONVENTIONAL COMMITS (required for releases):
   - feat: new feature (minor bump)
   - fix: bug fix (patch bump)
   - docs/refactor/test/chore: other changes
   - Breaking changes: feat!: or fix!:
   Example: feat(auth): add OAuth2 support

3. Push your branch:
   git push -u {{base_branch}} {{branch}}

4. Create PR with issue reference:
   gh pr create --title "{{issue_title}}" --body "Addresses: {{issue_path}}\n\n[description]"

5. Update issue with PR number:
   echo "- PR: #<number>" >> {{issue_path}} && git add {{issue_path}} && git commit -m 'docs: add PR link' && git push {{base_branch}}

6. Call /review to request a review of your work

If you hit merge conflicts:
  git fetch origin && git rebase {{base_branch}}
```

### resume-dirty.hbs

```handlebars
RESUME: You were working on {{worker_name}} but stopped without creating a PR.

Branch: {{branch}}
Status: {{status}}
Uncommitted changes:
{{#each changed_files}}
  • {{this}}
{{/each}}

STEPS:
1. Review your changes carefully

2. Update issue with summary:
   echo -e "\n### $(date +%Y-%m-%d) - Ready for Review\n- [what you did]" >> {{issue_path}}
   sed -i 's/Status: In Progress/Status: In Review/' {{issue_path}}
   git add {{issue_path}} && git commit -m 'chore: mark {{worker_name}} in review' && git push {{base_branch}}

3. Commit using CONVENTIONAL COMMITS:
   Example: feat(auth): add login endpoint

4. Push: git push -u {{base_branch}} {{branch}}

5. Create PR:
   gh pr create --title "..." --body "Addresses: {{issue_path}}\n\n..."

6. Update issue with PR number and push to {{base_branch}}

7. Call /review

If you hit conflicts: git fetch origin && git rebase {{base_branch}}
```

### nudge-idle.hbs

```handlebars
STATUS CHECK: You've been idle for a while ({{nudge_count}}/{{max_nudges}} nudges).

{{#if has_changes}}
You have uncommitted changes but no PR yet. What's blocking you?

1. If ready: commit (conventional format), push, create PR, update issue, call /review
2. If stuck: explain what you need help with
3. If complete but confused: review the workflow above and finish the PR
{{else}}
No commits yet, no PR. What's the current state?

1. Still working? Give a brief status update and continue
2. Stuck on something? Explain what's blocking you
3. Done but forgot to create PR? Commit your work, push, create PR, call /review
{{/if}}

{{#if (eq nudge_count max_nudges)}}
⚠️  This is your final nudge. If you need human help, call /review now.
{{/if}}
```

### nudge-ci.hbs

```handlebars
CI is failing on PR #{{pr_number}} (nudge {{nudge_count}}/{{max_nudges}}).

Fix these issues:
{{#each ci_failures}}
  • {{this}}
{{/each}}

{{#if error_logs}}
Error log (last 50 lines):
```
{{error_logs}}
```
{{/if}}

STEPS:
1. Fix the failing checks
2. Commit using conventional commits: fix(ci): fix linting errors
3. Push to your branch: git push
4. Verify CI passes on GitHub
5. Call /review when green

{{#if (gte nudge_count 2)}}
⚠️  CI has been failing for a while. If you're stuck, call /review and ask for help.
{{/if}}
```

### nudge-conflict.hbs

```handlebars
PR #{{pr_number}} has merge conflicts with {{base_branch}} (nudge {{nudge_count}}/{{max_nudges}}).

Resolve them:

1. Fetch latest {{base_branch}}:
   git fetch origin

2. Rebase on {{base_branch}}:
   git rebase {{base_branch}}

3. Git will stop at each conflict. For each:
   • Edit the conflicted files
   • Remove conflict markers (<<<<<<, ======, >>>>>>)
   • Test that the code still works
   • Stage the resolved file: git add <file>

4. Continue the rebase:
   git rebase --continue

5. Repeat steps 3-4 until all conflicts are resolved

6. Force push (safe, you're on your branch):
   git push --force-with-lease

7. Verify the PR is mergeable on GitHub

8. Call /review when conflicts are resolved

{{#if (gte nudge_count 2)}}
⚠️  Conflicts are complex? Call /review and ask for human help with the rebase.
{{/if}}
```

### nudge-review.hbs

```handlebars
PR #{{pr_number}} has unresolved review comments (nudge {{nudge_count}}/{{max_nudges}}).

Address all feedback:

{{#if inline_comments}}
Inline comments:
{{#each inline_comments}}
  • {{path}}:{{line}}: {{body}}
{{/each}}
{{/if}}

{{#if general_comments}}
General review feedback:
{{#each general_comments}}
> {{this}}
{{/each}}
{{/if}}

STEPS:
1. Address each comment thoroughly
2. Make the requested changes
3. Commit with conventional format: fix: address review feedback
4. Push: git push
5. Reply to each comment on GitHub explaining what you changed
6. Call /review to notify reviewers

{{#if (gte nudge_count 2)}}
⚠️  Don't ignore review feedback. If you disagree with a comment, discuss it on GitHub before proceeding.
{{/if}}
```

### nudge-commits.hbs

```handlebars
PR #{{pr_number}} has commits that don't follow conventional commit format (nudge {{nudge_count}}/{{max_nudges}}).

Bad commits:
{{#each bad_commits}}
  • {{hash}}: {{message}}
{{/each}}

Fix with interactive rebase:

1. Start rebase:
   git rebase -i {{base_branch}}

2. For each bad commit, change 'pick' to 'reword'

3. Update the commit message to:
   <type>(<scope>): <description>

   Types: feat|fix|docs|style|refactor|perf|test|chore|ci
   Example: feat(auth): add OAuth2 support
   Breaking changes: feat!: remove legacy API

4. Save and close the editor

5. Git will open an editor for each commit you marked 'reword'
   Update the message and save

6. Force push:
   git push --force-with-lease

7. Call /review

{{#if (gte nudge_count 2)}}
⚠️  Conventional commits are required for automated releases. Please fix them.
{{/if}}
```

## Template Development Workflow

### Creating Custom Templates

**1. Copy built-in to user config:**
```bash
mkdir -p ~/.config/jig/templates
jig template export spawn-new-issue > ~/.config/jig/templates/spawn-new-issue.hbs
```

**2. Edit template:**
```bash
$EDITOR ~/.config/jig/templates/spawn-new-issue.hbs
```

**3. Test template:**
```bash
jig template test spawn-new-issue --worker features/test
```

**4. Deploy to repo (optional):**
```bash
mkdir -p .jig/templates
cp ~/.config/jig/templates/spawn-new-issue.hbs .jig/templates/
git add .jig/templates/ && git commit -m "chore: add custom spawn template"
```

### Repo-Specific Customization

**Example: Add team-specific workflow to spawn template**

`.jig/templates/spawn-new-issue.hbs`:
```handlebars
{{issue_content}}

---
## Our Team's Workflow

1. Update Jira ticket: PROJ-{{issue_number}}
2. Post in #engineering when PR is ready
3. Assign to team lead for review
4. [rest of standard workflow...]
```

**Configure in `jig.toml`:**
```toml
[templates]
spawn = ".jig/templates/spawn-new-issue.hbs"

[templates.vars]
issueNumber = "1234"  # Could be auto-detected from issue frontmatter
teamChannel = "#engineering"
```

## Commands

```bash
# List available templates
jig template list

# Show current template (with hierarchy)
jig template show spawn-new-issue

# Export built-in template to stdout
jig template export spawn-new-issue

# Test template rendering with sample data
jig template test spawn-new-issue --worker features/test

# Validate template syntax
jig template validate .jig/templates/spawn-new-issue.hbs

# Reset to built-in (delete user/repo overrides)
jig template reset spawn-new-issue
jig template reset --all
```

## Implementation Phases

### Phase 1: Core Template System
1. Add `handlebars` dependency
2. Embed built-in templates in binary (include_str!)
3. Template loader with hierarchy (repo > user > built-in)
4. Variable injection structs

### Phase 2: Integration
1. Update spawn command to use templates
2. Update health system to use templates for nudges
3. Update GitHub integration to use templates
4. Pass context structs to template renderer

### Phase 3: CLI Tools
1. `jig template list/show/export`
2. `jig template test` with mock data
3. `jig template validate`
4. Template syntax error reporting

### Phase 4: Advanced Features
1. Custom variables from `jig.toml`
2. Helpers for common operations (truncate, format dates, etc.)
3. Conditional sections (if/unless/each)
4. Template includes/partials

## Acceptance Criteria

### Core
- [ ] Handlebars template engine integrated
- [ ] Built-in templates embedded in binary
- [ ] Template lookup: repo > user > built-in
- [ ] All context variables documented

### Templates
- [ ] spawn-new-issue.hbs
- [ ] resume-clean.hbs
- [ ] resume-dirty.hbs
- [ ] nudge-idle.hbs
- [ ] nudge-stuck.hbs
- [ ] nudge-ci.hbs
- [ ] nudge-conflict.hbs
- [ ] nudge-review.hbs
- [ ] nudge-commits.hbs

### Integration
- [ ] `jig spawn` uses spawn template
- [ ] `jig health` uses resume templates
- [ ] `jig health` uses nudge templates
- [ ] GitHub integration uses CI/conflict/review templates
- [ ] Context variables populated from worker/PR/issue state

### Configuration
- [ ] Per-repo template overrides in `jig.toml`
- [ ] Custom variables in `[templates.vars]`
- [ ] Global user templates in `~/.config/jig/templates/`

### CLI
- [ ] `jig template list` shows available templates
- [ ] `jig template show <name>` shows effective template (with source)
- [ ] `jig template export <name>` outputs built-in
- [ ] `jig template test <name>` renders with sample data
- [ ] `jig template validate <path>` checks syntax

## Testing

```bash
# Export built-in
jig template export spawn-new-issue > test.hbs

# Validate syntax
jig template validate test.hbs

# Test rendering
jig template test spawn-new-issue --worker features/test

# Create repo override
mkdir -p .jig/templates
echo "CUSTOM SPAWN TEMPLATE" > .jig/templates/spawn-new-issue.hbs

# Verify override is used
jig template show spawn-new-issue  # should show custom template

# Spawn worker and verify context
jig spawn features/test
tmux capture-pane -p -t jig-myrepo:features/test | grep "CUSTOM SPAWN TEMPLATE"
```

## Open Questions

1. Should templates support includes/partials? (e.g., shared workflow steps)
2. Should we support multiple template engines? (Handlebars is sufficient)
3. Should templates be able to execute shell commands? (No, security risk)
4. Should we version templates? (e.g., built-in v1, v2) (No, just update built-ins)

## Related Issues

- issues/features/worker-heartbeat-system.md (uses nudge templates)
- issues/features/github-integration.md (uses CI/conflict/review templates)
