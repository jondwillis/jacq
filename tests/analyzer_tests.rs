//! Tests for the capability analyzer.
//!
//! Written before the implementation (TDD). These define what the analyzer
//! should do — the implementation makes them pass.

use std::collections::BTreeMap;
use std::path::PathBuf;

use jacq::analyzer::{analyze, Severity};
use jacq::ir::*;
use jacq::targets::Target;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn minimal_manifest(name: &str, targets: Vec<Target>) -> PluginManifest {
    PluginManifest {
        name: name.to_string(),
        version: "1.0.0".to_string(),
        description: "test".to_string(),
        author: Author::default(),
        license: None,
        keywords: vec![],
        ir_version: Some("0.1".to_string()),
        targets,
        requires: None,
        fallbacks: BTreeMap::new(),
        vars: BTreeMap::new(),
    }
}

fn empty_ir(manifest: PluginManifest) -> PluginIR {
    PluginIR {
        manifest,
        skills: vec![],
        agents: vec![],
        hooks: vec![],
        mcp_servers: vec![],
        instructions: vec![],
        target_overrides: BTreeMap::new(),
        source_dir: PathBuf::from("/tmp/test"),
    }
}

fn skill(name: &str) -> SkillDef {
    SkillDef {
        name: name.to_string(),
        source_path: PathBuf::from(format!("skills/{name}.md")),
        frontmatter: SkillFrontmatter::default(),
        body: "test".into(),
    }
}

fn agent(name: &str) -> AgentDef {
    AgentDef {
        name: name.to_string(),
        source_path: PathBuf::from(format!("agents/{name}.md")),
        frontmatter: AgentFrontmatter::default(),
        body: "test".into(),
    }
}

fn hook(name: &str, event: HookEvent) -> HookDef {
    HookDef {
        name: name.to_string(),
        source_path: PathBuf::from(format!("hooks/{name}.yaml")),
        event,
        command: "test".to_string(),
        timeout: None,
        extra: BTreeMap::new(),
    }
}

fn mcp_server(name: &str) -> McpServerDef {
    McpServerDef {
        name: name.to_string(),
        source_path: PathBuf::from(format!("mcp/{name}.yaml")),
        command: "npx".to_string(),
        args: vec![],
        env: BTreeMap::new(),
        extra: BTreeMap::new(),
    }
}

// ===========================================================================
// Basic analysis
// ===========================================================================

mod basic {
    use super::*;

    #[test]
    fn empty_plugin_for_claude_code_has_no_issues() {
        let ir = empty_ir(minimal_manifest("test", vec![Target::ClaudeCode]));
        let report = analyze(&ir);
        assert!(report.diagnostics.is_empty());
        assert!(report.is_ok());
    }

    #[test]
    fn no_targets_means_no_analysis() {
        // A Claude Code native plugin with no declared targets — nothing to check
        let mut manifest = minimal_manifest("test", vec![]);
        manifest.ir_version = None;
        let ir = empty_ir(manifest);
        let report = analyze(&ir);
        assert!(report.diagnostics.is_empty());
    }

    #[test]
    fn report_has_per_target_summaries() {
        let ir = empty_ir(minimal_manifest("test", vec![Target::ClaudeCode, Target::Cursor]));
        let report = analyze(&ir);
        assert!(report.target_summaries.contains_key(&Target::ClaudeCode));
        assert!(report.target_summaries.contains_key(&Target::Cursor));
    }
}

// ===========================================================================
// Capability inference from plugin content
// ===========================================================================

mod inference {
    use super::*;

    #[test]
    fn skills_infer_skills_capability() {
        let mut ir = empty_ir(minimal_manifest("test", vec![Target::ClaudeCode]));
        ir.skills.push(skill("search"));

        let report = analyze(&ir);
        assert!(report.inferred_capabilities.contains(&"skills".to_string()));
    }

    #[test]
    fn agents_infer_agents_capability() {
        let mut ir = empty_ir(minimal_manifest("test", vec![Target::ClaudeCode]));
        ir.agents.push(agent("reviewer"));

        let report = analyze(&ir);
        assert!(report.inferred_capabilities.contains(&"agents".to_string()));
    }

    #[test]
    fn hooks_infer_specific_hook_capabilities() {
        let mut ir = empty_ir(minimal_manifest("test", vec![Target::ClaudeCode]));
        ir.hooks.push(hook("lint", HookEvent::PreToolUse));
        ir.hooks.push(hook("cleanup", HookEvent::Stop));

        let report = analyze(&ir);
        // Specific hook types inferred, not the parent "hooks"
        assert!(report.inferred_capabilities.contains(&"hooks.pre-tool-use".to_string()));
        assert!(report.inferred_capabilities.contains(&"hooks.stop".to_string()));
        assert!(
            !report.inferred_capabilities.contains(&"hooks".to_string()),
            "parent 'hooks' should not be inferred when specifics are present"
        );
    }

