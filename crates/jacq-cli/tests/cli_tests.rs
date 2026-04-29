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
        assert!(
            tmp.path()
                .join("claude-code")
                .join(".claude-plugin")
                .join("plugin.json")
                .exists()
        );
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

        assert!(
            tmp.path()
                .join("claude-code")
                .join(".claude-plugin")
                .join("plugin.json")
                .exists()
        );
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
        // used to fail. Now the parser runs a compatibility probe and emits
        // for every target the plugin can build for without errors. The
        // skills-only fixture is universally compatible, so all five dist
        // subdirs should appear.
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
        // The original bug case — claude-code must always be present
        assert!(
            tmp.path()
                .join("claude-code")
                .join(".claude-plugin")
                .join("plugin.json")
                .exists()
        );
        // The expanded behavior — every compatible target gets a dist subdir
        for target in ["opencode", "codex", "cursor", "openclaw"] {
            assert!(
                tmp.path().join(target).exists(),
                "expected dist/{target}/ for skills-only plugin (universally compatible)"
            );
        }
    }

    #[test]
    fn in_place_build_writes_wrappers_at_repo_root() {
        // No --output flag → in-place mode. The build directory IS the source
        // repo: jacq writes target wrappers (.claude-plugin/plugin.json etc.)
        // at the input path, not under a dist/ subdirectory.
        let tmp = TempDir::new().unwrap();
        let plugin_dir = tmp.path().join("in-place-target");
        std::fs::create_dir(&plugin_dir).unwrap();

        // Copy the claude-code-plugin fixture into the temp dir so the build
        // can mutate it without touching repo state.
        let fixture_dir = fixture("claude-code-plugin");
        copy_dir_recursive(&fixture_dir, &plugin_dir).unwrap();

        // Pre-build: no .codex-plugin should exist yet
        assert!(!plugin_dir.join(".codex-plugin").exists());

        let output = jacq()
            .args(["build", plugin_dir.to_str().unwrap(), "--target", "codex"])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "in-place build failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        // Post-build: codex wrapper exists at the SOURCE repo root
        assert!(
            plugin_dir.join(".codex-plugin/plugin.json").exists(),
            "expected .codex-plugin/plugin.json at source repo root"
        );

        // Components are NOT duplicated into a dist/ subdir
        assert!(!plugin_dir.join("dist").exists());
        assert!(!plugin_dir.join("codex").exists());

        // Original commands stay put — in-place mode doesn't touch source components
        assert!(plugin_dir.join("commands").exists());
    }

    #[test]
    fn in_place_build_polyglot_writes_all_target_wrappers() {
        // Multi-target in-place build: each target's wrapper lands at its
        // canonical location at the repo root, no clobbering between them.
        let tmp = TempDir::new().unwrap();
        let plugin_dir = tmp.path().join("polyglot");
        std::fs::create_dir(&plugin_dir).unwrap();
        copy_dir_recursive(&fixture("ir-plugin"), &plugin_dir).unwrap();

        let output = jacq()
            .args(["build", plugin_dir.to_str().unwrap()])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "polyglot build failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        // Both declared targets wrote their wrappers in-place
        assert!(plugin_dir.join(".claude-plugin/plugin.json").exists());
        assert!(plugin_dir.join("package.json").exists()); // OpenCode wrapper
    }

    #[test]
    fn output_flag_preserves_isolated_mode() {
        // --output is the explicit opt-in for the legacy isolated-tree emit.
        // The source repo MUST NOT be mutated when --output is provided.
        let tmp = TempDir::new().unwrap();
        let out_dir = tmp.path().join("dist");
        let plugin_dir = tmp.path().join("source");
        std::fs::create_dir(&plugin_dir).unwrap();
        copy_dir_recursive(&fixture("claude-code-plugin"), &plugin_dir).unwrap();

        let before_dir_listing = list_dir(&plugin_dir);

        let output = jacq()
            .args([
                "build",
                plugin_dir.to_str().unwrap(),
                "--target",
                "claude-code",
                "-o",
                out_dir.to_str().unwrap(),
            ])
            .output()
            .unwrap();
        assert!(output.status.success());

        // Output landed in dist/claude-code/, NOT at source root
        assert!(
            out_dir
                .join("claude-code/.claude-plugin/plugin.json")
                .exists()
        );

        // Source repo unchanged
        let after_dir_listing = list_dir(&plugin_dir);
        assert_eq!(
            before_dir_listing, after_dir_listing,
            "source repo modified by --output build (should be untouched)"
        );
    }
}

fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ft = entry.file_type()?;
        let dst_path = dst.join(entry.file_name());
        if ft.is_dir() {
            copy_dir_recursive(&entry.path(), &dst_path)?;
        } else {
            std::fs::copy(entry.path(), dst_path)?;
        }
    }
    Ok(())
}

fn list_dir(dir: &std::path::Path) -> Vec<String> {
    let mut entries: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    entries.sort();
    entries
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
    fn refuses_to_clobber_existing_plugin_yaml() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("plugin.yaml"), "# pre-existing\n").unwrap();

        let output = jacq()
            .args(["init", tmp.path().to_str().unwrap()])
            .output()
            .unwrap();
        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("plugin.yaml"),
            "expected error to mention plugin.yaml, got: {stderr}"
        );
    }

    #[test]
    fn scaffolds_into_existing_dir_without_plugin_yaml() {
        // Option C: bare scaffold may add a manifest to an existing repo,
        // as long as no plugin.yaml is already there. Pre-existing files
        // (e.g. README, .git) should be preserved untouched.
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("README.md"), "hello\n").unwrap();

        let output = jacq()
            .args(["init", tmp.path().to_str().unwrap()])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );

        assert!(tmp.path().join("plugin.yaml").exists());
        assert_eq!(
            std::fs::read_to_string(tmp.path().join("README.md")).unwrap(),
            "hello\n",
            "pre-existing README must not be modified"
        );
    }

    #[test]
    fn from_refuses_non_empty_target() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("stray.txt"), "x").unwrap();

        let output = jacq()
            .args([
                "init",
                tmp.path().to_str().unwrap(),
                "--from",
                fixture("claude-code-plugin").to_str().unwrap(),
            ])
            .output()
            .unwrap();
        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("not empty"),
            "expected 'not empty' error, got: {stderr}"
        );
    }

    #[test]
    fn bare_init_uses_cwd_basename() {
        // No NAME arg: derive plugin name from cwd's basename, write into cwd.
        let tmp = TempDir::new().unwrap();
        let plugin_dir = tmp.path().join("my-cwd-plugin");
        std::fs::create_dir(&plugin_dir).unwrap();

        let output = jacq()
            .current_dir(&plugin_dir)
            .args(["init"])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );

        assert!(plugin_dir.join("plugin.yaml").exists());
        let yaml = std::fs::read_to_string(plugin_dir.join("plugin.yaml")).unwrap();
        assert!(
            yaml.contains("name: my-cwd-plugin"),
            "expected name derived from cwd basename, got: {yaml}"
        );
    }
}
