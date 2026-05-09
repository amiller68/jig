use jig_core::prompt::Prompt;

const TEMPLATE_IDLE: &str = r#"STATUS CHECK: You've been idle for a while (nudge {{nudge_count}}).

{{#if has_changes}}
You have uncommitted changes but no PR yet. What's blocking you?

1. If ready: commit (conventional format), push, create PR, update issue, call /review
2. If stuck: explain what you need help with
3. If complete but confused: finish the PR
{{else}}
No recent commits. What's the current state?

1. Still working? Give a brief status update and continue
2. Stuck on something? Explain what's blocking you
3. Done but forgot to create PR? Commit, push, create PR, call /review
{{/if}}
"#;

const TEMPLATE_STUCK: &str = r#"STUCK PROMPT DETECTED: You appear to be waiting at an interactive prompt.
Auto-approving... (nudge {{nudge_count}})
"#;

const TEMPLATE_CI: &str = r#"CI is failing on your PR (nudge {{nudge_count}}).

Fix these issues:
{{#each ci_failures}}
  - {{this}}
{{/each}}

STEPS:
1. Fix the failing checks
2. Commit using conventional commits: fix(ci): fix linting errors
3. Push to your branch: git push
4. Verify CI passes
5. Call /review when green
"#;

const TEMPLATE_CONFLICT: &str = r#"Your PR has merge conflicts with {{base_branch}} (nudge {{nudge_count}}).

Resolve them:

1. git fetch origin
2. git rebase {{base_branch}}
3. Resolve conflicts, stage files, git rebase --continue
4. git push --force-with-lease
5. Call /review when conflicts are resolved
"#;

const TEMPLATE_REVIEW: &str = r#"Your PR has unresolved review comments (nudge {{nudge_count}}).

Address all feedback, commit, push, and call /review.
"#;

const TEMPLATE_BAD_COMMITS: &str = r#"Your PR has commits that don't follow conventional commit format (nudge {{nudge_count}}).

Bad commits:
{{#each bad_commits}}
  - {{this}}
{{/each}}

Fix with interactive rebase:

1. git rebase -i {{base_branch}}
2. Change 'pick' to 'reword' for each bad commit
3. Update message to: <type>(<scope>): <description>
   Types: feat|fix|docs|style|refactor|perf|test|chore|ci
4. git push --force-with-lease
5. Call /review
"#;

pub fn idle(count: u32, has_changes: bool) -> Prompt {
    Prompt::new(TEMPLATE_IDLE)
        .named("idle")
        .var_num("nudge_count", count)
        .var_bool("has_changes", has_changes)
}

pub fn stuck(count: u32) -> Prompt {
    Prompt::new(TEMPLATE_STUCK)
        .named("stuck")
        .var_num("nudge_count", count)
}

pub fn ci(count: u32, failures: Vec<String>) -> Prompt {
    Prompt::new(TEMPLATE_CI)
        .named("ci")
        .var_num("nudge_count", count)
        .var_list("ci_failures", failures)
}

pub fn conflict(count: u32, base_branch: &str) -> Prompt {
    Prompt::new(TEMPLATE_CONFLICT)
        .named("conflict")
        .var_num("nudge_count", count)
        .var("base_branch", base_branch)
}

pub fn review(count: u32) -> Prompt {
    Prompt::new(TEMPLATE_REVIEW)
        .named("review")
        .var_num("nudge_count", count)
}

pub fn bad_commits(count: u32, commits: Vec<String>, base_branch: &str) -> Prompt {
    Prompt::new(TEMPLATE_BAD_COMMITS)
        .named("bad_commits")
        .var_num("nudge_count", count)
        .var_list("bad_commits", commits)
        .var("base_branch", base_branch)
}

pub fn nudge_key_for_check(check_name: &str) -> &str {
    match check_name {
        "ci" => "ci",
        "conflicts" => "conflict",
        "reviews" => "review",
        "commits" => "bad_commits",
        _ => check_name,
    }
}

pub fn for_check(check_name: &str, count: u32, base_branch: &str) -> Option<Prompt> {
    match check_name {
        "ci" => Some(ci(count, Vec::new())),
        "conflicts" => Some(conflict(count, base_branch)),
        "reviews" => Some(review(count)),
        "commits" => Some(bad_commits(count, Vec::new(), base_branch)),
        _ => None,
    }
}