    #[test]
    fn mcp_servers_infer_mcp_capability() {
        let mut ir = empty_ir(minimal_manifest("test", vec![Target::ClaudeCode]));
        ir.mcp_servers.push(mcp_server("db"));

        let report = analyze(&ir);
        assert!(report.inferred_capabilities.contains(&"mcp-servers".to_string()));
    }

    #[test]
    fn instructions_infer_instructions_capability() {
        let mut ir = empty_ir(minimal_manifest("test", vec![Target::ClaudeCode]));
        ir.instructions.push(InstructionDef {
            name: "rules".to_string(),
            source_path: PathBuf::from("instructions/rules.md"),
            body: "Be nice".into(),
        });

        let report = analyze(&ir);
        assert!(report.inferred_capabilities.contains(&"instructions".to_string()));
    }
}

// ===========================================================================
// Unsupported capability detection
// ===========================================================================

mod unsupported {
    use super::*;

    #[test]
    fn hooks_on_cursor_produce_errors() {
        let mut ir = empty_ir(minimal_manifest("test", vec![Target::Cursor]));
        ir.hooks.push(hook("lint", HookEvent::PreToolUse));

        let report = analyze(&ir);
        assert!(!report.is_ok(), "should have errors");

        let errors: Vec<_> = report.errors().collect();
        assert!(!errors.is_empty());
        assert!(errors.iter().any(|d| {
            d.target == Target::Cursor && d.capability.contains("hooks")
        }));
    }

    #[test]
    fn hooks_on_openclaw_produce_errors() {
        let mut ir = empty_ir(minimal_manifest("test", vec![Target::OpenClaw]));
        ir.hooks.push(hook("lint", HookEvent::PreToolUse));

        let report = analyze(&ir);
        let errors: Vec<_> = report.errors().collect();
        assert!(!errors.is_empty());
    }

    #[test]
    fn partial_support_produces_warnings() {
        let mut ir = empty_ir(minimal_manifest("test", vec![Target::OpenCode]));
        ir.skills.push(skill("search"));

        let report = analyze(&ir);
        // OpenCode has Partial support for skills — should be a warning, not error
        let warnings: Vec<_> = report.warnings().collect();
        assert!(!warnings.is_empty());
        assert!(warnings.iter().any(|d| {
            d.target == Target::OpenCode
                && d.capability == "skills"
                && d.severity == Severity::Warning
        }));
    }

    #[test]
    fn flags_support_produces_warnings() {
        // Codex has Flags support for hooks — should warn
        let mut ir = empty_ir(minimal_manifest("test", vec![Target::Codex]));
        ir.hooks.push(hook("lint", HookEvent::PreToolUse));

        let report = analyze(&ir);
        let warnings: Vec<_> = report.warnings().collect();
        assert!(warnings.iter().any(|d| {
            d.target == Target::Codex && d.capability.contains("hooks")
        }));
    }

    #[test]
    fn full_support_produces_no_diagnostics() {
        let mut ir = empty_ir(minimal_manifest("test", vec![Target::ClaudeCode]));
        ir.skills.push(skill("search"));
        ir.hooks.push(hook("lint", HookEvent::PreToolUse));
        ir.agents.push(agent("reviewer"));
        ir.mcp_servers.push(mcp_server("db"));

        let report = analyze(&ir);
        assert!(report.is_ok(), "Claude Code supports everything: {report:?}");
    }

    #[test]
    fn multiple_targets_report_independently() {
        let mut ir = empty_ir(minimal_manifest(
            "test",
            vec![Target::ClaudeCode, Target::Cursor],
        ));
        ir.hooks.push(hook("lint", HookEvent::PreToolUse));

        let report = analyze(&ir);
        // Claude Code: no issues. Cursor: errors for hooks.
        let cc_diags: Vec<_> = report.for_target(Target::ClaudeCode).collect();
        let cursor_diags: Vec<_> = report.for_target(Target::Cursor).collect();

        assert!(cc_diags.is_empty(), "Claude Code should have no issues");
        assert!(!cursor_diags.is_empty(), "Cursor should have hook issues");
    }
}

// ===========================================================================
// Fallback resolution
// ===========================================================================

mod fallbacks {
    use super::*;

    #[test]
    fn declared_fallback_downgrades_error_to_info() {
        let mut manifest = minimal_manifest("test", vec![Target::Cursor]);
        manifest.fallbacks.insert(
            Capability::try_from("hooks.pre-tool-use".to_string()).unwrap(),
            BTreeMap::from([(Target::Cursor, FallbackStrategy::Skip)]),
        );

        let mut ir = empty_ir(manifest);
        ir.hooks.push(hook("lint", HookEvent::PreToolUse));

        let report = analyze(&ir);
        // With a fallback declared, this shouldn't be an error
        let errors: Vec<_> = report.errors().collect();
        assert!(
            errors.is_empty(),
            "fallback should prevent error: {errors:?}"
        );

        // Should be info instead
        let infos: Vec<_> = report.infos().collect();
        assert!(infos.iter().any(|d| {
            d.target == Target::Cursor && d.capability.contains("hooks")
        }));
    }

