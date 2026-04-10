//! Template engine — loads and renders Handlebars templates.
//!
//! Lookup order:
//! 1. Repo-specific: `<repo>/.jig/templates/<name>.hbs`
//! 2. User global: `~/.config/jig/templates/<name>.hbs`
//! 3. Built-in: embedded in binary

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use handlebars::Handlebars;
use serde::Serialize;

use crate::error::Result;
use crate::global::global_config_dir;

use super::builtin::BUILTIN_TEMPLATES;

/// Variables available to all templates.
#[derive(Debug, Clone, Serialize)]
pub struct TemplateContext {
    /// Arbitrary key-value pairs for template rendering.
    #[serde(flatten)]
    pub vars: HashMap<String, serde_json::Value>,
}

impl TemplateContext {
    pub fn new() -> Self {
        Self {
            vars: HashMap::new(),
        }
    }

    /// Set a string variable.
    pub fn set(&mut self, key: impl Into<String>, value: impl Into<String>) -> &mut Self {
        self.vars
            .insert(key.into(), serde_json::Value::String(value.into()));
        self
    }

    /// Set a numeric variable.
    pub fn set_num(&mut self, key: impl Into<String>, value: u32) -> &mut Self {
        self.vars.insert(key.into(), serde_json::json!(value));
        self
    }

    /// Set a boolean variable.
    pub fn set_bool(&mut self, key: impl Into<String>, value: bool) -> &mut Self {
        self.vars.insert(key.into(), serde_json::Value::Bool(value));
        self
    }

    /// Set a list variable.
    pub fn set_list(&mut self, key: impl Into<String>, values: Vec<String>) -> &mut Self {
        self.vars.insert(
            key.into(),
            serde_json::Value::Array(values.into_iter().map(serde_json::Value::String).collect()),
        );
        self
    }
}

impl Default for TemplateContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Template engine with hierarchical loading.
pub struct TemplateEngine<'a> {
    hbs: Handlebars<'a>,
    /// Repo root for repo-specific templates (optional).
    repo_root: Option<PathBuf>,
}

impl<'a> TemplateEngine<'a> {
    /// Create a new engine with built-in templates only.
    pub fn new() -> Self {
        let mut hbs = Handlebars::new();
        hbs.set_strict_mode(false);

        for (name, content) in BUILTIN_TEMPLATES {
            // Errors in built-in templates are bugs, so unwrap is appropriate
            hbs.register_template_string(name, content)
                .unwrap_or_else(|e| panic!("invalid built-in template '{}': {}", name, e));
        }

        Self {
            hbs,
            repo_root: None,
        }
    }

    /// Set the repo root for repo-specific template lookup.
    pub fn with_repo(mut self, repo_root: &Path) -> Self {
        self.repo_root = Some(repo_root.to_path_buf());
        self
    }

    /// Render a template by name with the given context.
    ///
    /// Lookup order:
    /// 1. Repo-specific `.jig/templates/<name>.hbs`
    /// 2. User global `~/.config/jig/templates/<name>.hbs`
    /// 3. Built-in (already registered)
    pub fn render(&self, name: &str, ctx: &TemplateContext) -> Result<String> {
        // Check repo-specific override
        if let Some(ref repo_root) = self.repo_root {
            let repo_path = repo_root
                .join(".jig")
                .join("templates")
                .join(format!("{}.hbs", name));
            if repo_path.exists() {
                let content = std::fs::read_to_string(&repo_path)?;
                return Ok(self.hbs.render_template(&content, ctx)?);
            }
        }

        // Check user global override
        if let Ok(config_dir) = global_config_dir() {
            let user_path = config_dir.join("templates").join(format!("{}.hbs", name));
            if user_path.exists() {
                let content = std::fs::read_to_string(&user_path)?;
                return Ok(self.hbs.render_template(&content, ctx)?);
            }
        }

        // Fall back to built-in
        if self.hbs.has_template(name) {
            Ok(self.hbs.render(name, ctx)?)
        } else {
            Err(crate::Error::Custom(format!(
                "template not found: {}",
                name
            )))
        }
    }

    /// Render a raw template string (not from the hierarchy).
    pub fn render_inline(&self, template: &str, ctx: &TemplateContext) -> Result<String> {
        Ok(self.hbs.render_template(template, ctx)?)
    }

    /// List all available built-in template names.
    pub fn builtin_names() -> Vec<&'static str> {
        BUILTIN_TEMPLATES.iter().map(|(name, _)| *name).collect()
    }
}

