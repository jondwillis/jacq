//! Integration tests for jacq's IR types.
//!
//! These tests verify that the IR can correctly parse both Claude Code native
//! plugin.json and the jacq IR superset plugin.yaml formats.

/// Helper to deserialize from YAML string.
fn from_yaml<T: serde::de::DeserializeOwned>(s: &str) -> T {
    serde_yaml::from_str(s).expect("YAML deserialization failed")
}

/// Helper to deserialize from JSON string.
fn from_json<T: serde::de::DeserializeOwned>(s: &str) -> T {
    serde_json::from_str(s).expect("JSON deserialization failed")
}

// ===========================================================================
// PluginManifest — the core invariant: "valid Claude Code plugin.json IS valid IR"
// ===========================================================================

mod manifest {
    use super::*;
    use jacq_core::ir::*;
    use jacq_core::targets::Target;

    #[test]
    fn parse_minimal_claude_code_plugin_json() {
        let json = r#"{
            "name": "notes-app",
            "version": "1.0.0",
            "description": "macOS Notes.app integration",
            "author": { "name": "Jon Willis" },
            "license": "MIT",
            "keywords": ["notes", "macos"]
        }"#;

        let manifest: PluginManifest = from_json(json);

        assert_eq!(manifest.name, "notes-app");
        assert_eq!(manifest.version, "1.0.0");
        assert_eq!(manifest.description, "macOS Notes.app integration");
        assert_eq!(manifest.license, Some("MIT".to_string()));
        assert_eq!(manifest.keywords, vec!["notes", "macos"]);

        // IR-specific fields default to None/empty
        assert!(manifest.ir_version.is_none());
        assert!(manifest.targets.is_empty());
        assert!(manifest.requires.is_none());
        assert!(manifest.fallbacks.is_empty());
    }

    #[test]
    fn parse_full_ir_manifest_yaml() {
        let yaml = r#"
ir_version: "0.1"
targets: [claude-code, opencode, codex]
name: my-plugin
version: "1.0.0"
description: "A cross-platform plugin"
author: "Jon Willis"
license: "MIT"
keywords: [ai, plugin]

requires:
  capabilities:
    - skills
    - hooks.pre-tool-use
    - mcp-servers
  permissions:
    - file-read
    - network

fallbacks:
  hooks.pre-tool-use:
    opencode: instruction-based
    cursor: skip
"#;

        let manifest: PluginManifest = from_yaml(yaml);

        assert_eq!(manifest.ir_version, Some("0.1".to_string()));
        assert_eq!(manifest.targets, vec![Target::ClaudeCode, Target::OpenCode, Target::Codex]);
        assert_eq!(manifest.name, "my-plugin");

        let reqs = manifest.requires.expect("requires should be present");
        assert_eq!(reqs.capabilities.len(), 3);
        assert_eq!(reqs.permissions.len(), 2);

        let hook_cap = Capability::try_from("hooks.pre-tool-use".to_string()).unwrap();
        assert!(manifest.fallbacks.contains_key(&hook_cap));
        let hook_fallbacks = &manifest.fallbacks[&hook_cap];
        assert!(hook_fallbacks.contains_key(&Target::OpenCode));
        assert!(hook_fallbacks.contains_key(&Target::Cursor));
    }

    #[test]
    fn author_string_form() {
        let yaml = r#"
name: test
version: "0.1.0"
description: test
author: "Jon Willis"
"#;
        let manifest: PluginManifest = from_yaml(yaml);
        match &manifest.author {
            Author::Name(name) => assert_eq!(name, "Jon Willis"),
            Author::Structured { .. } => panic!("expected Author::Name"),
        }
    }

    #[test]
    fn author_structured_form() {
        let json = r#"{
            "name": "test",
            "version": "0.1.0",
            "description": "test",
            "author": { "name": "Jon Willis", "email": "jon@example.com" }
        }"#;
        let manifest: PluginManifest = from_json(json);
        match &manifest.author {
            Author::Structured { name, email, .. } => {
                assert_eq!(name, "Jon Willis");
                assert_eq!(email.as_deref(), Some("jon@example.com"));
            }
            Author::Name(_) => panic!("expected Author::Structured"),
        }
    }

    #[test]
    fn author_defaults_when_absent() {
        let json = r#"{
            "name": "test",
            "version": "0.1.0",
            "description": "test"
        }"#;
        let manifest: PluginManifest = from_json(json);
        match &manifest.author {
            Author::Name(s) => assert!(s.is_empty()),
            _ => panic!("expected default Author::Name(\"\")"),
        }
    }

    // -- Negative tests --

    #[test]
    fn missing_name_fails() {
        let json = r#"{ "version": "1.0.0", "description": "test" }"#;
        let result: Result<PluginManifest, _> = serde_json::from_str(json);
        assert!(result.is_err(), "manifest without name should fail");
        let err = result.unwrap_err().to_string();
        assert!(err.contains("name"), "error should mention 'name': {err}");
    }

    #[test]
    fn missing_version_defaults() {
        let json = r#"{ "name": "test", "description": "test" }"#;
        let result: PluginManifest = serde_json::from_str(json).unwrap();
        assert_eq!(result.version, "0.0.0");
    }

    #[test]
    fn missing_description_defaults() {
        let json = r#"{ "name": "test", "version": "1.0.0" }"#;
        let result: PluginManifest = serde_json::from_str(json).unwrap();
        assert_eq!(result.description, "");
    }
}

