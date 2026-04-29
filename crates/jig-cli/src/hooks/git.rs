//! Git hook wrapper templates and utilities.
//!
//! Jig installs wrapper scripts into `.git/hooks/` that:
//! - Call `jig hooks <name>` for jig's logic
//! - Chain to `.user` suffix hooks if they exist
//! - Include a marker comment for detection

use jig_core::error::Result;

/// Marker prefix in the first comment line of jig-managed hooks.
pub const JIG_MANAGED_MARKER: &str = "# jig-managed: v1";

/// All hook names that jig manages.
pub const MANAGED_HOOKS: &[&str] = &["commit-msg", "post-commit", "post-merge", "pre-commit"];

const COMMIT_MSG_TEMPLATE: &str = r#"#!/bin/bash
# jig-managed: v1
# This hook was installed by jig. To uninstall: jig hooks uninstall

set -e

# Validate commit message against conventional commits spec
if command -v jig &> /dev/null; then
    jig hooks commit-msg "$@"
fi

# Run user hook if it exists
if [ -f .git/hooks/commit-msg.user ]; then
    .git/hooks/commit-msg.user "$@"
fi
"#;

const POST_COMMIT_TEMPLATE: &str = r#"#!/bin/bash
# jig-managed: v1
# This hook was installed by jig. To uninstall: jig hooks uninstall

set -e

# Run jig handler
if command -v jig &> /dev/null; then
    jig hooks post-commit "$@" || true
fi

# Run user hook if it exists
if [ -f .git/hooks/post-commit.user ]; then
    .git/hooks/post-commit.user "$@"
fi
"#;

const POST_MERGE_TEMPLATE: &str = r#"#!/bin/bash
# jig-managed: v1
# This hook was installed by jig. To uninstall: jig hooks uninstall

set -e

if command -v jig &> /dev/null; then
    jig hooks post-merge "$@" || true
fi

if [ -f .git/hooks/post-merge.user ]; then
    .git/hooks/post-merge.user "$@"
fi
"#;

const PRE_COMMIT_TEMPLATE: &str = r#"#!/bin/bash
# jig-managed: v1
# This hook was installed by jig. To uninstall: jig hooks uninstall

set -e

# Pre-commit can fail and block commit
if command -v jig &> /dev/null; then
    jig hooks pre-commit "$@"
fi

if [ -f .git/hooks/pre-commit.user ]; then
    .git/hooks/pre-commit.user "$@"
fi
"#;

/// Generate the wrapper script content for a given hook name.
pub fn generate_hook(hook_name: &str) -> Result<String> {
    match hook_name {
        "commit-msg" => Ok(COMMIT_MSG_TEMPLATE.to_string()),
        "post-commit" => Ok(POST_COMMIT_TEMPLATE.to_string()),
        "post-merge" => Ok(POST_MERGE_TEMPLATE.to_string()),
        "pre-commit" => Ok(PRE_COMMIT_TEMPLATE.to_string()),
        _ => Err(jig_core::Error::Custom(format!(
            "unsupported hook: {}",
            hook_name
        ))),
    }
}

/// Check if a hook script was installed by jig.
pub fn is_jig_managed(content: &str) -> bool {
    content
        .lines()
        .any(|line| line.starts_with("# jig-managed:"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_commit_msg() {
        let hook = generate_hook("commit-msg").unwrap();
        assert!(hook.contains(JIG_MANAGED_MARKER));
        assert!(hook.contains("jig hooks commit-msg"));
        assert!(hook.contains(".git/hooks/commit-msg.user"));
        // commit-msg should NOT have || true (failure blocks commit)
        assert!(!hook.contains("jig hooks commit-msg \"$@\" || true"));
    }

    #[test]
    fn generate_post_commit() {
        let hook = generate_hook("post-commit").unwrap();
        assert!(hook.contains(JIG_MANAGED_MARKER));
        assert!(hook.contains("jig hooks post-commit"));
        assert!(hook.contains(".git/hooks/post-commit.user"));
    }

    #[test]
    fn generate_post_merge() {
        let hook = generate_hook("post-merge").unwrap();
        assert!(hook.contains(JIG_MANAGED_MARKER));
        assert!(hook.contains("jig hooks post-merge"));
        assert!(hook.contains(".git/hooks/post-merge.user"));
    }

    #[test]
    fn generate_pre_commit() {
        let hook = generate_hook("pre-commit").unwrap();
        assert!(hook.contains(JIG_MANAGED_MARKER));
        assert!(hook.contains("jig hooks pre-commit"));
        assert!(hook.contains(".git/hooks/pre-commit.user"));
        // pre-commit should NOT have || true (failure blocks commit)
        assert!(!hook.contains("jig hooks pre-commit \"$@\" || true"));
    }

    #[test]
    fn generate_unsupported_fails() {
        assert!(generate_hook("pre-push").is_err());
    }

    #[test]
    fn post_commit_does_not_block() {
        let hook = generate_hook("post-commit").unwrap();
        assert!(hook.contains("|| true"));
    }

    #[test]
    fn is_jig_managed_detects_marker() {
        assert!(is_jig_managed(
            "#!/bin/bash\n# jig-managed: v1\nrest of hook"
        ));
        assert!(is_jig_managed("# jig-managed: v2\nfuture version"));
    }

    #[test]
    fn is_jig_managed_rejects_user_hooks() {
        assert!(!is_jig_managed("#!/bin/bash\necho 'user hook'"));
        assert!(!is_jig_managed(""));
    }

    #[test]
    fn all_templates_start_with_shebang() {
        for name in MANAGED_HOOKS {
            let hook = generate_hook(name).unwrap();
            assert!(hook.starts_with("#!/bin/bash"), "{} missing shebang", name);
        }
    }
}
