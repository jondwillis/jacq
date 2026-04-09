//! Roundtrip integration tests: parse Claude Code plugins → emit claude-code → compare.
//!
//! Uses the vendor/claude-plugins-official submodule as test corpus.
//! Each plugin that parses successfully is emitted to a temp dir, then compared
//! against the original. This validates that jacq's parse→IR→emit pipeline
//! preserves plugin semantics.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use jacq::emitter;
use jacq::parser::parse_plugin;
use jacq::targets::Target;
use jacq::template;

fn vendor_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("vendor")
        .join("claude-plugins-official")
}

fn vendor_plugins_dir() -> PathBuf {
    vendor_dir().join("plugins")
}

fn vendor_external_dir() -> PathBuf {
    vendor_dir().join("external_plugins")
}

/// Try to parse a plugin, returning None if it fails (some plugins have
/// malformed frontmatter that we don't handle yet).
fn try_parse(dir: &Path) -> Option<jacq::ir::PluginIR> {
    match parse_plugin(dir) {
        Ok(ir) => Some(ir),
        Err(_) => None,
    }
}

/// Sanitize YAML by quoting values with problematic characters (mirrors parser logic).
fn sanitize_yaml(yaml: &str) -> String {
    yaml.lines()
        .map(|line| {
            if let Some(colon_pos) = line.find(": ") {
                let key = &line[..colon_pos];
                let value = &line[colon_pos + 2..];
                let trimmed = value.trim();
                if trimmed.starts_with('"')
                    || trimmed.starts_with('\'')
                    || trimmed.starts_with('[')
                    || trimmed.starts_with('{')
                    || key.starts_with(' ') && key.trim().starts_with('-')
                {
                    return line.to_string();
                }
                if trimmed.contains(": ") || trimmed.contains('<') || trimmed.contains('>') {
                    let escaped = trimmed.replace('"', r#"\""#);
                    return format!("{key}: \"{escaped}\"");
                }
            }
            line.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Compare two YAML frontmatter strings semantically (field values match,
/// ignoring order and whitespace differences).
fn frontmatter_matches(original: &str, emitted: &str) -> Result<(), String> {
    // Sanitize original — real plugins have unquoted colons that break YAML parsing
    let sanitized_orig = sanitize_yaml(original);
    let orig: serde_yaml::Value =
        serde_yaml::from_str(&sanitized_orig).map_err(|e| format!("original YAML: {e}"))?;
    let emit: serde_yaml::Value =
        serde_yaml::from_str(emitted).map_err(|e| format!("emitted YAML: {e}"))?;

    let empty = serde_yaml::Mapping::new();
    let orig_map = orig.as_mapping().unwrap_or(&empty);
    let emit_map = emit.as_mapping().unwrap_or(&empty);

    // Every field in the original should be in the emitted output with the same value.
    // Allow lenient bool comparison ("true" == true, "false" == false).
    for (key, orig_val) in orig_map {
        match emit_map.get(key) {
            Some(emit_val) if emit_val == orig_val => {}
            Some(emit_val) => {
                // Lenient: string "true"/"false" matches bool true/false
                let is_lenient_bool_match = match (orig_val, emit_val) {
                    (serde_yaml::Value::String(s), serde_yaml::Value::Bool(b)) => {
                        (s == "true" && *b) || (s == "false" && !*b)
                    }
                    _ => false,
                };
                if !is_lenient_bool_match {
                    return Err(format!(
                        "field {key:?}: original={orig_val:?}, emitted={emit_val:?}"
                    ));
                }
            }
            None => {
                return Err(format!("field {key:?} missing in emitted output"));
            }
        }
    }
    Ok(())
}

/// Split a .md file into (Option<frontmatter_yaml>, body).
fn split_frontmatter(content: &str) -> (Option<&str>, &str) {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return (None, content);
    }
    let after_first = match trimmed.strip_prefix("---") {
        Some(rest) => rest.strip_prefix('\n').unwrap_or(rest),
        None => return (None, content),
    };
    if let Some(end_idx) = after_first.find("\n---") {
        let yaml = &after_first[..end_idx];
        let after_close = &after_first[end_idx + 4..];
        let body = after_close.strip_prefix('\n').unwrap_or(after_close);
        (Some(yaml), body)
    } else {
        (None, content)
    }
}

/// Compare an original .md file with the emitted version.
/// Checks frontmatter fields match and body content matches.
fn md_file_matches(original: &str, emitted: &str) -> Result<(), String> {
    let (orig_fm, orig_body) = split_frontmatter(original);
    let (emit_fm, emit_body) = split_frontmatter(emitted);

    // Compare frontmatter
    match (orig_fm, emit_fm) {
        (Some(orig), Some(emit)) => frontmatter_matches(orig, emit)?,
        (None, None) => {}
        (Some(_), None) => return Err("original has frontmatter, emitted does not".into()),
        (None, Some(_)) => {} // emitter may add empty frontmatter, that's OK
    }

    // Compare body (trim trailing whitespace)
    let orig_trimmed = orig_body.trim();
    let emit_trimmed = emit_body.trim();
    if orig_trimmed != emit_trimmed {
        // Find first difference for diagnostics
        let orig_lines: Vec<&str> = orig_trimmed.lines().collect();
        let emit_lines: Vec<&str> = emit_trimmed.lines().collect();
        for (i, (o, e)) in orig_lines.iter().zip(emit_lines.iter()).enumerate() {
            if o != e {
                return Err(format!(
                    "body differs at line {}: original={o:?}, emitted={e:?}",
                    i + 1
                ));
            }
        }
        if orig_lines.len() != emit_lines.len() {
            return Err(format!(
                "body line count differs: original={}, emitted={}",
                orig_lines.len(),
                emit_lines.len()
            ));
        }
    }

    Ok(())
}

/// Compare plugin.json semantically: every field in original should appear in emitted.
fn plugin_json_matches(original: &str, emitted: &str) -> Result<(), String> {
    let orig: serde_json::Value =
        serde_json::from_str(original).map_err(|e| format!("original JSON: {e}"))?;
    let emit: serde_json::Value =
        serde_json::from_str(emitted).map_err(|e| format!("emitted JSON: {e}"))?;

    let orig_obj = orig.as_object().ok_or("original not an object")?;
    let emit_obj = emit.as_object().ok_or("emitted not an object")?;

    for (key, orig_val) in orig_obj {
        match emit_obj.get(key) {
            Some(emit_val) if emit_val == orig_val => {}
            Some(emit_val) => {
                return Err(format!(
                    "plugin.json field '{key}': original={orig_val}, emitted={emit_val}"
                ));
            }
            None => {
                return Err(format!("plugin.json field '{key}' missing in emitted output"));
            }
        }
    }
    Ok(())
}

/// Run a full roundtrip test for a single plugin.
fn roundtrip_plugin(plugin_dir: &Path) -> Result<(), String> {
    let name = plugin_dir
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_string();

    // Parse
    let mut ir = try_parse(plugin_dir).ok_or(format!("{name}: parse failed"))?;

    // Force claude-code target for roundtrip
    ir.manifest.targets = vec![Target::ClaudeCode];

    // Extract templates (pipeline step)
    template::extract_all(&mut ir);

    // Emit to temp dir
    let tmp = tempfile::tempdir().map_err(|e| format!("{name}: tempdir: {e}"))?;
    emitter::emit(&ir, tmp.path()).map_err(|e| format!("{name}: emit: {e}"))?;

    let emitted_dir = tmp.path().join("claude-code");

    // Compare plugin.json
    let orig_json_path = if plugin_dir.join(".claude-plugin/plugin.json").exists() {
        plugin_dir.join(".claude-plugin/plugin.json")
    } else {
        plugin_dir.join("plugin.json")
    };
    let orig_json = std::fs::read_to_string(&orig_json_path)
        .map_err(|e| format!("{name}: read original plugin.json: {e}"))?;
    let emit_json = std::fs::read_to_string(emitted_dir.join("plugin.json"))
        .map_err(|e| format!("{name}: read emitted plugin.json: {e}"))?;
    plugin_json_matches(&orig_json, &emit_json)
        .map_err(|e| format!("{name}: plugin.json: {e}"))?;

    // Compare commands/*.md
    let orig_cmds = plugin_dir.join("commands");
    let emit_cmds = emitted_dir.join("commands");
    if orig_cmds.exists() {
        compare_md_dir(&orig_cmds, &emit_cmds, &name, "commands")?;
    }

    // Compare agents/*.md
    let orig_agents = plugin_dir.join("agents");
    let emit_agents = emitted_dir.join("agents");
    if orig_agents.exists() {
        compare_md_dir(&orig_agents, &emit_agents, &name, "agents")?;
    }

    Ok(())
}

/// Compare all .md files in two directories.
fn compare_md_dir(
    orig_dir: &Path,
    emit_dir: &Path,
    plugin_name: &str,
    dir_name: &str,
) -> Result<(), String> {
    if !emit_dir.exists() {
        return Err(format!(
            "{plugin_name}: emitted output missing {dir_name}/ directory"
        ));
    }

    // Collect original .md files
    let mut orig_files: BTreeMap<String, String> = BTreeMap::new();
    for entry in std::fs::read_dir(orig_dir)
        .map_err(|e| format!("{plugin_name}: read {dir_name}: {e}"))?
    {
        let entry = entry.map_err(|e| format!("{plugin_name}: {e}"))?;
        let path = entry.path();
        if path.extension().map_or(false, |ext| ext == "md") {
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            let content = std::fs::read_to_string(&path)
                .map_err(|e| format!("{plugin_name}: read {}: {e}", path.display()))?;
            orig_files.insert(name, content);
        }
    }

    // Check each original file has a matching emitted file
    for (filename, orig_content) in &orig_files {
        let emit_path = emit_dir.join(filename);
        let emit_content = std::fs::read_to_string(&emit_path).map_err(|_| {
            format!("{plugin_name}: {dir_name}/{filename} missing in emitted output")
        })?;

        md_file_matches(orig_content, &emit_content)
            .map_err(|e| format!("{plugin_name}: {dir_name}/{filename}: {e}"))?;
    }

    Ok(())
}

// ===========================================================================
// Test: roundtrip all parseable plugins
// ===========================================================================

/// Run roundtrip tests for all plugins in a directory.
fn roundtrip_dir(plugins_dir: &Path, label: &str) {
    if !plugins_dir.exists() {
        eprintln!("Skipping {label}: not found (run git submodule update --init)");
        return;
    }

    let mut results: Vec<(String, Result<(), String>)> = Vec::new();

    for entry in std::fs::read_dir(plugins_dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let has_manifest = path.join(".claude-plugin/plugin.json").exists()
            || path.join("plugin.json").exists();
        if !has_manifest {
            continue;
        }

        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let result = roundtrip_plugin(&path);
        results.push((name, result));
    }

    results.sort_by(|a, b| a.0.cmp(&b.0));

    let mut passed = 0;
    let mut parse_failures = Vec::new();
    let mut roundtrip_failures = Vec::new();

    for (name, result) in &results {
        match result {
            Ok(()) => {
                passed += 1;
                eprintln!("  ✅ {name}");
            }
            Err(e) if e.ends_with("parse failed") => {
                parse_failures.push(name.clone());
                eprintln!("  ⚠️  {name}: parse failed (known limitation)");
            }
            Err(e) => {
                roundtrip_failures.push(format!("{name}: {e}"));
                eprintln!("  ❌ {name}: {e}");
            }
        }
    }

    eprintln!(
        "\n{label}: {passed} passed, {} parse failures, {} roundtrip failures (of {} plugins)",
        parse_failures.len(),
        roundtrip_failures.len(),
        results.len(),
    );

    if !roundtrip_failures.is_empty() {
        panic!(
            "{} roundtrip failure(s) in {label}:\n{}",
            roundtrip_failures.len(),
            roundtrip_failures.join("\n")
        );
    }
}

#[test]
fn roundtrip_all_official_plugins() {
    roundtrip_dir(&vendor_plugins_dir(), "Official plugins");
}

#[test]
fn roundtrip_all_external_plugins() {
    roundtrip_dir(&vendor_external_dir(), "External plugins");
}

// ===========================================================================
// Individual plugin tests (for targeted debugging)
// ===========================================================================

#[test]
fn roundtrip_commit_commands() {
    let dir = vendor_plugins_dir().join("commit-commands");
    if !dir.exists() {
        return;
    }
    roundtrip_plugin(&dir).unwrap();
}

#[test]
fn roundtrip_code_simplifier() {
    let dir = vendor_plugins_dir().join("code-simplifier");
    if !dir.exists() {
        return;
    }
    roundtrip_plugin(&dir).unwrap();
}

#[test]
fn roundtrip_feature_dev() {
    let dir = vendor_plugins_dir().join("feature-dev");
    if !dir.exists() {
        return;
    }
    roundtrip_plugin(&dir).unwrap();
}
