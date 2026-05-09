use std::io;

use clap::Args;

use crate::cli::op::Op;
use crate::context::{Context, RepoConfig};

use super::{IssuesError, IssuesOutput};

/// Update an existing issue's fields
#[derive(Args, Debug, Clone)]
pub struct Update {
    /// Issue ID (e.g. "features/my-feature" or "AUT-123")
    pub id: String,

    /// New title
    #[arg(short, long)]
    pub title: Option<String>,

    /// New body/description (use "-" to read from stdin)
    #[arg(short, long)]
    pub body: Option<String>,

    /// Append body to existing description instead of replacing it
    #[arg(short, long)]
    pub append: bool,

    /// New priority (urgent, high, medium, low)
    #[arg(short, long)]
    pub priority: Option<String>,

    /// Labels to set — REPLACES the current label set (can specify multiple -l flags).
    /// Use --add-label / --remove-label for additive edits.
    #[arg(short, long)]
    pub label: Vec<String>,

    /// Labels to add to the existing set (repeatable)
    #[arg(long = "add-label")]
    pub add_label: Vec<String>,

    /// Labels to remove from the existing set (repeatable)
    #[arg(long = "remove-label")]
    pub remove_label: Vec<String>,

    /// Category/directory (file) or project name (Linear)
    #[arg(short, long)]
    pub category: Option<String>,

    /// Assignee — "me" for the authenticated user, or a Linear user ID
    /// (Linear provider only; ignored by the file provider)
    #[arg(short = 'A', long)]
    pub assignee: Option<String>,

    /// Add blocking dependencies (issue IDs that block this issue)
    #[arg(long, value_delimiter = ',')]
    pub blocked_by: Vec<String>,

    /// Remove blocking dependencies
    #[arg(long, value_delimiter = ',')]
    pub remove_blocked_by: Vec<String>,

    /// Parent issue ID (e.g. "JIG-19") to set as parent
    #[arg(short = 'P', long)]
    pub parent: Option<String>,

    /// Remove the parent issue relation
    #[arg(long)]
    pub remove_parent: bool,
}

fn read_body(body: Option<&str>) -> Result<Option<String>, IssuesError> {
    match body {
        Some("-") => {
            let mut buf = String::new();
            io::Read::read_to_string(&mut io::stdin(), &mut buf)?;
            Ok(Some(buf))
        }
        Some(text) => Ok(Some(text.to_string())),
        None => Ok(None),
    }
}

fn run(
    repo: &RepoConfig,
    global: &crate::context::Config,
    cmd: &Update,
) -> Result<IssuesOutput, IssuesError> {
    let pri = cmd.priority.as_deref().and_then(|s| s.parse().ok());
    let body_text = read_body(cmd.body.as_deref())?;

    if !cmd.label.is_empty() && (!cmd.add_label.is_empty() || !cmd.remove_label.is_empty()) {
        return Err(IssuesError::Usage(
            "--label (replace) cannot be combined with --add-label / --remove-label".to_string(),
        ));
    }

    if cmd.title.is_none()
        && body_text.is_none()
        && pri.is_none()
        && cmd.label.is_empty()
        && cmd.add_label.is_empty()
        && cmd.remove_label.is_empty()
        && cmd.category.is_none()
        && cmd.assignee.is_none()
        && cmd.blocked_by.is_empty()
        && cmd.remove_blocked_by.is_empty()
        && cmd.parent.is_none()
        && !cmd.remove_parent
    {
        return Err(IssuesError::Usage(
            "at least one field to update is required (--title, --body, --priority, --label, --add-label, --remove-label, --category, --assignee, --blocked-by, --remove-blocked-by, --parent, --remove-parent)".to_string(),
        ));
    }

    let effective_body = match (cmd.append, body_text) {
        (true, Some(new_body)) => {
            let provider = repo.issue_provider(global)?;
            let existing = provider
                .get(&cmd.id)?
                .ok_or_else(|| IssuesError::Usage(format!("issue not found: {}", cmd.id)))?;
            let existing_desc = existing
                .body()
                .strip_prefix(&format!("# {}\n\n", existing.title()))
                .unwrap_or(existing.body());
            if existing_desc.is_empty() {
                Some(new_body)
            } else {
                Some(format!("{}\n\n{}", existing_desc, new_body))
            }
        }
        (_, body) => body,
    };

    let mutate_labels = !cmd.add_label.is_empty() || !cmd.remove_label.is_empty();
    let computed_labels: Vec<String> = if !cmd.label.is_empty() {
        cmd.label.clone()
    } else if mutate_labels {
        let provider = repo.issue_provider(global)?;
        let existing = provider
            .get(&cmd.id)?
            .ok_or_else(|| IssuesError::Usage(format!("issue not found: {}", cmd.id)))?;
        let mut set: Vec<String> = existing.labels().to_vec();
        for add in &cmd.add_label {
            if !set.iter().any(|l| l.eq_ignore_ascii_case(add)) {
                set.push(add.clone());
            }
        }
        set.retain(|l| !cmd.remove_label.iter().any(|r| r.eq_ignore_ascii_case(l)));
        set
    } else {
        Vec::new()
    };

    let labels_changed = !cmd.label.is_empty() || mutate_labels;
    let has_field_updates = cmd.title.is_some()
        || effective_body.is_some()
        || pri.is_some()
        || labels_changed
        || cmd.category.is_some()
        || cmd.assignee.is_some();

    let linear_provider = repo.linear_provider(global)?;
    if has_field_updates || cmd.parent.is_some() || cmd.remove_parent {
        linear_provider.update_issue(
            &cmd.id,
            cmd.title.as_deref(),
            effective_body.as_deref(),
            pri.as_ref(),
            &computed_labels,
            cmd.category.as_deref(),
            cmd.assignee.as_deref(),
            cmd.parent.as_deref(),
            cmd.remove_parent,
        )?;
    }
    for blocker in &cmd.blocked_by {
        linear_provider.add_blocked_by(&cmd.id, blocker)?;
    }
    for blocker in &cmd.remove_blocked_by {
        linear_provider.remove_blocked_by(&cmd.id, blocker)?;
    }

    Ok(IssuesOutput::Updated(cmd.id.clone()))
}

impl Op for Update {
    type Error = IssuesError;
    type Output = IssuesOutput;

    fn run(&self) -> Result<Self::Output, Self::Error> {
        let cfg = Context::from_cwd()?;
        let repo = cfg.repo()?;
        run(repo, &cfg.config, self)
    }
}
