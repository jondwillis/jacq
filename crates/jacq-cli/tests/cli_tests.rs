//! CLI integration tests — run jacq as a subprocess and check output.

use std::path::PathBuf;
use std::process::Command;

use tempfile::TempDir;

fn jacq() -> Command {
    Command::new(env!("CARGO_BIN_EXE_jacq"))
}

fn fixture(name: &str) -> PathBuf {
    // Fixtures live in the sibling jacq-core crate — we share them across
    // the workspace rather than duplicating.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("jacq-core")
        .join("tests")
        .join("fixtures")
        .join(name)
}

// ===========================================================================
// jacq validate
// ===========================================================================

mod validate {
    use super::*;

    #[test]
    fn claude_code_plugin_validates() {
        let output = jacq()
            .args(["validate", fixture("claude-code-plugin").to_str().unwrap()])
            .output()
            .unwrap();
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("test-plugin"));
        assert!(stdout.contains("2 skill(s)"));
    }

    #[test]
    fn ir_plugin_validates() {
        let output = jacq()
            .args(["validate", fixture("ir-plugin").to_str().unwrap()])
            .output()
            .unwrap();
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("ir-test-plugin"));
        assert!(stdout.contains("claude-code: OK"));
        assert!(stdout.contains("opencode: OK"));
    }

    #[test]
    fn nonexistent_dir_fails() {
        let output = jacq()
            .args(["validate", "/nonexistent/path"])
            .output()
            .unwrap();
        assert!(!output.status.success());
    }

    #[test]
    fn empty_dir_fails() {
        let output = jacq()
            .args(["validate", fixture("empty-dir").to_str().unwrap()])
            .output()
            .unwrap();
        assert!(!output.status.success());
    }
}

// ===========================================================================
// jacq build
// ===========================================================================

mod build {
    use super::*;

    #[test]
    fn builds_ir_plugin() {
        let tmp = TempDir::new().unwrap();
        let output = jacq()
            .args([
                "build",
                fixture("ir-plugin").to_str().unwrap(),
                "-o",
                tmp.path().to_str().unwrap(),
            ])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "build failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        // Should have output directories for declared targets
        assert!(tmp.path().join("claude-code").join("plugin.json").exists());
        assert!(tmp.path().join("opencode").join("package.json").exists());
    }

    #[test]
    fn builds_single_target() {
        let tmp = TempDir::new().unwrap();
        let output = jacq()
            .args([
                "build",
                fixture("ir-plugin").to_str().unwrap(),
                "--target",
                "claude-code",
                "-o",
                tmp.path().to_str().unwrap(),
            ])
            .output()
            .unwrap();
        assert!(output.status.success());

        assert!(tmp.path().join("claude-code").join("plugin.json").exists());
        assert!(!tmp.path().join("opencode").exists());
    }

    #[test]
    fn no_targets_fails_for_ir_without_inference() {
        // IR-format manifests are explicit by contract — if the user wrote
        // plugin.yaml and omitted `targets:`, that's a real declaration of
        // "I haven't decided yet," not a layout signal we should infer from.
        // Native formats (.claude-plugin/, .cursor-plugin/, etc.) infer their
        // target from the manifest path; only IR can hit this error path.
        let tmp = TempDir::new().unwrap();
        let plugin_dir = tmp.path().join("ir-no-targets");
        std::fs::create_dir(&plugin_dir).unwrap();
        std::fs::write(
            plugin_dir.join("plugin.yaml"),
            "ir_version: \"0.1\"\nname: ir-no-targets\nversion: \"0.1.0\"\n",
        )
        .unwrap();

        let out_dir = tmp.path().join("dist");
        let output = jacq()
            .args([
                "build",
                plugin_dir.to_str().unwrap(),
                "-o",
                out_dir.to_str().unwrap(),
            ])
            .output()
            .unwrap();
        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("no targets"),
            "expected 'no targets' in stderr, got: {stderr}"
        );
    }

    #[test]
    fn native_claude_plugin_infers_target() {
        // The bug-report behavior: a native CC plugin with no `targets:` field
        // used to fail. Now the parser infers `[claude-code]` from the
        // .claude-plugin/plugin.json layout, and build succeeds.
        let tmp = TempDir::new().unwrap();
        let output = jacq()
            .args([
                "build",
                fixture("claude-code-plugin").to_str().unwrap(),
                "-o",
                tmp.path().to_str().unwrap(),
            ])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "build should succeed via target inference. stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("inferred targets"),
            "build should print an inference note. stderr: {stderr}"
        );
        assert!(tmp.path().join("claude-code").join("plugin.json").exists());
    }
}

