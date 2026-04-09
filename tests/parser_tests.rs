//! Integration tests for the parser — reads real plugin fixtures from disk.

use std::path::PathBuf;

use jacq::parser::parse_plugin;
use jacq::targets::Target;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

// ===========================================================================
// Claude Code native format (.claude-plugin/plugin.json + commands/)
// ===========================================================================

mod claude_code_native {
    use super::*;

    #[test]
    fn parses_manifest() {
        let ir = parse_plugin(&fixture("claude-code-plugin")).unwrap();
        assert_eq!(ir.manifest.name, "test-plugin");
        assert_eq!(ir.manifest.version, "1.0.0");
        assert_eq!(ir.manifest.description, "A test Claude Code plugin");
        assert_eq!(ir.manifest.license, Some("MIT".to_string()));
    }

    #[test]
    fn discovers_commands_as_skills() {
        let ir = parse_plugin(&fixture("claude-code-plugin")).unwrap();
        assert_eq!(ir.skills.len(), 2);

        let names: Vec<&str> = ir.skills.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"greet"));
        assert!(names.contains(&"farewell"));
    }

    #[test]
    fn parses_skill_frontmatter() {
        let ir = parse_plugin(&fixture("claude-code-plugin")).unwrap();
        let greet = ir.skills.iter().find(|s| s.name == "greet").unwrap();

        assert_eq!(
            greet.frontmatter.description.as_deref(),
            Some("Greet the user")
        );
        assert!(greet.frontmatter.argument_hint.is_some());
    }

    #[test]
    fn parses_skill_body() {
        let ir = parse_plugin(&fixture("claude-code-plugin")).unwrap();
        let greet = ir.skills.iter().find(|s| s.name == "greet").unwrap();
        assert!(greet.body.contains("$ARGUMENTS"));
    }

    #[test]
    fn ir_fields_default_for_native_plugin() {
        let ir = parse_plugin(&fixture("claude-code-plugin")).unwrap();
        assert!(ir.manifest.ir_version.is_none());
        assert!(ir.manifest.targets.is_empty());
        assert!(ir.manifest.requires.is_none());
    }

    #[test]
    fn no_agents_hooks_mcp_instructions() {
        let ir = parse_plugin(&fixture("claude-code-plugin")).unwrap();
        assert!(ir.agents.is_empty());
        assert!(ir.hooks.is_empty());
        assert!(ir.mcp_servers.is_empty());
        assert!(ir.instructions.is_empty());
        assert!(ir.target_overrides.is_empty());
    }

    #[test]
    fn source_dir_is_set() {
        let ir = parse_plugin(&fixture("claude-code-plugin")).unwrap();
        assert!(ir.source_dir.exists());
        assert!(ir.source_dir.is_absolute());
    }
}

// ===========================================================================
// IR format (plugin.yaml + skills/ + agents/ + hooks/ + mcp/ + instructions/)
// ===========================================================================

mod ir_format {
    use super::*;

    #[test]
    fn parses_ir_manifest() {
        let ir = parse_plugin(&fixture("ir-plugin")).unwrap();
        assert_eq!(ir.manifest.name, "ir-test-plugin");
        assert_eq!(ir.manifest.version, "2.0.0");
        assert_eq!(ir.manifest.ir_version, Some("0.1".to_string()));
        assert_eq!(
            ir.manifest.targets,
            vec![Target::ClaudeCode, Target::OpenCode]
        );
    }

    #[test]
    fn parses_requirements() {
        let ir = parse_plugin(&fixture("ir-plugin")).unwrap();
        let reqs = ir.manifest.requires.as_ref().unwrap();
        assert_eq!(reqs.capabilities.len(), 3);
        assert_eq!(reqs.permissions.len(), 2);
    }

    #[test]
    fn parses_fallbacks() {
        let ir = parse_plugin(&fixture("ir-plugin")).unwrap();
        assert!(ir.manifest.fallbacks.contains_key("hooks.pre-tool-use"));
    }

    #[test]
    fn discovers_skills() {
        let ir = parse_plugin(&fixture("ir-plugin")).unwrap();
        assert_eq!(ir.skills.len(), 1);
        assert_eq!(ir.skills[0].name, "search");
        assert_eq!(
            ir.skills[0].frontmatter.description.as_deref(),
            Some("Search the codebase")
        );
        assert_eq!(ir.skills[0].frontmatter.color.as_deref(), Some("green"));
    }

