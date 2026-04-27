//! Plugin directory parser.
//!
//! Reads a plugin from disk, auto-detecting the format:
//! - **IR format**: `plugin.yaml` at root
//! - **Claude Code native**: `.claude-plugin/plugin.json` or `plugin.json` at root
//!
//! Walks the directory for skills, agents, hooks, MCP servers, and instructions,
//! then assembles a `PluginIR`.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use walkdir::WalkDir;

use crate::error::{JacqError, Result};
use crate::ir::*;
use crate::targets::Target;

/// Parse a plugin directory into an in-memory IR.
pub fn parse_plugin(dir: &Path) -> Result<PluginIR> {
    if !dir.exists() {
        return Err(JacqError::DirectoryNotFound {
            path: dir.to_path_buf(),
        });
    }

    let dir = dir.canonicalize().map_err(|e| JacqError::IoWithPath {
        path: dir.to_path_buf(),
        source: e,
    })?;

    let (mut manifest, manifest_format) = parse_manifest(&dir)?;

    // Format-based target inference: if the manifest doesn't declare targets,
    // fall back to whatever the detected manifest layout implies. Per-target
    // native formats (.claude-plugin/, .cursor-plugin/, etc.) carry the signal
    // unambiguously — see `infer_default_targets` for the policy table.
    let targets_inferred = if manifest.targets.is_empty() {
        let inferred = infer_default_targets(manifest_format);
        if !inferred.is_empty() {
            manifest.targets = inferred;
            true
        } else {
            false
        }
    } else {
        false
    };

    let skills = parse_md_files(&dir, "skills", &manifest_format)?;
    let commands = parse_md_files(&dir, "commands", &manifest_format)?;
    let all_skills: Vec<SkillDef> = skills.into_iter().chain(commands).collect();

    let agents = parse_agent_files(&dir)?;
    let hooks = parse_hook_files(&dir)?;
    let mcp_servers = parse_mcp_files(&dir)?;
    let instructions = parse_instruction_files(&dir)?;
    let shared = parse_shared_files(&dir)?;
    let target_overrides = parse_target_overrides(&dir, &manifest)?;

    let output_styles = parse_output_style_files(&dir)?;
    let lsp_servers = parse_lsp_files(&dir)?;

    Ok(PluginIR {
        manifest,
        skills: all_skills,
        agents,
        hooks,
        mcp_servers,
        instructions,
        output_styles,
        lsp_servers,
        shared,
        target_overrides,
        source_dir: dir,
        targets_inferred,
    })
}

// ---------------------------------------------------------------------------
// Format detection
// ---------------------------------------------------------------------------

/// Where the manifest came from on disk.
///
/// Each per-target native format gets its own variant so we can infer the
/// implied target list when the manifest doesn't declare `targets:` itself.
/// Previously `.cursor-plugin/`, `.codex-plugin/`, `openclaw.plugin.json`, and
/// bare `plugin.json` all collapsed to `ClaudeCode`, which threw away the one
/// signal we need for inference.
#[derive(Debug, Clone, Copy, PartialEq)]
enum ManifestFormat {
    /// plugin.yaml at root (jacq IR format)
    Ir,
    /// .claude-plugin/plugin.json
    ClaudeCodeNative,
    /// .cursor-plugin/plugin.json
    CursorNative,
    /// .codex-plugin/plugin.json
    CodexNative,
    /// openclaw.plugin.json at root
    OpenClawNative,
    /// bare plugin.json at root — no per-target subdirectory
    RootPluginJson,
}

// ---------------------------------------------------------------------------
// WalkDir helper — collect entries, propagating errors instead of skipping
// ---------------------------------------------------------------------------

fn walk_files(dir: &Path, max_depth: usize, extensions: &[&str]) -> Result<Vec<walkdir::DirEntry>> {
    let mut entries = Vec::new();
    for result in WalkDir::new(dir).min_depth(1).max_depth(max_depth) {
        let entry = result.map_err(|e| {
            let path = e.path().unwrap_or(dir).to_path_buf();
            match e.into_io_error() {
                Some(io_err) => JacqError::IoWithPath {
                    path,
                    source: io_err,
                },
                None => JacqError::IoWithPath {
                    path,
                    source: std::io::Error::other("walkdir error"),
                },
            }
        })?;
        if entry.file_type().is_file()
            && let Some(ext) = entry.path().extension()
            && extensions.iter().any(|e| *e == ext)
        {
            entries.push(entry);
        }
    }
    Ok(entries)
}