// ===========================================================================
// jacq pack
// ===========================================================================

mod pack {
    use super::*;

    #[test]
    fn packs_single_target_archive() {
        let tmp = TempDir::new().unwrap();
        let output = jacq()
            .args([
                "pack",
                fixture("ir-plugin").to_str().unwrap(),
                "--target",
                "claude-code",
                "-o",
                tmp.path().to_str().unwrap(),
            ])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "pack failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        // Archive exists, has non-trivial size, and matches the expected name.
        let archive = tmp.path().join("ir-test-plugin-2.0.0-claude-code.tar.gz");
        assert!(archive.exists(), "archive missing at {}", archive.display());
        let size = std::fs::metadata(&archive).unwrap().len();
        assert!(size > 100, "archive suspiciously small: {size} bytes");
    }

    #[test]
    fn claude_code_pack_emits_marketplace_json() {
        let tmp = TempDir::new().unwrap();
        let output = jacq()
            .args([
                "pack",
                fixture("ir-plugin").to_str().unwrap(),
                "--target",
                "claude-code",
                "-o",
                tmp.path().to_str().unwrap(),
            ])
            .output()
            .unwrap();
        assert!(output.status.success());

        let marketplace = tmp.path().join("ir-test-plugin-marketplace.json");
        assert!(marketplace.exists());
        let json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&marketplace).unwrap()).unwrap();
        assert_eq!(json["name"], "ir-test-plugin");
        assert_eq!(json["version"], "2.0.0");
        assert_eq!(json["archive"], "ir-test-plugin-2.0.0-claude-code.tar.gz");
    }

    #[test]
    fn non_claude_targets_skip_marketplace_json() {
        let tmp = TempDir::new().unwrap();
        let output = jacq()
            .args([
                "pack",
                fixture("ir-plugin").to_str().unwrap(),
                "--target",
                "opencode",
                "-o",
                tmp.path().to_str().unwrap(),
            ])
            .output()
            .unwrap();
        assert!(output.status.success());

        // OpenCode archive present, no marketplace JSON.
        assert!(
            tmp.path()
                .join("ir-test-plugin-2.0.0-opencode.tar.gz")
                .exists()
        );
        assert!(!tmp.path().join("ir-test-plugin-marketplace.json").exists());
    }
}

// ===========================================================================
// jacq inspect
// ===========================================================================

mod inspect {
    use super::*;

    #[test]
    fn inspects_ir_plugin() {
        let output = jacq()
            .args(["inspect", fixture("ir-plugin").to_str().unwrap()])
            .output()
            .unwrap();
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("ir-test-plugin"));
        assert!(stdout.contains("Capability Matrix"));
        assert!(stdout.contains("Full"));
        assert!(stdout.contains("Partial"));
    }
}

// ===========================================================================
// jacq init
// ===========================================================================

mod init {
    use super::*;

    #[test]
    fn scaffolds_new_plugin() {
        let tmp = TempDir::new().unwrap();
        let plugin_dir = tmp.path().join("my-plugin");

        let output = jacq()
            .args(["init", plugin_dir.to_str().unwrap()])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );

        assert!(plugin_dir.join("plugin.yaml").exists());
        assert!(plugin_dir.join("skills").join("example.md").exists());
        assert!(plugin_dir.join("instructions").join("rules.md").exists());

        // Verify the generated plugin.yaml is valid
        let yaml = std::fs::read_to_string(plugin_dir.join("plugin.yaml")).unwrap();
        assert!(yaml.contains("name: my-plugin"));
        assert!(yaml.contains("ir_version"));
    }

    #[test]
    fn imports_from_existing() {
        let tmp = TempDir::new().unwrap();
        let plugin_dir = tmp.path().join("imported");

        let output = jacq()
            .args([
                "init",
                plugin_dir.to_str().unwrap(),
                "--from",
                fixture("claude-code-plugin").to_str().unwrap(),
            ])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );

        assert!(plugin_dir.join("plugin.yaml").exists());
        // init --from preserves the original source layout (commands/ for CC plugins)
        assert!(plugin_dir.join("commands").join("greet.md").exists());
        assert!(plugin_dir.join("commands").join("farewell.md").exists());
    }

    #[test]
    fn existing_dir_fails() {
        let tmp = TempDir::new().unwrap();
        // tmp.path() already exists
        let output = jacq()
            .args(["init", tmp.path().to_str().unwrap()])
            .output()
            .unwrap();
        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("already exists"));
    }
}
