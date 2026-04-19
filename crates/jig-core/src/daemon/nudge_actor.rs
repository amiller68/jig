//! Nudge actor — delivers nudge messages to tmux panes in a background thread.
//!
//! This prevents `send-keys` calls from blocking the tick thread when tmux
//! cannot deliver input (e.g. a hung pane).

use super::messages::{NudgeComplete, NudgeRequest};
use crate::events::{Event, EventLog, EventType};
use crate::host::tmux::TmuxWindow;

/// Spawn the nudge actor thread. Returns immediately.
///
/// The actor blocks on `rx.recv()` waiting for nudge requests, delivers each
/// nudge via tmux (with timeouts), appends to the event log, and sends
/// `NudgeComplete` back.
pub fn spawn(
    rx: flume::Receiver<NudgeRequest>,
    tx: flume::Sender<NudgeComplete>,
) -> std::thread::JoinHandle<()> {
    std::thread::Builder::new()
        .name("jig-nudge".into())
        .spawn(move || {
            while let Ok(req) = rx.recv() {
                let window = TmuxWindow::new(&req.session, &req.window);
                let error = deliver_nudge(&window, &req);

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

fn deliver_nudge(window: &TmuxWindow, req: &NudgeRequest) -> Option<String> {
    if let Err(e) = window.send_message(&req.message) {
        return Some(e.to_string());
    }

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
