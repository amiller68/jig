//! PR lifecycle monitoring — checks merged/closed/open PRs and injects actions.


pub(super) const TEMPLATE_CI: &str = r#"CI is failing on your PR (nudge {{nudge_count}}/{{max_nudges}}).

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

pub(super) const TEMPLATE_CONFLICT: &str = r#"Your PR has merge conflicts with {{base_branch}} (nudge {{nudge_count}}/{{max_nudges}}).

Resolve them:

1. git fetch origin
2. git rebase {{base_branch}}
3. Resolve conflicts, stage files, git rebase --continue
4. git push --force-with-lease
5. Call /review when conflicts are resolved
"#;

pub(super) const TEMPLATE_REVIEW: &str = r#"Your PR has unresolved review comments (nudge {{nudge_count}}/{{max_nudges}}).

Address all feedback, commit, push, and call /review.
"#;

pub(super) const TEMPLATE_BAD_COMMITS: &str = r#"Your PR has commits that don't follow conventional commit format (nudge {{nudge_count}}/{{max_nudges}}).

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

pub(super) const TEMPLATE_AUTO_REVIEW: &str = r#"AUTOMATED REVIEW: Your code has been reviewed (round {{review_round}}).

Verdict: CHANGES REQUESTED

Read the review at: .jig/reviews/{{review_file}}

Address each finding, then respond:
1. Read: cat .jig/reviews/{{review_file}}
2. Fix issues or prepare explanations for disputes
3. Respond: pipe your response to jig review respond --review {{review_number}}
4. Commit and push — the next review cycle triggers automatically on push

{{#if is_final_round}}
WARNING: This is round {{review_round}} of {{max_rounds}}. If not approved after this round, a human will be notified.
{{/if}}
"#;

/// Nudge key for each check type.
pub(super) fn nudge_key_for_check(check_name: &str) -> &str {
    match check_name {
        "ci" => "ci",
        "conflicts" => "conflict",
        "reviews" => "review",
        "commits" => "bad_commits",
        _ => check_name,
    }
}

/// Template for each check type.
pub(super) fn template_for_check(check_name: &str) -> &'static str {
    match check_name {
        "ci" => TEMPLATE_CI,
        "conflicts" => TEMPLATE_CONFLICT,
        "reviews" => TEMPLATE_REVIEW,
        "commits" => TEMPLATE_BAD_COMMITS,
        _ => "",
    }
}

