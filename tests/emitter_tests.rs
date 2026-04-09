//! Tests for target emitters — TDD.
//!
//! Each test constructs a PluginIR, emits to a temp directory, and verifies
//! the generated file structure and content.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use jacq::emitter::{emit, EmitOptions};
use jacq::ir::*;
use jacq::targets::Target;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn ir_manifest(name: &str, targets: Vec<Target>) -> PluginManifest {
    PluginManifest {
        name: name.to_string(),
        version: "1.0.0".to_string(),
        description: "Test plugin".to_string(),
        author: Author::Structured {
            name: "Test".to_string(),
            email: Some("test@example.com".to_string()),
        },
        license: Some("MIT".to_string()),
        keywords: vec!["test".to_string()],
        ir_version: Some("0.1".to_string()),
        targets,
        requires: None,
        fallbacks: BTreeMap::new(),
    }
}

fn sample_skill() -> SkillDef {
    SkillDef {
        name: "search".to_string(),
        source_path: PathBuf::from("skills/search.md"),
        frontmatter: SkillFrontmatter {
            description: Some("Search the codebase".to_string()),
            argument_hint: Some(StringOrVec::Single("[query]".to_string())),
            allowed_tools: Some(StringOrVec::Multiple(vec![
                "Grep".to_string(),
                "Glob".to_string(),
            ])),
            color: None,
            examples: None,
            extra: BTreeMap::new(),
        },
        body: "Search for: $ARGUMENTS\n".to_string(),
    }
}

fn sample_agent() -> AgentDef {
    AgentDef {
        name: "reviewer".to_string(),
        source_path: PathBuf::from("agents/reviewer.md"),
        frontmatter: AgentFrontmatter {
            description: Some("Code review agent".to_string()),
            model: Some("sonnet".to_string()),
            allowed_tools: Some(StringOrVec::Multiple(vec![
                "Read".to_string(),
                "Grep".to_string(),
            ])),
            color: None,
            extra: BTreeMap::new(),
        },
        body: "Review the code for quality.\n".to_string(),
    }
}

fn sample_mcp() -> McpServerDef {
    McpServerDef {
        name: "db-server".to_string(),
        source_path: PathBuf::from("mcp/db-server.yaml"),
        command: "npx".to_string(),
        args: vec!["-y".to_string(), "@test/db-mcp".to_string()],
        env: BTreeMap::from([("DB_URL".to_string(), "postgres://localhost/test".to_string())]),
        extra: BTreeMap::new(),
    }
}

fn sample_instruction() -> InstructionDef {
    InstructionDef {
        name: "rules".to_string(),
        source_path: PathBuf::from("instructions/rules.md"),
        body: "Always write tests first.\nKeep functions short.\n".to_string(),
    }
}

fn build_ir(targets: Vec<Target>) -> PluginIR {
    PluginIR {
        manifest: ir_manifest("test-plugin", targets),
        skills: vec![sample_skill()],
        agents: vec![sample_agent()],
        hooks: vec![],
        mcp_servers: vec![sample_mcp()],
        instructions: vec![sample_instruction()],
        target_overrides: BTreeMap::new(),
        source_dir: PathBuf::from("/tmp/test"),
    }
}

fn read_file(dir: &Path, rel: &str) -> String {
    std::fs::read_to_string(dir.join(rel)).unwrap_or_else(|e| panic!("failed to read {rel}: {e}"))
}

fn file_exists(dir: &Path, rel: &str) -> bool {
    dir.join(rel).exists()
}

// ===========================================================================
// Emitter trait / dispatch
// ===========================================================================

mod dispatch {
    use super::*;

    #[test]
    fn emit_creates_target_subdirectories() {
        let ir = build_ir(vec![Target::ClaudeCode, Target::OpenCode]);
        let tmp = TempDir::new().unwrap();
        let opts = EmitOptions { strict: false };

        emit(&ir, tmp.path(), &opts).unwrap();

        assert!(file_exists(tmp.path(), "claude-code"));
        assert!(file_exists(tmp.path(), "opencode"));
    }
}

// ===========================================================================
// Claude Code emitter
// ===========================================================================

mod claude_code {
    use super::*;

    fn emit_claude_code(ir: &PluginIR) -> (TempDir, PathBuf) {
        let tmp = TempDir::new().unwrap();
        let opts = EmitOptions { strict: false };
        emit(ir, tmp.path(), &opts).unwrap();
        let out = tmp.path().join("claude-code");
        (tmp, out)
    }

    #[test]
    fn emits_plugin_json() {
        let ir = build_ir(vec![Target::ClaudeCode]);
        let (_tmp, out) = emit_claude_code(&ir);

        let content = read_file(&out, "plugin.json");
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();

        assert_eq!(parsed["name"], "test-plugin");
        assert_eq!(parsed["version"], "1.0.0");
        assert_eq!(parsed["description"], "Test plugin");
        assert_eq!(parsed["license"], "MIT");
    }

    #[test]
    fn emits_skill_md_files() {
        let ir = build_ir(vec![Target::ClaudeCode]);
        let (_tmp, out) = emit_claude_code(&ir);

        assert!(file_exists(&out, "commands/search.md"));
        let content = read_file(&out, "commands/search.md");
        assert!(content.starts_with("---\n"));
        assert!(content.contains("description: Search the codebase"));
        assert!(content.contains("$ARGUMENTS"));
    }