// ===========================================================================
// Capability parsing — dotted path notation
// ===========================================================================

mod capability {
    use jacq_core::ir::*;

    #[test]
    fn parse_simple_capability() {
        let cap = Capability::try_from("skills".to_string()).unwrap();
        assert_eq!(cap.category, CapabilityCategory::Skills);
        assert!(cap.feature.is_none());
    }

    #[test]
    fn parse_dotted_capability() {
        let cap = Capability::try_from("hooks.pre-tool-use".to_string()).unwrap();
        assert_eq!(cap.category, CapabilityCategory::Hooks);
        assert_eq!(cap.feature.as_deref(), Some("pre-tool-use"));
    }

    #[test]
    fn capability_roundtrip() {
        let original = "agents.subagent".to_string();
        let cap = Capability::try_from(original.clone()).unwrap();
        let back: String = cap.into();
        assert_eq!(back, original);
    }

    #[test]
    fn all_categories_parse() {
        let cases = [
            ("skills", CapabilityCategory::Skills),
            ("agents", CapabilityCategory::Agents),
            ("hooks", CapabilityCategory::Hooks),
            ("mcp-servers", CapabilityCategory::McpServers),
            ("instructions", CapabilityCategory::Instructions),
            ("commands", CapabilityCategory::Commands),
        ];
        for (input, expected) in cases {
            let cap = Capability::try_from(input.to_string()).unwrap();
            assert_eq!(cap.category, expected, "failed for input: {input}");
        }
    }

    #[test]
    fn capabilities_deserialize_from_yaml_list() {
        let yaml = r#"
capabilities:
  - skills
  - hooks.pre-tool-use
  - mcp-servers
  - agents.subagent
permissions:
  - file-read
  - network
"#;
        let reqs: Requirements = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(reqs.capabilities.len(), 4);
        assert_eq!(reqs.capabilities[0].category, CapabilityCategory::Skills);
        assert_eq!(reqs.capabilities[1].category, CapabilityCategory::Hooks);
        assert_eq!(reqs.capabilities[1].feature.as_deref(), Some("pre-tool-use"));
        assert_eq!(reqs.capabilities[2].category, CapabilityCategory::McpServers);
        assert_eq!(reqs.capabilities[3].feature.as_deref(), Some("subagent"));

        assert_eq!(reqs.permissions.len(), 2);
        assert_eq!(reqs.permissions[0], Permission::FileRead);
        assert_eq!(reqs.permissions[1], Permission::Network);
    }

    // -- Serde roundtrip through YAML --

    #[test]
    fn capability_serde_roundtrip_yaml() {
        let caps = vec![
            Capability::try_from("skills".to_string()).unwrap(),
            Capability::try_from("hooks.pre-tool-use".to_string()).unwrap(),
            Capability::try_from("mcp-servers".to_string()).unwrap(),
        ];
        let yaml = serde_yaml::to_string(&caps).unwrap();
        let parsed: Vec<Capability> = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(caps, parsed);
    }

