//! End-to-end conformance tests for jacq's emitters.
//!
//! Each test packs the standard `claude-code-plugin` fixture with
//! `jacq build`, then validates the corresponding `dist/<target>/` output
//! against the target's real loader (where one exists). The point is to
//! catch the gap between "jacq emits something" and "the target CLI accepts
//! it" — a gap the existing roundtrip tests cannot see, because they only
//! verify that jacq can re-parse its own output.
//!
//! All tests here are `#[ignore]`'d by default. They shell out to external
//! binaries (`claude`, the bundled `Codex.app/.../codex`, `node`, `openclaw`)
//! that may not be present on every dev machine or CI runner. Run with:
//!
//!     cargo test --test e2e_conformance -- --ignored
//!
//! Each test that depends on an external binary detects its presence first
//! and skips cleanly (printing the reason) when it isn't available — better
//! than a noisy false failure on a missing optional dep.
//!
//! Tests that fail here describe real emitter bugs. Turning each red
//! assertion green is the emitter-fix work that follows.

use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("jacq-core")
        .join("tests")
        .join("fixtures")
        .join(name)
}

fn jacq() -> Command {
    Command::new(env!("CARGO_BIN_EXE_jacq"))
}

fn binary_in_path(name: &str) -> bool {
    Command::new("which")
        .arg(name)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Build `claude-code-plugin` for all inferred targets and return the dist tempdir.
fn build_fixture_to_temp() -> TempDir {
    let tmp = TempDir::new().expect("create dist tempdir");
    let output = jacq()
        .args([
            "build",
            fixture("claude-code-plugin").to_str().unwrap(),
            "-o",
            tmp.path().to_str().unwrap(),
        ])
        .output()
        .expect("jacq build failed to spawn");
    assert!(
        output.status.success(),
        "jacq build failed (fix this before debugging downstream conformance):\n\
         --- stdout ---\n{}\n--- stderr ---\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    tmp
}

fn copy_dir_recursive(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).expect("mkdir dst");
    for entry in std::fs::read_dir(src).expect("read src dir") {
        let entry = entry.expect("read entry");
        let target = dst.join(entry.file_name());
        if entry.file_type().expect("file type").is_dir() {
            copy_dir_recursive(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).expect("copy file");
        }
    }
}

// ---------------------------------------------------------------------------
// claude-code: shell out to `claude plugin validate <dir>`
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn claude_code_validates() {
    if !binary_in_path("claude") {
        eprintln!("SKIP: claude binary not found in PATH");
        return;
    }
    let dist = build_fixture_to_temp();
    let target_dir = dist.path().join("claude-code");

    let output = Command::new("claude")
        .args(["plugin", "validate"])
        .arg(&target_dir)
        .output()
        .expect("claude plugin validate failed to spawn");

    assert!(
        output.status.success(),
        "claude plugin validate failed for {}\n--- stdout ---\n{}\n--- stderr ---\n{}",
        target_dir.display(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

// ---------------------------------------------------------------------------
// codex: filesystem layout check (no binary required)
//
// Codex's loader (vendor/codex/codex-rs/core-skills/src/loader.rs) walks
// `$CODEX_HOME/skills/<name>/SKILL.md` — one directory per skill, the file
// itself always called SKILL.md. Verify the emitter produces that shape for
// every parsed skill, with no extras and no clobbering.
//
// We deliberately skip a runtime probe of the codex binary: skill loading
// only fires when a real session starts, which requires API auth + network.
// This filesystem check is the strongest signal we can give offline.
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn codex_skill_layout_is_correct() {
    let dist = build_fixture_to_temp();
    let skills_dir = dist.path().join("codex").join("skills");

    assert!(
        skills_dir.exists(),
        "no skills/ directory in codex output at {} — fixture has 2 commands \
         that should emit as skills",
        skills_dir.display()
    );

    // The fixture has commands/greet.md and commands/farewell.md, which jacq
    // treats as skills (Claude Code's commands and skills share an IR type).
    let expected: &[&str] = &["greet", "farewell"];

    for skill_name in expected {
        let skill_md = skills_dir.join(skill_name).join("SKILL.md");
        assert!(
            skill_md.exists(),
            "expected Codex skill at {} — Codex requires `skills/<name>/SKILL.md` \
             (one directory per skill, file always named SKILL.md)",
            skill_md.display()
        );
    }

    // Count every entry under skills/ — catches the SKILL.md collision bug
    // (multiple skills clobbering each other into a single file) and any
    // stray files the emitter shouldn't have written.
    let actual: Vec<String> = std::fs::read_dir(&skills_dir)
        .expect("read skills/")
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    let mut actual_sorted = actual.clone();
    actual_sorted.sort();
    let mut expected_sorted: Vec<String> = expected.iter().map(|s| s.to_string()).collect();
    expected_sorted.sort();
    assert_eq!(
        actual_sorted, expected_sorted,
        "codex skills/ contents don't match the source fixture's skill list. \
         Likely cause: skill-name parsing collapses every directory-style \
         skill to the literal `SKILL` (parser reads the file stem of \
         `SKILL.md` instead of the parent directory name)."
    );
}

// ---------------------------------------------------------------------------
// cursor: invoke the vendored validate-template.mjs script.
//
// Cursor is IDE-bound and has no headless plugin loader; the marketplace
// template repo ships a Node.js validator that checks plugin manifests
// offline against the marketplace schema. We stage the emitted single plugin
// into a marketplace shape (plugins/<name>/...) before invoking it.
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn cursor_validates_via_marketplace_script() {
    if !binary_in_path("node") {
        eprintln!("SKIP: node not found in PATH (required by cursor validate-template.mjs)");
        return;
    }
    let validator = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("vendor")
        .join("cursor-marketplace-template")
        .join("scripts")
        .join("validate-template.mjs");
    if !validator.exists() {
        eprintln!(
            "SKIP: vendored cursor validator not found at {}",
            validator.display()
        );
        return;
    }

    let dist = build_fixture_to_temp();
    let cursor_dir = dist.path().join("cursor");

    // Stage a synthetic marketplace wrapping jacq's single emitted plugin.
    // The validator (validate-template.mjs:251) reads
    // `<cwd>/.cursor-plugin/marketplace.json` and walks each entry's `source`
    // path under `metadata.pluginRoot`, then validates each plugin's own
    // `.cursor-plugin/plugin.json`. The plugin name in the marketplace entry
    // must match the per-plugin manifest's name (validator:331-335).
    let staging = TempDir::new().expect("create marketplace staging tempdir");
    let plugin_name = "test-plugin"; // matches the fixture's manifest name
    let plugin_slot = staging.path().join("plugins").join(plugin_name);
    copy_dir_recursive(&cursor_dir, &plugin_slot);

    let marketplace_dir = staging.path().join(".cursor-plugin");
    std::fs::create_dir_all(&marketplace_dir).expect("mkdir .cursor-plugin");
    let marketplace_json = format!(
        r#"{{
  "name": "jacq-conformance-marketplace",
  "owner": {{ "name": "jacq", "email": "noreply@example.com" }},
  "metadata": {{
    "description": "Synthetic marketplace for cursor conformance testing",
    "version": "0.1.0",
    "pluginRoot": "plugins"
  }},
  "plugins": [
    {{ "name": "{plugin_name}", "source": "{plugin_name}", "description": "fixture under test" }}
  ]
}}"#
    );
    std::fs::write(marketplace_dir.join("marketplace.json"), marketplace_json)
        .expect("write marketplace.json");

    let output = Command::new("node")
        .arg(&validator)
        .current_dir(staging.path())
        .output()
        .expect("node validate-template.mjs failed to spawn");

    assert!(
        output.status.success(),
        "cursor marketplace validator failed:\n--- stdout ---\n{}\n--- stderr ---\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

// ---------------------------------------------------------------------------
// opencode: deferred — `opencode plugin <module>` requires an NPM module
// name and our emitted output is a metadata-only package.json with no entry
// point. Wiring this test honestly requires fixing the emitter first.
// Tracked here so the gap is visible in `cargo test` output.
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn opencode_plugin_loads() {
    eprintln!(
        "SKIP: opencode validation not yet wired. \
         `opencode plugin <module>` accepts only NPM module names; \
         jacq's opencode emitter produces a metadata-only package.json with no \
         entry point. Re-enable this test when both halves exist."
    );
}

// ---------------------------------------------------------------------------
// openclaw: install --link into a sandboxed $HOME, list, uninstall.
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn openclaw_plugin_install_cycle() {
    if !binary_in_path("openclaw") {
        eprintln!("SKIP: openclaw binary not found in PATH");
        return;
    }
    let dist = build_fixture_to_temp();
    let openclaw_dir = dist.path().join("openclaw");
    let home = TempDir::new().expect("create HOME tempdir");

    let install = Command::new("openclaw")
        .env("HOME", home.path())
        .args(["plugins", "install", "--link"])
        .arg(&openclaw_dir)
        .output()
        .expect("openclaw install failed to spawn");
    assert!(
        install.status.success(),
        "openclaw plugins install --link failed:\n--- stdout ---\n{}\n--- stderr ---\n{}",
        String::from_utf8_lossy(&install.stdout),
        String::from_utf8_lossy(&install.stderr),
    );

    let list = Command::new("openclaw")
        .env("HOME", home.path())
        .args(["plugins", "list", "--json"])
        .output()
        .expect("openclaw list failed to spawn");
    assert!(
        list.status.success(),
        "openclaw plugins list --json failed:\n--- stderr ---\n{}",
        String::from_utf8_lossy(&list.stderr),
    );
    let listing = String::from_utf8_lossy(&list.stdout);
    assert!(
        listing.contains("test-plugin"),
        "expected test-plugin in `openclaw plugins list --json`:\n{listing}"
    );
}
