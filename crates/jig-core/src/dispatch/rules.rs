//! Dispatch rules — pure functions mapping state transitions to actions.

use crate::events::WorkerState;
use crate::global::GlobalConfig;
use crate::nudge::classify_nudge;
use crate::worker::WorkerStatus;

use super::Action;

/// Given an old and new worker state, return actions to execute.
pub fn dispatch_actions(
    worker_id: &str,
    old_state: &WorkerState,
    new_state: &WorkerState,
    config: &GlobalConfig,
) -> Vec<Action> {
    let mut actions = vec![];

    // State changed to something nudgeable
    if old_state.status != new_state.status {
        if let Some(nudge_type) = classify_nudge(new_state, config) {
            actions.push(Action::Nudge {
                worker_id: worker_id.to_string(),
                nudge_type,
            });
        } else if matches!(
            new_state.status,
            WorkerStatus::WaitingInput | WorkerStatus::Stalled | WorkerStatus::Idle
        ) {
            // Max nudges reached, escalate
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
            });
        }
    }

    // PR opened
    if old_state.pr_url.is_none() && new_state.pr_url.is_some() {
        actions.push(Action::Notify {
            worker_id: worker_id.to_string(),
            message: format!(
                "PR opened: {}",
                new_state.pr_url.as_deref().unwrap_or("unknown")
            ),
        });
    }

    // Transition to Failed
    if old_state.status != WorkerStatus::Failed && new_state.status == WorkerStatus::Failed {
        actions.push(Action::Notify {
            worker_id: worker_id.to_string(),
            message: "Worker failed".to_string(),
        });
    }

    actions
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nudge::NudgeType;

    fn default_config() -> GlobalConfig {
        GlobalConfig::default()
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

        let actions = dispatch_actions("test", &old, &new, &default_config());
        assert_eq!(actions.len(), 1);
        assert!(matches!(
            &actions[0],
            Action::Nudge {
                worker_id,
                nudge_type: NudgeType::Stuck,
            } if worker_id == "test"
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

        let actions = dispatch_actions("test", &old, &new, &default_config());
        assert_eq!(actions.len(), 1);
        assert!(
            matches!(&actions[0], Action::Notify { message, .. } if message.contains("Max nudges"))
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

        let actions = dispatch_actions("test", &old, &new, &default_config());
        assert_eq!(actions.len(), 1);
        assert!(matches!(
            &actions[0],
            Action::Nudge {
                nudge_type: NudgeType::Idle,
                ..
            }
        ));
    }

    #[test]
    fn pr_opened_triggers_notify() {
        let old = WorkerState::default();
        let mut new = WorkerState::default();
        new.pr_url = Some("https://github.com/pr/1".to_string());

        let actions = dispatch_actions("test", &old, &new, &default_config());
        assert_eq!(actions.len(), 1);
        assert!(
            matches!(&actions[0], Action::Notify { message, .. } if message.contains("PR opened"))
        );
    }

    #[test]
    fn no_change_no_actions() {
        let state = WorkerState {
            status: WorkerStatus::Running,
            ..Default::default()
        };

        let actions = dispatch_actions("test", &state, &state, &default_config());
        assert!(actions.is_empty());
    }

    #[test]
    fn same_status_no_retrigger() {
        let state = WorkerState {
            status: WorkerStatus::WaitingInput,
            ..Default::default()
        };

        let actions = dispatch_actions("test", &state, &state, &default_config());
        assert!(actions.is_empty());
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

        let actions = dispatch_actions("test", &old, &new, &default_config());
        assert_eq!(actions.len(), 1);
        assert!(
            matches!(&actions[0], Action::Notify { message, .. } if message.contains("failed"))
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

        let actions = dispatch_actions("test", &old, &new, &default_config());
        assert_eq!(actions.len(), 1);
        assert!(matches!(
            &actions[0],
            Action::Nudge {
                nudge_type: NudgeType::Idle,
                ..
            }
        ));
    }
}