// ---------------------------------------------------------------------------
// File name extraction — errors on empty names instead of producing ""
// ---------------------------------------------------------------------------

fn file_stem_or_err(path: &Path) -> Result<String> {
    let stem = path
        .file_stem()
        .and_then(|s| {
            let s = s.to_string_lossy().to_string();
            if s.is_empty() { None } else { Some(s) }
        })
        .ok_or_else(|| JacqError::ParseError {
            reason: format!("cannot derive name from path: {}", path.display()),
        })?;
    Ok(stem)
}

// ---------------------------------------------------------------------------
// Manifest parsing
// ---------------------------------------------------------------------------

fn parse_manifest(dir: &Path) -> Result<(PluginManifest, ManifestFormat)> {
    // Try IR format first: plugin.yaml at root
    let ir_path = dir.join("plugin.yaml");
    if ir_path.exists() {
        let content = read_file(&ir_path)?;
        let manifest: PluginManifest =
            serde_yaml::from_str(&content).map_err(|e| JacqError::ParseError {
                reason: format!("{}: {e}", ir_path.display()),
            })?;
        return Ok((manifest, ManifestFormat::Ir));
    }

    // Try Claude Code format: .claude-plugin/plugin.json
    let cc_path = dir.join(".claude-plugin").join("plugin.json");
    if cc_path.exists() {
        return parse_json_manifest(&cc_path).map(|m| (m, ManifestFormat::ClaudeCodeNative));
    }

    // Try Cursor format: .cursor-plugin/plugin.json
    let cursor_path = dir.join(".cursor-plugin").join("plugin.json");
    if cursor_path.exists() {
        return parse_json_manifest(&cursor_path).map(|m| (m, ManifestFormat::CursorNative));
    }

    // Try Codex format: .codex-plugin/plugin.json
    let codex_path = dir.join(".codex-plugin").join("plugin.json");
    if codex_path.exists() {
        return parse_json_manifest(&codex_path).map(|m| (m, ManifestFormat::CodexNative));
    }

    // Try OpenClaw format: openclaw.plugin.json
    let openclaw_path = dir.join("openclaw.plugin.json");
    if openclaw_path.exists() {
        return parse_json_manifest(&openclaw_path).map(|m| (m, ManifestFormat::OpenClawNative));
    }

    // Try root plugin.json — no per-target subdirectory hint, but historically
    // Claude Code plugins lived here before the .claude-plugin/ standard.
    let root_json = dir.join("plugin.json");
    if root_json.exists() {
        return parse_json_manifest(&root_json).map(|m| (m, ManifestFormat::RootPluginJson));
    }

    Err(JacqError::NoManifest {
        path: dir.to_path_buf(),
    })
}

/// Format-based default targets — applied when a manifest doesn't declare
/// `targets:` itself.
///
/// **Policy table** (see vendor survey in CLAUDE.md / commit history):
/// - `.claude-plugin/plugin.json`  → `[ClaudeCode]` (universal in CC ecosystem)
/// - `.cursor-plugin/plugin.json`  → `[Cursor]`     (Cursor template convention)
/// - `.codex-plugin/plugin.json`   → `[Codex]`      (no real-world repo uses
///   this yet, but the layout itself signals intent — forward-compat)
/// - `openclaw.plugin.json`        → `[OpenClaw]`   (universal in OpenClaw)
/// - root `plugin.json`            → `[ClaudeCode]` (legacy CC layout, the
///   only target that has historically used a bare manifest)
/// - `plugin.yaml` (IR)            → `[]` (empty — IR manifests are expected
///   to be explicit; not declaring targets is a real signal, not a default)
fn infer_default_targets(format: ManifestFormat) -> Vec<Target> {
    match format {
        ManifestFormat::Ir => vec![],
        ManifestFormat::ClaudeCodeNative => vec![Target::ClaudeCode],
        ManifestFormat::CursorNative => vec![Target::Cursor],
        ManifestFormat::CodexNative => vec![Target::Codex],
        ManifestFormat::OpenClawNative => vec![Target::OpenClaw],
        ManifestFormat::RootPluginJson => vec![Target::ClaudeCode],
    }
}

fn parse_json_manifest(path: &Path) -> Result<PluginManifest> {
    let content = read_file(path)?;
    serde_json::from_str(&content).map_err(|e| JacqError::ParseError {
        reason: format!("{}: {e}", path.display()),
    })
}

