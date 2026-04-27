//! Dispatch rules — pure functions mapping state transitions to actions.

use jig_core::config::ResolvedNudgeConfig;
use jig_core::prompt::Prompt;
use jig_core::worker::events::WorkerState;
use jig_core::worker::WorkerStatus;

use super::{Action, NotifyKind};

const TEMPLATE_IDLE: &str = r#"STATUS CHECK: You've been idle for a while (nudge {{nudge_count}}/{{max_nudges}}).

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

const TEMPLATE_STUCK: &str = r#"STUCK PROMPT DETECTED: You appear to be waiting at an interactive prompt.
Auto-approving... (nudge {{nudge_count}}/{{max_nudges}})
"#;

/// Given an old and new worker state, return actions to execute.
///
/// The `resolve` closure maps a nudge type key to its resolved config,
/// enabling per-repo and per-type overrides.
pub fn dispatch_actions<F>(
    worker_id: &str,
    old_state: &WorkerState,
    new_state: &WorkerState,
    resolve: F,
) -> Vec<Action>
where
    F: Fn(&str) -> ResolvedNudgeConfig,
{
    let mut actions = vec![];
    let is_transition = old_state.status != new_state.status;

    if !new_state.status.is_terminal() && new_state.pr_url.is_none() {
        let (nudge_key, template, is_stuck) = match new_state.status {
            WorkerStatus::WaitingInput => ("stuck", TEMPLATE_STUCK, true),
            WorkerStatus::Stalled | WorkerStatus::Idle => ("idle", TEMPLATE_IDLE, false),
            _ => ("", "", false),
        };

        if !nudge_key.is_empty() {
            let resolved = resolve(nudge_key);
            let count = new_state
                .nudge_counts
                .get(nudge_key)
                .copied()
                .unwrap_or(0);

            let cooldown_ok = match new_state.last_nudge_at.get(nudge_key) {
                Some(&last_ts) => {
                    let elapsed = chrono::Utc::now().timestamp() - last_ts;
                    elapsed >= resolved.cooldown_seconds as i64
                }
                None => true,
            };

            if count < resolved.max && cooldown_ok {
                let mut prompt = Prompt::new(template)
                    .named(nudge_key)
                    .var_num("nudge_count", count + 1)
                    .var_num("max_nudges", resolved.max)
                    .var_bool("is_final_nudge", count + 1 >= resolved.max);

                if nudge_key == "idle" {
                    prompt = prompt.var_bool("has_changes", new_state.commit_count > 0);
                }

                if let Ok(message) = prompt.render() {
                    actions.push(Action::Nudge {
                        worker_id: worker_id.to_string(),
                        message,
                        nudge_key: nudge_key.to_string(),
                        is_stuck,
                        is_pr_nudge: false,
                    });
                }
            } else if is_transition {
                actions.push(Action::Notify {
                    worker_id: worker_id.to_string(),
                    message: format!(
                        "Max nudges reached for {} worker, needs human attention",
                        match new_state.status {
                            WorkerStatus::WaitingInput => "stuck",
                            WorkerStatus::Stalled => "stalled",
                            WorkerStatus::Idle => "idle",
                            _ => "unknown",
                        }
                    ),
                    kind: NotifyKind::NeedsIntervention,
                });
            }
        }
    }

    tracing::debug!(
        worker = worker_id,
        transition = is_transition,
        action_count = actions.len(),
        "dispatch_actions"
    );

    // PR opened
    if old_state.pr_url.is_none() && new_state.pr_url.is_some() {
        let pr_url = new_state.pr_url.clone().unwrap_or_default();
        actions.push(Action::Notify {
            worker_id: worker_id.to_string(),
            message: format!("PR opened: {}", &pr_url),
            kind: NotifyKind::PrOpened { pr_url },
        });
    }

    // Transition to Failed
    if old_state.status != WorkerStatus::Failed && new_state.status == WorkerStatus::Failed {
        actions.push(Action::Notify {
            worker_id: worker_id.to_string(),
            message: "Worker failed".to_string(),
            kind: NotifyKind::NeedsIntervention,
        });
    }

    actions
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_resolve(key: &str) -> ResolvedNudgeConfig {
        let _ = key;
        ResolvedNudgeConfig {
            max: 3,
            cooldown_seconds: 300,
        }
    }

    #[test]
    fn waiting_input_triggers_nudge() {
        let old = WorkerState {
            status: WorkerStatus::Running,
            ..Default::default()
        };
        let new = WorkerState {
            status: WorkerStatus::WaitingInput,
            ..Default::default()
        };

        let actions = dispatch_actions("test", &old, &new, default_resolve);
        assert_eq!(actions.len(), 1);
        assert!(matches!(
            &actions[0],
            Action::Nudge {
                nudge_key,
                is_stuck: true,
                ..
            } if nudge_key == "stuck"
        ));
    }

    #[test]
    fn max_nudges_triggers_notify() {
        let old = WorkerState {
            status: WorkerStatus::Running,
            ..Default::default()
        };
        let mut new = WorkerState {
            status: WorkerStatus::WaitingInput,
            ..Default::default()
        };
        new.nudge_counts.insert("stuck".to_string(), 3);

        let actions = dispatch_actions("test", &old, &new, default_resolve);
        assert_eq!(actions.len(), 1);
        assert!(
            matches!(&actions[0], Action::Notify { message, kind: NotifyKind::NeedsIntervention, .. } if message.contains("Max nudges"))
        );
    }

    #[test]
    fn stalled_triggers_nudge() {
        let old = WorkerState {
            status: WorkerStatus::Running,
            ..Default::default()
        };
        let new = WorkerState {
            status: WorkerStatus::Stalled,
            ..Default::default()
        };

        let actions = dispatch_actions("test", &old, &new, default_resolve);
        assert_eq!(actions.len(), 1);
        assert!(matches!(
            &actions[0],
            Action::Nudge {
                nudge_key,
                is_stuck: false,
                ..
            } if nudge_key == "idle"
        ));
    }

    #[test]
    fn pr_opened_triggers_notify() {
        let old = WorkerState::default();
        let new = WorkerState {
            pr_url: Some("https://github.com/pr/1".to_string()),
            ..Default::default()
        };

        let actions = dispatch_actions("test", &old, &new, default_resolve);
        assert_eq!(actions.len(), 1);
        assert!(
            matches!(&actions[0], Action::Notify { kind: NotifyKind::PrOpened { .. }, message, .. } if message.contains("PR opened"))
        );
    }

    #[test]
    fn no_change_no_actions() {
        let state = WorkerState {
            status: WorkerStatus::Running,
            ..Default::default()
        };

        let actions = dispatch_actions("test", &state, &state, default_resolve);
        assert!(actions.is_empty());
    }

    #[test]
    fn same_status_still_nudges() {
        let state = WorkerState {
            status: WorkerStatus::WaitingInput,
            ..Default::default()
        };

        let actions = dispatch_actions("test", &state, &state, default_resolve);
        assert_eq!(actions.len(), 1);
        assert!(matches!(
            &actions[0],
            Action::Nudge {
                nudge_key,
                is_stuck: true,
                ..
            } if nudge_key == "stuck"
        ));
    }

    #[test]
    fn failed_triggers_notify() {
        let old = WorkerState {
            status: WorkerStatus::Running,
            ..Default::default()
        };
        let new = WorkerState {
            status: WorkerStatus::Failed,
            ..Default::default()
        };

        let actions = dispatch_actions("test", &old, &new, default_resolve);
        assert_eq!(actions.len(), 1);
        assert!(
            matches!(&actions[0], Action::Notify { kind: NotifyKind::NeedsIntervention, message, .. } if message.contains("failed"))
        );
    }

    #[test]
    fn idle_triggers_nudge() {
        let old = WorkerState {
            status: WorkerStatus::Running,
            ..Default::default()
        };
        let new = WorkerState {
            status: WorkerStatus::Idle,
            ..Default::default()
        };

        let actions = dispatch_actions("test", &old, &new, default_resolve);
        assert_eq!(actions.len(), 1);
        assert!(matches!(
            &actions[0],
            Action::Nudge {
                nudge_key,
                is_stuck: false,
                ..
            } if nudge_key == "idle"
        ));
    }
}