    #[test]
    fn fallback_skip_noted_in_diagnostic() {
        let mut manifest = minimal_manifest("test", vec![Target::Cursor]);
        manifest.fallbacks.insert(
            Capability::try_from("hooks.pre-tool-use".to_string()).unwrap(),
            BTreeMap::from([(Target::Cursor, FallbackStrategy::Skip)]),
        );

        let mut ir = empty_ir(manifest);
        ir.hooks.push(hook("lint", HookEvent::PreToolUse));

        let report = analyze(&ir);
        let infos: Vec<_> = report.infos().collect();
        assert!(infos.iter().any(|d| d.message.contains("skip")));
    }

    #[test]
    fn fallback_instruction_based_noted() {
        let mut manifest = minimal_manifest("test", vec![Target::OpenCode]);
        manifest.fallbacks.insert(
            Capability::try_from("hooks.pre-tool-use".to_string()).unwrap(),
            BTreeMap::from([(Target::OpenCode, FallbackStrategy::InstructionBased)]),
        );

        let mut ir = empty_ir(manifest);
        ir.hooks.push(hook("lint", HookEvent::PreToolUse));

        let report = analyze(&ir);
        // OpenCode has Partial for hooks.pre-tool-use, plus a fallback declared
        // Should be info, not warning
        let infos: Vec<_> = report.infos().collect();
        assert!(infos.iter().any(|d| d.message.contains("instruction")));
    }

    #[test]
    fn fallback_only_applies_to_declared_target() {
        let mut manifest = minimal_manifest(
            "test",
            vec![Target::Cursor, Target::OpenClaw],
        );
        // Fallback only for Cursor, not OpenClaw
        manifest.fallbacks.insert(
            Capability::try_from("hooks.pre-tool-use".to_string()).unwrap(),
            BTreeMap::from([(Target::Cursor, FallbackStrategy::Skip)]),
        );

        let mut ir = empty_ir(manifest);
        ir.hooks.push(hook("lint", HookEvent::PreToolUse));

        let report = analyze(&ir);
        // Cursor: info (fallback). OpenClaw: error (no fallback).
        let cursor_errors: Vec<_> = report
            .for_target(Target::Cursor)
            .filter(|d| d.severity == Severity::Error)
            .collect();
        let openclaw_errors: Vec<_> = report
            .for_target(Target::OpenClaw)
            .filter(|d| d.severity == Severity::Error)
            .collect();

        assert!(cursor_errors.is_empty(), "Cursor has fallback");
        assert!(!openclaw_errors.is_empty(), "OpenClaw has no fallback");
    }
}

// ===========================================================================
// Compatibility report
// ===========================================================================

mod report {
    use super::*;

    #[test]
    fn target_summary_includes_compatibility_level() {
        let mut ir = empty_ir(minimal_manifest(
            "test",
            vec![Target::ClaudeCode, Target::Cursor],
        ));
        ir.skills.push(skill("search"));
        ir.hooks.push(hook("lint", HookEvent::PreToolUse));

        let report = analyze(&ir);

        let cc = &report.target_summaries[&Target::ClaudeCode];
        assert!(cc.compatible(), "Claude Code should be fully compatible");

        let cursor = &report.target_summaries[&Target::Cursor];
        assert!(!cursor.compatible(), "Cursor should not be fully compatible");
    }

    #[test]
    fn error_count_in_summary() {
        let mut ir = empty_ir(minimal_manifest("test", vec![Target::Cursor]));
        ir.hooks.push(hook("lint", HookEvent::PreToolUse));
        ir.hooks.push(hook("cleanup", HookEvent::PostToolUse));

        let report = analyze(&ir);
        let cursor = &report.target_summaries[&Target::Cursor];
        assert!(cursor.error_count > 0);
    }
}

// ===========================================================================
// Integration: parse then analyze
// ===========================================================================

mod integration {
    use super::*;
    use jacq::parser::parse_plugin;

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join(name)
    }

    #[test]
    fn analyze_ir_plugin_fixture() {
        let ir = parse_plugin(&fixture("ir-plugin")).unwrap();
        let report = analyze(&ir);

        // IR plugin targets claude-code and opencode
        assert!(report.target_summaries.contains_key(&Target::ClaudeCode));
        assert!(report.target_summaries.contains_key(&Target::OpenCode));

        // Claude Code should be clean
        let cc = &report.target_summaries[&Target::ClaudeCode];
        assert!(cc.compatible());

        // OpenCode has partial support for some features but the plugin
        // declares a fallback for hooks.pre-tool-use → instruction-based
        // So that specific issue should be info, not error/warning
    }

    #[test]
    fn analyze_claude_code_native_no_targets() {
        let ir = parse_plugin(&fixture("claude-code-plugin")).unwrap();
        let report = analyze(&ir);
        // No targets declared → nothing to analyze
        assert!(report.diagnostics.is_empty());
    }
}
