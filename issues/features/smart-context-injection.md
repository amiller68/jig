# Smart Context Injection for Workers

**Status:** Planned  
**Priority:** Medium  
**Category:** Features

## Objective

Standardize and improve the context/prompts that workers receive when spawned, resumed, or nudged. Make the instructions clear, actionable, and consistent.

## Background

The issue-grinder has evolved sophisticated prompts for different scenarios:
- New worker spawn (issue context)
- Resume after exit (with/without uncommitted changes)
- Nudge for CI failures (with error logs)
- Nudge for review comments (with inline comments)
- Nudge for conflicts (with rebase instructions)
- Nudge for idle/stuck workers (status check)

These prompts are currently hardcoded in the bash script. They should be:
1. Templated and configurable
2. Part of jig's core logic
3. Contextual based on worker state

## Acceptance Criteria

### Context Templates
- [ ] Define template system for worker context
- [ ] Variables available: `{worker_name}`, `{issue_path}`, `{pr_number}`, `{repo}`, `{branch}`, etc.
- [ ] Store templates in `~/.config/jig/templates/`
- [ ] Defaults shipped with jig, user can override

### Spawn Context
- [ ] Template: `spawn-new-issue.md`
- [ ] Includes:
  - Full issue content
  - Workflow reminders (update issue, conventional commits, create PR, call /review)
  - Rebase instructions if conflicts occur
- [ ] Auto-inject on `jig spawn`

### Resume Context
- [ ] Template: `resume-exited.md`
- [ ] Variables: `{has_changes}`, `{commit_count}`, `{status}`
- [ ] Different messages for:
  - Clean worktree (no changes)
  - Uncommitted changes (remind to commit)
  - Already has commits (remind to create PR)

### Nudge Context
- [ ] Template: `nudge-idle.md` - idle worker at prompt
- [ ] Template: `nudge-stuck.md` - stuck at interactive prompt
- [ ] Template: `nudge-ci-failure.md` - CI failures (includes error logs)
- [ ] Template: `nudge-conflict.md` - merge conflicts (rebase instructions)
- [ ] Template: `nudge-review.md` - unresolved review comments (includes comments)
- [ ] Template: `nudge-bad-commits.md` - non-conventional commits (rebase instructions)

### Smart Variable Injection
- [ ] Detect worker state automatically
- [ ] Inject relevant variables:
  - `{ci_failures}` - list of failing checks
  - `{error_logs}` - truncated error output
  - `{review_comments}` - inline and general comments
  - `{conflict_files}` - list of files with conflicts
  - `{bad_commits}` - list of non-conventional commits
- [ ] Truncate long outputs (e.g., max 50 lines of logs)

### Configuration
- [ ] `jig config get templates.spawn` - show current template
- [ ] `jig config set templates.spawn "path/to/custom.md"` - override
- [ ] `jig config reset templates` - restore defaults

## Template Examples

**spawn-new-issue.md:**
```markdown
{issue_content}

---
IMPORTANT: When you start work, update the issue:
  sed -i 's/Status: Planned/Status: In Progress/' issues/{issue_path}
  echo -e "\n## Progress Log\n\n### $(date +%Y-%m-%d) - Started\n- Beginning implementation" >> issues/{issue_path}
  git add issues/{issue_path} && git commit -m 'chore: mark {worker_name} in progress' && git push origin main

When finished, you MUST:
1. Update issue with final summary and set to In Review
2. Commit using CONVENTIONAL COMMITS format (required for releases)
3. Push: git push -u origin {worker_name}
4. Create PR: gh pr create --title "<title>" --body "Addresses: issues/{issue_path}\n\n<description>"
5. Call /review to request a review

If you get review feedback later, address ALL comments thoroughly.
```

**nudge-ci-failure.md:**
```markdown
CI is failing on PR #{pr_number}. Fix these issues:

{ci_failures}

{error_logs}

Fix the issues, commit using conventional commits (fix: ...), push, then call /review.
```

**nudge-conflict.md:**
```markdown
PR #{pr_number} has merge conflicts with main. Resolve them:

1. Fetch latest main: git fetch origin
2. Rebase on main: git rebase origin/main
3. Git will stop at each conflict - resolve them manually
4. After resolving each file: git add <file>
5. Continue: git rebase --continue
6. Force push: git push --force-with-lease
7. Call /review when resolved

If the rebase is too complex, ask for human help.
```

## Implementation Notes

1. Add `templates/` directory to jig's config dir
2. Ship default templates with jig
3. Template engine: handlebars or tera
4. Inject templates into worker spawn/nudge commands
5. Update heartbeat system to use templates

## Related Issues

- #TBD: Worker heartbeat system
- #TBD: GitHub integration
- #TBD: Worker lifecycle management

## References

- Current prompts: `~/.openclaw/workspace/skills/issue-grinder/grind.sh`
  - Spawn: lines 554-580
  - Resume: lines 436-486
  - CI nudge: lines 629-640
  - Conflict nudge: lines 707-726
  - Review nudge: lines 798-822
  - Commit nudge: lines 873-887
