//! Nudge system — contextual messages to idle/stuck workers.
//!
//! Ties together the dispatch system (which decides *when* to nudge)
//! with templates (which decide *what* to say) and tmux (which delivers it).

use crate::config::ResolvedNudgeConfig;
use crate::error::Result;
use crate::events::{Event, EventLog, EventType, WorkerState};
use crate::templates::{TemplateContext, TemplateEngine};
use crate::tmux::{TmuxClient, TmuxTarget};
use crate::worker::WorkerStatus;

/// The kind of nudge to send, determines which template to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NudgeType {
    /// Worker idle at shell, no recent activity.
    Idle,
    /// Worker appears stuck at an interactive prompt.
    Stuck,
    /// CI is failing on the worker's PR.
    Ci,
    /// Worker's PR has merge conflicts.
    Conflict,
    /// Worker's PR has unresolved review comments.
    Review,
    /// Worker's PR has non-conventional commit messages.
    BadCommits,
}

impl NudgeType {
    /// Template name for this nudge type.
    pub fn template_name(&self) -> &'static str {
        match self {
            NudgeType::Idle => "nudge-idle",
            NudgeType::Stuck => "nudge-stuck",
            NudgeType::Ci => "nudge-ci",
            NudgeType::Conflict => "nudge-conflict",
            NudgeType::Review => "nudge-review",
            NudgeType::BadCommits => "nudge-bad-commits",
        }
    }

    /// Key used for nudge_counts in WorkerState.
    pub fn count_key(&self) -> &'static str {
        match self {
            NudgeType::Idle => "idle",
            NudgeType::Stuck => "stuck",
            NudgeType::Ci => "ci",
            NudgeType::Conflict => "conflict",
            NudgeType::Review => "review",
            NudgeType::BadCommits => "bad_commits",
        }
    }
}

/// Determine what kind of nudge a worker needs, if any.
///
/// Accepts a closure that resolves per-type nudge config, enabling per-repo overrides.
pub fn classify_nudge<F>(state: &WorkerState, resolve: F) -> Option<NudgeType>
where
    F: Fn(&str) -> ResolvedNudgeConfig,
{
    // Terminal states never get nudged
    if state.status.is_terminal() {
        return None;
    }

    // Workers with an open PR don't get idle nudges — the daemon's PR
    // lifecycle checks handle CI failures and review comments separately.
    if state.pr_url.is_some() {
        return None;
    }

    let nudge_type = match state.status {
        WorkerStatus::WaitingInput => NudgeType::Stuck,
        WorkerStatus::Stalled | WorkerStatus::Idle => NudgeType::Idle,
        _ => return None,
    };

    let resolved = resolve(nudge_type.count_key());

    // Check if we've exceeded max nudges for this type
    let count = state
        .nudge_counts
        .get(nudge_type.count_key())
        .copied()
        .unwrap_or(0);

    if count >= resolved.max {
        tracing::debug!(
            status = state.status.as_str(),
            nudge_type = nudge_type.count_key(),
            count = count,
            max = resolved.max,
            "max nudges reached, skipping"
        );
        return None; // Escalate via notification instead
    }

    // Check cooldown
    if let Some(&last_ts) = state.last_nudge_at.get(nudge_type.count_key()) {
        let now = chrono::Utc::now().timestamp();
        let elapsed = now - last_ts;
        if elapsed < resolved.cooldown_seconds as i64 {
            tracing::debug!(
                status = state.status.as_str(),
                nudge_type = nudge_type.count_key(),
                elapsed,
                cooldown = resolved.cooldown_seconds,
                "nudge cooldown active, skipping"
            );
            return None;
        }
    }

    tracing::debug!(
        status = state.status.as_str(),
        nudge_type = nudge_type.count_key(),
        count = count,
        max = resolved.max,
        "classified nudge"
    );

    Some(nudge_type)
}