impl Default for TemplateEngine<'_> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_builtin_nudge_idle() {
        let engine = TemplateEngine::new();
        let mut ctx = TemplateContext::new();
        ctx.set_num("nudge_count", 1);
        ctx.set_num("max_nudges", 3);
        ctx.set_bool("has_changes", true);
        ctx.set_bool("is_final_nudge", false);

        let result = engine.render("nudge-idle", &ctx).unwrap();
        assert!(result.contains("STATUS CHECK"));
        assert!(result.contains("nudge 1/3"));
        assert!(result.contains("uncommitted changes"));
    }

    #[test]
    fn render_builtin_nudge_idle_no_changes() {
        let engine = TemplateEngine::new();
        let mut ctx = TemplateContext::new();
        ctx.set_num("nudge_count", 2);
        ctx.set_num("max_nudges", 3);
        ctx.set_bool("has_changes", false);
        ctx.set_bool("is_final_nudge", false);

        let result = engine.render("nudge-idle", &ctx).unwrap();
        assert!(result.contains("No recent commits"));
    }

    #[test]
    fn render_builtin_nudge_stuck() {
        let engine = TemplateEngine::new();
        let mut ctx = TemplateContext::new();
        ctx.set_num("nudge_count", 1);
        ctx.set_num("max_nudges", 3);

        let result = engine.render("nudge-stuck", &ctx).unwrap();
        assert!(result.contains("STUCK PROMPT"));
        assert!(result.contains("Auto-approving"));
    }

    #[test]
    fn render_nudge_conflict() {
        let engine = TemplateEngine::new();
        let mut ctx = TemplateContext::new();
        ctx.set_num("nudge_count", 1);
        ctx.set_num("max_nudges", 3);
        ctx.set("base_branch", "origin/main");

        let result = engine.render("nudge-conflict", &ctx).unwrap();
        assert!(result.contains("merge conflicts"));
        assert!(result.contains("origin/main"));
    }

    #[test]
    fn render_inline_template() {
        let engine = TemplateEngine::new();
        let mut ctx = TemplateContext::new();
        ctx.set("name", "world");

        let result = engine.render_inline("Hello, {{name}}!", &ctx).unwrap();
        assert_eq!(result, "Hello, world!");
    }

    #[test]
    fn render_unknown_template_errors() {
        let engine = TemplateEngine::new();
        let ctx = TemplateContext::new();

        assert!(engine.render("nonexistent-template", &ctx).is_err());
    }

    #[test]
    fn repo_override_takes_precedence() {
        let tmp = tempfile::tempdir().unwrap();
        let templates_dir = tmp.path().join(".jig").join("templates");
        std::fs::create_dir_all(&templates_dir).unwrap();
        std::fs::write(
            templates_dir.join("nudge-idle.hbs"),
            "CUSTOM: nudge {{nudge_count}}",
        )
        .unwrap();

        let engine = TemplateEngine::new().with_repo(tmp.path());
        let mut ctx = TemplateContext::new();
        ctx.set_num("nudge_count", 2);

        let result = engine.render("nudge-idle", &ctx).unwrap();
        assert_eq!(result, "CUSTOM: nudge 2");
    }

    #[test]
    fn builtin_names_lists_all() {
        let names = TemplateEngine::builtin_names();
        assert!(names.contains(&"nudge-idle"));
        assert!(names.contains(&"nudge-stuck"));
        assert!(names.contains(&"nudge-ci"));
        assert!(names.contains(&"nudge-conflict"));
        assert!(names.contains(&"nudge-review"));
    }

    #[test]
    fn context_set_list() {
        let engine = TemplateEngine::new();
        let mut ctx = TemplateContext::new();
        ctx.set_list(
            "ci_failures",
            vec!["lint failed".to_string(), "test failed".to_string()],
        );
        ctx.set_num("nudge_count", 1);
        ctx.set_num("max_nudges", 3);

        let result = engine.render("nudge-ci", &ctx).unwrap();
        assert!(result.contains("lint failed"));
        assert!(result.contains("test failed"));
    }

    #[test]
    fn render_triage_prompt_with_all_fields() {
        let engine = TemplateEngine::new();
        let mut ctx = TemplateContext::new();
        ctx.set("issue_id", "ENG-42");
        ctx.set("issue_title", "Fix auth middleware");
        ctx.set(
            "issue_body",
            "The auth middleware drops tokens on redirect.",
        );
        ctx.set_list(
            "issue_labels",
            vec!["bug".to_string(), "security".to_string()],
        );
        ctx.set("repo_name", "my-app");

        let result = engine.render("triage-prompt", &ctx).unwrap();
        assert!(result.contains("ENG-42"));
        assert!(result.contains("Fix auth middleware"));
        assert!(result.contains("The auth middleware drops tokens on redirect."));
        assert!(result.contains("jig issues update ENG-42"));
        assert!(result.contains("jig issues status ENG-42 backlog"));
        assert!(result.contains("Do NOT implement any changes"));
    }
}
