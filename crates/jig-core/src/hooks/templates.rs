//! Git hook templates for jig-managed hooks

/// Marker prefix used to identify jig-managed hooks
pub const JIG_MANAGED_MARKER: &str = "# jig-managed:";

/// Current hook format version
pub const HOOK_VERSION: &str = "v1";

pub const POST_COMMIT_TEMPLATE: &str = r#"#!/bin/bash
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

pub const POST_MERGE_TEMPLATE: &str = r#"#!/bin/bash
# jig-managed: v1
# This hook was installed by jig. To uninstall: jig hooks uninstall

set -e

# Run jig handler
if command -v jig &> /dev/null; then
    jig hooks post-merge "$@" || true
fi

# Run user hook if it exists
if [ -f .git/hooks/post-merge.user ]; then
    .git/hooks/post-merge.user "$@"
fi
"#;

pub const PRE_COMMIT_TEMPLATE: &str = r#"#!/bin/bash
# jig-managed: v1
# This hook was installed by jig. To uninstall: jig hooks uninstall

set -e

# Pre-commit can fail and block commit
if command -v jig &> /dev/null; then
    jig hooks pre-commit "$@"
fi

# Run user hook if it exists
if [ -f .git/hooks/pre-commit.user ]; then
    .git/hooks/pre-commit.user "$@"
fi
"#;

/// All supported hook names
pub const SUPPORTED_HOOKS: &[&str] = &["post-commit", "post-merge", "pre-commit"];