    #[test]
    fn capability_serializes_as_plain_string_json() {
        let cap = Capability::try_from("hooks.pre-tool-use".to_string()).unwrap();
        let json = serde_json::to_string(&cap).unwrap();
        // Should be a plain string, not a struct with category/feature fields
        assert_eq!(json, r#""hooks.pre-tool-use""#);
    }

    // -- Negative tests --

    #[test]
    fn unknown_category_fails() {
        let result = Capability::try_from("foobar".to_string());
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("unknown capability category"), "error: {err}");
    }

    #[test]
    fn unknown_category_fails_in_yaml() {
        let yaml = r#"
capabilities:
  - foobar
permissions: []
"#;
        let result: Result<Requirements, _> = serde_yaml::from_str(yaml);
        assert!(result.is_err(), "unknown category should fail deserialization");
    }

    #[test]
    fn empty_feature_after_dot_fails() {
        let result = Capability::try_from("hooks.".to_string());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("empty feature"));
    }

    #[test]
    fn capability_ord_sorts_by_category_then_feature() {
        let mut caps = vec![
            Capability::try_from("skills".to_string()).unwrap(),
            Capability::try_from("agents.subagent".to_string()).unwrap(),
            Capability::try_from("agents".to_string()).unwrap(),
            Capability::try_from("hooks.stop".to_string()).unwrap(),
            Capability::try_from("hooks.pre-tool-use".to_string()).unwrap(),
        ];
        caps.sort();
        let names: Vec<String> = caps.into_iter().map(String::from).collect();
        assert_eq!(names, vec![
            "agents",
            "agents.subagent",
            "hooks.pre-tool-use",
            "hooks.stop",
            "skills",
        ]);
    }
}

// ===========================================================================
// FallbackStrategy
// ===========================================================================

mod fallback {
    use jacq_core::ir::FallbackStrategy;

    #[test]
    fn known_strategies_parse() {
        let cases = [
            ("instruction-based", FallbackStrategy::InstructionBased),
            ("skip", FallbackStrategy::Skip),
            ("prompt-template", FallbackStrategy::PromptTemplate),
            ("agents-md-section", FallbackStrategy::AgentsMdSection),
        ];
        for (input, expected) in cases {
            let fs: FallbackStrategy = serde_yaml::from_str(input).unwrap();
            assert_eq!(fs, expected, "failed for input: {input}");
        }
    }

    #[test]
    fn unknown_strategy_fails() {
        let result: Result<FallbackStrategy, _> = serde_yaml::from_str("my-custom-thing");
        assert!(result.is_err(), "unknown fallback strategy should fail");
    }

    #[test]
    fn typo_in_strategy_fails() {
        let result: Result<FallbackStrategy, _> = serde_yaml::from_str("instuction-based");
        assert!(result.is_err(), "typo should fail, not become a custom value");
    }

    #[test]
    fn fallback_serialization_roundtrip() {
        let strategy = FallbackStrategy::InstructionBased;
        let yaml = serde_yaml::to_string(&strategy).unwrap();
        let parsed: FallbackStrategy = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(parsed, FallbackStrategy::InstructionBased);
    }
}

// ===========================================================================
// Target
// ===========================================================================

mod target {
    use jacq_core::targets::Target;

    #[test]
    fn from_str_all_targets() {
        let cases = [
            ("claude-code", Target::ClaudeCode),
            ("opencode", Target::OpenCode),
            ("codex", Target::Codex),
            ("cursor", Target::Cursor),
            ("openclaw", Target::OpenClaw),
        ];
        for (input, expected) in cases {
            let parsed: Target = input.parse().unwrap();
            assert_eq!(parsed, expected);
        }
    }

