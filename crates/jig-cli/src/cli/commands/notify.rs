//! Notify command — inspect, test, and send notifications

use std::fmt;

use clap::{Args, Subcommand};

use crate::context::{Config, NotifyConfig};
use crate::notify::{NotificationEvent, NotificationQueue, Notifier};

use crate::cli::op::Op;
use crate::cli::ui;

/// Manage and inspect notifications
#[derive(Args, Debug, Clone)]
pub struct Notify {
    #[command(subcommand)]
    pub subcommand: NotifyCommands,
}

#[derive(Subcommand, Debug, Clone)]
pub enum NotifyCommands {
    /// Print resolved notification config and queue status
    Doctor,
    /// Emit a synthetic test notification through the full pipeline
    Test,
    /// Print the last N events from the notification queue
    Tail {
        /// Number of events to show
        #[arg(short, long, default_value = "10")]
        n: usize,
    },
    /// Emit a notification event (agent-facing)
    Send {
        #[command(subcommand)]
        kind: SendKind,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum SendKind {
    /// Worker needs human intervention
    NeedsIntervention {
        #[arg(long)]
        repo: String,
        #[arg(long)]
        worker: String,
        #[arg(long)]
        reason: String,
    },
    /// Worker started on a task
    WorkStarted {
        #[arg(long)]
        repo: String,
        #[arg(long)]
        worker: String,
        #[arg(long)]
        issue: Option<String>,
    },
    /// Worker completed its task
    WorkCompleted {
        #[arg(long)]
        repo: String,
        #[arg(long)]
        worker: String,
        #[arg(long)]
        pr_url: Option<String>,
    },
    /// PR was opened
    PrOpened {
        #[arg(long)]
        repo: String,
        #[arg(long)]
        worker: String,
        #[arg(long)]
        pr_url: String,
    },
    /// PR received review feedback
    FeedbackReceived {
        #[arg(long)]
        repo: String,
        #[arg(long)]
        worker: String,
        #[arg(long)]
        pr_url: String,
    },
    /// Worker addressed review feedback
    FeedbackAddressed {
        #[arg(long)]
        repo: String,
        #[arg(long)]
        worker: String,
        #[arg(long)]
        pr_url: String,
    },
}

impl SendKind {
    fn into_event(self) -> NotificationEvent {
        match self {
            Self::NeedsIntervention {
                repo,
                worker,
                reason,
            } => NotificationEvent::NeedsIntervention {
                repo,
                worker,
                reason,
            },
            Self::WorkStarted {
                repo,
                worker,
                issue,
            } => NotificationEvent::WorkStarted {
                repo,
                worker,
                issue,
            },
            Self::WorkCompleted {
                repo,
                worker,
                pr_url,
            } => NotificationEvent::WorkCompleted {
                repo,
                worker,
                pr_url,
            },
            Self::PrOpened {
                repo,
                worker,
                pr_url,
            } => NotificationEvent::PrOpened {
                repo,
                worker,
                pr_url,
            },
            Self::FeedbackReceived {
                repo,
                worker,
                pr_url,
            } => NotificationEvent::FeedbackReceived {
                repo,
                worker,
                pr_url,
            },
            Self::FeedbackAddressed {
                repo,
                worker,
                pr_url,
            } => NotificationEvent::FeedbackAddressed {
                repo,
                worker,
                pr_url,
            },
        }
    }
}

#[derive(Debug)]
pub enum NotifyOutput {
    Doctor,
    Test,
    Tail(TailOutput),
    Send,
}

#[derive(Debug)]
pub struct TailOutput {
    lines: Vec<String>,
}

impl fmt::Display for NotifyOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Doctor | Self::Test | Self::Send => Ok(()),
            Self::Tail(tail) => {
                for line in &tail.lines {
                    writeln!(f, "{}", line)?;
                }
                Ok(())
            }
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum NotifyError {
    #[error(transparent)]
    Core(#[from] jig_core::Error),
}

impl Op for Notify {
    type Error = NotifyError;
    type Output = NotifyOutput;

    fn run(&self) -> Result<Self::Output, Self::Error> {
        match &self.subcommand {
            NotifyCommands::Doctor => run_doctor(),
            NotifyCommands::Test => run_test(),
            NotifyCommands::Tail { n } => run_tail(*n),
            NotifyCommands::Send { kind } => run_send(kind.clone()),
        }
    }
}

fn run_doctor() -> Result<NotifyOutput, NotifyError> {
    let config_path = Config::default_path()?;
    let config_path_str = config_path.display().to_string();

    // Try to load config, capturing parse errors
    let (notify_config, parse_error) = match Config::load_from(&config_path) {
        Ok(cfg) => (cfg.notify, None),
        Err(e) => (NotifyConfig::default(), Some(e.to_string())),
    };

    let queue = NotificationQueue::global()?;
    let queue_path_str = queue.path().display().to_string();
    let queue_exists = queue.exists();

    let queue_size = if queue_exists {
        std::fs::metadata(queue.path()).ok().map(|m| m.len())
    } else {
        None
    };

    let (last_event_ts, last_event_line) = if queue_exists {
        match queue.tail(1) {
            Ok(events) if !events.is_empty() => {
                let last = &events[0];
                let line = last.to_json().unwrap_or_default();
                (Some(last.ts), Some(line))
            }
            _ => (None, None),
        }
    } else {
        (None, None)
    };

    // Print diagnostics to stderr
    ui::header("Notify config");
    ui::detail(&format!("config: {}", config_path_str));

    if let Some(ref err) = parse_error {
        ui::failure(&format!("TOML parse error: {}", err));
    }

    ui::detail(&format!(
        "exec: {}",
        notify_config.exec.as_deref().unwrap_or("<unset>")
    ));
    ui::detail(&format!(
        "webhook: {}",
        notify_config.webhook.as_deref().unwrap_or("<unset>")
    ));
    ui::detail(&format!(
        "events: {}",
        if notify_config.events.is_empty() {
            "<all>".to_string()
        } else {
            notify_config.events.join(", ")
        }
    ));

    eprintln!();
    ui::header("Queue");
    ui::detail(&format!("path: {}", queue_path_str));
    if queue_exists {
        ui::detail(&format!("size: {} bytes", queue_size.unwrap_or(0)));
        if let Some(ts) = last_event_ts {
            let dt = chrono::DateTime::from_timestamp(ts, 0)
                .map(|d| d.to_rfc3339())
                .unwrap_or_else(|| ts.to_string());
            ui::detail(&format!("last event: {}", dt));
        }
        if let Some(ref line) = last_event_line {
            ui::detail(&format!("last line: {}", line));
        }
    } else {
        ui::detail("status: no queue file (no events emitted yet)");
    }

    Ok(NotifyOutput::Doctor)
}

fn run_test() -> Result<NotifyOutput, NotifyError> {
    let config = Config::load()?;
    let queue = NotificationQueue::global()?;
    let notifier = Notifier::new(config.notify, queue);

    let event = NotificationEvent::NeedsIntervention {
        repo: "jig".into(),
        worker: "notify-test".into(),
        reason: "manual test from `jig notify test`".into(),
    };

    notifier.emit_strict(event)?;
    ui::success("emitted test notification");

    Ok(NotifyOutput::Test)
}

fn run_tail(n: usize) -> Result<NotifyOutput, NotifyError> {
    let queue = NotificationQueue::global()?;
    let events = queue.tail(n)?;

    let lines: Vec<String> = events.iter().filter_map(|e| e.to_json().ok()).collect();

    if lines.is_empty() {
        eprintln!("no events in queue");
    }

    Ok(NotifyOutput::Tail(TailOutput { lines }))
}

fn run_send(kind: SendKind) -> Result<NotifyOutput, NotifyError> {
    let config = Config::load()?;
    let queue = NotificationQueue::global()?;
    let notifier = Notifier::new(config.notify, queue);

    let event = kind.into_event();
    let type_name = event.type_name();
    notifier.emit_strict(event)?;
    ui::success(&format!("emitted {}", type_name));

    Ok(NotifyOutput::Send)
}
