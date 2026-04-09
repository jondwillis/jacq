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
    use jacq::ir::*;
    use jacq::targets::Target;

    #[test]
    fn parse_minimal_claude_code_plugin_json() {
        // This is what a real Claude Code plugin.json looks like (from notes-app-plugin).
        // It must parse successfully with all IR-specific fields defaulting.
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

        // Requirements parsed
        let reqs = manifest.requires.expect("requires should be present");
        assert_eq!(reqs.capabilities.len(), 3);
        assert_eq!(reqs.permissions.len(), 2);

        // Fallbacks parsed
        assert!(manifest.fallbacks.contains_key("hooks.pre-tool-use"));
        let hook_fallbacks = &manifest.fallbacks["hooks.pre-tool-use"];
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
            Author::Structured { name, email } => {
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
}

// ===========================================================================
// Capability parsing — dotted path notation
// ===========================================================================

mod capability {
    use jacq::ir::*;

    #[test]
    fn parse_simple_capability() {
        let cap = Capability::from("skills".to_string());
        assert_eq!(cap.category, CapabilityCategory::Skills);
        assert!(cap.feature.is_none());
    }

    #[test]
    fn parse_dotted_capability() {
        let cap = Capability::from("hooks.pre-tool-use".to_string());
        assert_eq!(cap.category, CapabilityCategory::Hooks);
        assert_eq!(cap.feature.as_deref(), Some("pre-tool-use"));
    }

    #[test]
    fn capability_roundtrip() {
        let original = "agents.subagent".to_string();
        let cap = Capability::from(original.clone());
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
            let cap = Capability::from(input.to_string());
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
}

// ===========================================================================
// FallbackStrategy
// ===========================================================================

mod fallback {
    use jacq::ir::FallbackStrategy;

    #[test]
    fn known_strategies_parse() {
        let yaml = r#"instruction-based"#;
        let fs: FallbackStrategy = serde_yaml::from_str(yaml).unwrap();
        assert!(matches!(fs, FallbackStrategy::InstructionBased));

        let yaml = r#"skip"#;
        let fs: FallbackStrategy = serde_yaml::from_str(yaml).unwrap();
        assert!(matches!(fs, FallbackStrategy::Skip));

        let yaml = r#"prompt-template"#;
        let fs: FallbackStrategy = serde_yaml::from_str(yaml).unwrap();
        assert!(matches!(fs, FallbackStrategy::PromptTemplate));

        let yaml = r#"agents-md-section"#;
        let fs: FallbackStrategy = serde_yaml::from_str(yaml).unwrap();
        assert!(matches!(fs, FallbackStrategy::AgentsMdSection));
    }

    #[test]
    fn custom_strategy_parses() {
        let yaml = r#"my-custom-thing"#;
        let fs: FallbackStrategy = serde_yaml::from_str(yaml).unwrap();
        match fs {
            FallbackStrategy::Custom(s) => assert_eq!(s, "my-custom-thing"),
            other => panic!("expected Custom, got {other:?}"),
        }
    }
}

// ===========================================================================
// Target
// ===========================================================================

mod target {
    use jacq::targets::Target;

    #[test]
    fn from_str_all_targets() {
        let cases = [
            ("claude-code", Target::ClaudeCode),
            ("opencode", Target::OpenCode),
            ("codex", Target::Codex),
            ("cursor", Target::Cursor),
            ("antigravity", Target::Antigravity),
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
}

// ===========================================================================
// Capability matrices
// ===========================================================================

mod matrices {
    use jacq::targets::*;

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
}

// ===========================================================================
// StringOrVec
// ===========================================================================

mod string_or_vec {
    use jacq::ir::StringOrVec;

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
    use jacq::ir::SkillFrontmatter;

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
    fn unknown_fields_captured_in_extra() {
        let yaml = r#"
description: "test"
custom-field: "preserved"
another-field: 42
"#;
        let fm: SkillFrontmatter = serde_yaml::from_str(yaml).unwrap();
        assert!(fm.extra.contains_key("custom-field"));
        assert!(fm.extra.contains_key("another-field"));
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
        assert!(fm.extra.is_empty());
    }
}

// ===========================================================================
// HookEvent
// ===========================================================================

mod hook_event {
    use jacq::ir::HookEvent;

    #[test]
    fn known_events() {
        let cases = [
            ("pre-tool-use", HookEvent::PreToolUse),
            ("post-tool-use", HookEvent::PostToolUse),
            ("stop", HookEvent::Stop),
        ];
        for (input, expected) in cases {
            let parsed: HookEvent = serde_yaml::from_str(&format!("\"{input}\"")).unwrap();
            assert_eq!(parsed, expected);
        }
    }

    #[test]
    fn unknown_event_is_captured() {
        let parsed: HookEvent = serde_yaml::from_str("\"on-session-start\"").unwrap();
        assert_eq!(parsed, HookEvent::Unknown);
    }
}

// ===========================================================================
// Permission
// ===========================================================================

mod permission {
    use jacq::ir::Permission;

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
    fn unknown_permission_captured() {
        let yaml = r#""database-access""#;
        let perm: Permission = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(perm, Permission::Unknown);
    }
}