    #[test]
    fn from_str_invalid_target() {
        let result: Result<Target, _> = "vscode".parse();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("unknown target"));
    }

    #[test]
    fn display_roundtrips() {
        for target in Target::all() {
            let s = target.to_string();
            let parsed: Target = s.parse().unwrap();
            assert_eq!(*target, parsed);
        }
    }

    #[test]
    fn serde_roundtrip_yaml() {
        let targets = vec![Target::ClaudeCode, Target::OpenCode, Target::Codex];
        let yaml = serde_yaml::to_string(&targets).unwrap();
        let parsed: Vec<Target> = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(targets, parsed);
    }

    #[test]
    fn target_all_covers_all_variants() {
        // If a new variant is added to Target but not to all(), this test
        // catches it via serde: we serialize all() and check we can
        // deserialize each known target string.
        let all = Target::all();
        assert_eq!(all.len(), 5, "update Target::all() when adding new targets");

        // Also verify as_str() and FromStr agree for every variant
        for t in all {
            let s = t.as_str();
            let parsed: Target = s.parse().unwrap_or_else(|e| {
                panic!("Target::as_str() returned '{s}' which FromStr rejects: {e}")
            });
            assert_eq!(*t, parsed);
        }
    }

    #[test]
    fn antigravity_is_removed() {
        let result: Result<Target, _> = "antigravity".parse();
        assert!(result.is_err(), "antigravity should not be a valid target");
    }
}

// ===========================================================================
// Capability matrices
// ===========================================================================

mod matrices {
    use jacq_core::targets::*;

    #[test]
    fn claude_code_supports_everything() {
        let matrix = capability_matrix(Target::ClaudeCode);
        for (cap, level) in &matrix {
            assert!(
                level.is_supported(),
                "Claude Code should support '{cap}' but got {level:?}"
            );
            assert_eq!(
                *level,
                SupportLevel::Full,
                "Claude Code should fully support '{cap}'"
            );
        }
    }

    #[test]
    fn cursor_has_no_hooks() {
        let matrix = capability_matrix(Target::Cursor);
        assert_eq!(matrix["hooks"], SupportLevel::None);
        assert_eq!(matrix["hooks.pre-tool-use"], SupportLevel::None);
        assert_eq!(matrix["hooks.post-tool-use"], SupportLevel::None);
        assert_eq!(matrix["hooks.stop"], SupportLevel::None);
    }

    #[test]
    fn all_targets_have_mcp_support() {
        for target in Target::all() {
            let matrix = capability_matrix(*target);
            assert!(
                matrix["mcp-servers"].is_supported(),
                "{target} should support mcp-servers"
            );
        }
    }

    #[test]
    fn all_targets_have_instructions_support() {
        for target in Target::all() {
            let matrix = capability_matrix(*target);
            assert_eq!(
                matrix["instructions"],
                SupportLevel::Full,
                "{target} should fully support instructions"
            );
        }
    }

    #[test]
    fn all_matrices_have_same_keys() {
        let reference = capability_matrix(Target::ClaudeCode);
        let ref_keys: Vec<&String> = reference.keys().collect();

        for target in Target::all() {
            let matrix = capability_matrix(*target);
            let keys: Vec<&String> = matrix.keys().collect();
            assert_eq!(
                ref_keys, keys,
                "{target}'s matrix has different keys than claude-code"
            );
        }
    }

    #[test]
    fn matrix_keys_match_capability_keys_constant() {
        let matrix = capability_matrix(Target::ClaudeCode);
        let matrix_keys: Vec<&str> = matrix.keys().map(|s| s.as_str()).collect();
        let mut expected: Vec<&str> = CAPABILITY_KEYS.to_vec();
        expected.sort();
        assert_eq!(matrix_keys, expected);
    }

    #[test]
    fn support_level_ordering() {
        assert!(SupportLevel::Full > SupportLevel::Partial);
        assert!(SupportLevel::Partial > SupportLevel::Flags);
        assert!(SupportLevel::Flags > SupportLevel::None);
        assert!(SupportLevel::Full > SupportLevel::None);
    }

    #[test]
    fn flags_is_supported() {
        assert!(SupportLevel::Flags.is_supported());
    }
}

// ===========================================================================
// StringOrVec
// ===========================================================================

mod string_or_vec {
    use jacq_core::ir::StringOrVec;

    #[test]
    fn single_string() {
        let yaml = r#""Bash(osascript:*)""#;
        let sov: StringOrVec = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(sov.as_vec(), vec!["Bash(osascript:*)"]);
    }

    #[test]
    fn multiple_strings() {
        let yaml = r#"
- Read
- Write
- Bash
"#;
        let sov: StringOrVec = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(sov.as_vec(), vec!["Read", "Write", "Bash"]);
    }
}