/// Build the template context for a nudge.
///
/// `base_branch` is used for conflict and bad-commits nudges to reference
/// the correct upstream branch (defaults to `origin/main` if `None`).
pub fn build_nudge_context(
    nudge_type: NudgeType,
    state: &WorkerState,
    resolved: ResolvedNudgeConfig,
    base_branch: Option<&str>,
) -> TemplateContext {
    let count = state
        .nudge_counts
        .get(nudge_type.count_key())
        .copied()
        .unwrap_or(0);

    let base = base_branch.unwrap_or(crate::config::DEFAULT_BASE_BRANCH);

    let mut ctx = TemplateContext::new();
    ctx.set_num("nudge_count", count + 1);
    ctx.set_num("max_nudges", resolved.max);
    ctx.set_bool("is_final_nudge", count + 1 >= resolved.max);

    match nudge_type {
        NudgeType::Idle => {
            ctx.set_bool("has_changes", state.commit_count > 0);
        }
        NudgeType::Ci => {
            // CI failures would be populated by the caller
        }
        NudgeType::Conflict => {
            ctx.set("base_branch", base);
        }
        NudgeType::BadCommits => {
            ctx.set("base_branch", base);
        }
        _ => {}
    }

    ctx
}

/// Execute a nudge: render the template, send via tmux, emit event.
pub fn execute_nudge(
    target: &TmuxTarget,
    nudge_type: NudgeType,
    state: &WorkerState,
    resolved: ResolvedNudgeConfig,
    engine: &TemplateEngine<'_>,
    tmux: &TmuxClient,
    event_log: &EventLog,
) -> Result<()> {
    let ctx = build_nudge_context(nudge_type, state, resolved, None);
    let message = engine.render(nudge_type.template_name(), &ctx)?;

    tracing::info!(
        target = ?target,
        nudge_type = nudge_type.count_key(),
        template = nudge_type.template_name(),
        "executing nudge"
    );

    match nudge_type {
        NudgeType::Stuck => {
            // For stuck prompts, auto-approve then send the context message
            tmux.auto_approve(target)?;
            std::thread::sleep(std::time::Duration::from_millis(500));
            tmux.send_message(target, &message)?;
        }
        _ => {
            tmux.send_message(target, &message)?;
        }
    }

    // Emit nudge event
    let event = Event::new(EventType::Nudge)
        .with_field("nudge_type", nudge_type.count_key())
        .with_field("message", message);
    event_log.append(&event)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn resolve_with_max(max: u32) -> impl Fn(&str) -> ResolvedNudgeConfig {
        move |_| ResolvedNudgeConfig {
            max,
            cooldown_seconds: 300,
        }
    }

    fn resolved(max: u32) -> ResolvedNudgeConfig {
        ResolvedNudgeConfig {
            max,
            cooldown_seconds: 300,
        }
    }

    #[test]
    fn classify_idle_worker() {
        let state = WorkerState {
            status: WorkerStatus::Idle,
            ..Default::default()
        };
        assert_eq!(
            classify_nudge(&state, resolve_with_max(3)),
            Some(NudgeType::Idle)
        );
    }

    #[test]
    fn classify_stalled_worker() {
        let state = WorkerState {
            status: WorkerStatus::Stalled,
            ..Default::default()
        };
        assert_eq!(
            classify_nudge(&state, resolve_with_max(3)),
            Some(NudgeType::Idle)
        );
    }

    #[test]
    fn classify_waiting_input() {
        let state = WorkerState {
            status: WorkerStatus::WaitingInput,
            ..Default::default()
        };
        assert_eq!(
            classify_nudge(&state, resolve_with_max(3)),
            Some(NudgeType::Stuck)
        );
    }

    #[test]
    fn classify_running_no_nudge() {
        let state = WorkerState {
            status: WorkerStatus::Running,
            ..Default::default()
        };
        assert_eq!(classify_nudge(&state, resolve_with_max(3)), None);
    }

    #[test]
    fn classify_terminal_no_nudge() {
        let state = WorkerState {
            status: WorkerStatus::Merged,
            ..Default::default()
        };
        assert_eq!(classify_nudge(&state, resolve_with_max(3)), None);
    }

    #[test]
    fn max_nudges_returns_none() {
        let mut counts = HashMap::new();
        counts.insert("idle".to_string(), 3);
        let state = WorkerState {
            status: WorkerStatus::Idle,
            nudge_counts: counts,
            ..Default::default()
        };
        assert_eq!(classify_nudge(&state, resolve_with_max(3)), None);
    }

    #[test]
    fn below_max_still_nudges() {
        let mut counts = HashMap::new();
        counts.insert("idle".to_string(), 2);
        let state = WorkerState {
            status: WorkerStatus::Idle,
            nudge_counts: counts,
            ..Default::default()
        };
        assert_eq!(
            classify_nudge(&state, resolve_with_max(3)),
            Some(NudgeType::Idle)
        );
    }

    #[test]
    fn cooldown_prevents_nudge() {
        let now = chrono::Utc::now().timestamp();
        let mut last_nudge_at = HashMap::new();
        last_nudge_at.insert("idle".to_string(), now - 100); // 100s ago
        let state = WorkerState {
            status: WorkerStatus::Idle,
            last_nudge_at,
            ..Default::default()
        };
        // Cooldown is 300s, last nudge was 100s ago → should skip
        assert_eq!(classify_nudge(&state, resolve_with_max(3)), None);
    }

    #[test]
    fn cooldown_expired_allows_nudge() {
        let now = chrono::Utc::now().timestamp();
        let mut last_nudge_at = HashMap::new();
        last_nudge_at.insert("idle".to_string(), now - 400); // 400s ago
        let state = WorkerState {
            status: WorkerStatus::Idle,
            last_nudge_at,
            ..Default::default()
        };
        // Cooldown is 300s, last nudge was 400s ago → should allow
        assert_eq!(
            classify_nudge(&state, resolve_with_max(3)),
            Some(NudgeType::Idle)
        );
    }

    #[test]
    fn context_idle_with_commits() {
        let state = WorkerState {
            commit_count: 3,
            ..Default::default()
        };
        let ctx = build_nudge_context(NudgeType::Idle, &state, resolved(3), None);
        assert_eq!(ctx.vars["has_changes"], serde_json::json!(true));
        assert_eq!(ctx.vars["nudge_count"], serde_json::json!(1));
        assert_eq!(ctx.vars["max_nudges"], serde_json::json!(3));
        assert_eq!(ctx.vars["is_final_nudge"], serde_json::json!(false));
    }

    #[test]
    fn context_idle_no_commits() {
        let state = WorkerState::default();
        let ctx = build_nudge_context(NudgeType::Idle, &state, resolved(3), None);
        assert_eq!(ctx.vars["has_changes"], serde_json::json!(false));
    }

    #[test]
    fn context_final_nudge() {
        let mut counts = HashMap::new();
        counts.insert("idle".to_string(), 2);
        let state = WorkerState {
            status: WorkerStatus::Idle,
            nudge_counts: counts,
            ..Default::default()
        };
        let ctx = build_nudge_context(NudgeType::Idle, &state, resolved(3), None);
        assert_eq!(ctx.vars["nudge_count"], serde_json::json!(3));
        assert_eq!(ctx.vars["is_final_nudge"], serde_json::json!(true));
    }

    #[test]
    fn nudge_type_template_names() {
        assert_eq!(NudgeType::Idle.template_name(), "nudge-idle");
        assert_eq!(NudgeType::Stuck.template_name(), "nudge-stuck");
        assert_eq!(NudgeType::Ci.template_name(), "nudge-ci");
        assert_eq!(NudgeType::Conflict.template_name(), "nudge-conflict");
        assert_eq!(NudgeType::Review.template_name(), "nudge-review");
        assert_eq!(NudgeType::BadCommits.template_name(), "nudge-bad-commits");
    }

    #[test]
    fn render_idle_nudge_message() {
        let engine = TemplateEngine::new();
        let state = WorkerState {
            commit_count: 1,
            ..Default::default()
        };
        let ctx = build_nudge_context(NudgeType::Idle, &state, resolved(3), None);
        let msg = engine.render("nudge-idle", &ctx).unwrap();
        assert!(msg.contains("STATUS CHECK"));
        assert!(msg.contains("nudge 1/3"));
        assert!(msg.contains("uncommitted changes"));
    }

    #[test]
    fn render_stuck_nudge_message() {
        let engine = TemplateEngine::new();
        let state = WorkerState::default();
        let ctx = build_nudge_context(NudgeType::Stuck, &state, resolved(3), None);
        let msg = engine.render("nudge-stuck", &ctx).unwrap();
        assert!(msg.contains("STUCK PROMPT"));
        assert!(msg.contains("Auto-approving"));
    }
}
