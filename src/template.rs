//! Template extraction, validation, and rendering.
//!
//! Scans plugin bodies for `{{variable}}` references, validates them against
//! declared variables in the manifest, and renders them via Tera with
//! target-specific values.

use std::collections::BTreeMap;

use tera::{Context, Tera};

use crate::error::{JacqError, Result};
use crate::ir::*;
use crate::targets::Target;

// ---------------------------------------------------------------------------
// Extraction — scan a body string for {{var}} references
// ---------------------------------------------------------------------------

/// Scan a body string and return a `BodyContent`.
/// If the body contains `{{...}}` patterns or `{% include %}` directives,
/// returns `Template` with extracted refs. Otherwise returns `Plain`.
pub fn extract(body: &str) -> BodyContent {
    let variables = extract_variables(body);
    let includes = extract_includes(body);
    if variables.is_empty() && includes.is_empty() {
        BodyContent::Plain(body.to_string())
    } else {
        BodyContent::Template(TemplateBody {
            raw: body.to_string(),
            variables,
            includes,
        })
    }
}

/// Extract all `{{name}}` variable references from a string.
fn extract_variables(body: &str) -> Vec<VariableRef> {
    let mut vars = Vec::new();
    let bytes = body.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i + 1 < len {
        if bytes[i] == b'{' && bytes[i + 1] == b'{' {
            let start = i;
            i += 2;
            // Skip whitespace after {{
            while i < len && bytes[i] == b' ' {
                i += 1;
            }
            let name_start = i;
            // Collect name characters (alphanumeric + underscore + hyphen)
            while i < len && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_' || bytes[i] == b'-') {
                i += 1;
            }
            let name_end = i;
            // Skip whitespace before }}
            while i < len && bytes[i] == b' ' {
                i += 1;
            }
            if i + 1 < len && bytes[i] == b'}' && bytes[i + 1] == b'}' {
                let end = i + 2;
                if name_end > name_start {
                    vars.push(VariableRef {
                        name: body[name_start..name_end].to_string(),
                        span: (start, end),
                    });
                }
                i = end;
            }
        } else {
            i += 1;
        }
    }

    vars
}

/// Extract `{% include "name" %}` references from a template body.
fn extract_includes(body: &str) -> Vec<String> {
    let mut includes = Vec::new();
    let pattern = "{% include \"";
    let mut search_from = 0;

    while let Some(start) = body[search_from..].find(pattern) {
        let abs_start = search_from + start + pattern.len();
        if let Some(end) = body[abs_start..].find('"') {
            let name = &body[abs_start..abs_start + end];
            if !name.is_empty() {
                includes.push(name.to_string());
            }
        }
        search_from = abs_start;
    }

    includes
}

// ---------------------------------------------------------------------------
// Validation — check all referenced vars are declared
// ---------------------------------------------------------------------------

/// Validate that all template variables in the IR are declared in manifest.vars.
/// Returns a list of errors (empty = valid).
pub fn validate(ir: &PluginIR) -> Vec<JacqError> {
    let mut errors = Vec::new();

    let check_body = |body: &BodyContent, path: &std::path::Path, errors: &mut Vec<JacqError>| {
        if let BodyContent::Template(tmpl) = body {
            for var in &tmpl.variables {
                if !ir.manifest.vars.contains_key(&var.name) {
                    errors.push(JacqError::UndeclaredVariable {
                        name: var.name.clone(),
                        path: path.to_path_buf(),
                        span: var.span,
                    });
                }
            }
        }
    };

    for skill in &ir.skills {
        check_body(&skill.body, &skill.source_path, &mut errors);
    }
    for agent in &ir.agents {
        check_body(&agent.body, &agent.source_path, &mut errors);
    }
    for instruction in &ir.instructions {
        check_body(&instruction.body, &instruction.source_path, &mut errors);
    }
    for fragment in &ir.shared {
        check_body(&fragment.body, &fragment.source_path, &mut errors);
    }

    // Check include references resolve to known shared fragments
    let shared_names: std::collections::HashSet<&str> =
        ir.shared.iter().map(|f| f.name.as_str()).collect();

    let check_includes = |body: &BodyContent, path: &std::path::Path, errors: &mut Vec<JacqError>| {
        if let BodyContent::Template(tmpl) = body {
            for inc in &tmpl.includes {
                if !shared_names.contains(inc.as_str()) {
                    errors.push(JacqError::MissingInclude {
                        name: inc.clone(),
                        path: path.to_path_buf(),
                    });
                }
            }
        }
    };

    for skill in &ir.skills {
        check_includes(&skill.body, &skill.source_path, &mut errors);
    }
    for agent in &ir.agents {
        check_includes(&agent.body, &agent.source_path, &mut errors);
    }
    for instruction in &ir.instructions {
        check_includes(&instruction.body, &instruction.source_path, &mut errors);
    }
    for fragment in &ir.shared {
        check_includes(&fragment.body, &fragment.source_path, &mut errors);
    }

    // Check required vars have values for all targets
    for (name, var_def) in &ir.manifest.vars {
        if var_def.required && var_def.default.is_none() {
            for target in &ir.manifest.targets {
                if !var_def.targets.contains_key(target) {
                    errors.push(JacqError::MissingVariableValue {
                        name: name.clone(),
                        target: *target,
                    });
                }
            }
        }
    }

    errors
}

