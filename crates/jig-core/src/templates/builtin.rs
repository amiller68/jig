//! Built-in templates embedded in the binary.

/// Template names and their built-in content.
pub const BUILTIN_TEMPLATES: &[(&str, &str)] = &[
    ("spawn-preamble", SPAWN_PREAMBLE),
    ("triage-prompt", TRIAGE_PROMPT),
    ("nudge-idle", NUDGE_IDLE),
    ("nudge-stuck", NUDGE_STUCK),
    ("nudge-ci", NUDGE_CI),
    ("nudge-conflict", NUDGE_CONFLICT),
    ("nudge-review", NUDGE_REVIEW),
    ("nudge-bad-commits", NUDGE_BAD_COMMITS),
    ("nudge-auto-review", NUDGE_AUTO_REVIEW),
];

const SPAWN_PREAMBLE: &str = r#"AUTONOMOUS MODE: You have been spawned by jig as a parallel worker in auto mode (--dangerously-skip-permissions). Work independently without human interaction.

YOUR GOAL: Complete the task below and create a draft PR. Definition of done: code committed (conventional commits), draft PR created via `jig pr` or /draft, and issue marked complete (see completion instructions in the task). Call /review when ready.

HOW MONITORING WORKS: A daemon watches your activity via tool-use events. If you go idle or get stuck for ~5 minutes, you'll receive automated nudge messages (up to {{max_nudges}}). After that, a human is notified. Do not wait for input.

IF YOU GET STUCK:
- Do NOT enter plan mode or ask for confirmation — just proceed
- If a command fails, try to fix it yourself
- If tests fail, debug and fix them
- If unsure about an approach, pick the simplest one and go
- If truly blocked, explain what's blocking you so the nudge system can relay it

AUTOMATED REVIEW: After you create a draft PR, an automated review agent may review your code. If it requests changes, you'll receive a nudge with the path to a review file (e.g. .jig/reviews/001.md). When that happens:

1. Read the review file to see the findings
2. Address each finding — fix issues or prepare explanations
3. Submit your response: jig review respond --review <N> (pipe your response markdown to stdin)
4. Commit and push your changes
5. The next review cycle triggers automatically on push

Response format (pipe to jig review respond --review N):

# Response to Review NNN

## Addressed
- `file:line` — finding description: what you did to fix it

## Disputed
- `file:line` — finding description: why you disagree

## Deferred
- `file:line` — finding description: why this is out of scope

## Notes
Any additional context.

TASK:
{{task_context}}
"#;

const TRIAGE_PROMPT: &str = r#"You are triaging issue {{issue_id}}: {{issue_title}}

## Issue Description

{{issue_body}}

## Your Task

Investigate this issue in the codebase and produce a scoped analysis. Do NOT implement any changes -- you are read-only.

1. **Identify affected code** -- find the relevant files, functions, and modules
2. **Assess scope** -- is this a small fix, a medium refactor, or a large feature?
3. **Propose approach** -- outline what an implementing agent (or human) would need to do
4. **Flag risks** -- note any dependencies, breaking changes, or areas needing careful handling
5. **Suggest priority** -- based on severity and scope, suggest Urgent/High/Medium/Low

## Output

When you have completed your investigation, update the Linear issue with your findings using the jig CLI, then change the issue status to Backlog.

Run: `jig issues update {{issue_id}} --body "your investigation findings"`
Then: `jig issues status {{issue_id}} backlog`

Structure your findings as:

### Investigation
[Your findings about affected code, scope, and approach]

### Affected Files
- `path/to/file.rs` -- reason

### Proposed Approach
1. Step one
2. Step two

### Complexity
[Small | Medium | Large]

### Suggested Priority
[Urgent | High | Medium | Low]

### Risks
- [Any risks or concerns]
"#;

const NUDGE_IDLE: &str = r#"STATUS CHECK: You've been idle for a while (nudge {{nudge_count}}/{{max_nudges}}).

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

{{#if is_final_nudge}}
This is your final nudge. If you need human help, say so now.
{{/if}}
"#;

const NUDGE_STUCK: &str = r#"STUCK PROMPT DETECTED: You appear to be waiting at an interactive prompt.
Auto-approving... (nudge {{nudge_count}}/{{max_nudges}})
"#;

const NUDGE_CI: &str = r#"CI is failing on your PR (nudge {{nudge_count}}/{{max_nudges}}).

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

const NUDGE_CONFLICT: &str = r#"Your PR has merge conflicts with {{base_branch}} (nudge {{nudge_count}}/{{max_nudges}}).

Resolve them:

1. git fetch origin
2. git rebase {{base_branch}}
3. Resolve conflicts, stage files, git rebase --continue
4. git push --force-with-lease
5. Call /review when conflicts are resolved
"#;

const NUDGE_REVIEW: &str = r#"Your PR has unresolved review comments (nudge {{nudge_count}}/{{max_nudges}}).

Address all feedback, commit, push, and call /review.
"#;

const NUDGE_AUTO_REVIEW: &str = r#"AUTOMATED REVIEW: Your code has been reviewed (round {{review_round}}).

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

const NUDGE_BAD_COMMITS: &str = r#"Your PR has commits that don't follow conventional commit format (nudge {{nudge_count}}/{{max_nudges}}).

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