// ===========================================================================
// SkillFrontmatter
// ===========================================================================

mod skill_frontmatter {
    use jacq_core::ir::SkillFrontmatter;

    #[test]
    fn parse_typical_frontmatter() {
        let yaml = r##"
description: "CRUD operations for macOS Notes.app"
argument-hint: "create, read, update, delete"
allowed-tools: "Bash(osascript:*)"
color: "#FFD700"
"##;
        let fm: SkillFrontmatter = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(fm.description.as_deref(), Some("CRUD operations for macOS Notes.app"));
        assert!(fm.argument_hint.is_some());
        assert!(fm.allowed_tools.is_some());
        assert_eq!(fm.color.as_deref(), Some("#FFD700"));
    }

    #[test]
    fn unknown_fields_rejected() {
        let yaml = r#"
description: "test"
custom-field: "should fail"
"#;
        let result: Result<SkillFrontmatter, _> = serde_yaml::from_str(yaml);
        assert!(result.is_err(), "unknown fields should be rejected");
    }

    #[test]
    fn empty_frontmatter_defaults() {
        let yaml = "{}";
        let fm: SkillFrontmatter = serde_yaml::from_str(yaml).unwrap();
        assert!(fm.description.is_none());
        assert!(fm.argument_hint.is_none());
        assert!(fm.allowed_tools.is_none());
        assert!(fm.color.is_none());
        assert!(fm.examples.is_none());
    }
}

// ===========================================================================
// AgentFrontmatter
// ===========================================================================

mod agent_frontmatter {
    use jacq_core::ir::AgentFrontmatter;

    #[test]
    fn parse_agent_frontmatter() {
        let yaml = r#"
description: "Code review agent"
model: "sonnet"
tools:
  - Read
  - Grep
  - Glob
color: blue
"#;
        let fm: AgentFrontmatter = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(fm.description.as_deref(), Some("Code review agent"));
        assert_eq!(fm.model.as_deref(), Some("sonnet"));
        assert!(fm.tools.is_some());
        assert_eq!(fm.color.as_deref(), Some("blue"));
    }

    #[test]
    fn agent_unknown_fields_rejected() {
        let yaml = r#"
description: "test"
custom-agent-field: true
"#;
        let result: Result<AgentFrontmatter, _> = serde_yaml::from_str(yaml);
        assert!(result.is_err(), "unknown agent fields should be rejected");
    }
}

// ===========================================================================
// HookDef
// ===========================================================================

mod hook_def {
    use jacq_core::ir::{HookDef, HookEvent};

    #[test]
    fn parse_hook_def() {
        let yaml = r#"
name: lint-check
event: PreToolUse
type: command
command: "eslint --check"
timeout: 5000
"#;
        let hook: HookDef = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(hook.name, "lint-check");
        assert_eq!(hook.event, HookEvent::PreToolUse);
        assert_eq!(hook.command.as_deref(), Some("eslint --check"));
        assert_eq!(hook.timeout, Some(5000));
    }

    #[test]
    fn hook_without_timeout() {
        let yaml = r#"
name: simple
event: Stop
command: "echo done"
"#;
        let hook: HookDef = serde_yaml::from_str(yaml).unwrap();
        assert!(hook.timeout.is_none());
        assert_eq!(hook.event, HookEvent::Stop);
    }

    #[test]
    fn known_events() {
        let cases = [
            ("PreToolUse", HookEvent::PreToolUse),
            ("PostToolUse", HookEvent::PostToolUse),
            ("Stop", HookEvent::Stop),
            ("SessionStart", HookEvent::SessionStart),
            ("UserPromptSubmit", HookEvent::UserPromptSubmit),
            ("FileChanged", HookEvent::FileChanged),
            ("SessionEnd", HookEvent::SessionEnd),
        ];
        for (input, expected) in cases {
            let parsed: HookEvent = serde_yaml::from_str(&format!("\"{input}\"")).unwrap();
            assert_eq!(parsed, expected);
        }
    }

    #[test]
    fn unknown_event_is_rejected() {
        let result: Result<HookEvent, _> = serde_yaml::from_str("\"OnSessionStart\"");
        assert!(result.is_err(), "unknown event should fail deserialization");
    }
}