// ---------------------------------------------------------------------------
// Rendering — substitute variables via Tera
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// RenderEngine — pre-built Tera + Context for efficient multi-body rendering
// ---------------------------------------------------------------------------

/// Pre-built rendering engine for a specific target.
///
/// Holds a Tera instance with shared fragments registered and a Context with
/// all variable values resolved. Built once per target, reused across all
/// body renders — avoids re-registering fragments and re-resolving vars.
pub struct RenderEngine {
    tera: Tera,
    context: Context,
}

impl RenderEngine {
    /// Build an engine for a specific target.
    pub fn new(
        vars: &BTreeMap<String, VarDef>,
        shared: &[SharedFragment],
        target: Target,
    ) -> Result<Self> {
        let mut tera = Tera::default();
        for fragment in shared {
            tera.add_raw_template(&fragment.name, fragment.body.as_raw())
                .map_err(|e| JacqError::Serialization {
                    reason: format!(
                        "failed to register shared fragment '{}': {e}",
                        fragment.name
                    ),
                })?;
        }

        let mut context = Context::new();
        for (name, var_def) in vars {
            let value = var_def
                .targets
                .get(&target)
                .or(var_def.default.as_ref())
                .cloned()
                .unwrap_or_default();
            context.insert(name, &value);
        }

        Ok(RenderEngine { tera, context })
    }

    /// Render a body. Plain bodies pass through; Template bodies go through Tera.
    pub fn render(&self, body: &BodyContent) -> Result<String> {
        match body {
            BodyContent::Plain(s) => Ok(s.clone()),
            BodyContent::Template(tmpl) => {
                let mut tera = self.tera.clone();
                tera.render_str(&tmpl.raw, &self.context)
                    .map_err(|e| JacqError::Serialization {
                        reason: format!("template rendering failed: {e}"),
                    })
            }
        }
    }
}

/// Convenience: render a single body without a pre-built engine.
/// Use `RenderEngine` when rendering multiple bodies for the same target.
pub fn render(
    body: &BodyContent,
    vars: &BTreeMap<String, VarDef>,
    shared: &[SharedFragment],
    target: Target,
) -> Result<String> {
    RenderEngine::new(vars, shared, target)?.render(body)
}

// ---------------------------------------------------------------------------
// Pipeline helper — extract templates from all bodies in an IR
// ---------------------------------------------------------------------------