    #[test]
    fn emits_agent_md_files() {
        let ir = build_ir(vec![Target::ClaudeCode]);
        let (_tmp, out) = emit_claude_code(&ir);

        assert!(file_exists(&out, "agents/reviewer.md"));
        let content = read_file(&out, "agents/reviewer.md");
        assert!(content.contains("description: Code review agent"));
        assert!(content.contains("model: sonnet"));
        assert!(content.contains("Review the code"));
    }

    #[test]
    fn emits_mcp_json() {
        let ir = build_ir(vec![Target::ClaudeCode]);
        let (_tmp, out) = emit_claude_code(&ir);

        assert!(file_exists(&out, ".mcp.json"));
        let content = read_file(&out, ".mcp.json");
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();

        assert!(parsed["mcpServers"]["db-server"].is_object());
        assert_eq!(parsed["mcpServers"]["db-server"]["command"], "npx");
    }

    #[test]
    fn emits_claude_md_from_instructions() {
        let ir = build_ir(vec![Target::ClaudeCode]);
        let (_tmp, out) = emit_claude_code(&ir);

        assert!(file_exists(&out, "CLAUDE.md"));
        let content = read_file(&out, "CLAUDE.md");
        assert!(content.contains("Always write tests first"));
    }

    #[test]
    fn roundtrip_parse_emitted_claude_code() {
        // Emit a plugin, then parse it back — should produce equivalent IR
        let ir = build_ir(vec![Target::ClaudeCode]);
        let (_tmp, out) = emit_claude_code(&ir);

        let parsed = jacq::parser::parse_plugin(&out).unwrap();
        assert_eq!(parsed.manifest.name, "test-plugin");
        assert_eq!(parsed.skills.len(), 1);
        assert_eq!(parsed.skills[0].name, "search");
    }
}

// ===========================================================================
// OpenCode emitter
// ===========================================================================

mod opencode {
    use super::*;

    fn emit_opencode(ir: &PluginIR) -> (TempDir, PathBuf) {
        let tmp = TempDir::new().unwrap();
        let opts = EmitOptions { strict: false };
        emit(ir, tmp.path(), &opts).unwrap();
        let out = tmp.path().join("opencode");
        (tmp, out)
    }

    #[test]
    fn emits_package_json() {
        let ir = build_ir(vec![Target::OpenCode]);
        let (_tmp, out) = emit_opencode(&ir);

        let content = read_file(&out, "package.json");
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();

        assert_eq!(parsed["name"], "test-plugin");
        assert_eq!(parsed["version"], "1.0.0");
        assert_eq!(parsed["description"], "Test plugin");
    }

    #[test]
    fn emits_agents_md() {
        let ir = build_ir(vec![Target::OpenCode]);
        let (_tmp, out) = emit_opencode(&ir);

        assert!(file_exists(&out, "AGENTS.md"));
        let content = read_file(&out, "AGENTS.md");
        assert!(content.contains("Always write tests first"));
    }

    #[test]
    fn agents_md_includes_skill_descriptions() {
        let ir = build_ir(vec![Target::OpenCode]);
        let (_tmp, out) = emit_opencode(&ir);

        let content = read_file(&out, "AGENTS.md");
        // Skills should be documented in AGENTS.md since OpenCode has partial skill support
        assert!(content.contains("search"));
    }
}

// ===========================================================================
// Codex emitter
// ===========================================================================

mod codex {
    use super::*;

    fn emit_codex(ir: &PluginIR) -> (TempDir, PathBuf) {
        let tmp = TempDir::new().unwrap();
        let opts = EmitOptions { strict: false };
        emit(ir, tmp.path(), &opts).unwrap();
        let out = tmp.path().join("codex");
        (tmp, out)
    }

    #[test]
    fn emits_plugin_json() {
        let ir = build_ir(vec![Target::Codex]);
        let (_tmp, out) = emit_codex(&ir);

        let content = read_file(&out, "plugin.json");
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();

        assert_eq!(parsed["name"], "test-plugin");
        assert_eq!(parsed["version"], "1.0.0");
    }

    #[test]
    fn emits_agents_md() {
        let ir = build_ir(vec![Target::Codex]);
        let (_tmp, out) = emit_codex(&ir);

        assert!(file_exists(&out, "AGENTS.md"));
        let content = read_file(&out, "AGENTS.md");
        assert!(content.contains("Always write tests first"));
    }

    #[test]
    fn emits_skill_files() {
        let ir = build_ir(vec![Target::Codex]);
        let (_tmp, out) = emit_codex(&ir);

        // Codex has full skill support — should emit skill .md files
        assert!(file_exists(&out, "skills/search.md"));
    }
}

// ===========================================================================
// Integration: parse → analyze → emit
// ===========================================================================

mod integration {
    use super::*;
    use jacq::analyzer::analyze;
    use jacq::parser::parse_plugin;

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join(name)
    }

    #[test]
    fn full_pipeline_ir_plugin() {
        let ir = parse_plugin(&fixture("ir-plugin")).unwrap();
        let report = analyze(&ir);
        // Claude Code should be compatible
        assert!(report.target_summaries[&Target::ClaudeCode].compatible);

        let tmp = TempDir::new().unwrap();
        let opts = EmitOptions { strict: false };
        emit(&ir, tmp.path(), &opts).unwrap();

        // Should have output for both declared targets
        assert!(file_exists(tmp.path(), "claude-code/plugin.json"));
        assert!(file_exists(tmp.path(), "opencode/package.json"));
    }
}