/// Read a file with path context in errors.
fn read_file(path: &Path) -> Result<String> {
    fs::read_to_string(path).map_err(|e| JacqError::IoWithPath {
        path: path.to_path_buf(),
        source: e,
    })
}

// ---------------------------------------------------------------------------
// YAML frontmatter helpers
// ---------------------------------------------------------------------------

/// Sanitize YAML frontmatter by quoting values that contain problematic characters.
/// Many real-world Claude Code plugins have unquoted values with colons, angle brackets,
/// etc. that are technically invalid YAML. This makes parsing lenient.
fn sanitize_yaml(yaml: &str) -> String {
    yaml.lines()
        .map(|line| {
            // Match "key: value" where value is not already quoted and contains
            // characters that break YAML parsing (: after the first one, <, >)
            if let Some(colon_pos) = line.find(": ") {
                let key = &line[..colon_pos];
                let value = &line[colon_pos + 2..];
                // Skip if already quoted, is a list item, or is a simple value
                let trimmed = value.trim();
                if trimmed.starts_with('"')
                    || trimmed.starts_with('\'')
                    || trimmed.starts_with('[')
                    || trimmed.starts_with('{')
                    || key.starts_with(' ') && key.trim().starts_with('-')
                {
                    return line.to_string();
                }
                // If the value contains additional colons or angle brackets, quote it
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

/// Try parsing YAML frontmatter, with lenient fallback for malformed values.
fn parse_yaml_frontmatter<T: serde::de::DeserializeOwned>(
    yaml: &str,
    path: &std::path::Path,
) -> Result<T> {
    serde_yaml::from_str(yaml)
        .or_else(|_| {
            let sanitized = sanitize_yaml(yaml);
            serde_yaml::from_str(&sanitized)
        })
        .map_err(|e| JacqError::InvalidFrontmatter {
            path: path.to_path_buf(),
            reason: e.to_string(),
        })
}

// ---------------------------------------------------------------------------
// YAML frontmatter extraction
// ---------------------------------------------------------------------------

/// Split a file into YAML frontmatter and markdown body.
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

// ---------------------------------------------------------------------------
// Skill/command parsing (.md files with YAML frontmatter)
// ---------------------------------------------------------------------------

fn parse_md_files(dir: &Path, subdir: &str, _format: &ManifestFormat) -> Result<Vec<SkillDef>> {
    let search_dir = dir.join(subdir);
    if !search_dir.exists() {
        return Ok(vec![]);
    }

    let entries = walk_files(&search_dir, 2, &["md"])?;
    let mut skills = Vec::new();

    for entry in entries {
        let path = entry.path();
        let content = read_file(path)?;
        let name = file_stem_or_err(path)?;
        let rel_path = path.strip_prefix(dir).unwrap_or(path).to_path_buf();

        let (yaml_str, body) = split_frontmatter(&content);
        let frontmatter: SkillFrontmatter = match yaml_str {
            Some(yaml) => parse_yaml_frontmatter(yaml, &rel_path)?,
            None => SkillFrontmatter::default(),
        };

        skills.push(SkillDef {
            name,
            source_path: rel_path,
            frontmatter,
            body: body.to_string().into(),
        });
    }

    skills.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(skills)
}

// ---------------------------------------------------------------------------
// Agent parsing (.md files with YAML frontmatter)
// ---------------------------------------------------------------------------

fn parse_agent_files(dir: &Path) -> Result<Vec<AgentDef>> {
    let agents_dir = dir.join("agents");
    if !agents_dir.exists() {
        return Ok(vec![]);
    }

    let entries = walk_files(&agents_dir, 2, &["md"])?;
    let mut agents = Vec::new();

    for entry in entries {
        let path = entry.path();
        let content = read_file(path)?;
        let name = file_stem_or_err(path)?;
        let rel_path = path.strip_prefix(dir).unwrap_or(path).to_path_buf();

        let (yaml_str, body) = split_frontmatter(&content);
        let frontmatter: AgentFrontmatter = match yaml_str {
            Some(yaml) => parse_yaml_frontmatter(yaml, &rel_path)?,
            None => AgentFrontmatter::default(),
        };

        agents.push(AgentDef {
            name,
            source_path: rel_path,
            frontmatter,
            body: body.to_string().into(),
        });
    }

    agents.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(agents)
}

// ---------------------------------------------------------------------------
// Hook parsing (.yaml files)
// ---------------------------------------------------------------------------

fn parse_hook_files(dir: &Path) -> Result<Vec<HookDef>> {
    let hooks_dir = dir.join("hooks");
    if !hooks_dir.exists() {
        return Ok(vec![]);
    }

    let entries = walk_files(&hooks_dir, 1, &["yaml", "yml"])?;
    let mut hooks = Vec::new();

    for entry in entries {
        let path = entry.path();
        let content = read_file(path)?;
        let rel_path = path.strip_prefix(dir).unwrap_or(path).to_path_buf();

        let mut hook: HookDef =
            serde_yaml::from_str(&content).map_err(|e| JacqError::InvalidFrontmatter {
                path: rel_path.clone(),
                reason: e.to_string(),
            })?;
        hook.source_path = rel_path;
        hooks.push(hook);
    }

    hooks.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(hooks)
}

// ---------------------------------------------------------------------------
// MCP server parsing (.yaml files)
// ---------------------------------------------------------------------------

fn parse_mcp_files(dir: &Path) -> Result<Vec<McpServerDef>> {
    let mcp_dir = dir.join("mcp");
    if !mcp_dir.exists() {
        return Ok(vec![]);
    }

    let entries = walk_files(&mcp_dir, 1, &["yaml", "yml"])?;
    let mut servers = Vec::new();

    for entry in entries {
        let path = entry.path();
        let content = read_file(path)?;
        let rel_path = path.strip_prefix(dir).unwrap_or(path).to_path_buf();

        let mut server: McpServerDef =
            serde_yaml::from_str(&content).map_err(|e| JacqError::InvalidFrontmatter {
                path: rel_path.clone(),
                reason: e.to_string(),
            })?;
        server.source_path = rel_path;
        servers.push(server);
    }

    servers.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(servers)
}

// ---------------------------------------------------------------------------
// LSP server parsing (.json files)
// ---------------------------------------------------------------------------
//
// LSP configs are JSON in the wild (VS Code settings, Claude Code's lsp blocks)
// and `LspServerDef` carries `serde_json::Value` fields for free-form
// initializationOptions/settings, so JSON is the natural on-disk format.

fn parse_lsp_files(dir: &Path) -> Result<Vec<LspServerDef>> {
    let lsp_dir = dir.join("lsp");
    if !lsp_dir.exists() {
        return Ok(vec![]);
    }

    let entries = walk_files(&lsp_dir, 1, &["json"])?;
    let mut servers = Vec::new();

    for entry in entries {
        let path = entry.path();
        let content = read_file(path)?;
        let rel_path = path.strip_prefix(dir).unwrap_or(path).to_path_buf();

        let server: LspServerDef =
            serde_json::from_str(&content).map_err(|e| JacqError::InvalidFrontmatter {
                path: rel_path,
                reason: e.to_string(),
            })?;
        servers.push(server);
    }

    servers.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(servers)
}

// ---------------------------------------------------------------------------
// Instruction parsing (.md files, body only — no frontmatter)
// ---------------------------------------------------------------------------

fn parse_instruction_files(dir: &Path) -> Result<Vec<InstructionDef>> {
    let instr_dir = dir.join("instructions");
    if !instr_dir.exists() {
        return Ok(vec![]);
    }

    let entries = walk_files(&instr_dir, 1, &["md"])?;
    let mut instructions = Vec::new();

    for entry in entries {
        let path = entry.path();
        let content = read_file(path)?;
        let name = file_stem_or_err(path)?;
        let rel_path = path.strip_prefix(dir).unwrap_or(path).to_path_buf();

        instructions.push(InstructionDef {
            name,
            source_path: rel_path,
            body: content.into(),
        });
    }

    instructions.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(instructions)
}

// ---------------------------------------------------------------------------
// Output style parsing (.md files, body only)
// ---------------------------------------------------------------------------

fn parse_output_style_files(dir: &Path) -> Result<Vec<OutputStyleDef>> {
    let styles_dir = dir.join("output-styles");
    if !styles_dir.exists() {
        return Ok(vec![]);
    }

    let entries = walk_files(&styles_dir, 1, &["md"])?;
    let mut styles = Vec::new();

    for entry in entries {
        let path = entry.path();
        let content = read_file(path)?;
        let name = file_stem_or_err(path)?;
        let rel_path = path.strip_prefix(dir).unwrap_or(path).to_path_buf();

        styles.push(OutputStyleDef {
            name,
            source_path: rel_path,
            body: content.into(),
        });
    }

    styles.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(styles)
}

// ---------------------------------------------------------------------------
// Shared fragment parsing (.md files, body only — same pattern as instructions)
// ---------------------------------------------------------------------------

fn parse_shared_files(dir: &Path) -> Result<Vec<SharedFragment>> {
    let shared_dir = dir.join("shared");
    if !shared_dir.exists() {
        return Ok(vec![]);
    }

    let entries = walk_files(&shared_dir, 1, &["md"])?;
    let mut fragments = Vec::new();

    for entry in entries {
        let path = entry.path();
        let content = read_file(path)?;
        let name = file_stem_or_err(path)?;
        let rel_path = path.strip_prefix(dir).unwrap_or(path).to_path_buf();

        fragments.push(SharedFragment {
            name,
            source_path: rel_path,
            body: content.into(),
        });
    }

    fragments.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(fragments)
}

// ---------------------------------------------------------------------------
// Target overrides (targets/<target-name>/*)
// ---------------------------------------------------------------------------

fn parse_target_overrides(
    dir: &Path,
    manifest: &PluginManifest,
) -> Result<BTreeMap<Target, Vec<TargetOverride>>> {
    let targets_dir = dir.join("targets");
    if !targets_dir.exists() {
        return Ok(BTreeMap::new());
    }

    let mut overrides = BTreeMap::new();

    for target in &manifest.targets {
        let target_dir = targets_dir.join(target.as_str());
        if !target_dir.exists() {
            continue;
        }

        let mut files = Vec::new();
        for result in WalkDir::new(&target_dir).min_depth(1) {
            let entry = result.map_err(|e| {
                let path = e.path().unwrap_or(&target_dir).to_path_buf();
                match e.into_io_error() {
                    Some(io_err) => JacqError::IoWithPath {
                        path,
                        source: io_err,
                    },
                    None => JacqError::IoWithPath {
                        path,
                        source: std::io::Error::other("walkdir error"),
                    },
                }
            })?;
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.path();
            let rel_path = path.strip_prefix(&target_dir).unwrap_or(path).to_path_buf();
            let content = fs::read(path).map_err(|e| JacqError::IoWithPath {
                path: path.to_path_buf(),
                source: e,
            })?;
            files.push(TargetOverride {
                path: rel_path,
                content,
            });
        }

        if !files.is_empty() {
            files.sort_by(|a, b| a.path.cmp(&b.path));
            overrides.insert(*target, files);
        }
    }

    Ok(overrides)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_frontmatter_with_yaml() {
        let content = "---\ndescription: test\n---\nBody here";
        let (fm, body) = split_frontmatter(content);
        assert_eq!(fm, Some("description: test"));
        assert_eq!(body, "Body here");
    }

    #[test]
    fn split_frontmatter_no_yaml() {
        let content = "Just a body with no frontmatter";
        let (fm, body) = split_frontmatter(content);
        assert!(fm.is_none());
        assert_eq!(body, content);
    }

    #[test]
    fn split_frontmatter_no_closing_delimiter() {
        let content = "---\ndescription: test\nno closing";
        let (fm, body) = split_frontmatter(content);
        assert!(fm.is_none());
        assert_eq!(body, content);
    }

    #[test]
    fn split_frontmatter_multiline_yaml() {
        let content = "---\ndescription: test\ncolor: blue\nallowed-tools:\n  - Read\n  - Write\n---\n\nMarkdown body\n";
        let (fm, body) = split_frontmatter(content);
        assert!(fm.unwrap().contains("description: test"));
        assert!(fm.unwrap().contains("allowed-tools:"));
        assert!(body.starts_with('\n'));
        assert!(body.contains("Markdown body"));
    }

    #[test]
    fn split_frontmatter_real_skill_format() {
        let content = "\
---
description: CRUD operations for macOS Notes.app
argument-hint: [describe what you want to do]
allowed-tools: Bash(osascript:*)
---

You are a macOS Notes.app assistant.
";
        let (fm, body) = split_frontmatter(content);
        let yaml = fm.expect("should have frontmatter");
        assert!(yaml.contains("description: CRUD operations"));
        assert!(yaml.contains("allowed-tools: Bash(osascript:*)"));

        let parsed: SkillFrontmatter = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(
            parsed.description.as_deref(),
            Some("CRUD operations for macOS Notes.app")
        );

        assert!(body.contains("You are a macOS Notes.app assistant."));
    }
}
