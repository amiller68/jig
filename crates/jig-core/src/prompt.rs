//! Generic prompt context builder — renders via Handlebars.

use std::collections::HashMap;

use handlebars::Handlebars;

use crate::error::Result;

/// A context builder that takes a Handlebars template string and renders
/// it with accumulated vars.
#[derive(Debug, Clone)]
pub struct Prompt {
    template: String,
    name: Option<String>,
    vars: HashMap<String, serde_json::Value>,
}

impl Prompt {
    pub fn new(template: &str) -> Self {
        Self {
            template: template.to_string(),
            name: None,
            vars: HashMap::new(),
        }
    }

    pub fn named(mut self, name: &str) -> Self {
        self.name = Some(name.to_string());
        self
    }

    // -- Chainable builders --------------------------------------------------

    pub fn task(mut self, text: &str) -> Self {
        self.set_var("task_context", text);
        self
    }

    pub fn var(mut self, key: &str, value: impl Into<String>) -> Self {
        self.set_var(key, &value.into());
        self
    }

    pub fn var_num(mut self, key: &str, value: u32) -> Self {
        self.vars.insert(key.to_string(), serde_json::json!(value));
        self
    }

    pub fn var_bool(mut self, key: &str, value: bool) -> Self {
        self.vars
            .insert(key.to_string(), serde_json::Value::Bool(value));
        self
    }

    pub fn var_list(mut self, key: &str, values: Vec<String>) -> Self {
        self.vars.insert(
            key.to_string(),
            serde_json::Value::Array(values.into_iter().map(serde_json::Value::String).collect()),
        );
        self
    }

    // -- Terminal methods ----------------------------------------------------

    pub fn name(&self) -> &str {
        self.name.as_deref().unwrap_or("unknown")
    }

    pub fn render(self) -> Result<String> {
        let hbs = Handlebars::new();
        let rendered = hbs.render_template(&self.template, &self.vars)?;
        Ok(rendered)
    }

    fn set_var(&mut self, key: &str, value: &str) {
        self.vars.insert(
            key.to_string(),
            serde_json::Value::String(value.to_string()),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_render() {
        let result = Prompt::new("Hello {{name}}, you have {{count}} items.")
            .var("name", "Alice")
            .var_num("count", 3)
            .render()
            .unwrap();

        assert_eq!(result, "Hello Alice, you have 3 items.");
    }

    #[test]
    fn render_with_task() {
        let result = Prompt::new("TASK: {{task_context}}")
            .task("Fix the bug")
            .render()
            .unwrap();

        assert_eq!(result, "TASK: Fix the bug");
    }

    #[test]
    fn render_with_list() {
        let result = Prompt::new("{{#each items}}{{this}} {{/each}}")
            .var_list("items", vec!["a".into(), "b".into()])
            .render()
            .unwrap();

        assert_eq!(result, "a b ");
    }
}
