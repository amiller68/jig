//! Nudge actor — delivers nudge messages to tmux panes in a background thread.
//!
//! This prevents `send-keys` calls from blocking the tick thread when tmux
//! cannot deliver input (e.g. a hung pane).

use super::messages::{NudgeComplete, NudgeRequest};
use crate::events::{Event, EventLog, EventType};
use crate::tmux::{TmuxClient, TmuxTarget};

/// Spawn the nudge actor thread. Returns immediately.
///
/// The actor blocks on `rx.recv()` waiting for nudge requests, delivers each
/// nudge via its own `TmuxClient` (with timeouts), appends to the event log,
/// and sends `NudgeComplete` back.
pub fn spawn(
    rx: flume::Receiver<NudgeRequest>,
    tx: flume::Sender<NudgeComplete>,
) -> std::thread::JoinHandle<()> {
    std::thread::Builder::new()
        .name("jig-nudge".into())
        .spawn(move || {
            let tmux = TmuxClient::new();

            while let Ok(req) = rx.recv() {
                let target = TmuxTarget::new(&req.session, &req.window);
                let error = deliver_nudge(&tmux, &target, &req);

                if let Some(ref err) = error {
                    tracing::warn!(
                        worker = %req.worker_key,
                        nudge_type = %req.nudge_type_key,
                        "nudge delivery failed: {}",
                        err
                    );
                } else {
                    tracing::info!(
                        worker = %req.worker_key,
                        nudge_type = %req.nudge_type_key,
                        "nudge delivered"
                    );
                }

                let resp = NudgeComplete {
                    worker_key: req.worker_key,
                    nudge_type_key: req.nudge_type_key,
                    error,
                };

                if tx.send(resp).is_err() {
                    break;
                }
            }
        })
        .expect("failed to spawn nudge actor thread")
}

/// Deliver a single nudge, returning an error string on failure.
fn deliver_nudge(tmux: &TmuxClient, target: &TmuxTarget, req: &NudgeRequest) -> Option<String> {
    let result = if req.is_stuck {
        // For stuck prompts, auto-approve then send the context message
        tmux.auto_approve(target).and_then(|()| {
            std::thread::sleep(std::time::Duration::from_millis(500));
            tmux.send_message(target, &req.message)
        })
    } else {
        tmux.send_message(target, &req.message)
    };

    if let Err(e) = &result {
        return Some(e.to_string());
    }

    // Append event log entry
    let event_log = match EventLog::for_worker(&req.repo_name, &req.worker_name) {
        Ok(log) => log,
        Err(e) => return Some(format!("failed to open event log: {}", e)),
    };

    let event = Event::new(EventType::Nudge)
        .with_field("nudge_type", req.nudge_type_key.as_str())
        .with_field("message", req.message.as_str());

    if let Err(e) = event_log.append(&event) {
        return Some(format!("failed to append nudge event: {}", e));
    }

    None
}
