pub mod nudge;
pub mod triage;

use jig_core::issues::Issue;
use jig_core::issues::IssueProvider;
use jig_core::prompt::Prompt;

const SPAWN_PREAMBLE: &str = r#"AUTONOMOUS MODE: You have been spawned by jig as a parallel worker in auto mode (--dangerously-skip-permissions). Work independently without human interaction.

YOUR GOAL: Complete the task below and create a draft PR. Definition of done: code committed (conventional commits), draft PR created via `jig pr` or /draft, and issue marked complete (see completion instructions in the task). Call /review when ready.

IMPORTANT: Create the draft PR using `jig pr` (or `/draft`, which wraps it). NEVER use `gh pr create` directly — it bypasses parent branch resolution and will target the wrong base branch.

HOW MONITORING WORKS: A daemon watches your activity via tool-use events. If you go idle or get stuck for ~5 minutes, you'll receive automated nudge messages. Do not wait for input.

IF YOU GET STUCK:
- Do NOT enter plan mode or ask for confirmation — just proceed
- If a command fails, try to fix it yourself
- If tests fail, debug and fix them
- If unsure about an approach, pick the simplest one and go
- If truly blocked, explain what's blocking you so the nudge system can relay it

TASK:
{{task_context}}
"#;

/// Build a fully composed spawn prompt from an issue.
pub fn spawn_task(issue: &Issue, provider: &IssueProvider) -> Prompt {
    let parent = issue.parent().and_then(|r| provider.get(r).ok().flatten());

    let parent_section = match &parent {
        Some(p) => format!(
            "PARENT ISSUE ({}): {}\n{}\n\n---\n\nSUB-TASK:\n",
            p.id(),
            p.title(),
            p.body()
        ),
        None => String::new(),
    };

    let task_context = format!(
        "{}{}\n\n{}\n\nISSUE COMPLETION: This issue is tracked by Linear. \
         Status sync is handled automatically — no manual status update is needed.",
        parent_section,
        issue.title(),
        issue.body(),
    );

    wrap_preamble(Prompt::new(&task_context))
}

/// Build a fully composed spawn prompt from raw task context.
pub fn spawn_task_raw(task_context: &str) -> Prompt {
    let task = Prompt::new(task_context);
    wrap_preamble(task)
}

/// Build a resume prompt (reuses the spawn preamble with the given context).
pub fn resume_task(task_context: &str) -> Prompt {
    wrap_preamble(Prompt::new(task_context))
}

fn wrap_preamble(task: Prompt) -> Prompt {
    let task_context = task.render().unwrap_or_default();
    Prompt::new(SPAWN_PREAMBLE)
        .var("task_context", &task_context)
}