    #[test]
    fn discovers_agents() {
        let ir = parse_plugin(&fixture("ir-plugin")).unwrap();
        assert_eq!(ir.agents.len(), 1);
        assert_eq!(ir.agents[0].name, "reviewer");
        assert_eq!(
            ir.agents[0].frontmatter.description.as_deref(),
            Some("Code review agent")
        );
        assert_eq!(ir.agents[0].frontmatter.model.as_deref(), Some("sonnet"));
        assert!(ir.agents[0].body.contains("code review agent"));
    }

    #[test]
    fn discovers_hooks() {
        let ir = parse_plugin(&fixture("ir-plugin")).unwrap();
        assert_eq!(ir.hooks.len(), 1);
        assert_eq!(ir.hooks[0].name, "lint-check");
        assert_eq!(ir.hooks[0].command, "eslint --check");
        assert_eq!(ir.hooks[0].timeout, Some(5000));
    }

    #[test]
    fn discovers_mcp_servers() {
        let ir = parse_plugin(&fixture("ir-plugin")).unwrap();
        assert_eq!(ir.mcp_servers.len(), 1);
        assert_eq!(ir.mcp_servers[0].name, "db-server");
        assert_eq!(ir.mcp_servers[0].command, "npx");
        assert_eq!(ir.mcp_servers[0].args, vec!["-y", "@test/db-mcp"]);
        assert_eq!(
            ir.mcp_servers[0].env.get("DB_URL").map(|s| s.as_str()),
            Some("postgres://localhost/test")
        );
    }

    #[test]
    fn discovers_instructions() {
        let ir = parse_plugin(&fixture("ir-plugin")).unwrap();
        assert_eq!(ir.instructions.len(), 1);
        assert_eq!(ir.instructions[0].name, "rules");
        assert!(ir.instructions[0].body.contains("Always write tests"));
    }

    #[test]
    fn discovers_target_overrides() {
        let ir = parse_plugin(&fixture("ir-plugin")).unwrap();
        assert!(ir.target_overrides.contains_key(&Target::OpenCode));
        let opencode_files = &ir.target_overrides[&Target::OpenCode];
        assert_eq!(opencode_files.len(), 1);
        assert_eq!(
            opencode_files[0].path.to_str().unwrap(),
            "custom-tool.ts"
        );
        let content = String::from_utf8_lossy(&opencode_files[0].content);
        assert!(content.contains("searchTool"));
    }
}

// ===========================================================================
// Error cases
// ===========================================================================

mod errors {
    use super::*;

    #[test]
    fn no_manifest_returns_error() {
        let result = parse_plugin(&fixture("empty-dir"));
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, jacq::error::JacqError::NoManifest { .. }),
            "expected NoManifest, got {err:?}"
        );
    }

    #[test]
    fn nonexistent_dir_returns_error() {
        let result = parse_plugin(&fixture("does-not-exist"));
        assert!(result.is_err());
    }

    #[test]
    fn bad_frontmatter_returns_error() {
        let result = parse_plugin(&fixture("bad-frontmatter"));
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, jacq::error::JacqError::InvalidFrontmatter { .. }),
            "expected InvalidFrontmatter, got {err:?}"
        );
    }
}

// ===========================================================================
// Real plugin: parse notes-app-plugin from /Volumes/Sidecar
// ===========================================================================

mod real_plugins {
    use super::*;
    use std::path::Path;

    #[test]
    fn parse_notes_app_plugin() {
        let path = Path::new("/Volumes/Sidecar/notes-app-plugin");
        if !path.exists() {
            // Skip if the real plugin isn't available (CI, etc.)
            return;
        }

        let ir = parse_plugin(path).unwrap();
        assert_eq!(ir.manifest.name, "notes-app");
        assert_eq!(ir.manifest.version, "1.0.0");
        assert!(!ir.skills.is_empty(), "should discover commands/");

        let notes = ir.skills.iter().find(|s| s.name == "notes");
        assert!(notes.is_some(), "should find notes.md command");

        let notes = notes.unwrap();
        assert!(notes
            .frontmatter
            .description
            .as_ref()
            .unwrap()
            .contains("Notes.app"));
        assert!(notes.body.contains("AppleScript"));
    }
}
