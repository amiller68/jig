//! Git hook wrapper generation for jig
//!
//! Provides hook templates that allow jig to coexist with user hooks
//! by chaining jig logic before user hooks via a `.user` suffix convention.

pub mod templates;

pub use templates::SUPPORTED_HOOKS;

use crate::error::{Error, Result};
use templates::{
    JIG_MANAGED_MARKER, POST_COMMIT_TEMPLATE, POST_MERGE_TEMPLATE, PRE_COMMIT_TEMPLATE,
};

/// Generate hook content for the given hook name.
pub fn generate_hook(hook_name: &str) -> Result<String> {
    match hook_name {
        "post-commit" => Ok(POST_COMMIT_TEMPLATE.to_string()),
        "post-merge" => Ok(POST_MERGE_TEMPLATE.to_string()),
        "pre-commit" => Ok(PRE_COMMIT_TEMPLATE.to_string()),
        _ => Err(Error::UnsupportedHook(hook_name.to_string())),
    }
}

/// Check whether hook content was installed by jig.
pub fn is_jig_managed(content: &str) -> bool {
    content
        .lines()
        .any(|line| line.starts_with(JIG_MANAGED_MARKER))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_post_commit() {
        let hook = generate_hook("post-commit").unwrap();
        assert!(hook.contains("# jig-managed: v1"));
        assert!(hook.contains("jig hooks post-commit"));
        assert!(hook.contains(".git/hooks/post-commit.user"));
        // post-commit should not fail the pipeline
        assert!(hook.contains(r#"jig hooks post-commit "$@" || true"#));
    }

    #[test]
    fn test_generate_post_merge() {
        let hook = generate_hook("post-merge").unwrap();
        assert!(hook.contains("# jig-managed: v1"));
        assert!(hook.contains("jig hooks post-merge"));
        assert!(hook.contains(".git/hooks/post-merge.user"));
        // post-merge should not fail the pipeline
        assert!(hook.contains(r#"jig hooks post-merge "$@" || true"#));
    }

    #[test]
    fn test_generate_pre_commit() {
        let hook = generate_hook("pre-commit").unwrap();
        assert!(hook.contains("# jig-managed: v1"));
        assert!(hook.contains("jig hooks pre-commit"));
        assert!(hook.contains(".git/hooks/pre-commit.user"));
        // pre-commit should be able to fail and block the commit
        assert!(!hook.contains(r#"jig hooks pre-commit "$@" || true"#));
    }

    #[test]
    fn test_generate_unsupported_hook() {
        let result = generate_hook("unknown-hook");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("unknown-hook"));
    }

    #[test]
    fn test_is_jig_managed_with_marker() {
        let content = "#!/bin/bash\n# jig-managed: v1\nrest of hook";
        assert!(is_jig_managed(content));
    }

    #[test]
    fn test_is_jig_managed_without_marker() {
        let content = "#!/bin/bash\necho 'user hook'";
        assert!(!is_jig_managed(content));
    }

    #[test]
    fn test_is_jig_managed_empty() {
        assert!(!is_jig_managed(""));
    }

    #[test]
    fn test_supported_hooks_list() {
        assert_eq!(SUPPORTED_HOOKS.len(), 3);
        assert!(SUPPORTED_HOOKS.contains(&"post-commit"));
        assert!(SUPPORTED_HOOKS.contains(&"post-merge"));
        assert!(SUPPORTED_HOOKS.contains(&"pre-commit"));
    }

    #[test]
    fn test_all_supported_hooks_generate() {
        for hook_name in SUPPORTED_HOOKS {
            let result = generate_hook(hook_name);
            assert!(result.is_ok(), "Failed to generate hook: {hook_name}");
        }
    }

    #[test]
    fn test_all_templates_start_with_shebang() {
        for hook_name in SUPPORTED_HOOKS {
            let hook = generate_hook(hook_name).unwrap();
            assert!(
                hook.starts_with("#!/bin/bash"),
                "{hook_name} template missing shebang"
            );
        }
    }

    #[test]
    fn test_all_templates_have_set_e() {
        for hook_name in SUPPORTED_HOOKS {
            let hook = generate_hook(hook_name).unwrap();
            assert!(
                hook.contains("set -e"),
                "{hook_name} template missing set -e"
            );
        }
    }
}