// ===========================================================================
// McpServerDef
// ===========================================================================

mod mcp_server_def {
    use jacq_core::ir::McpServerDef;

    #[test]
    fn parse_mcp_server() {
        let yaml = r#"
name: notes-server
source_path: mcp/notes.yaml
command: npx
args:
  - "-y"
  - "@notes/mcp-server"
env:
  NODE_ENV: production
"#;
        let server: McpServerDef = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(server.name, "notes-server");
        assert_eq!(server.command, "npx");
        assert_eq!(server.args, vec!["-y", "@notes/mcp-server"]);
        assert_eq!(server.env["NODE_ENV"], "production");
    }

    #[test]
    fn mcp_server_minimal() {
        let yaml = r#"
name: simple
source_path: mcp/simple.yaml
command: my-server
"#;
        let server: McpServerDef = serde_yaml::from_str(yaml).unwrap();
        assert!(server.args.is_empty());
        assert!(server.env.is_empty());
    }
}

// ===========================================================================
// PluginIR (top-level AST)
// ===========================================================================

mod plugin_ir {
    use std::collections::BTreeMap;
    use std::path::PathBuf;
    use jacq_core::ir::*;

    #[test]
    fn construct_minimal_plugin_ir() {
        let ir = PluginIR {
            manifest: PluginManifest {
                name: "test".to_string(),
                version: "1.0.0".to_string(),
                description: "test plugin".to_string(),
                author: Author::default(),
                license: None,
                keywords: vec![],
                homepage: None,
                repository: None,
                commands: None,
                agents: None,
                skills: None,
                hooks: None,
                mcp_servers_config: None,
                output_styles: None,
                lsp_servers: None,
                user_config: None,
                channels: None,
                display_name: None,
                logo: None,
                apps: None,
                interface: None,
                id: None,
                config_schema: None,
                providers: None,
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
            output_styles: vec![],
            lsp_servers: vec![],
            shared: vec![],
            target_overrides: BTreeMap::new(),
            source_dir: PathBuf::from("/tmp/test"),
        };

        assert_eq!(ir.manifest.name, "test");
        assert!(ir.skills.is_empty());
    }

    #[test]
    fn source_dir_skipped_in_serialization() {
        let ir = PluginIR {
            manifest: PluginManifest {
                name: "test".to_string(),
                version: "1.0.0".to_string(),
                description: "test".to_string(),
                author: Author::default(),
                license: None,
                keywords: vec![],
                homepage: None,
                repository: None,
                commands: None,
                agents: None,
                skills: None,
                hooks: None,
                mcp_servers_config: None,
                output_styles: None,
                lsp_servers: None,
                user_config: None,
                channels: None,
                display_name: None,
                logo: None,
                apps: None,
                interface: None,
                id: None,
                config_schema: None,
                providers: None,
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
            output_styles: vec![],
            lsp_servers: vec![],
            shared: vec![],
            target_overrides: BTreeMap::new(),
            source_dir: PathBuf::from("/secret/path"),
        };

        let json = serde_json::to_string(&ir).unwrap();
        assert!(!json.contains("/secret/path"), "source_dir should not appear in serialized output");

        // Deserializing back gives empty source_dir
        let parsed: PluginIR = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.source_dir, PathBuf::new());
    }
}

// ===========================================================================
// Permission
// ===========================================================================

mod permission {
    use jacq_core::ir::Permission;

    #[test]
    fn known_permissions() {
        let yaml = r#"
- file-read
- file-write
- network
- subprocess
"#;
        let perms: Vec<Permission> = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(perms, vec![
            Permission::FileRead,
            Permission::FileWrite,
            Permission::Network,
            Permission::Subprocess,
        ]);
    }

    #[test]
    fn unknown_permission_rejected() {
        let yaml = r#""database-access""#;
        let result: Result<Permission, _> = serde_yaml::from_str(yaml);
        assert!(result.is_err(), "unknown permission should fail deserialization");
    }

    #[test]
    fn permission_serialization_roundtrip() {
        let perms = vec![Permission::FileRead, Permission::Network];
        let yaml = serde_yaml::to_string(&perms).unwrap();
        let parsed: Vec<Permission> = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(perms, parsed);
    }
}