/// Scan all bodies in the IR and upgrade Plain → Template where {{vars}} or includes are found.
pub fn extract_all(ir: &mut PluginIR) {
    for skill in &mut ir.skills {
        skill.body = extract(skill.body.as_raw());
    }
    for agent in &mut ir.agents {
        agent.body = extract(agent.body.as_raw());
    }
    for instruction in &mut ir.instructions {
        instruction.body = extract(instruction.body.as_raw());
    }
    for fragment in &mut ir.shared {
        fragment.body = extract(fragment.body.as_raw());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- Extraction tests --

    #[test]
    fn extract_no_variables() {
        let body = extract("Just plain text with no templates.");
        assert!(matches!(body, BodyContent::Plain(_)));
        assert!(!body.has_variables());
    }

    #[test]
    fn extract_single_variable() {
        let body = extract("Hello {{name}}, welcome!");
        assert!(body.has_variables());
        if let BodyContent::Template(tmpl) = &body {
            assert_eq!(tmpl.variables.len(), 1);
            assert_eq!(tmpl.variables[0].name, "name");
        } else {
            panic!("expected Template");
        }
    }

    #[test]
    fn extract_multiple_variables() {
        let body = extract("Search {{project_name}} for: {{arguments_var}}");
        if let BodyContent::Template(tmpl) = &body {
            assert_eq!(tmpl.variables.len(), 2);
            assert_eq!(tmpl.variables[0].name, "project_name");
            assert_eq!(tmpl.variables[1].name, "arguments_var");
        } else {
            panic!("expected Template");
        }
    }

    #[test]
    fn extract_with_spaces() {
        let body = extract("Hello {{ name }}, welcome {{ role }}!");
        if let BodyContent::Template(tmpl) = &body {
            assert_eq!(tmpl.variables.len(), 2);
            assert_eq!(tmpl.variables[0].name, "name");
            assert_eq!(tmpl.variables[1].name, "role");
        } else {
            panic!("expected Template");
        }
    }

    #[test]
    fn extract_preserves_dollar_arguments() {
        // $ARGUMENTS is a Claude Code runtime variable, not a jacq template variable
        let body = extract("Search for: $ARGUMENTS");
        assert!(matches!(body, BodyContent::Plain(_)));
    }

    #[test]
    fn extract_spans_are_correct() {
        let text = "Hello {{name}}!";
        let body = extract(text);
        if let BodyContent::Template(tmpl) = &body {
            let var = &tmpl.variables[0];
            assert_eq!(&text[var.span.0..var.span.1], "{{name}}");
        } else {
            panic!("expected Template");
        }
    }

    #[test]
    fn extract_hyphenated_names() {
        let body = extract("Use {{allowed-tools}} here");
        if let BodyContent::Template(tmpl) = &body {
            assert_eq!(tmpl.variables[0].name, "allowed-tools");
        } else {
            panic!("expected Template");
        }
    }

    // -- Rendering tests --

    #[test]
    fn render_plain_returns_as_is() {
        let body = BodyContent::Plain("Hello world".to_string());
        let result = render(&body, &BTreeMap::new(), &[], Target::ClaudeCode).unwrap();
        assert_eq!(result, "Hello world");
    }

    #[test]
    fn render_template_substitutes_default() {
        let body = extract("Hello {{name}}!");
        let vars = BTreeMap::from([(
            "name".to_string(),
            VarDef {
                description: None,
                default: Some("World".to_string()),
                required: false,
                targets: BTreeMap::new(),
            },
        )]);
        let result = render(&body, &vars, &[], Target::ClaudeCode).unwrap();
        assert_eq!(result, "Hello World!");
    }

    #[test]
    fn render_template_uses_target_override() {
        let body = extract("Args: {{arguments_var}}");
        let vars = BTreeMap::from([(
            "arguments_var".to_string(),
            VarDef {
                description: None,
                default: Some("$ARGUMENTS".to_string()),
                required: false,
                targets: BTreeMap::from([
                    (Target::Codex, "$INPUT".to_string()),
                    (Target::OpenCode, "${args}".to_string()),
                ]),
            },
        )]);

        let cc = render(&body, &vars, &[], Target::ClaudeCode).unwrap();
        assert_eq!(cc, "Args: $ARGUMENTS"); // falls back to default

        let codex = render(&body, &vars, &[], Target::Codex).unwrap();
        assert_eq!(codex, "Args: $INPUT"); // target override

        let oc = render(&body, &vars, &[], Target::OpenCode).unwrap();
        assert_eq!(oc, "Args: ${args}"); // target override
    }

    // -- Validation tests --

    #[test]
    fn validate_declared_vars_pass() {
        let mut ir = crate::ir::PluginIR {
            manifest: PluginManifest {
                name: "test".to_string(),
                version: "1.0.0".to_string(),
                description: "test".to_string(),
                author: Author::default(),
                license: None,
                keywords: vec![],
                ir_version: None,
                targets: vec![],
                requires: None,
                fallbacks: BTreeMap::new(),
                vars: BTreeMap::from([(
                    "name".to_string(),
                    VarDef {
                        description: None,
                        default: Some("World".to_string()),
                        required: false,
                        targets: BTreeMap::new(),
                    },
                )]),
            },
            skills: vec![SkillDef {
                name: "greet".to_string(),
                source_path: "skills/greet.md".into(),
                frontmatter: SkillFrontmatter::default(),
                body: extract("Hello {{name}}!"),
            }],
            agents: vec![],
            hooks: vec![],
            mcp_servers: vec![],
            instructions: vec![],
            shared: vec![],
            target_overrides: BTreeMap::new(),
            source_dir: std::path::PathBuf::new(),
        };
        extract_all(&mut ir);
        let errors = validate(&ir);
        assert!(errors.is_empty(), "declared vars should pass: {errors:?}");
    }

    #[test]
    fn validate_undeclared_var_produces_error() {
        let mut ir = crate::ir::PluginIR {
            manifest: PluginManifest {
                name: "test".to_string(),
                version: "1.0.0".to_string(),
                description: "test".to_string(),
                author: Author::default(),
                license: None,
                keywords: vec![],
                ir_version: None,
                targets: vec![],
                requires: None,
                fallbacks: BTreeMap::new(),
                vars: BTreeMap::new(), // no vars declared
            },
            skills: vec![SkillDef {
                name: "greet".to_string(),
                source_path: "skills/greet.md".into(),
                frontmatter: SkillFrontmatter::default(),
                body: extract("Hello {{undefined_var}}!"),
            }],
            agents: vec![],
            hooks: vec![],
            mcp_servers: vec![],
            instructions: vec![],
            shared: vec![],
            target_overrides: BTreeMap::new(),
            source_dir: std::path::PathBuf::new(),
        };
        extract_all(&mut ir);
        let errors = validate(&ir);
        assert_eq!(errors.len(), 1);
        assert!(matches!(&errors[0], JacqError::UndeclaredVariable { name, .. } if name == "undefined_var"));
    }

    #[test]
    fn validate_required_var_missing_target_value() {
        let ir = crate::ir::PluginIR {
            manifest: PluginManifest {
                name: "test".to_string(),
                version: "1.0.0".to_string(),
                description: "test".to_string(),
                author: Author::default(),
                license: None,
                keywords: vec![],
                ir_version: None,
                targets: vec![Target::ClaudeCode, Target::Codex],
                requires: None,
                fallbacks: BTreeMap::new(),
                vars: BTreeMap::from([(
                    "args".to_string(),
                    VarDef {
                        description: None,
                        default: None, // no default
                        required: true,
                        targets: BTreeMap::from([
                            (Target::ClaudeCode, "$ARGUMENTS".to_string()),
                            // Codex missing!
                        ]),
                    },
                )]),
            },
            skills: vec![],
            agents: vec![],
            hooks: vec![],
            mcp_servers: vec![],
            instructions: vec![],
            shared: vec![],
            target_overrides: BTreeMap::new(),
            source_dir: std::path::PathBuf::new(),
        };
        let errors = validate(&ir);
        assert_eq!(errors.len(), 1);
        assert!(matches!(&errors[0], JacqError::MissingVariableValue { name, target } if name == "args" && *target == Target::Codex));
    }

    // -- Include tests --

    #[test]
    fn extract_detects_include() {
        let body = extract("Before\n{% include \"error-handling\" %}\nAfter");
        assert!(matches!(body, BodyContent::Template(_)));
        if let BodyContent::Template(tmpl) = &body {
            assert_eq!(tmpl.includes, vec!["error-handling"]);
            assert!(tmpl.variables.is_empty());
        }
    }

    #[test]
    fn extract_detects_include_and_vars() {
        let body = extract("Hello {{name}}\n{% include \"rules\" %}");
        if let BodyContent::Template(tmpl) = &body {
            assert_eq!(tmpl.variables.len(), 1);
            assert_eq!(tmpl.includes, vec!["rules"]);
        } else {
            panic!("expected Template");
        }
    }

    #[test]
    fn render_include_resolves_shared_fragment() {
        let body = extract("Start\n{% include \"greeting\" %}\nEnd");
        let shared = vec![SharedFragment {
            name: "greeting".to_string(),
            source_path: "shared/greeting.md".into(),
            body: "Hello from shared!".into(),
        }];
        let result = render(&body, &BTreeMap::new(), &shared, Target::ClaudeCode).unwrap();
        assert!(result.contains("Start"));
        assert!(result.contains("Hello from shared!"));
        assert!(result.contains("End"));
    }

    #[test]
    fn render_include_with_variables() {
        let body = extract("{% include \"tmpl\" %}");
        let shared = vec![SharedFragment {
            name: "tmpl".to_string(),
            source_path: "shared/tmpl.md".into(),
            body: "Project: {{project_name}}".into(),
        }];
        let vars = BTreeMap::from([(
            "project_name".to_string(),
            VarDef {
                description: None,
                default: Some("my-app".to_string()),
                required: false,
                targets: BTreeMap::new(),
            },
        )]);
        let result = render(&body, &vars, &shared, Target::ClaudeCode).unwrap();
        assert_eq!(result, "Project: my-app");
    }

    #[test]
    fn render_nested_include() {
        let body = extract("{% include \"outer\" %}");
        let shared = vec![
            SharedFragment {
                name: "inner".to_string(),
                source_path: "shared/inner.md".into(),
                body: "INNER".into(),
            },
            SharedFragment {
                name: "outer".to_string(),
                source_path: "shared/outer.md".into(),
                body: "OUTER {% include \"inner\" %} OUTER".into(),
            },
        ];
        let result = render(&body, &BTreeMap::new(), &shared, Target::ClaudeCode).unwrap();
        assert!(result.contains("OUTER"));
        assert!(result.contains("INNER"));
    }

    #[test]
    fn validate_missing_include_produces_error() {
        let ir = crate::ir::PluginIR {
            manifest: PluginManifest {
                name: "test".to_string(),
                version: "1.0.0".to_string(),
                description: "test".to_string(),
                author: Author::default(),
                license: None,
                keywords: vec![],
                ir_version: None,
                targets: vec![],
                requires: None,
                fallbacks: BTreeMap::new(),
                vars: BTreeMap::new(),
            },
            skills: vec![SkillDef {
                name: "broken".to_string(),
                source_path: "skills/broken.md".into(),
                frontmatter: SkillFrontmatter::default(),
                body: extract("{% include \"nonexistent\" %}"),
            }],
            agents: vec![],
            hooks: vec![],
            mcp_servers: vec![],
            instructions: vec![],
            shared: vec![],
            target_overrides: BTreeMap::new(),
            source_dir: std::path::PathBuf::new(),
        };
        let errors = validate(&ir);
        assert_eq!(errors.len(), 1);
        assert!(matches!(&errors[0], JacqError::MissingInclude { name, .. } if name == "nonexistent"));
    }

    #[test]
    fn validate_valid_include_passes() {
        let ir = crate::ir::PluginIR {
            manifest: PluginManifest {
                name: "test".to_string(),
                version: "1.0.0".to_string(),
                description: "test".to_string(),
                author: Author::default(),
                license: None,
                keywords: vec![],
                ir_version: None,
                targets: vec![],
                requires: None,
                fallbacks: BTreeMap::new(),
                vars: BTreeMap::new(),
            },
            skills: vec![SkillDef {
                name: "good".to_string(),
                source_path: "skills/good.md".into(),
                frontmatter: SkillFrontmatter::default(),
                body: extract("{% include \"rules\" %}"),
            }],
            agents: vec![],
            hooks: vec![],
            mcp_servers: vec![],
            instructions: vec![],
            shared: vec![SharedFragment {
                name: "rules".to_string(),
                source_path: "shared/rules.md".into(),
                body: "Some rules".into(),
            }],
            target_overrides: BTreeMap::new(),
            source_dir: std::path::PathBuf::new(),
        };
        let errors = validate(&ir);
        assert!(errors.is_empty(), "valid include should pass: {errors:?}");
    }

    #[test]
    fn extract_all_processes_shared_fragments() {
        let mut ir = crate::ir::PluginIR {
            manifest: PluginManifest {
                name: "test".to_string(),
                version: "1.0.0".to_string(),
                description: "test".to_string(),
                author: Author::default(),
                license: None,
                keywords: vec![],
                ir_version: None,
                targets: vec![],
                requires: None,
                fallbacks: BTreeMap::new(),
                vars: BTreeMap::new(),
            },
            skills: vec![],
            agents: vec![],
            hooks: vec![],
            mcp_servers: vec![],
            instructions: vec![],
            shared: vec![SharedFragment {
                name: "tmpl".to_string(),
                source_path: "shared/tmpl.md".into(),
                body: "Hello {{name}}!".into(),
            }],
            target_overrides: BTreeMap::new(),
            source_dir: std::path::PathBuf::new(),
        };
        extract_all(&mut ir);
        assert!(ir.shared[0].body.has_variables());
    }
}
